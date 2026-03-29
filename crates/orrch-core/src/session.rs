use std::path::PathBuf;
use std::time::Instant;

use crate::backend::BackendKind;

/// State of a managed Claude Code session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Working,
    Waiting,
    Idle,
    Dead,
}

impl SessionState {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Working => "⚙",
            Self::Waiting => "❓",
            Self::Idle => "💤",
            Self::Dead => "💀",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Idle => "idle",
            Self::Dead => "dead",
        }
    }
}

/// A managed Claude Code session running in a PTY.
pub struct Session {
    pub sid: String,
    pub project_dir: PathBuf,
    pub pid: nix::unistd::Pid,
    pub master_fd: i32,
    pub state: SessionState,
    pub backend: BackendKind,
    pub goal: Option<String>,
    pub started_at: Instant,
    pub output_buffer: Vec<u8>,
    pub last_output_time: Option<Instant>,
}

impl Session {
    pub fn new(
        sid: String,
        project_dir: PathBuf,
        pid: nix::unistd::Pid,
        master_fd: i32,
        backend: BackendKind,
        goal: Option<String>,
    ) -> Self {
        Self {
            sid,
            project_dir,
            pid,
            master_fd,
            state: SessionState::Working,
            backend,
            goal,
            started_at: Instant::now(),
            output_buffer: Vec::new(),
            last_output_time: None,
        }
    }

    pub fn goal_display(&self) -> &str {
        self.goal.as_deref().unwrap_or("(no goal)")
    }

    pub fn display_name(&self) -> &str {
        self.project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

    pub fn uptime(&self) -> String {
        let elapsed = self.started_at.elapsed().as_secs();
        if elapsed < 60 {
            format!("{elapsed}s")
        } else if elapsed < 3600 {
            format!("{}m{:02}s", elapsed / 60, elapsed % 60)
        } else {
            format!("{}h{:02}m", elapsed / 3600, (elapsed % 3600) / 60)
        }
    }
}

/// An external Claude Code process not managed by orrchestrator.
#[derive(Debug, Clone)]
pub struct ExternalSession {
    pub pid: u32,
    pub project_dir: String,
    pub cmdline: String,
    pub session_name: String, // user-set name from Claude's session data
    pub host: String,         // machine name (empty = local)
}

impl ExternalSession {
    pub fn display_name(&self) -> &str {
        if !self.session_name.is_empty() {
            return &self.session_name;
        }
        if self.project_dir.is_empty() {
            return "unknown";
        }
        std::path::Path::new(&self.project_dir)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

    /// Returns a host badge like "@orrpheus" for remote sessions, empty for local.
    pub fn host_badge(&self) -> String {
        if self.host.is_empty() {
            String::new()
        } else {
            format!("@{}", self.host)
        }
    }

    pub fn is_remote(&self) -> bool {
        !self.host.is_empty()
    }
}

/// Read the session name from Claude's session file (~/.claude/sessions/<PID>.json).
pub fn read_session_name(pid: u32) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = std::path::Path::new(&home)
        .join(".claude")
        .join("sessions")
        .join(format!("{pid}.json"));

    if let Ok(contents) = std::fs::read_to_string(path) {
        // Simple JSON extraction without a full parser — find "name":"..."
        if let Some(pos) = contents.find("\"name\"") {
            let rest = &contents[pos..];
            if let Some(colon) = rest.find(':') {
                let after_colon = rest[colon + 1..].trim_start();
                if after_colon.starts_with('"') {
                    let inner = &after_colon[1..];
                    if let Some(end) = inner.find('"') {
                        return inner[..end].to_string();
                    }
                }
            }
        }
    }
    String::new()
}
