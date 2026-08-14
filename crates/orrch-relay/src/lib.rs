//! orrch-relay — MoE-aware, affinity-batched inference scheduler.
pub mod affinity;
pub mod clock;
pub mod engine;
pub mod gateway;
pub mod metrics;
pub mod runner;
pub mod scheduler;
pub mod server;
pub mod types;
pub mod worker;
pub use types::*;

/// Start the relay: build state, spawn worker, serve the gateway. Returns when
/// the listener exits. Call from the orrchestrator binary behind an env toggle.
pub async fn run_from_env() -> anyhow::Result<()> {
    let cfg = server::RelayConfig::from_env();
    let state = server::build_state(&cfg);
    let _worker_handle = state.worker.clone().spawn(); // runs for the process lifetime
    let app = gateway::router(state);
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!("orrch-relay listening on http://{}", cfg.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
