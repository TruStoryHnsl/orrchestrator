//! OpenAI-compatible HTTP surface. Embeds → submits → streams SSE back.
use crate::affinity::classify;
use crate::server::RelayState;
use crate::types::{CompletionRequest, QueuedRequest, TokenEvent};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures::StreamExt;
use futures::stream::Stream;
use std::convert::Infallible;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

pub fn router(state: RelayState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(state)
}

async fn models() -> impl IntoResponse {
    Json(serde_json::json!({
        "object": "list",
        "data": [{ "id": "relay", "object": "model", "owned_by": "orrch-relay" }]
    }))
}

async fn chat_completions(
    State(state): State<RelayState>,
    Json(req): Json<CompletionRequest>,
) -> Response {
    let id = state
        .next_id
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let desc = classify(&req, state.embedder.as_ref()).await;
    let (tx, rx) = mpsc::channel::<TokenEvent>(64);
    let qr = QueuedRequest {
        id,
        request: req,
        tx,
    };
    if state.worker.submit(qr, desc).await.is_err() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": { "message": "relay queue full", "type": "rate_limit_exceeded" } })),
        )
            .into_response();
    }
    sse_from(rx).into_response()
}

fn sse_from(rx: mpsc::Receiver<TokenEvent>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = ReceiverStream::new(rx).map(|ev| {
        let data = match ev {
            TokenEvent::Token(t) => {
                serde_json::json!({ "choices": [{ "delta": { "content": t } }] }).to_string()
            }
            TokenEvent::Done => "[DONE]".to_string(),
            TokenEvent::Error(e) => serde_json::json!({ "error": e }).to_string(),
        };
        Ok(Event::default().data(data))
    });
    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)),
    )
}
