use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;
use tokio::sync::watch;

use crate::assets;
use crate::pty::handle_pty_ws;
use crate::state::{state_hash, WebAction, WebAppState};

#[derive(Clone)]
pub struct ServerState {
    pub state_rx: Arc<watch::Receiver<WebAppState>>,
    pub action_queue: Arc<Mutex<VecDeque<WebAction>>>,
}

pub fn build_router(srv: ServerState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/terminal", get(serve_terminal))
        .route("/ui", get(serve_ui))
        .route("/static/css/main.css", get(serve_main_css))
        .route("/static/js/ws.js", get(serve_ws_js))
        .route("/static/js/layout.js", get(serve_layout_js))
        .route("/static/js/intentions.js", get(serve_intentions_js))
        .route("/static/js/sessions.js", get(serve_sessions_js))
        .route("/static/js/mobile.js", get(serve_mobile_js))
        .route("/pty", get(pty_ws))
        .route("/state", get(state_ws))
        .route("/action", post(handle_action))
        .with_state(srv)
}

async fn serve_index() -> impl IntoResponse { html(assets::INDEX_HTML) }
async fn serve_terminal() -> impl IntoResponse { html(assets::TERMINAL_HTML) }
async fn serve_ui() -> impl IntoResponse { html(assets::UI_HTML) }
async fn serve_main_css() -> impl IntoResponse { css(assets::MAIN_CSS) }
async fn serve_ws_js() -> impl IntoResponse { js(assets::WS_JS) }
async fn serve_layout_js() -> impl IntoResponse { js(assets::LAYOUT_JS) }
async fn serve_intentions_js() -> impl IntoResponse { js(assets::INTENTIONS_JS) }
async fn serve_sessions_js() -> impl IntoResponse { js(assets::SESSIONS_JS) }
async fn serve_mobile_js() -> impl IntoResponse { js(assets::MOBILE_JS) }

async fn pty_ws(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_pty_ws)
}

async fn state_ws(ws: WebSocketUpgrade, State(srv): State<ServerState>) -> Response {
    ws.on_upgrade(move |socket| state_ws_handler(socket, srv))
}

async fn state_ws_handler(mut socket: WebSocket, srv: ServerState) {
    let mut rx = (*srv.state_rx).clone();
    let mut last_hash = [0u8; 32];
    loop {
        let snapshot = rx.borrow_and_update().clone();
        let hash = state_hash(&snapshot);
        if hash != last_hash {
            last_hash = hash;
            if let Ok(json) = serde_json::to_string(&snapshot) {
                if socket.send(Message::Text(json.into())).await.is_err() { return; }
            }
        }
        if rx.changed().await.is_err() { return; }
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
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], s).into_response()
}
fn css(s: &'static str) -> Response {
    ([(header::CONTENT_TYPE, "text/css")], s).into_response()
}
fn js(s: &'static str) -> Response {
    ([(header::CONTENT_TYPE, "application/javascript")], s).into_response()
}
