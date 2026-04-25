use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{header, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, watch};

use crate::assets;
use crate::state::{state_hash, WebAction, WebAppState};

#[derive(Clone)]
pub struct ServerState {
    pub state_rx: Arc<watch::Receiver<WebAppState>>,
    pub action_queue: Arc<Mutex<VecDeque<WebAction>>>,
    /// Broadcasts ANSI bytes emitted by the TUI backend to all connected
    /// terminal-websocket clients.
    pub terminal_tx: broadcast::Sender<Vec<u8>>,
    /// Forwards keystrokes received from terminal-websocket clients to the
    /// main TUI event loop.
    pub input_tx: Arc<mpsc::UnboundedSender<Vec<u8>>>,
    /// Raised when a new terminal client connects. The main loop polls
    /// this to trigger a full TUI redraw so the newcomer sees the screen.
    pub redraw_flag: Arc<std::sync::atomic::AtomicBool>,
    /// Optional bearer token. When set, non-localhost requests must present
    /// it via `Authorization: Bearer <token>`, `?token=<token>`, or
    /// `Cookie: orrch_token=<token>`.
    pub auth_token: Option<String>,
}

pub fn build_router(srv: ServerState) -> Router {
    let token = srv.auth_token.clone();
    Router::new()
        .route("/", get(serve_index))
        .route("/landing", get(serve_landing))
        .route("/terminal", get(serve_terminal))
        .route("/ui", get(serve_ui))
        .route("/static/css/main.css", get(serve_main_css))
        .route("/static/js/ws.js", get(serve_ws_js))
        .route("/static/js/layout.js", get(serve_layout_js))
        .route("/static/js/intentions.js", get(serve_intentions_js))
        .route("/static/js/sessions.js", get(serve_sessions_js))
        .route("/static/js/mobile.js", get(serve_mobile_js))
        .route("/pty", get(terminal_ws))
        .route("/state", get(state_ws))
        .route("/size", get(get_size))
        .route("/action", post(handle_action))
        .layer(middleware::from_fn(move |req, next| {
            let token = token.clone();
            async move { auth_middleware(token, req, next).await }
        }))
        .with_state(srv)
}

/// Optional-token auth gate.
///
/// - When `token` is `None`: pass-through (unconfigured = open).
/// - When `token` is `Some`: requests from `127.0.0.1` / `::1` pass through
///   (operator's own machine is trusted). All other requests must present
///   the token via `Authorization: Bearer <token>`, `?token=<token>`, or
///   `Cookie: orrch_token=<token>`. Otherwise returns 401.
///
/// The cookie path supports the bookmark-friendly login flow: visit
/// `https://orrchestrator.com/?token=...` once and the browser caches the
/// cookie for subsequent requests.
async fn auth_middleware(
    token: Option<String>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let Some(token) = token else {
        return next.run(req).await;
    };

    // Trust loopback unconditionally. ConnectInfo is populated by
    // `into_make_service_with_connect_info`; if it's missing we err on the
    // side of requiring the token.
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<std::net::SocketAddr>>() {
        if is_loopback(addr.ip()) {
            return next.run(req).await;
        }
    }

    if request_has_token(&req, &token) {
        // Set a cookie if the token came in via query string so subsequent
        // requests can omit it. SameSite=Lax + HttpOnly + Secure on https.
        let scheme_is_https = req.uri().scheme_str() == Some("https")
            || req.headers().get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok()) == Some("https");
        let mut response = next.run(req).await;
        if scheme_is_https {
            // Best-effort cookie set — drop on header insertion failure.
            if let Ok(cookie) = format!(
                "orrch_token={token}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=2592000"
            ).parse() {
                response.headers_mut().append(header::SET_COOKIE, cookie);
            }
        }
        return response;
    }

    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer realm=\"orrchestrator\"")],
        "401 — token required. Pass ?token=<TOKEN>, Authorization: Bearer <TOKEN>, or Cookie orrch_token=<TOKEN>.",
    )
        .into_response()
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

fn request_has_token(req: &Request<axum::body::Body>, expected: &str) -> bool {
    // Authorization: Bearer <token>
    if let Some(h) = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(rest) = h.strip_prefix("Bearer ") {
            if constant_time_eq(rest.trim(), expected) {
                return true;
            }
        }
    }
    // Cookie: orrch_token=<token>
    if let Some(cookies) = req.headers().get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for piece in cookies.split(';') {
            let kv = piece.trim();
            if let Some(value) = kv.strip_prefix("orrch_token=") {
                if constant_time_eq(value, expected) {
                    return true;
                }
            }
        }
    }
    // ?token=<token>
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("token=") {
                let decoded = percent_decode(value);
                if constant_time_eq(&decoded, expected) {
                    return true;
                }
            }
        }
    }
    false
}

/// Constant-time comparison so a malicious caller can't time the auth.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Minimal percent-decoder for the `?token=` parameter — enough for typical
/// URL-safe tokens. Falls back to the raw input if decoding fails.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}


// `/` serves the terminal mirror — that's the primary, reliable 1-to-1
// TUI display. The native UI is secondary at `/ui`.
async fn serve_index() -> impl IntoResponse { html(assets::TERMINAL_HTML) }
async fn serve_landing() -> impl IntoResponse { html(assets::INDEX_HTML) }
async fn serve_terminal() -> impl IntoResponse { html(assets::TERMINAL_HTML) }
async fn serve_ui() -> impl IntoResponse { html(assets::UI_HTML) }
async fn serve_main_css() -> impl IntoResponse { css(assets::MAIN_CSS) }
async fn serve_ws_js() -> impl IntoResponse { js(assets::WS_JS) }
async fn serve_layout_js() -> impl IntoResponse { js(assets::LAYOUT_JS) }
async fn serve_intentions_js() -> impl IntoResponse { js(assets::INTENTIONS_JS) }
async fn serve_sessions_js() -> impl IntoResponse { js(assets::SESSIONS_JS) }
async fn serve_mobile_js() -> impl IntoResponse { js(assets::MOBILE_JS) }

/// Terminal WebSocket: streams TUI output (broadcast bytes) to the client
/// and receives keystrokes back.
///
/// Note the endpoint is still `/pty` for backwards compatibility with the
/// existing terminal.html; it's no longer a real PTY.
async fn terminal_ws(ws: WebSocketUpgrade, State(srv): State<ServerState>) -> Response {
    ws.on_upgrade(move |socket| terminal_ws_handler(socket, srv))
}

async fn terminal_ws_handler(socket: WebSocket, srv: ServerState) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    let mut term_rx = srv.terminal_tx.subscribe();
    let input_tx = Arc::clone(&srv.input_tx);

    tracing::info!(
        "Terminal WS client connected — {} total receivers now",
        srv.terminal_tx.receiver_count()
    );

    // Signal the main loop to emit a full redraw so this new client sees
    // the current screen rather than whatever partial diffs come next.
    srv.redraw_flag.store(true, std::sync::atomic::Ordering::Relaxed);

    // Outbound: TUI broadcast → WebSocket
    let send_task = tokio::spawn(async move {
        let mut bytes_sent: usize = 0;
        loop {
            match term_rx.recv().await {
                Ok(chunk) => {
                    bytes_sent += chunk.len();
                    tracing::info!(
                        "WS send {} bytes (total {})",
                        chunk.len(),
                        bytes_sent
                    );
                    if ws_sink.send(Message::Binary(chunk.into())).await.is_err() {
                        tracing::warn!("WS send failed, disconnecting");
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("WS lagged by {n} chunks");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("terminal_tx closed, disconnecting ws");
                    break;
                }
            }
        }
    });

    // Inbound: WebSocket keystrokes → TUI input channel.
    // xterm.js sends keystrokes as binary (Uint8Array from TextEncoder).
    // We deliberately drop Text messages to avoid misinterpreting JSON
    // control frames or stray debug messages as keypresses.
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_stream.next().await {
            match msg {
                Message::Binary(bytes) => {
                    if input_tx.send(bytes.to_vec()).is_err() { break; }
                }
                Message::Close(_) => break,
                _ => {} // ignore Text, Ping, Pong
            }
        }
    });

    // When either task ends, abort the other and let the handler return.
    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }
}

async fn state_ws(ws: WebSocketUpgrade, State(srv): State<ServerState>) -> Response {
    ws.on_upgrade(move |socket| state_ws_handler(socket, srv))
}

async fn state_ws_handler(mut socket: WebSocket, srv: ServerState) {
    let mut rx = (*srv.state_rx).clone();

    // Send initial state unconditionally
    let snapshot = rx.borrow_and_update().clone();
    let mut last_hash = state_hash(&snapshot);
    if let Ok(json) = serde_json::to_string(&snapshot) {
        if socket.send(Message::Text(json.into())).await.is_err() { return; }
    }

    // Then watch for changes
    loop {
        if rx.changed().await.is_err() { return; }
        let snapshot = rx.borrow_and_update().clone();
        let hash = state_hash(&snapshot);
        if hash != last_hash {
            last_hash = hash;
            if let Ok(json) = serde_json::to_string(&snapshot) {
                if socket.send(Message::Text(json.into())).await.is_err() { return; }
            }
        }
    }
}

async fn get_size(State(srv): State<ServerState>) -> impl IntoResponse {
    let state = srv.state_rx.borrow();
    let body = serde_json::json!({ "cols": state.term_cols, "rows": state.term_rows });
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body.to_string(),
    ).into_response()
}

async fn handle_action(State(srv): State<ServerState>, Json(body): Json<Value>) -> StatusCode {
    if let Ok(action) = serde_json::from_value::<WebAction>(body) {
        if let Ok(mut q) = srv.action_queue.lock() {
            q.push_back(action);
        }
    }
    StatusCode::OK
}

fn html(s: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
        ],
        s,
    ).into_response()
}
fn css(s: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css"),
            (header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
        ],
        s,
    ).into_response()
}
fn js(s: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/javascript"),
            (header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
        ],
        s,
    ).into_response()
}


#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn build_req(headers: &[(&str, &str)], uri: &str) -> Request<axum::body::Body> {
        let mut builder = Request::builder().uri(uri);
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        builder.body(axum::body::Body::empty()).unwrap()
    }

    #[test]
    fn token_via_bearer_header() {
        let req = build_req(&[("Authorization", "Bearer hunter2")], "/");
        assert!(request_has_token(&req, "hunter2"));
        assert!(!request_has_token(&req, "wrong"));
    }

    #[test]
    fn token_via_cookie() {
        let req = build_req(&[("Cookie", "foo=bar; orrch_token=hunter2; baz=qux")], "/");
        assert!(request_has_token(&req, "hunter2"));
        assert!(!request_has_token(&req, "wrong"));
    }

    #[test]
    fn token_via_query_string() {
        let req = build_req(&[], "/?foo=1&token=hunter2&bar=2");
        assert!(request_has_token(&req, "hunter2"));
        assert!(!request_has_token(&req, "wrong"));
    }

    #[test]
    fn token_query_percent_decoded() {
        // "%20" decodes to a space character
        let req = build_req(&[], "/?token=hunter%20two");
        assert!(request_has_token(&req, "hunter two"));
    }

    #[test]
    fn no_token_anywhere_rejects() {
        let req = build_req(&[], "/somepath");
        assert!(!request_has_token(&req, "any"));
    }

    #[test]
    fn constant_time_eq_handles_lengths() {
        assert!(!constant_time_eq("a", "ab"));
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback("127.0.0.1".parse().unwrap()));
        assert!(is_loopback("127.0.0.5".parse().unwrap()));
        assert!(is_loopback("::1".parse().unwrap()));
        assert!(!is_loopback("192.168.1.1".parse().unwrap()));
        assert!(!is_loopback("8.8.8.8".parse().unwrap()));
    }
}
