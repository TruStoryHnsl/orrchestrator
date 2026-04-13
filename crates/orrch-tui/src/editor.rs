//! Clipboard utilities and external nvim spawning.
//!
//! The custom inline editor has been replaced by spawning real nvim instances.
//! This module retains clipboard helpers used by single-line text inputs (SpawnGoal).

use std::process::{Command, Stdio};

/// Read from system clipboard using native CLI tools.
/// Tries wl-paste (Wayland) → xclip (X11) → arboard (fallback).
pub fn clipboard_get() -> Option<String> {
    // Wayland
    if let Ok(out) = Command::new("wl-paste").arg("--no-newline").stdout(Stdio::piped()).stderr(Stdio::null()).output() {
        if out.status.success() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    // X11
    if let Ok(out) = Command::new("xclip").args(["-selection", "clipboard", "-o"]).stdout(Stdio::piped()).stderr(Stdio::null()).output() {
        if out.status.success() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    // Last resort
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Write to system clipboard using native CLI tools.
/// Tries wl-copy (Wayland) → xclip (X11) → arboard (fallback).
pub fn clipboard_set(text: &str) -> bool {
    // Wayland
    if let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).stderr(Stdio::null()).spawn() {
        if let Some(stdin) = child.stdin.take() {
            use std::io::Write;
            let mut stdin = stdin;
            let _ = stdin.write_all(text.as_bytes());
            drop(stdin);
            if let Ok(status) = child.wait() {
                if status.success() { return true; }
            }
        }
    }
    // X11
    if let Ok(mut child) = Command::new("xclip").args(["-selection", "clipboard"]).stdin(Stdio::piped()).stderr(Stdio::null()).spawn() {
        if let Some(stdin) = child.stdin.take() {
            use std::io::Write;
            let mut stdin = stdin;
            let _ = stdin.write_all(text.as_bytes());
            drop(stdin);
            if let Ok(status) = child.wait() {
                if status.success() { return true; }
            }
        }
    }
    // Last resort
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut c| c.set_text(text).ok())
        .is_some()
}

// ─── External Nvim Spawning ─────────────────────────────────────────

/// What kind of editing session this is.
#[derive(Debug, Clone)]
pub enum VimKind {
    /// Global feedback — will be routed to projects by content.
    GlobalFeedback,
    /// Feedback targeting a specific project (index into App.projects).
    ProjectFeedback(usize),
    /// Append to a project's master plan.
    MasterPlanAppend(usize),
    /// New idea for the vault.
    NewIdea,
    /// Intake review — editing optimized instructions before distribution.
    IntakeReview,
    /// Edit a PLAN.md file from the Design > Plans panel.
    PlanFile,
}

/// A request from App to the main loop to spawn nvim.
#[derive(Debug)]
pub struct VimRequest {
    pub file: std::path::PathBuf,
    pub kind: VimKind,
    /// Window title — shown in taskbar, alt-tab, and nvim titlebar.
    pub title: String,
}

/// An nvim process that is running in a separate terminal window.
pub struct PendingEditor {
    pub child: std::process::Child,
    pub file: std::path::PathBuf,
    pub kind: VimKind,
}

/// Check if a graphical display server is available.
pub fn has_display() -> bool {
    std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok()
}

/// Find a terminal emulator on the system.
fn find_terminal() -> Option<String> {
    if let Ok(t) = std::env::var("TERMINAL") {
        return Some(t);
    }
    for name in &["alacritty", "kitty", "konsole", "gnome-terminal", "xfce4-terminal", "xterm"] {
        if Command::new("which").arg(name).stdout(Stdio::null()).stderr(Stdio::null()).status()
            .map(|s| s.success()).unwrap_or(false) {
            return Some(name.to_string());
        }
    }
    None
}

/// Build nvim `-c` args that brand the window as orrchestrator-owned.
///
/// Sets three things:
/// 1. Terminal title (visible in taskbar / alt-tab)
/// 2. Persistent statusline at the bottom: "[orrchestrator] Feedback  file.md    :wq save | :q! discard"
/// 3. StatusLine highlight in orrchestrator's accent color (#E94560) so it's unmistakable
fn vim_title_args(title: &str) -> Vec<String> {
    let esc = title.replace(' ', "\\ ");
    vec![
        "-c".into(), format!("set title titlestring={esc}"),
        "-c".into(), format!("set laststatus=2 scrolloff=2 statusline={esc}\\ \\ %f%m"),
        "-c".into(), "hi StatusLine cterm=NONE gui=NONE".into(),
    ]
}

fn request_window_focus(title: &str) {
    if title.trim().is_empty() {
        return;
    }

    let escaped = title.replace('\'', "'\"'\"'");
    let script = format!("sleep 0.2; wmctrl -a '{escaped}' >/dev/null 2>&1");
    let _ = Command::new("sh")
        .args(["-lc", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Get the nvim `-c` args for branding (used by the blocking fallback in main.rs).
pub fn vim_title_args_pub(title: &str) -> Vec<String> {
    vim_title_args(title)
}

/// Spawn nvim in a new terminal window. Returns the child process on success.
///
/// The terminal process is detached (new session via setsid) so it survives
/// orrchestrator crashes or restarts. Orphaned windows are re-adopted on startup.
pub fn spawn_vim_window(file: &std::path::Path, title: &str) -> Option<std::process::Child> {
    use std::os::unix::process::CommandExt;

    if !has_display() { return None; }
    let terminal = find_terminal()?;
    let file_str = file.to_str()?;
    let vim_args = vim_title_args(title);

    let mut cmd = Command::new(&terminal);

    // Each terminal has different syntax for "run this command"
    match terminal.as_str() {
        "gnome-terminal" => { cmd.arg("--title").arg(title).arg("--").arg("nvim").args(&vim_args).arg(file_str); }
        "kitty" => { cmd.arg("--title").arg(title).arg("nvim").args(&vim_args).arg(file_str); }
        "xterm" => { cmd.arg("-T").arg(title).arg("-e").arg("nvim").args(&vim_args).arg(file_str); }
        "konsole" => { cmd.arg("-e").arg("nvim").args(&vim_args).arg(file_str); }
        _ => { cmd.arg("--title").arg(title).arg("-e").arg("nvim").args(&vim_args).arg(file_str); }
    }

    // Detach: new session so the terminal survives orrchestrator exit/crash.
    // SAFETY: setsid() is async-signal-safe and has no preconditions.
    unsafe { cmd.pre_exec(|| { libc::setsid(); Ok(()) }); }

    let child = cmd.spawn().ok()?;
    request_window_focus(title);
    Some(child)
}
