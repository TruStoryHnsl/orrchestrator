//! Affinity classification: embed a prompt (Approach A) or use a caller hint
//! (Approach B). Embedding failure degrades to no-affinity, never an error.
use crate::types::{AffinityDescriptor, CompletionRequest};
use async_trait::async_trait;

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
}

/// Cosine similarity. Returns 0.0 for zero-length or mismatched vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Path B (hint) wins; else Path A (embed); else None on embedder failure.
pub async fn classify<E: Embedder + ?Sized>(
    req: &CompletionRequest,
    embedder: &E,
) -> AffinityDescriptor {
    if let Some(hint) = &req.affinity_hint {
        return AffinityDescriptor::Tag(hint.clone());
    }
    match embedder.embed(&req.prompt_text()).await {
        Ok(v) if !v.is_empty() => AffinityDescriptor::Vector(v),
        _ => AffinityDescriptor::None,
    }
}

/// Embedder backed by an Ollama `/api/embeddings` endpoint.
pub struct OllamaEmbedder {
    pub base_url: String,
    pub model: String,
    client: reqwest::Client,
}
impl OllamaEmbedder {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), model: model.into(), client: reqwest::Client::new() }
    }
}
#[async_trait]
impl Embedder for OllamaEmbedder {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let url = format!("{}/api/embeddings", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({ "model": self.model, "prompt": text });
        let resp = self.client.post(url).json(&body).send().await?;
        let json: serde_json::Value = resp.error_for_status()?.json().await?;
        let arr = json
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| anyhow::anyhow!("no embedding field"))?;
        Ok(arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
    }
}

/// Test embedder: either returns a constant vector or always errors.
#[cfg(test)]
pub struct MockEmbedder {
    constant: Option<Vec<f32>>,
}
#[cfg(test)]
impl MockEmbedder {
    pub fn constant(v: Vec<f32>) -> Self { Self { constant: Some(v) } }
    pub fn failing() -> Self { Self { constant: None } }
}
#[cfg(test)]
#[async_trait]
impl Embedder for MockEmbedder {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        match &self.constant {
            Some(v) => Ok(v.clone()),
            None => anyhow::bail!("mock embedder failure"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical_is_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        assert!((cosine(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
    }

    #[tokio::test]
    async fn hint_overrides_embedding() {
        let embedder = MockEmbedder::failing(); // must NOT be called when a hint is present
        let mut req = test_req("anything");
        req.affinity_hint = Some("code-review".into());
        let d = classify(&req, &embedder).await;
        match d {
            AffinityDescriptor::Tag(t) => assert_eq!(t, "code-review"),
            _ => panic!("expected Tag"),
        }
    }

    #[tokio::test]
    async fn embedder_failure_degrades_to_none() {
        let embedder = MockEmbedder::failing();
        let d = classify(&test_req("hello"), &embedder).await;
        assert!(matches!(d, AffinityDescriptor::None));
    }

    #[tokio::test]
    async fn embedding_path_returns_vector() {
        let embedder = MockEmbedder::constant(vec![0.1, 0.2, 0.3]);
        let d = classify(&test_req("hello"), &embedder).await;
        assert!(matches!(d, AffinityDescriptor::Vector(_)));
    }

    fn test_req(text: &str) -> crate::types::CompletionRequest {
        crate::types::CompletionRequest {
            model: "m".into(),
            messages: vec![crate::types::ChatMessage { role: "user".into(), content: text.into() }],
            stream: false,
            affinity_hint: None,
            extra: serde_json::Map::new(),
        }
    }
}
