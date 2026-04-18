pub mod assets;
pub mod pty;
pub mod server;
pub mod state;

pub use state::{WebAction, WebAppState, WebIdea, WebProject, WebSession};

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::sync::watch;

use server::{ServerState, build_router};

pub struct WebUiServer {
    pub port: u16,
    state_tx: watch::Sender<WebAppState>,
    action_queue: Arc<Mutex<VecDeque<WebAction>>>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl WebUiServer {
    pub async fn start(port: u16) -> Result<Self> {
        let (state_tx, state_rx) = watch::channel(WebAppState::default());
        let action_queue: Arc<Mutex<VecDeque<WebAction>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let srv = ServerState {
            state_rx: Arc::new(state_rx),
            action_queue: Arc::clone(&action_queue),
        };
        let router = build_router(srv);
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = tokio::net::TcpListener::bind(addr).await
            .context("WebUI bind failed")?;
        let actual_port = listener.local_addr()?.port();

        tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async { let _ = shutdown_rx.await; })
                .await
                .ok();
        });

        tracing::info!("WebUI on :{actual_port}");
        Ok(WebUiServer { port: actual_port, state_tx, action_queue, _shutdown: shutdown_tx })
    }

    pub fn update_state(&self, state: WebAppState) {
        let _ = self.state_tx.send(state);
    }

    pub fn drain_actions(&self) -> Vec<WebAction> {
        self.action_queue.lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    }
}
