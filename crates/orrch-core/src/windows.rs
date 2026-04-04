//! Session management via categorized tmux sessions + Wayland window management.
//!
//! Sessions are organized into tmux session groups, each running in its own
//! alacritty terminal window:
//!   - `orrch-dev`  — Claude Code development sessions
//!   - `orrch-edit` — Vim editing sessions (feedback, project files)
//!   - `orrch-proc` — Feedback processing sessions
//!
//! Window management uses kdotool (KDE Wayland) for minimize/maximize/focus,
//! falling back to qdbus simple calls. No KWin scripting API (that crashes Plasma).

use std::path::Path;
use std::process::Command;

/// Tmux session categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCategory {
    Dev,
    Edit,
    Proc,
}

impl SessionCategory {
    pub fn tmux_name(&self) -> &'static str {
        match self {
            Self::Dev => "orrch-dev",
            Self::Edit => "orrch-edit",
            Self::Proc => "orrch-proc",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Dev => "Dev Sessions",
            Self::Edit => "Editors",
            Self::Proc => "Processing",
        }
    }

    pub fn all() -> &'static [SessionCategory] {
        &[Self::Dev, Self::Edit, Self::Proc]
    }
}

// ─── Tmux Session Management ────────────────────────────────────────

/// Ensure a categorized tmux session exists. Creates it in a new alacritty
/// terminal window if not present.
pub fn ensure_session(cat: SessionCategory) -> bool {
    let name = cat.tmux_name();

    // Check if already exists
    if tmux_has_session(name) {
        return true;
    }

    // Create the tmux session detached first
    let _ = Command::new("tmux")
        .args(["new-session", "-d", "-s", name, "-n", "hub"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    // Apply custom status bar
    apply_custom_status_bar(name);

    // Bind F9 to jump to most urgent window in this session
    // Use string concat to avoid Rust interpolating #{...} as format args
    let f9_cmd = "tmux select-window -t ".to_string() + name
        + ":$(tmux list-windows -t " + name
        + " -F '#{window_index}' | head -1)";
    let _ = Command::new("tmux")
        .args(["bind-key", "-T", "root", "F9", "run-shell", &f9_cmd])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    // Launch an alacritty window attached to this tmux session
    let _ = Command::new("alacritty")
        .args(["--title", &format!("[orrch] {}", cat.label()), "-e", "tmux", "attach-session", "-t", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    true
}

/// Spawn a command as a new tmux window in the given category.
/// Returns the window name on success.
pub fn spawn_in_category(
    cat: SessionCategory,
    window_name: &str,
    shell_cmd: &str,
) -> anyhow::Result<String> {
    ensure_session(cat);
    let name = cat.tmux_name();

    // Sanitize window name
    let clean_name: String = window_name
        .replace('.', "_")
        .replace(':', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(40)
        .collect();

    // Kill existing window with same name to prevent duplicates
    let _ = Command::new("tmux")
        .args(["kill-window", "-t", &format!("{name}:{clean_name}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let output = Command::new("tmux")
        .args(["new-window", "-t", name, "-n", &clean_name, "bash", "-c", shell_cmd])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()?;

    if output.status.success() {
        record_spawned_window(name, &clean_name);
        Ok(clean_name)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tmux new-window failed: {stderr}")
    }
}

/// Spawn a Claude session in the Dev category.
pub fn spawn_tmux_session(
    project_dir: &Path,
    backend_cmd: &[String],
    goal: Option<&str>,
    session_name: &str,
) -> anyhow::Result<String> {
    let dir_str = project_dir.to_string_lossy();

    // Write goal to temp file for safe shell escaping
    let goal_file = if let Some(g) = goal {
        if !g.is_empty() {
            let tmp = std::env::temp_dir().join(format!("orrch-goal-{}.txt", std::process::id()));
            let _ = std::fs::write(&tmp, g);
            Some(tmp)
        } else { None }
    } else { None };

    let backend_str = backend_cmd.iter().map(|a| shell_escape(a)).collect::<Vec<_>>().join(" ");

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

    spawn_in_category(SessionCategory::Dev, session_name, &shell_cmd)
}

/// Spawn a vim editing session in the Edit category.
pub fn spawn_vim_in_tmux(file_path: &Path, window_name: &str) -> anyhow::Result<String> {
    let cmd = format!("nvim {}", shell_escape(&file_path.to_string_lossy()));
    spawn_in_category(SessionCategory::Edit, window_name, &cmd)
}

/// Spawn the develop-feature workflow dispatcher in the Proc category.
/// The workflow script is a bash dispatcher that spawns claude -p subprocesses
/// for each agent step. The Hypervisor is the script, not an LLM.
pub fn spawn_workflow(project_dir: &Path, goal: &str) -> anyhow::Result<String> {
    let dir_str = project_dir.to_string_lossy();

    // Locate the workflow script relative to the orrchestrator project
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
    let script = format!("{home}/projects/orrchestrator/library/tools/run_workflow.sh");

    let cmd = format!(
        "cd {} && bash {} {} {}",
        shell_escape(&dir_str),
        shell_escape(&script),
        shell_escape(&dir_str),
        shell_escape(goal),
    );

    spawn_in_category(SessionCategory::Proc, "workflow", &cmd)
}

// ─── Hub Edit Window ────────────────────────────────────────────────

/// The canonical hub window name used for the shared editor hub.
pub const HUB_EDIT_WINDOW: &str = "hub-edit";

/// Returns true if a window named `hub-edit` exists in the `orrch-edit` session.
pub fn hub_edit_window_exists() -> bool {
    let session = SessionCategory::Edit.tmux_name();
    let output = Command::new("tmux")
        .args(["list-windows", "-t", session, "-F", "#{window_name}"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines().any(|l| l.trim() == HUB_EDIT_WINDOW)
        }
        _ => false,
    }
}

/// Open a file for editing via the hub model.
///
/// - First call: creates a new `hub-edit` window in `orrch-edit` and opens nvim.
/// - Subsequent calls: sends `:vsp <file>` to the existing `hub-edit` window,
///   opening the file as a vertical split alongside the current buffer.
///
/// The `orrch-edit` session is created if it does not yet exist. The alacritty
/// window for that session is focused after the operation.
pub fn hub_vim_open(file_path: &Path) -> anyhow::Result<()> {
    ensure_session(SessionCategory::Edit);
    let session = SessionCategory::Edit.tmux_name();

    if hub_edit_window_exists() {
        // Open in vertical split inside the existing hub-edit window.
        // We escape the path for the Ex command, not for shell — colons are safe
        // in the Ex command, but spaces and backslashes need escaping.
        let escaped = vim_ex_escape(&file_path.to_string_lossy());
        let ex_cmd = format!(":vsp {escaped}");

        let target = format!("{session}:{HUB_EDIT_WINDOW}");
        // Ensure nvim is in normal mode before sending the Ex command,
        // in case it is currently in insert mode.
        Command::new("tmux")
            .args(["send-keys", "-t", &target, "Escape", ""])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok();
        // Small delay to let nvim process the Escape before we send the Ex command.
        std::thread::sleep(std::time::Duration::from_millis(50));
        let status = Command::new("tmux")
            .args(["send-keys", "-t", &target, &ex_cmd, "Enter"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => anyhow::bail!("tmux send-keys exited with: {}", s),
            Err(e) => anyhow::bail!("tmux send-keys failed: {e}"),
        }
    } else {
        // Create the hub-edit window with nvim pre-loaded.
        let file_escaped = shell_escape(&file_path.to_string_lossy());
        let cmd = format!("nvim {file_escaped}");
        let output = Command::new("tmux")
            .args([
                "new-window",
                "-t", session,
                "-n", HUB_EDIT_WINDOW,
                "bash", "-c", &cmd,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tmux new-window (hub-edit) failed: {stderr}");
        }
    }

    // Focus the orrch-edit alacritty window so the user sees the editor.
    focus_category_window(SessionCategory::Edit);
    Ok(())
}

/// Escape a file path for use in a Vim Ex command (`:vsp <path>`).
///
/// Spaces and backslashes must be escaped. Everything else is safe inside
/// the Ex command line (which does not go through a shell).
fn vim_ex_escape(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 10);
    for ch in path.chars() {
        match ch {
            ' ' | '\\' | '|' | '#' | '%' | '\n' | '\r' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

// ─── Listing & Status ───────────────────────────────────────────────

/// A tmux window with status information.
#[derive(Debug, Clone)]
pub struct ManagedSession {
    pub category: SessionCategory,
    pub index: u32,
    pub name: String,
    pub cwd: String,
    pub active: bool,
    pub status: SessionStatus,
    pub last_output: String,
}

/// Inferred status of a tmux window based on its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Working,
    Idle,
    WaitingForInput,
    Dead,
}

impl SessionStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Working => "⚙",
            Self::Idle => "💤",
            Self::WaitingForInput => "❓",
            Self::Dead => "💀",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Idle => "idle",
            Self::WaitingForInput => "waiting",
            Self::Dead => "dead",
        }
    }
}

/// List all managed sessions across all categories with status.
pub fn list_all_sessions() -> Vec<ManagedSession> {
    let mut all = Vec::new();
    for cat in SessionCategory::all() {
        all.extend(list_sessions_in(*cat));
    }
    all
}

/// List sessions in a specific category with status inference.
pub fn list_sessions_in(cat: SessionCategory) -> Vec<ManagedSession> {
    let name = cat.tmux_name();
    let output = match Command::new("tmux")
        .args(["list-windows", "-t", name, "-F",
            "#{window_index}\t#{window_name}\t#{pane_current_path}\t#{window_active}\t#{pane_current_command}"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().filter_map(|line| {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 5 { return None; }
        let index: u32 = parts[0].parse().unwrap_or(0);
        let win_name = parts[1].to_string();
        let cwd = parts[2].to_string();
        let active = parts[3] == "1";
        let cmd = parts[4];

        // Skip the default "hub" placeholder window
        if win_name == "hub" { return None; }

        // Infer status from last pane output
        let (status, last_output) = infer_session_status(name, index);

        Some(ManagedSession { category: cat, index, name: win_name, cwd, active, status, last_output })
    }).collect()
}

/// Infer whether a session is working, idle, or waiting by reading its pane content.
fn infer_session_status(tmux_session: &str, window_index: u32) -> (SessionStatus, String) {
    let output = Command::new("tmux")
        .args(["capture-pane", "-t", &format!("{tmux_session}:{window_index}"), "-p"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    let Ok(out) = output else { return (SessionStatus::Dead, String::new()); };
    if !out.status.success() { return (SessionStatus::Dead, String::new()); }

    let text = String::from_utf8_lossy(&out.stdout);
    let last_line = text.lines().rev()
        .find(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("───") && !t.starts_with("⏵")
        })
        .unwrap_or("")
        .trim()
        .to_string();

    // Check for waiting-for-input signals
    let text_lower = text.to_lowercase();
    let status = if text_lower.contains("do you want to proceed")
        || text_lower.contains("y/n")
        || text_lower.contains("[y/n]")
        || text_lower.contains("approve or deny")
        || text_lower.contains("waiting for")
        || (text_lower.contains("❯") && last_line.contains("❯"))
    {
        SessionStatus::WaitingForInput
    } else if last_line.contains("bypass permissions") || last_line.contains("esc to interrupt") {
        // Claude is at its prompt = idle
        SessionStatus::Idle
    } else {
        SessionStatus::Working
    };

    let display = last_line.chars().take(60).collect();
    (status, display)
}

// ─── Window Management (kdotool / qdbus) ────────────────────────────

/// Focus an alacritty window for a tmux session category.
pub fn focus_category_window(cat: SessionCategory) -> bool {
    let title = format!("[orrch] {}", cat.label());
    focus_window_by_title(&title)
}

/// Focus a window by its title substring.
pub fn focus_window_by_title(title: &str) -> bool {
    // Try kdotool first (Wayland native)
    if let Some(wid) = kdotool_search(title) {
        return kdotool_activate(wid);
    }
    false
}

/// Minimize a window by title.
pub fn minimize_window(title: &str) -> bool {
    if let Some(wid) = kdotool_search(title) {
        return kdotool_minimize(wid);
    }
    false
}

/// Restore (un-minimize) a window by title.
pub fn restore_window(title: &str) -> bool {
    if let Some(wid) = kdotool_search(title) {
        return kdotool_activate(wid);
    }
    false
}

/// Select a specific tmux window and focus its alacritty terminal.
pub fn select_and_focus(cat: SessionCategory, window_index: u32) -> bool {
    let name = cat.tmux_name();
    let _ = Command::new("tmux")
        .args(["select-window", "-t", &format!("{name}:{window_index}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    focus_category_window(cat)
}

/// Kill a specific tmux window.
pub fn kill_session(cat: SessionCategory, window_name: &str) -> bool {
    let name = cat.tmux_name();
    Command::new("tmux")
        .args(["kill-window", "-t", &format!("{name}:{window_name}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// ─── Kill All Managed Sessions ──────────────────────────────────────

/// Kill all managed tmux sessions. Best-effort — logs failures but does not panic.
pub fn kill_all_managed_tmux_sessions() {
    for cat in SessionCategory::all() {
        let name = cat.tmux_name();
        if !tmux_has_session(name) { continue; }
        let result = Command::new("tmux")
            .args(["kill-session", "-t", name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if let Err(e) = result {
            tracing::warn!("Failed to kill tmux session {name}: {e}");
        }
    }
}

// ─── Session State File Tracking ────────────────────────────────────

/// A record of a spawned tmux window.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionRecord {
    pub category: String,
    pub window_name: String,
    pub spawned_at: u64,
}

fn orrch_config_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(home).join(".config").join("orrchestrator")
}

fn session_records_path() -> std::path::PathBuf {
    orrch_config_dir().join("managed-sessions.json")
}

/// Append a record for a newly spawned window.
pub fn record_spawned_window(cat: &str, window_name: &str) {
    let path = session_records_path();
    let mut records = load_session_records();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    records.push(SessionRecord {
        category: cat.to_string(),
        window_name: window_name.to_string(),
        spawned_at: now,
    });
    if let Ok(json) = serde_json::to_string_pretty(&records) {
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(&path));
        let _ = std::fs::write(&path, json);
    }
}

/// Remove all session records (call after clean exit).
pub fn clear_session_records() {
    let _ = std::fs::remove_file(session_records_path());
}

/// Load session records from disk. Returns empty vec if file is missing or malformed.
pub fn load_session_records() -> Vec<SessionRecord> {
    let path = session_records_path();
    let Ok(data) = std::fs::read_to_string(&path) else { return Vec::new(); };
    serde_json::from_str(&data).unwrap_or_default()
}

// ─── Orphan Detection ───────────────────────────────────────────────

/// Detect session records that refer to tmux windows that no longer exist.
/// Returns the orphaned records.
pub fn detect_orphaned_sessions() -> Vec<SessionRecord> {
    let records = load_session_records();
    records.into_iter().filter(|rec| {
        // If the tmux session is entirely gone, the window is orphaned
        if !tmux_has_session(&rec.category) { return true; }
        // Check if the specific window still exists in that session
        let output = Command::new("tmux")
            .args(["list-windows", "-t", &rec.category, "-F", "#{window_name}"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();
        let window_exists = output.ok().filter(|o| o.status.success()).map(|o| {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines().any(|l| l.trim() == rec.window_name)
        }).unwrap_or(false);
        !window_exists
    }).collect()
}

// ─── Jump to Most Urgent Window ─────────────────────────────────────

/// Focus the most urgent window in a session category
/// (WaitingForInput > Working > Idle > Dead).
pub fn jump_to_most_urgent(cat: SessionCategory) {
    let sessions = list_sessions_in(cat);
    if sessions.is_empty() { return; }
    let priority = |s: &SessionStatus| match s {
        SessionStatus::WaitingForInput => 0,
        SessionStatus::Working => 1,
        SessionStatus::Idle => 2,
        SessionStatus::Dead => 3,
    };
    if let Some(most_urgent) = sessions.iter().min_by_key(|s| priority(&s.status)) {
        select_and_focus(cat, most_urgent.index);
    }
}

// ─── Split-Off Editor Detection ─────────────────────────────────────

/// Returns window names in the orrch-edit session that are NOT the hub window.
pub fn detect_split_off_editors(expected_hub: &str) -> Vec<String> {
    let name = SessionCategory::Edit.tmux_name();
    let output = match Command::new("tmux")
        .args(["list-windows", "-t", name, "-F", "#{window_name}"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|n| !n.is_empty() && n != expected_hub && n != "hub")
        .collect()
}

// ─── Custom Status Bar ──────────────────────────────────────────────

/// Apply orrchestrator custom status bar to a tmux session.
/// Also installs the status script to ~/.config/orrchestrator/ if not present.
fn apply_custom_status_bar(session_name: &str) {
    let config_dir = orrch_config_dir();
    let script_dst = config_dir.join("orrch-tmux-status.sh");

    // Install status script if not present
    if !script_dst.exists() {
        // Try to copy from library/tools relative to the binary or projects dir
        let candidates = [
            std::env::var("HOME").unwrap_or_default() + "/projects/orrchestrator/library/tools/orrch-tmux-status.sh",
        ];
        for src in &candidates {
            if std::path::Path::new(src).exists() {
                let _ = std::fs::create_dir_all(&config_dir);
                let _ = std::fs::copy(src, &script_dst);
                // chmod +x
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(&script_dst) {
                        let mut perms = meta.permissions();
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(&script_dst, perms);
                    }
                }
                break;
            }
        }
    }

    let script_path = script_dst.to_string_lossy().into_owned();
    let opts: &[(&str, &str)] = &[
        ("status", "on"),
        ("status-interval", "5"),
        ("status-left", " [orrch: #{session_name}] "),
        ("status-style", "bg=colour235,fg=colour250"),
        ("status-left-style", "bold,fg=colour203"),
    ];
    let name = session_name;
    for (key, val) in opts {
        let _ = Command::new("tmux")
            .args(["set-option", "-t", name, key, val])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    // Set status-right with the script path interpolated
    let status_right = format!("#({script_path} #{{session_name}}) %H:%M");
    let _ = Command::new("tmux")
        .args(["set-option", "-t", name, "status-right", &status_right])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

// ─── kdotool helpers ────────────────────────────────────────────────

fn kdotool_search(title: &str) -> Option<String> {
    let output = Command::new("kdotool")
        .args(["search", "--name", title])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().next().map(|l| l.trim().to_string()).filter(|s| !s.is_empty())
}

fn kdotool_activate(window_id: String) -> bool {
    Command::new("kdotool")
        .args(["windowactivate", &window_id])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn kdotool_minimize(window_id: String) -> bool {
    Command::new("kdotool")
        .args(["windowminimize", &window_id])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// ─── Legacy stubs ───────────────────────────────────────────────────
// Keep TMUX_SESSION for backward compat during transition
pub const TMUX_SESSION: &str = "orrch-dev";

pub fn ensure_tmux_session() -> bool { ensure_session(SessionCategory::Dev) }
pub fn list_tmux_windows() -> Vec<TmuxWindow> {
    list_sessions_in(SessionCategory::Dev).into_iter().map(|s| TmuxWindow {
        index: s.index, name: s.name, cwd: s.cwd, active: s.active,
    }).collect()
}
pub fn select_tmux_window(index: u32) -> bool { select_and_focus(SessionCategory::Dev, index) }
pub fn kill_tmux_window(window_name: &str) -> bool { kill_session(SessionCategory::Dev, window_name) }
pub fn tmux_available() -> bool {
    Command::new("which").arg("tmux").output().is_ok_and(|o| o.status.success())
}

#[derive(Debug, Clone)]
pub struct TmuxWindow {
    pub index: u32,
    pub name: String,
    pub cwd: String,
    pub active: bool,
}

// Stubs for any remaining old callers
pub fn move_to_output(_: &str, _: &str) {}
pub fn restore_and_raise(_: &str) {}
pub fn toggle_minimize(_: &str) {}
pub fn minimize_all_managed() {}
pub fn restore_all_managed() {}
pub fn bring_to_current_desktop(_: &str) -> bool { false }
pub fn hide_window(_: &str) {}

// ─── Helpers ────────────────────────────────────────────────────────

fn tmux_has_session(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn shell_escape(s: &str) -> String {
    if s.contains(|c: char| c.is_whitespace() || c == '\'' || c == '"' || c == '\\' || c == '$' || c == '`') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}
