//! Session management via tmux.
//!
//! All managed sessions run as named tmux windows inside a single "orrch" tmux
//! session. The user swaps between them with tmux hotkeys or by selecting in the
//! orrchestrator TUI.
//!
//! This replaces the previous KWin/qdbus window management which caused Plasma
//! desktop crashes due to race conditions in KWin's scripting engine.

use std::path::Path;
use std::process::Command;

/// The tmux session name that orrchestrator owns.
pub const TMUX_SESSION: &str = "orrch";

/// Ensure the orrch tmux session exists. Creates it detached if not.
pub fn ensure_tmux_session() -> bool {
    // Check if session already exists — suppress stderr
    let check = Command::new("tmux")
        .args(["has-session", "-t", TMUX_SESSION])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if check.is_ok_and(|s| s.success()) {
        return true; // Already exists, nothing to do
    }
    // Doesn't exist — create it detached
    Command::new("tmux")
        .args(["new-session", "-d", "-s", TMUX_SESSION, "-n", "hub"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Spawn a Claude Code session as a new tmux window in the orrch session.
///
/// Returns the window name on success.
pub fn spawn_tmux_session(
    project_dir: &Path,
    backend_cmd: &[String],
    goal: Option<&str>,
    session_name: &str,
) -> anyhow::Result<String> {
    ensure_tmux_session();

    // Sanitize window name for tmux (no dots or colons)
    let window_name = session_name
        .replace('.', "_")
        .replace(':', "-")
        .chars()
        .take(40)
        .collect::<String>();

    let dir_str = project_dir.to_string_lossy();

    // Write goal to a temp file to avoid shell escaping issues with long/complex goals
    let goal_file = if let Some(g) = goal {
        if !g.is_empty() {
            let tmp = std::env::temp_dir().join(format!("orrch-goal-{}.txt", std::process::id()));
            let _ = std::fs::write(&tmp, g);
            Some(tmp)
        } else { None }
    } else { None };

    let backend_str = backend_cmd
        .iter()
        .map(|a| shell_escape(a))
        .collect::<Vec<_>>()
        .join(" ");

    // Build shell command: read goal from file if present, pass to backend
    let shell_cmd = if let Some(ref gf) = goal_file {
        format!(
            "cd {} && goal=$(cat {}) && rm -f {} && {} \"$goal\"",
            shell_escape(&dir_str),
            shell_escape(&gf.to_string_lossy()),
            shell_escape(&gf.to_string_lossy()),
            backend_str,
        )
    } else {
        format!("cd {} && {}", shell_escape(&dir_str), backend_str)
    };

    // Ensure session exists before creating window
    ensure_tmux_session();

    let output = Command::new("tmux")
        .args([
            "new-window",
            "-t", TMUX_SESSION,
            "-n", &window_name,
            "bash", "-c", &shell_cmd,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()?;

    if output.status.success() {
        Ok(window_name)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tmux new-window failed: {stderr}")
    }
}

/// List all tmux windows in the orrch session.
pub fn list_tmux_windows() -> Vec<TmuxWindow> {
    let output = match Command::new("tmux")
        .args([
            "list-windows",
            "-t", TMUX_SESSION,
            "-F", "#{window_index}\t#{window_name}\t#{pane_current_path}\t#{window_active}",
        ])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                Some(TmuxWindow {
                    index: parts[0].parse().unwrap_or(0),
                    name: parts[1].to_string(),
                    cwd: parts[2].to_string(),
                    active: parts[3] == "1",
                })
            } else {
                None
            }
        })
        .collect()
}

/// Select (focus) a specific tmux window by index.
pub fn select_tmux_window(index: u32) -> bool {
    Command::new("tmux")
        .args([
            "select-window",
            "-t", &format!("{TMUX_SESSION}:{index}"),
        ])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Kill a specific tmux window by name.
pub fn kill_tmux_window(window_name: &str) -> bool {
    Command::new("tmux")
        .args([
            "kill-window",
            "-t", &format!("{TMUX_SESSION}:{window_name}"),
        ])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Check if tmux is available on the system.
pub fn tmux_available() -> bool {
    Command::new("which")
        .arg("tmux")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[derive(Debug, Clone)]
pub struct TmuxWindow {
    pub index: u32,
    pub name: String,
    pub cwd: String,
    pub active: bool,
}

// ─── Stubs for removed KWin functions ────────────────────────────────
// These are no-ops that satisfy any remaining call sites during transition.
// They will be removed once all callers are updated.

pub fn move_to_output(_title: &str, _output: &str) {}
pub fn restore_and_raise(_title: &str) {}
pub fn toggle_minimize(_title: &str) {}
pub fn minimize_all_managed() {}
pub fn restore_all_managed() {}
pub fn bring_to_current_desktop(_title: &str) -> bool { false }
pub fn hide_window(_title: &str) {}

fn shell_escape(s: &str) -> String {
    if s.contains(|c: char| c.is_whitespace() || c == '\'' || c == '"' || c == '\\' || c == '$' || c == '`') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}
