//! Shared types for the relay scheduler.
use serde::{Deserialize, Serialize};

/// One OpenAI chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Incoming OpenAI-compatible completion request. Unknown OpenAI fields are
/// preserved in `extra` and forwarded verbatim to the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    /// Relay extension: caller-supplied affinity key (Approach B override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affinity_hint: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl CompletionRequest {
    /// Concatenated message text used as the embedding input.
    pub fn prompt_text(&self) -> String {
        self.messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// How a request will be grouped for cache locality.
#[derive(Debug, Clone)]
pub enum AffinityDescriptor {
    /// Embedding vector (Approach A).
    Vector(Vec<f32>),
    /// Exact-match tag (Approach B).
    Tag(String),
    /// No affinity signal — FIFO.
    None,
}

/// A token (or end-of-stream / error) flowing back to a waiting client.
#[derive(Debug, Clone)]
pub enum TokenEvent {
    Token(String),
    Done,
    Error(String),
}

use tokio::sync::mpsc;

/// A request plus the channel its tokens stream back through.
pub struct QueuedRequest {
    pub id: u64,
    pub request: CompletionRequest,
    pub tx: mpsc::Sender<TokenEvent>,
}
