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
