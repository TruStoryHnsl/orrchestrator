//! Assembles relay state (embedder + scheduler + worker) and serves the
//! gateway over HTTP. `ORRCH_RELAY_*` env vars configure it.
use crate::affinity::{Embedder, OllamaEmbedder};
use crate::clock::SystemClock;
use crate::engine::{Engine, OpenAiEngine};
use crate::scheduler::{Scheduler, SchedulerPolicy};
use crate::worker::Worker;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RelayState {
    pub embedder: Arc<dyn Embedder>,
    pub worker: Arc<Worker<SystemClock>>,
    pub next_id: Arc<AtomicU64>,
}

pub struct RelayConfig {
    pub bind: String,
    pub engine_url: String,
    pub engine_key: Option<String>,
    pub embedder_url: String,
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
                similarity_threshold: g("ORRCH_RELAY_SIM_THRESHOLD", "0.75")
                    .parse()
                    .unwrap_or(0.75),
                max_queue_depth: g("ORRCH_RELAY_QUEUE_DEPTH", "256").parse().unwrap_or(256),
            },
        }
    }
}

/// Build state and the worker (not yet spawned).
pub fn build_state(cfg: &RelayConfig) -> RelayState {
    let embedder: Arc<dyn Embedder> =
        Arc::new(OllamaEmbedder::new(cfg.embedder_url.clone(), cfg.embedder_model.clone()));
    let engine: Arc<dyn Engine> =
        Arc::new(OpenAiEngine::new(cfg.engine_url.clone(), cfg.engine_key.clone()));
    let sched = Arc::new(Mutex::new(Scheduler::new(cfg.policy.clone(), SystemClock)));
    let worker = Arc::new(Worker::new(sched, engine));
    RelayState { embedder, worker, next_id: Arc::new(AtomicU64::new(0)) }
}
