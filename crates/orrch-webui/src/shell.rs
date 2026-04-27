//! Independent tmux-session bridge for the WebUI's second swipe page.
//!
//! Page 1 of the WebUI mirrors orrchestrator's own TUI (driven by
//! `terminal_tx` from the main loop). Page 2 is fully independent: it
//! attaches to a *separate* tmux session — by default `orrch-web-shell`,
//! overrideable via `ORRCH_WEBUI_TMUX_SESSION` — and lets the user type
//! into it from the browser as if SSHed in.
//!
//! Mechanism
//! ─────────
//! * Outbound (tmux → browser): a single background poller calls
//!   `tmux capture-pane -p -e -J` every ~120 ms, hashes the output, and
//!   broadcasts the bytes when they change. Polling is dead simple,
//!   handles attach/detach/resize, and avoids the FIFO/pipe-pane cleanup
//!   tarpit. 120 ms is well below the perceptual threshold for typing.
//! * Inbound (browser → tmux): each WebSocket message is forwarded to
//!   `tmux send-keys`. Printable bytes go through `send-keys -l`
//!   (literal — tmux otherwise interprets `~`, `[`, etc. as escape
//!   codes). Named keys (Enter, arrows, Ctrl-*) translate to their tmux
//!   key spec.
//!
//! The poller is started lazily on the first WebSocket connection —
//! sessions that never open the swipe page never spawn tmux work.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

/// Capture buffer size for the broadcast channel. Each slow client may
/// drop frames — the next poll resends the full pane so they recover.
const SHELL_CHANNEL_CAPACITY: usize = 64;

/// Polling interval for `tmux capture-pane`. 120 ms keeps typing latency
/// imperceptible without burning CPU on idle sessions.
const POLL_INTERVAL: Duration = Duration::from_millis(120);

/// Default tmux session name when `ORRCH_WEBUI_TMUX_SESSION` isn't set.
pub const DEFAULT_SHELL_SESSION: &str = "orrch-web-shell";

#[derive(Clone)]
pub struct ShellBridge {
    pub session_name: String,
    pub tx: broadcast::Sender<Vec<u8>>,
    poller_started: Arc<AtomicBool>,
}

impl ShellBridge {
    /// Build a bridge using `ORRCH_WEBUI_TMUX_SESSION` (or the default).
    pub fn from_env() -> Self {
        let session_name = std::env::var("ORRCH_WEBUI_TMUX_SESSION")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_SHELL_SESSION.to_string());
        let (tx, _) = broadcast::channel(SHELL_CHANNEL_CAPACITY);
        ShellBridge {
            session_name,
            tx,
            poller_started: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Idempotent: spawn the background poller exactly once across the
    /// process lifetime. Called from each new WS handler — only the
    /// first wins.
    pub fn ensure_poller(&self) {
        if self
            .poller_started
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return; // already running
        }
        let session = self.session_name.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            poll_loop(session, tx).await;
        });
    }

    /// Ensure the tmux session exists. Creates it detached if missing.
    /// Returns `true` if the session is alive after the call.
    pub fn ensure_session(&self) -> bool {
        ensure_tmux_session(&self.session_name)
    }

    /// Get the current pane size. Returns `(cols, rows)` or `(80, 24)`
    /// if tmux/the session isn't reachable.
    pub fn pane_size(&self) -> (u32, u32) {
        pane_size(&self.session_name).unwrap_or((80, 24))
    }

    /// Forward a keystroke payload (bytes from the browser WS) to tmux.
    /// Decodes a small set of named ANSI escapes; everything else is
    /// sent literally via `send-keys -l`.
    pub fn send_input(&self, bytes: &[u8]) {
        send_input(&self.session_name, bytes);
    }

    /// Resize the underlying tmux window to `cols × rows`. The client
    /// computes these from the actual rendered xterm.js viewport on
    /// connect / orientation change / window resize and pushes them
    /// over the WebSocket as a JSON control message. Values are clamped
    /// to a sane range inside `resize_session`.
    pub fn resize(&self, cols: u32, rows: u32) -> bool {
        resize_session(&self.session_name, cols, rows)
    }
}

/// One-shot capture for new WS clients. Returns a clear-screen-prefixed
/// frame so the browser xterm renders the current pane immediately
/// instead of waiting for the next poll-driven diff.
pub fn initial_snapshot(session: &str) -> Option<Vec<u8>> {
    let bytes = capture_pane(session)?;
    let mut frame = Vec::with_capacity(bytes.len() + 8);
    frame.extend_from_slice(b"\x1b[2J\x1b[H");
    frame.extend_from_slice(&bytes);
    Some(frame)
}

// ── internals ─────────────────────────────────────────────────────────

fn tmux_has_session(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn ensure_tmux_session(name: &str) -> bool {
    if tmux_has_session(name) {
        return true;
    }
    // Create detached. Default 80×24 — the WebUI's xterm.js client will
    // immediately drive a resize to its actual viewport dims, so this is
    // just a placeholder. We deliberately don't set 200×50 anymore: that
    // value used to leak through when a client failed to send a resize,
    // smearing content across an unscrollable horizontal axis on phones.
    let ok = Command::new("tmux")
        .args(["new-session", "-d", "-s", name, "-x", "80", "-y", "24"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        tracing::warn!("failed to create tmux session {name}");
        return false;
    }
    // Disable the status line so pane_height == window_height. The `-g`
    // matters: tmux reserves a row for the status line based on the
    // GLOBAL option even when the per-session option is off, so we need
    // both. (Quick repro: `set-option -t S status off; resize-window -y
    // 17` gives pane height 16, but `set-option -g status off` then the
    // same resize gives 17.) We scope it to this server-tmux instance,
    // which is fine — the orrch-web-shell session has no status line
    // role anyway.
    let _ = Command::new("tmux")
        .args(["set-option", "-g", "-t", name, "status", "off"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    ok
}

/// Resize the tmux window backing `name` to `cols × rows`. Values are
/// clamped to a sensible range so a malicious / buggy client can't ask
/// tmux for a 100k-column pane. Returns `true` on success.
fn resize_session(name: &str, cols: u32, rows: u32) -> bool {
    let cols = cols.clamp(1, 500);
    let rows = rows.clamp(1, 200);
    Command::new("tmux")
        .args([
            "resize-window",
            "-t", name,
            "-x", &cols.to_string(),
            "-y", &rows.to_string(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn capture_pane(name: &str) -> Option<Vec<u8>> {
    let output = Command::new("tmux")
        .args([
            "capture-pane",
            "-t", name,
            "-e",  // include ANSI colors
            "-J",  // join wrapped lines
            "-p",  // print to stdout
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
}

fn pane_size(name: &str) -> Option<(u32, u32)> {
    let output = Command::new("tmux")
        .args([
            "display-message", "-p", "-t", name,
            "#{pane_width} #{pane_height}",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout);
    let mut iter = s.split_whitespace();
    let cols: u32 = iter.next()?.parse().ok()?;
    let rows: u32 = iter.next()?.parse().ok()?;
    if cols == 0 || rows == 0 {
        return None;
    }
    Some((cols, rows))
}

async fn poll_loop(session: String, tx: broadcast::Sender<Vec<u8>>) {
    let mut last_hash: Option<[u8; 32]> = None;
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        // No subscribers? still poll cheaply so the next subscriber gets
        // an immediate frame; but skip the broadcast.
        let has_subs = tx.receiver_count() > 0;
        if !has_subs {
            // Reset hash so a re-attach gets a full frame even if the
            // pane content hasn't changed since the last subscriber left.
            last_hash = None;
            continue;
        }
        // Make sure the session still exists. tmux exits will be reflected
        // by `capture_pane` returning None — we recreate on demand.
        if !tmux_has_session(&session) {
            ensure_tmux_session(&session);
        }
        let Some(bytes) = capture_pane(&session) else {
            continue;
        };
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let hash: [u8; 32] = hasher.finalize().into();
        if last_hash.as_ref() == Some(&hash) {
            continue;
        }
        last_hash = Some(hash);
        // Frame format: clear screen + home + payload, so the browser
        // xterm renders the freshly captured pane verbatim.
        let mut frame = Vec::with_capacity(bytes.len() + 8);
        frame.extend_from_slice(b"\x1b[2J\x1b[H");
        frame.extend_from_slice(&bytes);
        let _ = tx.send(frame);
    }
}

/// Translate a single keystroke payload into a tmux send-keys invocation.
///
/// xterm.js sends keystrokes as raw byte sequences (TextEncoder output):
/// printable bytes go through unchanged; named keys come in as ANSI
/// escapes (e.g. arrow up = `\x1b[A`). We preserve that behavior — bare
/// `\x1b...` sequences are passed through as `send-keys -l` literal so
/// tmux delivers them to the inner shell unmodified.
fn send_input(session: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    // Special-case lone CR / CRLF → Enter — tmux's `send-keys -l "\r"`
    // doesn't always trigger newline in some shells.
    if bytes == b"\r" || bytes == b"\r\n" || bytes == b"\n" {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", session, "Enter"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        return;
    }
    // Lone backspace (0x7f or 0x08).
    if bytes == [0x7f] || bytes == [0x08] {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", session, "BSpace"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        return;
    }
    // Lone tab.
    if bytes == [b'\t'] {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", session, "Tab"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        return;
    }
    // Bare ESC.
    if bytes == [0x1b] {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", session, "Escape"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        return;
    }
    // Default path: send the raw byte sequence to the pane via `-H` (hex).
    // This avoids tmux interpreting `~`, `[`, etc. as escape syntax and
    // delivers ANSI sequences (arrow keys etc.) verbatim to the shell.
    let hex = bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>();
    let mut args: Vec<&str> = vec!["send-keys", "-t", session, "-H"];
    for h in &hex {
        args.push(h);
    }
    let _ = Command::new("tmux")
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_default_session_name() {
        // env var unset → default. SAFETY: tests in this module are
        // not parallelized with anything that reads this var.
        unsafe { std::env::remove_var("ORRCH_WEBUI_TMUX_SESSION"); }
        let b = ShellBridge::from_env();
        assert_eq!(b.session_name, DEFAULT_SHELL_SESSION);
    }

    #[test]
    fn pane_size_returns_default_when_no_tmux() {
        // For a session that almost certainly doesn't exist.
        let b = ShellBridge {
            session_name: "this-session-does-not-exist-123456".to_string(),
            tx: broadcast::channel::<Vec<u8>>(8).0,
            poller_started: Arc::new(AtomicBool::new(false)),
        };
        let (c, r) = b.pane_size();
        assert!(c > 0 && r > 0);
    }
}
