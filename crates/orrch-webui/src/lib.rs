pub mod assets;
pub mod server;
pub mod state;
pub mod tee;

pub use state::{WebAction, WebAppState, WebIdea, WebProject, WebSession};
pub use tee::TeeWriter;

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::sync::{broadcast, watch};

use server::{ServerState, build_router};

/// Fixed port for the local HTTP listener. Stable so the user can bookmark
/// http://localhost:8484 across restarts.
pub const DEFAULT_PORT: u16 = 8484;

/// Default port for the public TLS listener. 8443 (not 443) so the binary
/// can bind without root. Bind 443 directly via:
///   sudo setcap cap_net_bind_service=+ep ./target/release/orrchestrator
pub const DEFAULT_TLS_PORT: u16 = 8443;

/// Size of each terminal-broadcast buffer (bytes). Must be big enough to
/// hold a full frame of ANSI escape sequences + characters. 64 KiB is ample.
const TERMINAL_BUFFER_SIZE: usize = 64 * 1024;

/// Size of the broadcast ring buffer (number of chunks retained for slow
/// consumers). If a client is too slow, it drops packets — the TUI can
/// always re-send the full frame on the next tick.
const TERMINAL_CHANNEL_CAPACITY: usize = 128;

/// Keystroke channel capacity for WebUI → TUI input.
const INPUT_CHANNEL_CAPACITY: usize = 256;

/// User-facing configuration for the WebUI server. Built from env vars by
/// `WebUiConfig::from_env`; passed explicitly into `WebUiServer::start`.
///
/// All fields are optional except `local_port` — the local HTTP listener is
/// always on. TLS, auth, and the public URL are layered on top when the
/// matching env vars are set.
#[derive(Debug, Clone)]
pub struct WebUiConfig {
    /// Port for the always-on local HTTP listener. Defaults to 8484.
    pub local_port: u16,
    /// Optional TLS configuration. When `Some`, a second listener binds
    /// to `tls_addr` and serves the same router behind rustls.
    pub tls: Option<TlsConfig>,
    /// Optional bearer token. When `Some`, all non-localhost requests must
    /// present `Authorization: Bearer <token>`, `Cookie: orrch_token=<token>`,
    /// or `?token=<token>`. Localhost connections always bypass auth.
    pub auth_token: Option<String>,
    /// Optional public-facing URL displayed to users (e.g. in the Esc menu).
    /// Pure cosmetics — does not affect binding or routing.
    pub public_url: Option<String>,
}

impl Default for WebUiConfig {
    fn default() -> Self {
        Self {
            local_port: DEFAULT_PORT,
            tls: None,
            auth_token: None,
            public_url: None,
        }
    }
}

impl WebUiConfig {
    /// Build a config from environment variables.
    ///
    /// | Variable                | Effect                                  |
    /// |-------------------------|-----------------------------------------|
    /// | `ORRCH_WEBUI_PORT`      | local HTTP port (default 8484)          |
    /// | `ORRCH_WEBUI_TLS_CERT`  | path to fullchain.pem (enables TLS)     |
    /// | `ORRCH_WEBUI_TLS_KEY`   | path to privkey.pem (enables TLS)       |
    /// | `ORRCH_WEBUI_TLS_PORT`  | TLS port (default 8443)                 |
    /// | `ORRCH_WEBUI_TLS_BIND`  | TLS bind address (default 0.0.0.0)      |
    /// | `ORRCH_WEBUI_TOKEN`     | bearer token required for non-localhost |
    /// | `ORRCH_WEBUI_PUBLIC_URL`| display string (e.g. https://x.com)     |
    ///
    /// TLS only activates when BOTH `ORRCH_WEBUI_TLS_CERT` and
    /// `ORRCH_WEBUI_TLS_KEY` are set — partial config is treated as off.
    pub fn from_env() -> Self {
        let local_port = std::env::var("ORRCH_WEBUI_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PORT);

        let tls = match (
            std::env::var("ORRCH_WEBUI_TLS_CERT").ok(),
            std::env::var("ORRCH_WEBUI_TLS_KEY").ok(),
        ) {
            (Some(cert), Some(key)) if !cert.is_empty() && !key.is_empty() => {
                let port = std::env::var("ORRCH_WEBUI_TLS_PORT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEFAULT_TLS_PORT);
                let bind = std::env::var("ORRCH_WEBUI_TLS_BIND")
                    .unwrap_or_else(|_| "0.0.0.0".to_string());
                Some(TlsConfig {
                    cert_path: PathBuf::from(cert),
                    key_path: PathBuf::from(key),
                    bind,
                    port,
                })
            }
            _ => None,
        };

        let auth_token = std::env::var("ORRCH_WEBUI_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());

        let public_url = std::env::var("ORRCH_WEBUI_PUBLIC_URL")
            .ok()
            .filter(|s| !s.is_empty());

        Self { local_port, tls, auth_token, public_url }
    }
}

/// TLS-specific configuration. Both files must exist and be readable.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Path to a PEM-encoded certificate chain (e.g. Let's Encrypt's `fullchain.pem`).
    pub cert_path: PathBuf,
    /// Path to the matching PEM-encoded private key (`privkey.pem`).
    pub key_path: PathBuf,
    /// Address to bind the TLS listener (default `0.0.0.0`).
    pub bind: String,
    /// Port for the TLS listener (default 8443).
    pub port: u16,
}

pub struct WebUiServer {
    /// Local HTTP port. Always bound to 127.0.0.1.
    pub port: u16,
    /// Public TLS URL when TLS is enabled (e.g. `https://orrchestrator.com`
    /// when `ORRCH_WEBUI_PUBLIC_URL` is set; otherwise the bound TLS addr).
    pub public_url: Option<String>,
    /// Bearer token required by non-localhost requests, when configured.
    pub auth_token: Option<String>,
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
    /// Compatibility wrapper — equivalent to `start_with_config` using the
    /// default `WebUiConfig` and the supplied port.
    pub async fn start(port: u16) -> Result<Self> {
        let mut cfg = WebUiConfig::from_env();
        cfg.local_port = port;
        Self::start_with_config(cfg).await
    }

    /// Start the WebUI server using the provided configuration.
    ///
    /// Always binds an HTTP listener on `127.0.0.1:cfg.local_port`. When
    /// `cfg.tls` is set, additionally binds a TLS listener on the configured
    /// address. Both listeners share the same router and state.
    pub async fn start_with_config(cfg: WebUiConfig) -> Result<Self> {
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
            auth_token: cfg.auth_token.clone(),
        };
        let router = build_router(srv);

        // Local HTTP listener — always on 127.0.0.1 so authentication is
        // unnecessary for the operator's own machine.
        let local_addr = SocketAddr::from(([127, 0, 0, 1], cfg.local_port));
        let listener = tokio::net::TcpListener::bind(local_addr).await
            .with_context(|| format!("WebUI HTTP bind failed on {local_addr}"))?;
        let actual_port = listener.local_addr()?.port();

        let local_router = router.clone();
        tokio::spawn(async move {
            axum::serve(
                listener,
                local_router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async { let _ = shutdown_rx.await; })
            .await
            .ok();
        });
        tracing::info!("WebUI HTTP listening on http://127.0.0.1:{actual_port}");

        // Optional TLS listener.
        let public_url = if let Some(tls) = cfg.tls.clone() {
            // rustls 0.23 requires a process-level crypto provider before
            // any TLS work. Idempotent: only the first call wins; subsequent
            // calls return Err, which is fine.
            let _ = rustls::crypto::ring::default_provider().install_default();

            let bind_str = format!("{}:{}", tls.bind, tls.port);
            let bind_addr: SocketAddr = bind_str.parse()
                .with_context(|| format!("invalid TLS bind addr: {bind_str}"))?;
            let rustls_cfg = axum_server::tls_rustls::RustlsConfig::from_pem_file(
                &tls.cert_path,
                &tls.key_path,
            )
            .await
            .with_context(|| format!(
                "loading TLS cert/key from {} and {}",
                tls.cert_path.display(), tls.key_path.display()
            ))?;

            let tls_router = router.clone();
            tokio::spawn(async move {
                if let Err(e) = axum_server::bind_rustls(bind_addr, rustls_cfg)
                    .serve(tls_router.into_make_service_with_connect_info::<SocketAddr>())
                    .await
                {
                    tracing::error!("WebUI TLS listener exited: {e}");
                }
            });
            tracing::info!("WebUI TLS listening on https://{bind_addr}");

            Some(cfg.public_url.clone().unwrap_or_else(|| format!("https://{bind_addr}")))
        } else {
            // No TLS configured: public_url, if set, still gets surfaced
            // (e.g. when the user has put a separate reverse proxy in front
            // and just wants the friendly hostname displayed).
            cfg.public_url.clone()
        };

        Ok(WebUiServer {
            port: actual_port,
            public_url,
            auth_token: cfg.auth_token,
            state_tx,
            action_queue,
            terminal_tx,
            input_rx: Arc::new(Mutex::new(input_rx)),
            redraw_flag,
            _shutdown: shutdown_tx,
        })
    }

    /// Local HTTP URL (always available).
    pub fn local_url(&self) -> String {
        format!("http://localhost:{}", self.port)
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
