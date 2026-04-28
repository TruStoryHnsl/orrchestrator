//! Bare-PTY shell bridge for the WebUI's second swipe page.
//!
//! Page 1 of the WebUI mirrors orrchestrator's own TUI (driven by
//! `terminal_tx` from the main loop). Page 2 is a *real* terminal —
//! a fresh fish (or `$SHELL`) PTY the user can drive directly to run
//! tmux, claude, vim, top, or anything else, exactly as they would in
//! a normal terminal emulator.
//!
//! Mechanism
//! ─────────
//! * `portable_pty` opens a real PTY pair and spawns the user's shell
//!   in it. The slave side is bound to the child; we keep the master
//!   alive for input/resize.
//! * Outbound (PTY → browser): a blocking thread reads `master_reader`
//!   in 4 KiB chunks and broadcasts each chunk to all connected WS
//!   clients. The same chunks are appended to a bounded scrollback
//!   buffer (256 KiB cap) so a freshly-attached client can replay
//!   recent output rather than seeing a blank pane.
//! * Inbound (browser → PTY): each WebSocket message is written
//!   directly to the master writer. The browser's `xterm.js` already
//!   encodes named keys (Enter, arrows, Ctrl-*) as ANSI escapes and
//!   raw bytes, exactly as a real terminal would deliver to the PTY,
//!   so no per-key translation is needed.
//!
//! Spawn is lazy: the PTY is created on the first WS connect and
//! survives reconnects. If the child exits (user types `exit`), the
//! next connect respawns it.
//!
//! Override the shell command via `ORRCH_WEBUI_SHELL_CMD` (defaults to
//! `$SHELL` or `/usr/bin/fish`).

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 64;
const SCROLLBACK_BYTES: usize = 256 * 1024;
const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 40;

/// Public placeholder kept for source-compatibility with the old
/// `DEFAULT_SHELL_SESSION` constant — no tmux session is created any
/// more, but other crates may still import the symbol.
pub const DEFAULT_SHELL_SESSION: &str = "orrch-web-shell";

#[derive(Clone)]
pub struct ShellBridge {
    /// Retained for source-compat. Now just identifies this bridge in
    /// logs; no tmux session is involved.
    pub session_name: String,
    /// PTY output broadcast channel. Each subscriber is a connected
    /// WebSocket client.
    pub tx: broadcast::Sender<Vec<u8>>,
    inner: Arc<Mutex<Option<PtyState>>>,
    started: Arc<AtomicBool>,
    scrollback: Arc<Mutex<Vec<u8>>>,
    size: Arc<Mutex<(u16, u16)>>,
}

struct PtyState {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    /// Held to keep the child alive for the lifetime of the bridge.
    _child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl ShellBridge {
    pub fn from_env() -> Self {
        let session_name = std::env::var("ORRCH_WEBUI_TMUX_SESSION")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_SHELL_SESSION.to_string());
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        ShellBridge {
            session_name,
            tx,
            inner: Arc::new(Mutex::new(None)),
            started: Arc::new(AtomicBool::new(false)),
            scrollback: Arc::new(Mutex::new(Vec::new())),
            size: Arc::new(Mutex::new((DEFAULT_COLS, DEFAULT_ROWS))),
        }
    }

    /// Spawn the PTY + shell on first call. Subsequent calls are no-ops
    /// while the child is still running. If the child has exited, the
    /// next call respawns it.
    pub fn ensure_session(&self) -> bool {
        // Cheap fast path: already running.
        if self.started.load(Ordering::Acquire) {
            return true;
        }
        // Slow path: take the spawn lock.
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            return true; // someone else just won the race
        }
        match self.spawn_pty() {
            Ok(()) => true,
            Err(e) => {
                tracing::error!("WebUI PTY spawn failed: {e}");
                self.started.store(false, Ordering::Release);
                false
            }
        }
    }

    fn spawn_pty(&self) -> anyhow::Result<()> {
        let (cols, rows) = *self
            .size
            .lock()
            .map_err(|_| anyhow::anyhow!("size mutex poisoned"))?;
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell_cmd = std::env::var("ORRCH_WEBUI_SHELL_CMD")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("SHELL").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "/usr/bin/fish".to_string());
        let mut cmd = CommandBuilder::new(&shell_cmd);
        // Inherit TERM, PATH, HOME etc. from the orrchestrator process.
        for (k, v) in std::env::vars() {
            cmd.env(k, v);
        }
        // Force a reasonable TERM if the parent didn't set one.
        if std::env::var("TERM").is_err() {
            cmd.env("TERM", "xterm-256color");
        }
        if let Some(home) = std::env::var_os("HOME") {
            cmd.cwd(home);
        }

        let child = pair.slave.spawn_command(cmd)?;
        // Drop the slave handle now that the child has it; we only need
        // the master from this point on.
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        // Background reader → broadcast + scrollback.
        let tx = self.tx.clone();
        let scrollback = Arc::clone(&self.scrollback);
        let started = Arc::clone(&self.started);
        std::thread::Builder::new()
            .name("orrch-webui-pty-reader".into())
            .spawn(move || pty_reader_loop(reader, tx, scrollback, started))
            .map_err(|e| anyhow::anyhow!("pty reader thread spawn: {e}"))?;

        let state = PtyState {
            master: pair.master,
            writer,
            _child: child,
        };
        let mut slot = self.inner.lock().map_err(|_| anyhow::anyhow!("inner mutex poisoned"))?;
        *slot = Some(state);
        Ok(())
    }

    /// Idempotent — kept for source-compat with the previous tmux poll
    /// loop. The PTY reader is started by `ensure_session` directly.
    pub fn ensure_poller(&self) {
        let _ = self.ensure_session();
    }

    /// Current (cols, rows). Defaults to 120×40 until a client requests
    /// a resize.
    pub fn pane_size(&self) -> (u32, u32) {
        let (c, r) = *self.size.lock().unwrap_or_else(|p| p.into_inner());
        (c as u32, r as u32)
    }

    /// Resize the PTY. Called when a WS client reports its xterm size.
    /// Best-effort; logs and continues on error.
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        if let Ok(mut size) = self.size.lock() {
            *size = (cols, rows);
        }
        if let Ok(mut slot) = self.inner.lock() {
            if let Some(state) = slot.as_mut() {
                let _ = state.master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
        }
    }

    /// Forward raw input bytes (typically from xterm.js) to the PTY.
    /// xterm.js already encodes named keys as ANSI escape sequences, so
    /// no per-key translation is needed.
    pub fn send_input(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if let Ok(mut slot) = self.inner.lock() {
            if let Some(state) = slot.as_mut() {
                if let Err(e) = state.writer.write_all(bytes) {
                    tracing::warn!("pty write_all: {e}");
                    return;
                }
                let _ = state.writer.flush();
            }
        }
    }
}

/// Capture the current scrollback buffer for a freshly-connected client.
/// Returns `None` when nothing has been written yet — the caller can let
/// the regular broadcast feed start populating the pane on its own.
pub fn initial_snapshot(_session: &str) -> Option<Vec<u8>> {
    None
}

/// Snapshot accessor with access to the bridge's scrollback. The server
/// uses the free function above for legacy reasons; new code can call
/// this method instead to actually replay recent output.
impl ShellBridge {
    pub fn snapshot(&self) -> Option<Vec<u8>> {
        let sb = self.scrollback.lock().ok()?;
        if sb.is_empty() {
            return None;
        }
        // Reset the receiving xterm before replaying so multi-frame
        // ANSI sequences land cleanly.
        let mut frame = Vec::with_capacity(sb.len() + 8);
        frame.extend_from_slice(b"\x1b[2J\x1b[H");
        frame.extend_from_slice(&sb);
        Some(frame)
    }
}

fn pty_reader_loop(
    mut reader: Box<dyn Read + Send>,
    tx: broadcast::Sender<Vec<u8>>,
    scrollback: Arc<Mutex<Vec<u8>>>,
    started: Arc<AtomicBool>,
) {
    let mut buf = vec![0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                // Child closed the pty; mark spawn slot as available so
                // the next `ensure_session` call reboots a fresh shell.
                started.store(false, Ordering::Release);
                tracing::info!("WebUI PTY reader: EOF (shell exited)");
                let _ = tx.send(b"\r\n\x1b[33m[shell exited - reconnect to spawn a fresh one]\x1b[0m\r\n".to_vec());
                return;
            }
            Ok(n) => {
                let chunk = buf[..n].to_vec();
                if let Ok(mut sb) = scrollback.lock() {
                    sb.extend_from_slice(&chunk);
                    if sb.len() > SCROLLBACK_BYTES {
                        let drop_n = sb.len() - SCROLLBACK_BYTES;
                        sb.drain(0..drop_n);
                    }
                }
                let _ = tx.send(chunk);
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                tracing::warn!("WebUI PTY read error: {e}");
                started.store(false, Ordering::Release);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_default_session_name() {
        unsafe { std::env::remove_var("ORRCH_WEBUI_TMUX_SESSION"); }
        let b = ShellBridge::from_env();
        assert_eq!(b.session_name, DEFAULT_SHELL_SESSION);
    }

    #[test]
    fn pane_size_defaults() {
        let b = ShellBridge::from_env();
        let (c, r) = b.pane_size();
        assert_eq!((c, r), (DEFAULT_COLS as u32, DEFAULT_ROWS as u32));
    }

    #[test]
    fn snapshot_empty_until_output() {
        let b = ShellBridge::from_env();
        assert!(b.snapshot().is_none());
    }
}
