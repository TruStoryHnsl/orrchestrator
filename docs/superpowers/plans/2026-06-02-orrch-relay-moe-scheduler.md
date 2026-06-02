# orrch-relay MoE-aware Inference Scheduler — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an engine-agnostic, affinity-batched LLM request scheduler inside the orrchestrator binary (new `orrch-relay` crate), exposed as an OpenAI-compatible HTTP proxy, with a deepseek-v4-flash `llama-server` runner as its first consumer.

**Architecture:** A single background worker pulls requests from a `Scheduler` queue in affinity-contiguous order (semantically-similar prompts grouped to keep the engine's expert/page cache hot), runs them one at a time against a black-box engine adapter, and streams tokens back to each waiting HTTP client. Affinity = local embedding similarity (core) or a caller-supplied hint (override). Ordering is always an optimization — if embedding fails, the queue degrades to FIFO; requests never fail because of it.

**Tech Stack:** Rust, tokio (async runtime), axum (HTTP), reqwest (engine + embedder HTTP clients), serde/serde_json, async-trait, futures. Reuses the orrchestrator cargo workspace.

**Spec:** `docs/superpowers/specs/2026-06-02-orrch-relay-moe-scheduler-design.md`

---

## File Structure

New crate `crates/orrch-relay/` with focused modules:

| File | Responsibility |
|---|---|
| `crates/orrch-relay/Cargo.toml` | crate manifest, workspace member |
| `src/lib.rs` | module wiring + public re-exports |
| `src/types.rs` | shared types: `ChatMessage`, `CompletionRequest`, `QueuedRequest`, `AffinityDescriptor`, `TokenEvent` |
| `src/clock.rs` | `Clock` trait + `SystemClock` + `FakeClock` (deterministic time for tests) |
| `src/affinity.rs` | `Embedder` trait, `OllamaEmbedder`, `MockEmbedder`, `cosine`, `classify()` |
| `src/scheduler.rs` | `Scheduler<C: Clock>`, `SchedulerPolicy`, `enqueue`/`next` ordering logic |
| `src/engine.rs` | `Engine` trait, `OpenAiEngine` (drives llama-server), `MockEngine` |
| `src/worker.rs` | the single serializing worker loop (scheduler → engine → client channels) |
| `src/gateway.rs` | axum OpenAI-compatible HTTP handlers + SSE streaming |
| `src/metrics.rs` | counters: queue depth, realized tok/s, wait histogram, affinity contiguity |
| `src/runner.rs` | deepseek-v4-flash `llama-server` config + process supervisor |
| `src/server.rs` | `RelayServer::spawn()` — assembles state, starts worker + axum listener |
| `tests/scheduler_ordering.rs` | integration test: ordering + starvation |
| `tests/degradation.rs` | integration test: embedder-down → FIFO |
| `src/main.rs` (modify) | wire `RelayServer::spawn()` behind `ORRCH_RELAY_ENABLE` |

---

## Task 1: Scaffold the crate + core types

**Files:**
- Create: `crates/orrch-relay/Cargo.toml`
- Create: `crates/orrch-relay/src/lib.rs`
- Create: `crates/orrch-relay/src/types.rs`
- Modify: `Cargo.toml` (workspace `members`)

- [ ] **Step 1: Add crate to workspace members**

In the root `Cargo.toml`, add `"crates/orrch-relay"` to the `[workspace] members` array (keep alphabetical with the other `crates/orrch-*` entries).

- [ ] **Step 2: Create the crate manifest**

`crates/orrch-relay/Cargo.toml`:
```toml
[package]
name = "orrch-relay"
version = { workspace = true }
edition = { workspace = true }
license = { workspace = true }

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = { workspace = true }
anyhow = { workspace = true }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"] }
axum = { version = "0.7", features = ["json"] }
reqwest = { workspace = true, features = ["json", "stream"] }
async-trait = "0.1"
futures = "0.3"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["rt", "macros", "sync", "time", "test-util"] }
```
(If `reqwest`/`workspace` features differ, match the existing `orrch-core` usage; `reqwest` is already a workspace dep used optionally by `orrch-hwfit`.)

- [ ] **Step 3: Write the types**

`crates/orrch-relay/src/types.rs`:
```rust
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
```

- [ ] **Step 4: Wire lib.rs**

`crates/orrch-relay/src/lib.rs`:
```rust
//! orrch-relay — MoE-aware, affinity-batched inference scheduler.
pub mod types;
pub use types::*;
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p orrch-relay`
Expected: compiles (warnings OK; unused-code warnings expected at this stage).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/orrch-relay/
git commit -m "feat(relay): scaffold orrch-relay crate + core types"
```

---

## Task 2: Clock abstraction

**Files:**
- Create: `crates/orrch-relay/src/clock.rs`
- Modify: `crates/orrch-relay/src/lib.rs`
- Test: inline `#[cfg(test)]` in `clock.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/orrch-relay/src/clock.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fake_clock_advances() {
        let c = FakeClock::new();
        assert_eq!(c.now_ms(), 0);
        c.advance(500);
        assert_eq!(c.now_ms(), 500);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p orrch-relay clock`
Expected: FAIL — `FakeClock` not found / module not declared.

- [ ] **Step 3: Implement the clock**

Prepend to `crates/orrch-relay/src/clock.rs`:
```rust
//! Time abstraction so the scheduler is deterministically testable.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    /// Milliseconds since an arbitrary fixed epoch (monotonic enough for waits).
    fn now_ms(&self) -> u64;
}

/// Wall-clock implementation for production.
#[derive(Debug, Default, Clone)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Manually-advanced clock for tests.
#[derive(Debug, Clone, Default)]
pub struct FakeClock(Arc<AtomicU64>);
impl FakeClock {
    pub fn new() -> Self {
        FakeClock(Arc::new(AtomicU64::new(0)))
    }
    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}
impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
```

- [ ] **Step 4: Declare module**

Add to `crates/orrch-relay/src/lib.rs`: `pub mod clock;`

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p orrch-relay clock`
Expected: PASS (1 test).

- [ ] **Step 6: Commit**

```bash
git add crates/orrch-relay/src/clock.rs crates/orrch-relay/src/lib.rs
git commit -m "feat(relay): deterministic Clock abstraction"
```

---

## Task 3: Affinity — embedder trait, cosine, classifier

**Files:**
- Create: `crates/orrch-relay/src/affinity.rs`
- Modify: `crates/orrch-relay/src/lib.rs`
- Test: inline in `affinity.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/orrch-relay/src/affinity.rs`:
```rust
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
        // A request with a hint must NOT call the embedder.
        let embedder = MockEmbedder::failing(); // panics if called
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
            messages: vec![crate::types::ChatMessage {
                role: "user".into(),
                content: text.into(),
            }],
            stream: false,
            affinity_hint: None,
            extra: serde_json::Map::new(),
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p orrch-relay affinity`
Expected: FAIL — `cosine`, `classify`, `MockEmbedder` not found.

- [ ] **Step 3: Implement affinity**

Prepend to `crates/orrch-relay/src/affinity.rs`:
```rust
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
        Self {
            base_url: base_url.into(),
            model: model.into(),
            client: reqwest::Client::new(),
        }
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
    pub fn constant(v: Vec<f32>) -> Self {
        Self { constant: Some(v) }
    }
    pub fn failing() -> Self {
        Self { constant: None }
    }
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
```

- [ ] **Step 4: Declare module**

Add to `crates/orrch-relay/src/lib.rs`: `pub mod affinity;`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p orrch-relay affinity`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/orrch-relay/src/affinity.rs crates/orrch-relay/src/lib.rs
git commit -m "feat(relay): affinity classifier (embed core + hint override)"
```

---

## Task 4: Scheduler — queue, clustering order, anti-starvation

**Files:**
- Create: `crates/orrch-relay/src/scheduler.rs`
- Modify: `crates/orrch-relay/src/lib.rs`
- Test: inline in `scheduler.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/orrch-relay/src/scheduler.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;
    use crate::types::AffinityDescriptor;

    fn policy() -> SchedulerPolicy {
        SchedulerPolicy { max_wait_ms: 1000, similarity_threshold: 0.8, max_queue_depth: 100 }
    }

    // Three "A-like" vectors and two "B-like" vectors interleaved on enqueue;
    // next() should emit them grouped by cluster, not in arrival order.
    #[test]
    fn groups_similar_requests_contiguously() {
        let clock = FakeClock::new();
        let mut s = Scheduler::new(policy(), clock);
        let a = || AffinityDescriptor::Vector(vec![1.0, 0.0]);
        let b = || AffinityDescriptor::Vector(vec![0.0, 1.0]);
        // arrival order: A B A B A  (ids 0..4)
        s.enqueue(0, a()).unwrap();
        s.enqueue(1, b()).unwrap();
        s.enqueue(2, a()).unwrap();
        s.enqueue(3, b()).unwrap();
        s.enqueue(4, a()).unwrap();
        let mut order = vec![];
        while let Some(id) = s.next() {
            order.push(id);
        }
        // First request out is id 0 (oldest, starts cluster A). Then the rest
        // of cluster A (2,4) before switching to cluster B (1,3).
        assert_eq!(order[0], 0);
        let a_ids: Vec<u64> = order.iter().take(3).copied().collect();
        assert_eq!(a_ids, vec![0, 2, 4], "all A-cluster before B-cluster");
        assert_eq!(&order[3..], &[1, 3], "B-cluster last");
    }

    // A lone odd request must not wait past max_wait even if a cluster is hot.
    #[test]
    fn aging_prevents_starvation() {
        let clock = FakeClock::new();
        let mut s = Scheduler::new(policy(), clock.clone());
        let a = || AffinityDescriptor::Vector(vec![1.0, 0.0]);
        let odd = AffinityDescriptor::Vector(vec![0.0, 1.0]);
        s.enqueue(0, a()).unwrap(); // becomes the hot cluster
        s.enqueue(99, odd).unwrap(); // the lone odd one, enqueued early
        // pull id 0 (starts cluster A)
        assert_eq!(s.next(), Some(0));
        // keep feeding cluster A
        s.enqueue(1, a()).unwrap();
        // before max_wait, A is preferred over the odd one
        assert_eq!(s.next(), Some(1));
        s.enqueue(2, a()).unwrap();
        // advance past max_wait → the aged odd request must jump the queue
        clock.advance(1001);
        assert_eq!(s.next(), Some(99), "aged request preempts hot cluster");
    }

    #[test]
    fn rejects_when_full() {
        let clock = FakeClock::new();
        let mut p = policy();
        p.max_queue_depth = 1;
        let mut s = Scheduler::new(p, clock);
        s.enqueue(0, AffinityDescriptor::None).unwrap();
        assert!(matches!(s.enqueue(1, AffinityDescriptor::None), Err(EnqueueError::Full)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p orrch-relay scheduler`
Expected: FAIL — `Scheduler`, `SchedulerPolicy`, `EnqueueError` not found.

- [ ] **Step 3: Implement the scheduler**

Prepend to `crates/orrch-relay/src/scheduler.rs`:
```rust
//! Affinity-ordering queue. Pure logic over (descriptors + policy + clock):
//! groups similar requests contiguously to keep the engine cache hot, while
//! aging guarantees no request starves past `max_wait_ms`.
use crate::affinity::cosine;
use crate::clock::Clock;
use crate::types::AffinityDescriptor;

#[derive(Debug, Clone)]
pub struct SchedulerPolicy {
    pub max_wait_ms: u64,
    pub similarity_threshold: f32,
    pub max_queue_depth: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EnqueueError {
    Full,
}

struct Entry {
    id: u64,
    descriptor: AffinityDescriptor,
    enqueued_ms: u64,
}

pub struct Scheduler<C: Clock> {
    policy: SchedulerPolicy,
    clock: C,
    queue: Vec<Entry>,
    last: Option<AffinityDescriptor>,
}

impl<C: Clock> Scheduler<C> {
    pub fn new(policy: SchedulerPolicy, clock: C) -> Self {
        Self { policy, clock, queue: Vec::new(), last: None }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn enqueue(&mut self, id: u64, descriptor: AffinityDescriptor) -> Result<(), EnqueueError> {
        if self.queue.len() >= self.policy.max_queue_depth {
            return Err(EnqueueError::Full);
        }
        self.queue.push(Entry { id, descriptor, enqueued_ms: self.clock.now_ms() });
        Ok(())
    }

    /// Pick the next request id to execute, or None if the queue is empty.
    /// Priority: (1) any request older than max_wait → oldest such (anti-starve);
    /// (2) else the request most affine to the last-executed cluster above the
    /// similarity threshold; (3) else the oldest request (start a new cluster).
    pub fn next(&mut self) -> Option<u64> {
        if self.queue.is_empty() {
            return None;
        }
        let now = self.clock.now_ms();

        // (1) anti-starvation: oldest request past max_wait.
        let aged = self
            .queue
            .iter()
            .enumerate()
            .filter(|(_, e)| now.saturating_sub(e.enqueued_ms) >= self.policy.max_wait_ms)
            .min_by_key(|(_, e)| e.enqueued_ms)
            .map(|(i, _)| i);

        let idx = if let Some(i) = aged {
            i
        } else if let Some(last) = &self.last {
            // (2) best affinity to the hot cluster, else (3) oldest.
            let best = self
                .queue
                .iter()
                .enumerate()
                .map(|(i, e)| (i, affinity(last, &e.descriptor)))
                .filter(|(_, sim)| *sim >= self.policy.similarity_threshold)
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i);
            best.unwrap_or_else(|| oldest_idx(&self.queue))
        } else {
            // (3) no hot cluster yet → oldest.
            oldest_idx(&self.queue)
        };

        let entry = self.queue.remove(idx);
        self.last = Some(entry.descriptor);
        Some(entry.id)
    }
}

fn oldest_idx(q: &[Entry]) -> usize {
    q.iter()
        .enumerate()
        .min_by_key(|(_, e)| e.enqueued_ms)
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Similarity between two descriptors: cosine for vectors, 1.0 for equal tags,
/// 0.0 otherwise.
fn affinity(a: &AffinityDescriptor, b: &AffinityDescriptor) -> f32 {
    match (a, b) {
        (AffinityDescriptor::Vector(x), AffinityDescriptor::Vector(y)) => cosine(x, y),
        (AffinityDescriptor::Tag(x), AffinityDescriptor::Tag(y)) if x == y => 1.0,
        _ => 0.0,
    }
}
```

- [ ] **Step 4: Declare module**

Add to `crates/orrch-relay/src/lib.rs`: `pub mod scheduler;`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p orrch-relay scheduler`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/orrch-relay/src/scheduler.rs crates/orrch-relay/src/lib.rs
git commit -m "feat(relay): affinity-ordering scheduler with anti-starvation"
```

---

## Task 5: Engine adapter trait + OpenAI engine + mock

**Files:**
- Create: `crates/orrch-relay/src/engine.rs`
- Modify: `crates/orrch-relay/src/lib.rs`
- Test: inline in `engine.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/orrch-relay/src/engine.rs`:
```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p orrch-relay engine`
Expected: FAIL — `Engine`, `MockEngine` not found.

- [ ] **Step 3: Implement the engine module**

Prepend to `crates/orrch-relay/src/engine.rs`:
```rust
//! Black-box engine adapters. The relay never modifies the engine; it only
//! forwards one request at a time and relays the token stream back.
use crate::types::{CompletionRequest, TokenEvent};
use async_trait::async_trait;
use futures::stream::BoxStream;
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
        use futures::StreamExt;
        let url = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));
        // Force streaming so we can relay incrementally.
        let mut body = serde_json::to_value(req)?;
        body["stream"] = serde_json::Value::Bool(true);
        let mut rb = self.client.post(url).json(&body);
        if let Some(k) = &self.api_key {
            rb = rb.bearer_auth(k);
        }
        let resp = rb.send().await?.error_for_status()?;
        let byte_stream = resp.bytes_stream();
        // Parse OpenAI SSE: lines beginning "data: {json}", terminated by
        // "data: [DONE]". Each chunk's choices[0].delta.content is a token.
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
```

- [ ] **Step 4: Declare module**

Add to `crates/orrch-relay/src/lib.rs`: `pub mod engine;`

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p orrch-relay engine`
Expected: PASS (1 test).

- [ ] **Step 6: Commit**

```bash
git add crates/orrch-relay/src/engine.rs crates/orrch-relay/src/lib.rs
git commit -m "feat(relay): Engine trait + OpenAI adapter + mock"
```

---

## Task 6: Worker loop — serialize scheduler → engine → client channels

**Files:**
- Create: `crates/orrch-relay/src/worker.rs`
- Modify: `crates/orrch-relay/src/types.rs` (add `QueuedRequest` + channel type), `src/lib.rs`
- Test: `crates/orrch-relay/tests/scheduler_ordering.rs`

- [ ] **Step 1: Add the channel-carrying queued type**

Append to `crates/orrch-relay/src/types.rs`:
```rust
use tokio::sync::mpsc;

/// A request plus the channel its tokens stream back through.
pub struct QueuedRequest {
    pub id: u64,
    pub request: CompletionRequest,
    pub tx: mpsc::Sender<TokenEvent>,
}
```

- [ ] **Step 2: Write the failing integration test**

`crates/orrch-relay/tests/scheduler_ordering.rs`:
```rust
//! End-to-end (in-process) ordering test: interleaved A/B requests must reach
//! the engine grouped by affinity cluster, proven by the order the MockEngine
//! records. No real network.
use orrch_relay::clock::SystemClock;
use orrch_relay::engine::MockEngine;
use orrch_relay::scheduler::{Scheduler, SchedulerPolicy};
use orrch_relay::types::{AffinityDescriptor, ChatMessage, CompletionRequest, QueuedRequest, TokenEvent};
use orrch_relay::worker::Worker;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

fn req(model: &str, text: &str) -> CompletionRequest {
    CompletionRequest {
        model: model.into(),
        messages: vec![ChatMessage { role: "user".into(), content: text.into() }],
        stream: false,
        affinity_hint: None,
        extra: serde_json::Map::new(),
    }
}

#[tokio::test]
async fn interleaved_requests_dispatch_in_cluster_order() {
    let policy = SchedulerPolicy { max_wait_ms: 60_000, similarity_threshold: 0.8, max_queue_depth: 100 };
    let sched = Arc::new(Mutex::new(Scheduler::new(policy, SystemClock)));
    let engine = Arc::new(MockEngine::new(vec!["ok".into()]));
    let worker = Worker::new(sched.clone(), engine.clone());
    let handle = worker.spawn();

    let a = || AffinityDescriptor::Vector(vec![1.0, 0.0]);
    let b = || AffinityDescriptor::Vector(vec![0.0, 1.0]);

    // Enqueue A B A B A as models "a0","b1","a2","b3","a4" and drain each.
    let specs = [("a0", a()), ("b1", b()), ("a2", a()), ("b3", b()), ("a4", a())];
    let mut drains = vec![];
    for (i, (model, desc)) in specs.into_iter().enumerate() {
        let (tx, mut rx) = mpsc::channel(8);
        worker.submit(QueuedRequest { id: i as u64, request: req(model, "x"), tx }, desc).await.unwrap();
        drains.push(tokio::spawn(async move { while rx.recv().await.is_some() {} }));
    }
    for d in drains { let _ = d.await; }
    handle.shutdown().await;

    // MockEngine recorded the model names in dispatch order.
    let order = engine.received_models();
    let a_first_three: Vec<&String> = order.iter().take(3).collect();
    assert!(a_first_three.iter().all(|m| m.starts_with('a')), "A-cluster dispatched first: {order:?}");
    assert!(order[3..].iter().all(|m| m.starts_with('b')), "B-cluster last: {order:?}");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p orrch-relay --test scheduler_ordering`
Expected: FAIL — `Worker` not found.

- [ ] **Step 4: Implement the worker**

`crates/orrch-relay/src/worker.rs`:
```rust
//! The single serializing worker. Owns the scheduler; one in-flight request at
//! a time. Submitting wakes it; it picks the next id, runs the engine, and
//! pumps tokens into that request's channel.
use crate::engine::Engine;
use crate::scheduler::{EnqueueError, Scheduler};
use crate::types::{AffinityDescriptor, QueuedRequest, TokenEvent};
use crate::clock::Clock;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify};

pub struct Worker<C: Clock + 'static> {
    sched: Arc<Mutex<Scheduler<C>>>,
    engine: Arc<dyn Engine>,
    // id → the channel + request body waiting to run.
    pending: Arc<Mutex<HashMap<u64, QueuedRequest>>>,
    notify: Arc<Notify>,
    shutdown: Arc<Notify>,
}

pub struct WorkerHandle {
    shutdown: Arc<Notify>,
    join: tokio::task::JoinHandle<()>,
}
impl WorkerHandle {
    pub async fn shutdown(self) {
        self.shutdown.notify_one();
        let _ = self.join.await;
    }
}

impl<C: Clock + 'static> Worker<C> {
    pub fn new(sched: Arc<Mutex<Scheduler<C>>>, engine: Arc<dyn Engine>) -> Self {
        Self {
            sched,
            engine,
            pending: Arc::new(Mutex::new(HashMap::new())),
            notify: Arc::new(Notify::new()),
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Enqueue a request with its descriptor. Errors if the queue is full.
    pub async fn submit(&self, qr: QueuedRequest, desc: AffinityDescriptor) -> Result<(), EnqueueError> {
        let id = qr.id;
        {
            let mut s = self.sched.lock().await;
            s.enqueue(id, desc)?;
        }
        self.pending.lock().await.insert(id, qr);
        self.notify.notify_one();
        Ok(())
    }

    pub fn spawn(self) -> WorkerHandle {
        let shutdown = self.shutdown.clone();
        let join = tokio::spawn(async move { self.run().await });
        WorkerHandle { shutdown, join }
    }

    async fn run(self) {
        loop {
            // Pull next id, or wait for a submit / shutdown.
            let next_id = {
                let mut s = self.sched.lock().await;
                s.next()
            };
            let Some(id) = next_id else {
                tokio::select! {
                    _ = self.notify.notified() => continue,
                    _ = self.shutdown.notified() => return,
                }
            };
            let Some(qr) = self.pending.lock().await.remove(&id) else { continue };
            // Run the engine and pump tokens to this request's channel.
            match self.engine.complete(&qr.request).await {
                Ok(mut stream) => {
                    while let Some(ev) = stream.next().await {
                        let done = matches!(ev, TokenEvent::Done | TokenEvent::Error(_));
                        if qr.tx.send(ev).await.is_err() {
                            break; // client disconnected
                        }
                        if done {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let _ = qr.tx.send(TokenEvent::Error(e.to_string())).await;
                }
            }
        }
    }
}
```

- [ ] **Step 5: Declare module**

Add to `crates/orrch-relay/src/lib.rs`: `pub mod worker;`

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p orrch-relay --test scheduler_ordering`
Expected: PASS.
*Note:* because `next()` drains synchronously as fast as submits arrive, this test asserts the dispatch ORDER recorded by `MockEngine`. If timing makes the first request dispatch before all five are enqueued, raise the submit rate by pre-enqueuing all five before spawning the worker (acceptable test refactor — keep the cluster-order assertion).

- [ ] **Step 7: Commit**

```bash
git add crates/orrch-relay/src/worker.rs crates/orrch-relay/src/types.rs crates/orrch-relay/src/lib.rs crates/orrch-relay/tests/scheduler_ordering.rs
git commit -m "feat(relay): serializing worker loop + in-process ordering test"
```

---

## Task 7: Gateway — OpenAI-compatible HTTP + SSE streaming

**Files:**
- Create: `crates/orrch-relay/src/gateway.rs`, `crates/orrch-relay/src/server.rs`
- Modify: `crates/orrch-relay/src/lib.rs`
- Test: `crates/orrch-relay/tests/degradation.rs`

- [ ] **Step 1: Implement server assembly**

`crates/orrch-relay/src/server.rs`:
```rust
//! Assembles relay state (embedder + scheduler + worker) and serves the
//! gateway over HTTP. `ORRCH_RELAY_*` env vars configure it.
use crate::affinity::{Embedder, OllamaEmbedder};
use crate::clock::SystemClock;
use crate::engine::{Engine, OpenAiEngine};
use crate::scheduler::{Scheduler, SchedulerPolicy};
use crate::worker::Worker;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RelayState {
    pub embedder: Arc<dyn Embedder>,
    pub worker: Arc<Worker<SystemClock>>,
    pub next_id: Arc<std::sync::atomic::AtomicU64>,
}

pub struct RelayConfig {
    pub bind: String,            // e.g. "127.0.0.1:8585"
    pub engine_url: String,      // llama-server base url
    pub engine_key: Option<String>,
    pub embedder_url: String,    // ollama base url
    pub embedder_model: String,
    pub policy: SchedulerPolicy,
}

impl RelayConfig {
    pub fn from_env() -> Self {
        let g = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        Self {
            bind: g("ORRCH_RELAY_BIND", "127.0.0.1:8585"),
            engine_url: g("ORRCH_RELAY_ENGINE_URL", "http://127.0.0.1:8080"),
            engine_key: std::env::var("ORRCH_RELAY_ENGINE_KEY").ok(),
            embedder_url: g("ORRCH_RELAY_EMBED_URL", "http://127.0.0.1:11434"),
            embedder_model: g("ORRCH_RELAY_EMBED_MODEL", "bge-small"),
            policy: SchedulerPolicy {
                max_wait_ms: g("ORRCH_RELAY_MAX_WAIT_MS", "750").parse().unwrap_or(750),
                similarity_threshold: g("ORRCH_RELAY_SIM_THRESHOLD", "0.75").parse().unwrap_or(0.75),
                max_queue_depth: g("ORRCH_RELAY_QUEUE_DEPTH", "256").parse().unwrap_or(256),
            },
        }
    }
}

/// Build state and spawn the worker. Returns state for the axum router.
pub fn build_state(cfg: &RelayConfig) -> RelayState {
    let embedder: Arc<dyn Embedder> =
        Arc::new(OllamaEmbedder::new(cfg.embedder_url.clone(), cfg.embedder_model.clone()));
    let engine: Arc<dyn Engine> =
        Arc::new(OpenAiEngine::new(cfg.engine_url.clone(), cfg.engine_key.clone()));
    let sched = Arc::new(Mutex::new(Scheduler::new(cfg.policy.clone(), SystemClock)));
    let worker = Arc::new(Worker::new(sched, engine));
    // The worker takes self by value to spawn; clone the Arc'd internals instead:
    // spawn a detached run via a dedicated constructor (see note in Step 4).
    RelayState {
        embedder,
        worker,
        next_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    }
}
```

> **Implementation note (resolve in this task):** `Worker::spawn(self)` consumes `self`, but `RelayState` needs a shared `Arc<Worker>` for handlers to call `submit`. Adjust `Worker` so `run` takes `self: Arc<Self>` (change `pub fn spawn(self)` → `pub fn spawn(self: Arc<Self>)` and `async fn run(self: Arc<Self>)`, cloning the `Arc` fields inside). Update Task 6's test accordingly (`Arc::new(worker).spawn()`). This is the one cross-task signature change; make it here and re-run `cargo test -p orrch-relay --test scheduler_ordering` to confirm green.

- [ ] **Step 2: Implement the gateway handlers**

`crates/orrch-relay/src/gateway.rs`:
```rust
//! OpenAI-compatible HTTP surface. Embeds → submits → streams SSE back.
use crate::affinity::classify;
use crate::server::RelayState;
use crate::types::{CompletionRequest, QueuedRequest, TokenEvent};
use axum::{
    extract::State,
    response::{sse::Event, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::atomic::Ordering;
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
) -> impl IntoResponse {
    let id = state.next_id.fetch_add(1, Ordering::SeqCst);
    // Affinity (degrades to None on embedder failure — never errors).
    let desc = classify(&req, state.embedder.as_ref()).await;
    let (tx, rx) = mpsc::channel::<TokenEvent>(64);
    let qr = QueuedRequest { id, request: req, tx };
    if state.worker.submit(qr, desc).await.is_err() {
        // Queue full → 429 as an SSE error event (clients reading the stream).
        let (etx, erx) = mpsc::channel::<TokenEvent>(1);
        let _ = etx.send(TokenEvent::Error("queue full (429)".into())).await;
        return sse_from(erx);
    }
    sse_from(rx)
}

fn sse_from(rx: mpsc::Receiver<TokenEvent>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = ReceiverStream::new(rx).map(|ev| {
        let data = match ev {
            TokenEvent::Token(t) => serde_json::json!({
                "choices": [{ "delta": { "content": t } }]
            })
            .to_string(),
            TokenEvent::Done => "[DONE]".to_string(),
            TokenEvent::Error(e) => serde_json::json!({ "error": e }).to_string(),
        };
        Ok(Event::default().data(data))
    });
    use futures::StreamExt;
    Sse::new(stream)
}
```
Add `tokio-stream = { version = "0.1", features = ["sync"] }` to `[dependencies]` in `crates/orrch-relay/Cargo.toml`.

- [ ] **Step 3: Wire modules + a spawn entrypoint**

Add to `crates/orrch-relay/src/lib.rs`:
```rust
pub mod metrics;
pub mod gateway;
pub mod server;

use std::sync::Arc;

/// Start the relay: build state, spawn worker, serve the gateway. Returns when
/// the listener exits. Call from the orrchestrator binary behind an env toggle.
pub async fn run_from_env() -> anyhow::Result<()> {
    let cfg = server::RelayConfig::from_env();
    let state = server::build_state(&cfg);
    Arc::clone(&state.worker).spawn_detached();
    let app = gateway::router(state);
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!("orrch-relay listening on http://{}", cfg.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
```
Add a `spawn_detached(self: Arc<Self>)` convenience to `Worker` that calls the `Arc`-based `run` without returning a handle (the binary owns lifetime). Keep the handle-returning `spawn` for tests.

- [ ] **Step 4: Write the degradation integration test**

`crates/orrch-relay/tests/degradation.rs`:
```rust
//! Embedder failure must NOT fail requests — the queue degrades to FIFO and
//! completions still return. Observable: tokens come back despite a dead embedder.
use orrch_relay::affinity::classify;
use orrch_relay::types::{ChatMessage, CompletionRequest};

// Reuse the crate's failing mock via a local re-impl (test-only embedder).
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
        messages: vec![ChatMessage { role: "user".into(), content: "hi".into() }],
        stream: false,
        affinity_hint: None,
        extra: serde_json::Map::new(),
    };
    let d = classify(&req, &DeadEmbedder).await;
    // None means the request still schedules (FIFO), proving graceful degradation.
    assert!(matches!(d, orrch_relay::types::AffinityDescriptor::None));
}
```

- [ ] **Step 5: Build + run all crate tests**

Run: `cargo test -p orrch-relay`
Expected: PASS (all prior tests + degradation). Fix any signature drift from the `Arc<Worker>` change.

- [ ] **Step 6: Commit**

```bash
git add crates/orrch-relay/
git commit -m "feat(relay): OpenAI-compatible gateway + SSE + server assembly"
```

---

## Task 8: Metrics

**Files:**
- Create: `crates/orrch-relay/src/metrics.rs`
- Test: inline

- [ ] **Step 1: Write the failing test**

`crates/orrch-relay/src/metrics.rs`:
```rust
//! Lightweight atomic counters for queue health + affinity-contiguity (the
//! cache-hit-quality proxy). Snapshot is exposed via /health later if wanted.
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct Metrics {
    pub submitted: AtomicU64,
    pub completed: AtomicU64,
    pub rejected_full: AtomicU64,
    pub cluster_continuations: AtomicU64, // consecutive same-cluster dispatches
    pub cluster_switches: AtomicU64,
}

impl Metrics {
    pub fn contiguity_ratio(&self) -> f64 {
        let c = self.cluster_continuations.load(Ordering::Relaxed) as f64;
        let s = self.cluster_switches.load(Ordering::Relaxed) as f64;
        if c + s == 0.0 { 0.0 } else { c / (c + s) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contiguity_ratio_computes() {
        let m = Metrics::default();
        m.cluster_continuations.fetch_add(3, Ordering::Relaxed);
        m.cluster_switches.fetch_add(1, Ordering::Relaxed);
        assert!((m.contiguity_ratio() - 0.75).abs() < 1e-9);
    }
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p orrch-relay metrics`
Expected: PASS (1 test). (`pub mod metrics;` was added in Task 7 Step 3.)

- [ ] **Step 3: Commit**

```bash
git add crates/orrch-relay/src/metrics.rs
git commit -m "feat(relay): queue + affinity-contiguity metrics"
```

---

## Task 9: Wire into the orrchestrator binary

**Files:**
- Modify: `src/main.rs` (or wherever subsystems spawn — match the WebUI server's pattern)
- Modify: root `Cargo.toml` (add `orrch-relay` to the binary crate's `[dependencies]`)
- Modify: `packaging/config/launch.env.example` (document new env vars)

- [ ] **Step 1: Add the dependency**

In the binary crate's `Cargo.toml` `[dependencies]`, add: `orrch-relay = { path = "crates/orrch-relay" }`.

- [ ] **Step 2: Spawn the relay behind the env toggle**

In `src/main.rs`, near where other always-on subsystems (the WebUI server) are spawned, add:
```rust
// MoE-aware inference scheduler. Off by default; enable with ORRCH_RELAY_ENABLE=1.
if std::env::var("ORRCH_RELAY_ENABLE").as_deref() == Ok("1") {
    tokio::spawn(async {
        if let Err(e) = orrch_relay::run_from_env().await {
            tracing::error!("orrch-relay exited: {e}");
        }
    });
}
```
If `main` is not already async/tokio, spawn it on the existing runtime the WebUI server uses (mirror that exact spawn site — do not create a second runtime).

- [ ] **Step 3: Surface the URL in the Esc menu**

Find where the WebUI URLs are collected for the Esc menu (search `ORRCH_WEBUI_PUBLIC_URL` / the Esc-menu URL list in `orrch-tui`). Add a line, when `ORRCH_RELAY_ENABLE=1`, showing `Relay (OpenAI API): http://<ORRCH_RELAY_BIND>/v1`. Match the existing formatting helper exactly.

- [ ] **Step 4: Document env vars**

Append to `packaging/config/launch.env.example`:
```bash
# ── orrch-relay (MoE-aware inference scheduler) ──
# ORRCH_RELAY_ENABLE=1                    # turn the relay on
# ORRCH_RELAY_BIND=127.0.0.1:8585         # OpenAI-compatible listen addr
# ORRCH_RELAY_ENGINE_URL=http://127.0.0.1:8080   # llama-server base url
# ORRCH_RELAY_EMBED_URL=http://127.0.0.1:11434   # ollama base url
# ORRCH_RELAY_EMBED_MODEL=bge-small       # embedding model for affinity
# ORRCH_RELAY_MAX_WAIT_MS=750             # anti-starvation latency cap
# ORRCH_RELAY_SIM_THRESHOLD=0.75          # cluster cosine threshold
# ORRCH_RELAY_QUEUE_DEPTH=256             # backpressure cap (429 above)
```

- [ ] **Step 5: Build the whole workspace**

Run: `cargo build`
Expected: workspace compiles. Run `cargo test -p orrch-relay` again — still green.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs Cargo.toml packaging/config/launch.env.example crates/orrch-tui/
git commit -m "feat(relay): wire scheduler into orrchestrator binary behind ORRCH_RELAY_ENABLE"
```

---

## Task 10: deepseek-v4-flash runner (config + llama-server supervisor)

**Files:**
- Create: `crates/orrch-relay/src/runner.rs`
- Create: `crates/orrch-relay/runner/deepseek-v4-flash.env.example`
- Modify: `crates/orrch-relay/src/lib.rs`
- Test: inline (config parsing only — process launch is verified manually in Task 11)

- [ ] **Step 1: Write the failing test**

`crates/orrch-relay/src/runner.rs`:
```rust
//! Builds the `llama-server` command line for deepseek-v4-flash on a
//! RAM-constrained + NVMe box, and supervises the process (restart on exit).
use std::process::Command;

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub llama_server_bin: String,
    pub gguf_path: String,
    pub gpu_layers: u32,
    pub n_cpu_moe: u32,   // expert layers kept on CPU/RAM (mmap-streamed)
    pub ctx_size: u32,
    pub port: u16,
}

impl RunnerConfig {
    /// Sane defaults for an 8GB-VRAM / 27GB-RAM / NVMe box per the spec.
    pub fn deepseek_default(gguf_path: impl Into<String>) -> Self {
        Self {
            llama_server_bin: "llama-server".into(),
            gguf_path: gguf_path.into(),
            gpu_layers: 999,     // offload all attention/dense it can; experts go to CPU
            n_cpu_moe: 999,      // keep all MoE expert tensors CPU/mmap side
            ctx_size: 8192,      // operating context (NOT the model's 1M max)
            port: 8080,
        }
    }

    /// The argv `llama-server` is launched with (order-stable for testing).
    pub fn argv(&self) -> Vec<String> {
        vec![
            "-m".into(), self.gguf_path.clone(),
            "--n-gpu-layers".into(), self.gpu_layers.to_string(),
            "--n-cpu-moe".into(), self.n_cpu_moe.to_string(),
            "--ctx-size".into(), self.ctx_size.to_string(),
            "--mmap".into(),
            "--host".into(), "127.0.0.1".into(),
            "--port".into(), self.port.to_string(),
        ]
    }

    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.llama_server_bin);
        c.args(self.argv());
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argv_uses_cpu_moe_mmap_and_operating_context() {
        let cfg = RunnerConfig::deepseek_default("/models/dsv4f-Q4_K_M.gguf");
        let argv = cfg.argv();
        assert!(argv.windows(2).any(|w| w == ["--n-cpu-moe", "999"]));
        assert!(argv.contains(&"--mmap".to_string()));
        assert!(argv.windows(2).any(|w| w == ["--ctx-size", "8192"]),
            "operating context, not the model's 1M max");
        assert!(argv.windows(2).any(|w| w[0] == "-m" && w[1].ends_with(".gguf")));
    }
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p orrch-relay runner`
Expected: FAIL first (module undeclared) → add `pub mod runner;` to `lib.rs` → PASS (1 test).

- [ ] **Step 3: Add a supervisor loop (no test — verified manually in Task 11)**

Append to `crates/orrch-relay/src/runner.rs`:
```rust
use std::time::Duration;

/// Launch llama-server and restart it if it exits. Runs until the process is
/// killed externally. Intended to be spawned on the tokio runtime.
pub async fn supervise(cfg: RunnerConfig) {
    loop {
        tracing::info!("starting llama-server: {:?}", cfg.argv());
        match cfg.command().spawn() {
            Ok(mut child) => {
                let status = tokio::task::spawn_blocking(move || child.wait())
                    .await
                    .ok()
                    .and_then(|r| r.ok());
                tracing::warn!("llama-server exited ({status:?}); restarting in 3s");
            }
            Err(e) => tracing::error!("failed to spawn llama-server: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
```

- [ ] **Step 4: Write the runner env template**

`crates/orrch-relay/runner/deepseek-v4-flash.env.example`:
```bash
# deepseek-v4-flash runner — source alongside orrchestrator's launch.env.
# Put the GGUF on the NVMe (sdc / WD SN550), NOT the virtio HDD volume.
ORRCH_RELAY_ENABLE=1
ORRCH_RELAY_ENGINE_URL=http://127.0.0.1:8080
DSV4F_GGUF=/mnt/nvme/models/DeepSeek-V4-Flash-Q4_K_M.gguf
DSV4F_CTX=8192
# orrch-hwfit rates this disk_stream ~0.4 tok/s on NVMe — patient/background use.
```

- [ ] **Step 5: Commit**

```bash
git add crates/orrch-relay/src/runner.rs crates/orrch-relay/runner/ crates/orrch-relay/src/lib.rs
git commit -m "feat(relay): deepseek-v4-flash llama-server runner config + supervisor"
```

---

## Task 11: End-to-end verification (manual, real model — the real proof)

**Files:**
- Create: `crates/orrch-relay/VERIFY.md` (the runbook + observed results)

This task is verification, not code. Per the project's testing rules, the feature is NOT done until someone OBSERVES deepseek-v4-flash generating through the scheduler. Do this in a session separate from the one that wrote the code.

- [ ] **Step 1: Acquire the GGUF onto the NVMe**

Download the unsloth DeepSeek-V4-Flash Q4_K_M GGUF to the NVMe-backed path (`sdc`). Confirm with `ls -lh` and that the path is on the SSD, not `/dev/sda`.

- [ ] **Step 2: Launch llama-server**

Run (adjust path):
```bash
llama-server -m /mnt/nvme/models/DeepSeek-V4-Flash-Q4_K_M.gguf \
  --n-gpu-layers 999 --n-cpu-moe 999 --ctx-size 8192 --mmap \
  --host 127.0.0.1 --port 8080
```
Expected: it loads (mmap; first load slow off disk) and prints "listening on 127.0.0.1:8080". Record load time + observed RAM/VRAM via `free -g` / `nvidia-smi`.

- [ ] **Step 3: Start an embedder**

`ollama pull bge-small` (or nomic-embed-text) and confirm `ollama` is serving on `:11434`.

- [ ] **Step 4: Launch orrchestrator with the relay on**

```bash
ORRCH_RELAY_ENABLE=1 ORRCH_RELAY_ENGINE_URL=http://127.0.0.1:8080 \
ORRCH_RELAY_EMBED_URL=http://127.0.0.1:11434 ORRCH_RELAY_EMBED_MODEL=bge-small \
orrchestrator
```
Confirm the Esc menu shows the relay URL.

- [ ] **Step 5: Send a real request through the proxy and OBSERVE tokens**

```bash
curl -N http://127.0.0.1:8585/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"relay","stream":true,"messages":[{"role":"user","content":"Say hello in five words."}]}'
```
Expected: a stream of SSE `data:` chunks containing real generated tokens, ending in `[DONE]`. **Record the actual output text and the wall-clock tok/s.** Status language: "Works — I watched deepseek-v4-flash generate '<actual text>' through the relay at <N> tok/s."

- [ ] **Step 6: Observe affinity ordering on a batch**

Submit ~10 mixed requests (5 code-ish, 5 prose-ish) concurrently; capture `Metrics::contiguity_ratio` (expose via `/health` if not already) and compare realized tok/s against the same 10 shuffled with the relay's similarity threshold set to 0 (affinity disabled). Record both numbers. Honest assertion: "ordering changed and throughput did not regress" — a speedup on this disk-bound box is a bonus, not a requirement.

- [ ] **Step 7: Write observed results + commit**

Fill `crates/orrch-relay/VERIFY.md` with the ACTUAL observed outputs (generated text, load time, tok/s, contiguity ratio, RAM/VRAM). Then:
```bash
git add crates/orrch-relay/VERIFY.md
git commit -m "test(relay): end-to-end verification with real deepseek-v4-flash"
```

---

## Self-Review

**Spec coverage:** §1 purpose → Tasks 1–11. §2 architecture (gateway/affinity/scheduler/engine/worker) → Tasks 3–7. §3.1 gateway → T7. §3.2 affinity A+B → T3. §3.3 scheduler+aging → T4. §3.4 engine adapters → T5. §3.5 metrics → T8. §3.6 runner → T10. §4 error handling: embedder-down→FIFO (T3/T7 degradation test), queue-full→429 (T4/T7), client disconnect (T6 worker `send` err → break), engine crash/restart (T10 supervisor), starvation (T4 aging test). §5 testing incl. real end-to-end → T11. §6 YAGNI honored (no concurrency/multi-node/learning/prefetch/persistence). §7 open questions resolved in plan: own port `:8585` (T7), bge-small default (T9), tuning defaults (T9).

**Placeholder scan:** No TBD/TODO in requirements. Two explicit, bounded "resolve in this task" notes (the `Arc<Worker>` signature change in T7; the test-rate note in T6) are concrete instructions with the exact change spelled out, not deferrals.

**Type consistency:** `CompletionRequest`/`ChatMessage`/`TokenEvent`/`QueuedRequest` (types.rs) used identically across T3/T5/T6/T7. `AffinityDescriptor` variants (`Vector`/`Tag`/`None`) consistent T3↔T4. `Scheduler::next() -> Option<u64>`, `enqueue() -> Result<(), EnqueueError>` consistent T4↔T6. `Engine::complete -> BoxStream<'static, TokenEvent>` consistent T5↔T6. `Worker::submit`/`spawn` reconciled to `Arc<Self>` in T7 with the test fix called out. `RunnerConfig::argv()` defined and tested in T10.

**Known cross-task edit:** the `Worker` `Arc<Self>` change (T7 Step 1 note) touches T6's code/test — flagged explicitly so the executor updates both.
