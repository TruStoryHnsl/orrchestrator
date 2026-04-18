use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode};
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
}

pub fn build_router(srv: ServerState) -> Router {
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
        .route("/action", post(handle_action))
        .with_state(srv)
}

// `/` serves the native UI directly — that's the primary experience.
// `/terminal` still serves the xterm mirror for anyone who wants it.
// The old two-button landing page (assets::INDEX_HTML) is still available
// at `/landing` for reference/debug.
async fn serve_index() -> impl IntoResponse { html(assets::UI_HTML) }
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
