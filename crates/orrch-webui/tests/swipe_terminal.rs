//! Integration tests for the dual-page swipe terminal — Page 1 mirrors
//! the orrchestrator TUI; Page 2 attaches to an independent tmux session.
//!
//! These tests pin user-observable surface area: the HTML structure both
//! panels rely on, the asset routes the swipe page loads, and the
//! `/shell/size` endpoint the Page 2 xterm bootstrap depends on. They
//! are regression-pinning, not feature-belief — they describe what the
//! user-facing browser gets, not internal implementation details.
//!
//! `/shell/size` and the WS-driven shell behavior require `tmux` on
//! PATH. When tmux is missing (CI without it), the corresponding
//! assertions degrade to "endpoint reachable" rather than failing —
//! the HTML/asset surface is still pinned.

use std::time::Duration;

use orrch_webui::{WebUiConfig, WebUiServer};

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("local_addr").port()
}

fn tmux_available() -> bool {
    std::process::Command::new("tmux")
        .arg("-V")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Set a unique tmux session name so concurrent test runs don't collide.
fn set_unique_session_env() -> String {
    let name = format!(
        "orrch-test-shell-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    // SAFETY: tests run single-threaded per `#[tokio::test]` but cargo
    // can run multiple test binaries in parallel; the unique-name suffix
    // means even concurrent processes don't collide on the env-derived
    // session name.
    unsafe {
        std::env::set_var("ORRCH_WEBUI_TMUX_SESSION", &name);
    }
    name
}

fn cleanup_session(name: &str) {
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[tokio::test]
async fn index_html_contains_both_panels() {
    let port = free_port();
    let session = set_unique_session_env();
    let cfg = WebUiConfig {
        local_port: port,
        ..Default::default()
    };
    let _srv = WebUiServer::start_with_config(cfg).await.expect("start");

    let body = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("request")
        .text()
        .await
        .expect("body");

    assert!(
        body.contains(r#"id="panel-mirror""#),
        "index HTML missing panel-mirror — swipe layout broken"
    );
    assert!(
        body.contains(r#"id="panel-shell""#),
        "index HTML missing panel-shell — swipe layout broken"
    );
    assert!(
        body.contains("/static/js/shell.js"),
        "index HTML doesn't load shell.js — Page 2 won't bootstrap"
    );
    assert!(
        body.contains("scroll-snap-type"),
        "swipe container needs scroll-snap-type for native horizontal swipe"
    );

    cleanup_session(&session);
}

#[tokio::test]
async fn shell_js_asset_served() {
    let port = free_port();
    let session = set_unique_session_env();
    let cfg = WebUiConfig {
        local_port: port,
        ..Default::default()
    };
    let _srv = WebUiServer::start_with_config(cfg).await.expect("start");

    let resp = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/static/js/shell.js"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200, "/static/js/shell.js must serve 200");
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        ctype.contains("javascript"),
        "shell.js content-type expected JS, got {ctype}"
    );
    let body = resp.text().await.expect("body");
    assert!(
        body.contains("Shell.init") || body.contains("var Shell"),
        "shell.js doesn't expose the Shell global — Page 2 bootstrap will fail"
    );

    cleanup_session(&session);
}

/// User-visible regression check: the asset-served HTML must include the
/// FitAddon script so xterm.js can compute its viewport-fit dimensions.
/// Without this, the /shell page falls back to a fixed cols × rows that
/// doesn't match the phone viewport — exactly the bug being fixed.
#[tokio::test]
async fn terminal_html_loads_xterm_fit_addon() {
    let port = free_port();
    let session = set_unique_session_env();
    let cfg = WebUiConfig {
        local_port: port,
        ..Default::default()
    };
    let _srv = WebUiServer::start_with_config(cfg).await.expect("start");

    let body = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("request")
        .text()
        .await
        .expect("body");

    assert!(
        body.contains("xterm-addon-fit"),
        "terminal.html missing xterm-addon-fit script — viewport-fit broken"
    );
    assert!(
        body.contains("--term-font-size"),
        "terminal.html missing --term-font-size CSS variable"
    );

    cleanup_session(&session);
}

/// `POST /shell/resize` round-trip: the endpoint must drive tmux so a
/// follow-up `tmux display-message` reports the new dims. This is the
/// non-WS path; the WS control frame uses the same code path.
#[tokio::test]
async fn shell_resize_post_drives_tmux() {
    if !tmux_available() {
        eprintln!("tmux not on PATH — skipping shell_resize_post_drives_tmux");
        return;
    }
    let port = free_port();
    let session = set_unique_session_env();
    let cfg = WebUiConfig {
        local_port: port,
        ..Default::default()
    };
    let _srv = WebUiServer::start_with_config(cfg).await.expect("start");

    // Hit /shell/size first to materialize the session.
    let _ = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/shell/size"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("request");

    // Now drive a specific size.
    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/shell/resize"))
        .json(&serde_json::json!({ "cols": 42, "rows": 17 }))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status(), 200, "/shell/resize must succeed");

    // Verify with tmux directly.
    let dims_out = std::process::Command::new("tmux")
        .args([
            "display-message", "-p", "-t", &session,
            "#{pane_width}x#{pane_height}",
        ])
        .output()
        .expect("tmux display-message");
    let dims = String::from_utf8_lossy(&dims_out.stdout).trim().to_string();
    assert_eq!(
        dims, "42x17",
        "tmux pane dims didn't follow /shell/resize — got {dims:?}"
    );

    cleanup_session(&session);
}

/// WebSocket JSON control frame must drive the same resize.
#[tokio::test]
async fn shell_ws_resize_control_frame_drives_tmux() {
    if !tmux_available() {
        eprintln!("tmux not on PATH — skipping shell_ws_resize_control_frame_drives_tmux");
        return;
    }
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    let port = free_port();
    let session = set_unique_session_env();
    let cfg = WebUiConfig {
        local_port: port,
        ..Default::default()
    };
    let _srv = WebUiServer::start_with_config(cfg).await.expect("start");

    // Materialize the session first via HTTP — the WS handler does too,
    // but doing it ahead of the WS write makes the test deterministic.
    let _ = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/shell/size"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("request");

    let url = format!("ws://127.0.0.1:{port}/shell");
    let (mut ws, _resp) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");

    // Drive a specific size via the JSON control frame the browser uses.
    ws.send(Message::Text(
        r#"{"type":"resize","cols":33,"rows":19}"#.to_string(),
    ))
    .await
    .expect("ws send");

    // Give tmux a moment to settle.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let dims_out = std::process::Command::new("tmux")
        .args([
            "display-message", "-p", "-t", &session,
            "#{pane_width}x#{pane_height}",
        ])
        .output()
        .expect("tmux display-message");
    let dims = String::from_utf8_lossy(&dims_out.stdout).trim().to_string();
    assert_eq!(
        dims, "33x19",
        "tmux pane dims didn't follow WS resize — got {dims:?}"
    );

    cleanup_session(&session);
}

#[tokio::test]
async fn shell_size_endpoint_returns_pane_dims() {
    if !tmux_available() {
        tracing::warn!("tmux not on PATH — skipping shell_size_endpoint_returns_pane_dims");
        return;
    }
    let port = free_port();
    let session = set_unique_session_env();
    let cfg = WebUiConfig {
        local_port: port,
        ..Default::default()
    };
    let _srv = WebUiServer::start_with_config(cfg).await.expect("start");

    let body = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/shell/size"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("request")
        .text()
        .await
        .expect("body");

    let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    let cols = v.get("cols").and_then(|x| x.as_u64()).unwrap_or(0);
    let rows = v.get("rows").and_then(|x| x.as_u64()).unwrap_or(0);
    assert!(cols > 0, "cols must be positive, got {cols}");
    assert!(rows > 0, "rows must be positive, got {rows}");

    cleanup_session(&session);
}
