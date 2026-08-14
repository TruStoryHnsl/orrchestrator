//! Embedder failure must NOT fail requests — the queue degrades to FIFO and
//! completions still schedule. Observable: classify yields None, not an error.
use orrch_relay::types::{ChatMessage, CompletionRequest};

struct DeadEmbedder;
#[async_trait::async_trait]
impl orrch_relay::affinity::Embedder for DeadEmbedder {
    async fn embed(&self, _t: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!("dead")
    }
}

#[tokio::test]
async fn dead_embedder_yields_none_affinity_not_error() {
    let req = CompletionRequest {
        model: "m".into(),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: "hi".into(),
        }],
        stream: false,
        affinity_hint: None,
        extra: serde_json::Map::new(),
    };
    let d = orrch_relay::affinity::classify(&req, &DeadEmbedder).await;
    assert!(matches!(d, orrch_relay::types::AffinityDescriptor::None));
}
