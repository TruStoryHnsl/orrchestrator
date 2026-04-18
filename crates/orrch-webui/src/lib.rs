pub mod assets;
pub mod server;
pub mod state;
pub mod tee;

pub use state::{WebAction, WebAppState, WebIdea, WebProject, WebSession};
pub use tee::TeeWriter;

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::sync::{broadcast, watch};

use server::{ServerState, build_router};

/// Fixed port for the WebUI. Stable so the user can bookmark http://localhost:8484.
pub const DEFAULT_PORT: u16 = 8484;

/// Size of each terminal-broadcast buffer (bytes). Must be big enough to
/// hold a full frame of ANSI escape sequences + characters. 64 KiB is ample.
const TERMINAL_BUFFER_SIZE: usize = 64 * 1024;

/// Size of the broadcast ring buffer (number of chunks retained for slow
/// consumers). If a client is too slow, it drops packets — the TUI can
/// always re-send the full frame on the next tick.
const TERMINAL_CHANNEL_CAPACITY: usize = 128;

/// Keystroke channel capacity for WebUI → TUI input.
const INPUT_CHANNEL_CAPACITY: usize = 256;

pub struct WebUiServer {
    pub port: u16,
    state_tx: watch::Sender<WebAppState>,
    action_queue: Arc<Mutex<VecDeque<WebAction>>>,
    pub terminal_tx: broadcast::Sender<Vec<u8>>,
    input_rx: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>>>,
    /// Set to `true` when a new terminal client connects. The main loop
    /// polls this flag; when set, it does a full redraw so the client
    /// sees the current screen (not just the diff since last frame).
    redraw_flag: Arc<std::sync::atomic::AtomicBool>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl WebUiServer {
    pub async fn start(port: u16) -> Result<Self> {
        let (state_tx, state_rx) = watch::channel(WebAppState::default());
        let action_queue: Arc<Mutex<VecDeque<WebAction>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let (terminal_tx, _) = broadcast::channel::<Vec<u8>>(TERMINAL_CHANNEL_CAPACITY);
        let (input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        let redraw_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let srv = ServerState {
            state_rx: Arc::new(state_rx),
            action_queue: Arc::clone(&action_queue),
            terminal_tx: terminal_tx.clone(),
            input_tx: Arc::new(input_tx),
            redraw_flag: Arc::clone(&redraw_flag),
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
        Ok(WebUiServer {
            port: actual_port,
            state_tx,
            action_queue,
            terminal_tx,
            input_rx: Arc::new(Mutex::new(input_rx)),
            redraw_flag,
            _shutdown: shutdown_tx,
        })
    }

    /// Consume and return the "new client connected" flag, if set.
    /// The main loop calls this every tick; if true it forces a full redraw.
    pub fn take_redraw_request(&self) -> bool {
        self.redraw_flag.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    pub fn update_state(&self, state: WebAppState) {
        let _ = self.state_tx.send(state);
    }

    pub fn drain_actions(&self) -> Vec<WebAction> {
        self.action_queue.lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    }

    /// Drain all pending keystrokes from WebUI clients (non-blocking).
    /// Returns a Vec of byte sequences — each sequence is one keypress or paste.
    pub fn drain_input(&self) -> Vec<Vec<u8>> {
        let Ok(mut rx) = self.input_rx.lock() else { return Vec::new(); };
        let mut out = Vec::new();
        while let Ok(bytes) = rx.try_recv() {
            out.push(bytes);
        }
        out
    }

    /// Create a writer that tees into both the provided local writer (e.g.
    /// stdout) and the terminal broadcast channel. Pass this to
    /// `CrosstermBackend::new` so every ANSI byte orrchestrator emits is
    /// mirrored to connected browser clients.
    pub fn tee_writer<W: std::io::Write + Send>(&self, local: W) -> TeeWriter<W> {
        TeeWriter::new(local, self.terminal_tx.clone(), TERMINAL_BUFFER_SIZE)
    }
}

// Expose the input channel capacity constant for consumers that need it.
pub const INPUT_CAPACITY: usize = INPUT_CHANNEL_CAPACITY;
