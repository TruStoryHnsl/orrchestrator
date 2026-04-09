//! HTTP server scaffold for the web node editor.
//!
//! Uses `tiny_http` — a blocking, single-file HTTP/1.1 server that runs on a
//! dedicated OS thread. The caller gets back a `ServerHandle` that exposes the
//! bound address (for opening in a browser) and joins / stops the worker
//! thread on drop.
//!
//! Routing is intentionally hand-rolled so we can keep this crate lean:
//!
//! - `GET /`               → embedded `index.html`
//! - `GET /app.js`         → embedded client script
//! - `GET /style.css`      → embedded stylesheet
//! - `GET /api/workforces` → list of workforce summaries as JSON
//! - `GET /api/workforce/:name`  → full workforce JSON
//! - `POST /api/workforce/:name` → accepts JSON, writes markdown to disk
//! - anything else         → 404
//!
//! The JSON routes live in [`crate::api`]; the static asset bodies live in
//! [`crate::assets`]. This module wires them together.

use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use tiny_http::{Header, Method, Response, Server};

use crate::api;
use crate::assets;

/// Handle to a running web editor HTTP server.
///
/// Dropping the handle signals the worker thread to stop and joins it. The
/// `addr()` accessor returns the socket address the server is bound to —
/// callers can format `http://{addr}/` and open a browser to it.
pub struct ServerHandle {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ServerHandle {
    /// Socket address the HTTP server is bound to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Base URL (e.g. `http://127.0.0.1:43211`) suitable for `xdg-open`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Ask the worker thread to stop accepting new connections and join it.
    /// Called automatically by `Drop`, but exposed for callers that want a
    /// deterministic shutdown point.
    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Launch the web editor HTTP server.
///
/// - `workforces_dir`: directory holding `*.md` workforce files. GET requests
///   read from here; POST requests write here.
/// - `port`: the TCP port to bind to. Pass `0` for an ephemeral port (useful
///   for tests and for the TUI launcher, which doesn't care about the exact
///   number).
///
/// Returns a [`ServerHandle`] whose `addr()` exposes the actually bound
/// address.
pub fn launch_webedit_server(workforces_dir: PathBuf, port: u16) -> Result<ServerHandle> {
    // Bind via std so we can extract the ephemeral port BEFORE tiny_http
    // takes ownership of the listener.
    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("binding 127.0.0.1:{port}"))?;
    let addr = listener.local_addr().context("reading bound addr")?;
    // tiny_http wants a `Server::from_listener` style constructor. The 0.12
    // API takes a `TcpListener` directly.
    let server = Server::from_listener(listener, None)
        .map_err(|e| anyhow::anyhow!("tiny_http init failed: {e}"))?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_worker = Arc::clone(&stop);
    let dir = workforces_dir.clone();

    let thread = thread::Builder::new()
        .name("orrch-webedit".into())
        .spawn(move || {
            run_loop(server, &dir, stop_worker);
        })
        .context("spawning webedit worker thread")?;

    Ok(ServerHandle {
        addr,
        stop,
        thread: Some(thread),
    })
}

/// Main request loop. Exits when `stop` is set to true OR on an unrecoverable
/// recv error. We poll with a short timeout so the stop flag is checked on
/// every iteration without pinning a core.
fn run_loop(server: Server, workforces_dir: &Path, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        match server.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(request)) => {
                if let Err(e) = handle_request(request, workforces_dir) {
                    tracing::warn!("webedit request handler failed: {e:#}");
                }
            }
            Ok(None) => {
                // timeout — loop and re-check stop flag
            }
            Err(e) => {
                tracing::warn!("webedit server recv failed: {e}");
                break;
            }
        }
    }
}

/// Route a single HTTP request to the appropriate handler.
fn handle_request(mut request: tiny_http::Request, workforces_dir: &Path) -> Result<()> {
    let method = request.method().clone();
    let url = request.url().to_string();
    tracing::debug!(?method, %url, "webedit request");

    // Strip query string if present — we don't use it today, but future
    // handlers shouldn't trip over `?foo=bar`.
    let path = url.split('?').next().unwrap_or("/");

    let response = match (&method, path) {
        (&Method::Get, "/") => html_response(assets::INDEX_HTML),
        (&Method::Get, "/app.js") => js_response(assets::APP_JS),
        (&Method::Get, "/style.css") => css_response(assets::STYLE_CSS),
        (&Method::Get, "/api/workforces") => api::list_workforces(workforces_dir),
        (&Method::Get, p) if p.starts_with("/api/workforce/") => {
            let name = &p["/api/workforce/".len()..];
            api::get_workforce(workforces_dir, name)
        }
        (&Method::Post, p) if p.starts_with("/api/workforce/") => {
            let name = &p["/api/workforce/".len()..];
            let mut body = String::new();
            request
                .as_reader()
                .read_to_string(&mut body)
                .context("reading request body")?;
            api::put_workforce(workforces_dir, name, &body)
        }
        _ => not_found(),
    };

    request.respond(response).context("sending response")?;
    Ok(())
}

fn html_response(body: &'static str) -> Response<std::io::Cursor<Vec<u8>>> {
    text_response(body, "text/html; charset=utf-8")
}

fn js_response(body: &'static str) -> Response<std::io::Cursor<Vec<u8>>> {
    text_response(body, "application/javascript; charset=utf-8")
}

fn css_response(body: &'static str) -> Response<std::io::Cursor<Vec<u8>>> {
    text_response(body, "text/css; charset=utf-8")
}

fn text_response(body: &str, content_type: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_string(body);
    if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()) {
        resp = resp.with_header(h);
    }
    resp
}

fn not_found() -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string("not found").with_status_code(404)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;

    /// Spin up a server on an ephemeral port against an empty tempdir, GET
    /// `/`, and assert a 200 status line.
    #[test]
    fn server_serves_index_on_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let handle =
            launch_webedit_server(tmp.path().to_path_buf(), 0).expect("server starts");
        let addr = handle.addr();

        let mut stream = TcpStream::connect(addr).expect("connect");
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .expect("write request");

        let mut reader = BufReader::new(&stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).expect("read status");
        assert!(
            status_line.starts_with("HTTP/1.1 200"),
            "expected 200, got: {status_line:?}"
        );

        // drain the rest so the server cleanly closes before shutdown
        let mut rest = Vec::new();
        let _ = reader.into_inner().read_to_end(&mut rest);

        handle.shutdown();
    }

    #[test]
    fn server_returns_404_for_unknown_route() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let handle =
            launch_webedit_server(tmp.path().to_path_buf(), 0).expect("server starts");
        let addr = handle.addr();

        let mut stream = TcpStream::connect(addr).expect("connect");
        stream
            .write_all(
                b"GET /nope HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .expect("write");
        let mut reader = BufReader::new(&stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).expect("read");
        assert!(status_line.starts_with("HTTP/1.1 404"), "got: {status_line:?}");

        handle.shutdown();
    }

    /// Full end-to-end persistence contract:
    ///   1. Spin up the server against an empty tempdir
    ///   2. POST a JSON workforce with 3 agents via a real TCP socket
    ///   3. GET the same workforce via a second socket
    ///   4. Assert the parsed response has 3 agents
    ///
    /// This locks down the editor's save/load round-trip — if anything
    /// upstream (parser, serializer, HTTP routing) regresses, this test
    /// fails loudly instead of the bug only surfacing in the browser.
    #[test]
    fn roundtrip_post_then_get_over_socket() {
        use orrch_workforce::{parse_workforce_markdown, AgentNode, Workforce};

        let tmp = tempfile::tempdir().expect("tempdir");
        let handle =
            launch_webedit_server(tmp.path().to_path_buf(), 0).expect("server starts");
        let addr = handle.addr();

        // Build a 3-agent workforce via the public types + parser so we know
        // the JSON shape matches what the API expects.
        let base_md = "---\nname: Round Trip\ndescription: socket roundtrip fixture\noperations:\n  - DEVELOP FEATURE\n---\n\n## Agents\n\n| ID | Agent Profile | User-Facing |\n|----|---------------|-------------|\n| pm | Project Manager | yes |\n";
        let mut wf: Workforce =
            parse_workforce_markdown(base_md).expect("fixture parses");
        wf.agents.push(AgentNode {
            id: "dev1".into(),
            agent_profile: "Developer".into(),
            user_facing: false,
            nested_workforce: None,
        });
        wf.agents.push(AgentNode {
            id: "dev2".into(),
            agent_profile: "Developer".into(),
            user_facing: false,
            nested_workforce: None,
        });
        assert_eq!(wf.agents.len(), 3, "fixture sanity");

        let body = serde_json::to_string(&wf).expect("serialize");
        let request = format!(
            "POST /api/workforce/Round%20Trip HTTP/1.1\r\n\
             Host: localhost\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            body.len(),
            body
        );

        // --- POST ---
        let mut stream = TcpStream::connect(addr).expect("connect POST");
        stream.write_all(request.as_bytes()).expect("write POST");
        let mut reader = BufReader::new(&stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).expect("read POST status");
        assert!(
            status_line.starts_with("HTTP/1.1 200"),
            "POST expected 200, got: {status_line:?}"
        );
        // Drain so the server can cleanly close before the next connection.
        let mut rest = Vec::new();
        let _ = reader.into_inner().read_to_end(&mut rest);

        // --- GET ---
        let mut stream = TcpStream::connect(addr).expect("connect GET");
        stream
            .write_all(
                b"GET /api/workforce/Round%20Trip HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .expect("write GET");
        let mut reader = BufReader::new(&stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).expect("read GET status");
        assert!(
            status_line.starts_with("HTTP/1.1 200"),
            "GET expected 200, got: {status_line:?}"
        );

        // Skip headers (consume until empty line). Keep reading through the
        // BufReader so we don't lose already-buffered body bytes — calling
        // `into_inner()` before we finish reading would drop them.
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).expect("read header");
            if n == 0 || line == "\r\n" || line == "\n" {
                break;
            }
        }
        // Read the body to EOF via the SAME BufReader so buffered bytes
        // aren't discarded.
        let mut body_buf = Vec::new();
        reader.read_to_end(&mut body_buf).expect("read GET body");
        let body_str = String::from_utf8(body_buf).expect("utf8");
        let got: Workforce =
            serde_json::from_str(&body_str).expect("parse GET body as Workforce");
        assert_eq!(
            got.agents.len(),
            3,
            "roundtripped workforce should preserve all 3 agents, got: {body_str}"
        );
        assert_eq!(got.name, "Round Trip");

        handle.shutdown();
    }
}
