//! Black-box engine adapters. The relay never modifies the engine; it only
//! forwards one request at a time and relays the token stream back.
use crate::types::{CompletionRequest, TokenEvent};
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use std::sync::{Arc, Mutex};

#[async_trait]
pub trait Engine: Send + Sync {
    /// Run one completion, yielding token events ending in `Done` (or `Error`).
    async fn complete(&self, req: &CompletionRequest)
        -> anyhow::Result<BoxStream<'static, TokenEvent>>;
}

/// Drives any OpenAI-compatible `/v1/chat/completions` server (llama-server,
/// ktransformers, vLLM, …). `LlamaCppAdapter` is just this pointed at llama-server.
pub struct OpenAiEngine {
    pub base_url: String,
    pub api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiEngine {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self { base_url: base_url.into(), api_key, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl Engine for OpenAiEngine {
    async fn complete(&self, req: &CompletionRequest)
        -> anyhow::Result<BoxStream<'static, TokenEvent>> {
        let url = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));
        let mut body = serde_json::to_value(req)?;
        body["stream"] = serde_json::Value::Bool(true);
        let mut rb = self.client.post(url).json(&body);
        if let Some(k) = &self.api_key {
            rb = rb.bearer_auth(k);
        }
        let resp = rb.send().await?.error_for_status()?;
        let byte_stream = resp.bytes_stream();
        let mapped = byte_stream.flat_map(|chunk| {
            let events = match chunk {
                Ok(bytes) => parse_sse_chunk(&bytes),
                Err(e) => vec![TokenEvent::Error(e.to_string())],
            };
            futures::stream::iter(events)
        });
        Ok(mapped.boxed())
    }
}

/// Parse one (possibly multi-line) SSE byte chunk into token events.
fn parse_sse_chunk(bytes: &[u8]) -> Vec<TokenEvent> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(payload) = line.strip_prefix("data:") else { continue };
        let payload = payload.trim();
        if payload == "[DONE]" {
            out.push(TokenEvent::Done);
            continue;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
            if let Some(tok) = json["choices"][0]["delta"]["content"].as_str() {
                if !tok.is_empty() {
                    out.push(TokenEvent::Token(tok.to_string()));
                }
            }
        }
    }
    out
}

/// Test engine: streams canned tokens, records the models it was asked to run
/// (so tests can assert the ORDER the scheduler dispatched).
pub struct MockEngine {
    tokens: Vec<String>,
    received: Arc<Mutex<Vec<String>>>,
}
impl MockEngine {
    pub fn new(tokens: Vec<String>) -> Self {
        Self { tokens, received: Arc::new(Mutex::new(Vec::new())) }
    }
    pub fn received_models(&self) -> Vec<String> {
        self.received.lock().unwrap().clone()
    }
}
#[async_trait]
impl Engine for MockEngine {
    async fn complete(&self, req: &CompletionRequest)
        -> anyhow::Result<BoxStream<'static, TokenEvent>> {
        self.received.lock().unwrap().push(req.model.clone());
        let mut events: Vec<TokenEvent> =
            self.tokens.iter().cloned().map(TokenEvent::Token).collect();
        events.push(TokenEvent::Done);
        Ok(futures::stream::iter(events).boxed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatMessage, CompletionRequest};
    use futures::StreamExt;

    fn req() -> CompletionRequest {
        CompletionRequest {
            model: "m".into(),
            messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
            stream: true,
            affinity_hint: None,
            extra: serde_json::Map::new(),
        }
    }

    #[tokio::test]
    async fn mock_engine_streams_canned_tokens_and_records_order() {
        let eng = MockEngine::new(vec!["hel".into(), "lo".into()]);
        let mut stream = eng.complete(&req()).await.unwrap();
        let mut out = String::new();
        while let Some(ev) = stream.next().await {
            match ev {
                TokenEvent::Token(t) => out.push_str(&t),
                TokenEvent::Done => break,
                TokenEvent::Error(e) => panic!("{e}"),
            }
        }
        assert_eq!(out, "hello");
        assert_eq!(eng.received_models(), vec!["m".to_string()]);
    }
}
