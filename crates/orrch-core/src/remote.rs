//! Remote host management — discover and manage Claude sessions across the network.
//!
//! Uses orrch-agent.sh (piped via SSH stdin) for cross-platform discovery and
//! session management. The agent handles Linux/macOS differences transparently.

use serde::Deserialize;
use tracing::debug;

use crate::session::ExternalSession;

/// Embedded agent script — sent to remote hosts via SSH stdin each invocation.
/// This ensures remotes always run the latest version without manual deployment.
const AGENT_SCRIPT: &str = include_str!("../../../agent/orrch-agent.sh");

/// A machine on the network that can run Claude Code sessions.
#[derive(Debug, Clone)]
pub struct RemoteHost {
    pub name: String,
    pub ssh_target: String, // e.g. "orrgate", "coltonorr@orrpheus"
    pub is_local: bool,
    pub reachable: bool,
    pub capabilities: Option<HostCapabilities>,
}

/// Capabilities reported by the agent's `check` command.
#[derive(Debug, Clone, Deserialize)]
pub struct HostCapabilities {
    pub os: String,
    pub mux: String,      // "tmux", "screen", or "nohup"
    pub claude: bool,
    pub gemini: bool,
    pub projects_dir: String,
    pub hostname: String,
}

/// Known hosts from the workspace configuration.
pub fn known_hosts() -> Vec<RemoteHost> {
    let hostname = get_hostname();
    vec![
        RemoteHost {
            name: "orrion".into(),
            ssh_target: "orrion".into(),
            is_local: hostname == "orrion",
            reachable: false,
            capabilities: None,
        },
        RemoteHost {
            name: "orrgate".into(),
            ssh_target: "orrgate".into(),
            is_local: hostname == "orrgate",
            reachable: false,
            capabilities: None,
        },
        RemoteHost {
            name: "orrpheus".into(),
            ssh_target: "coltonorr@orrpheus".into(),
            is_local: hostname.to_lowercase().starts_with("orrpheus"),
            reachable: false,
            capabilities: None,
        },
    ]
}

// ─── Agent invocation helper ────────────────────────────────────────

/// Run an agent subcommand on a remote host by piping the script via SSH stdin.
/// Returns stdout on success, None on failure.
async fn run_agent(host: &RemoteHost, subcommand: &str) -> Option<String> {
    let mut child = tokio::process::Command::new("ssh")
        .args([
            "-o", "ConnectTimeout=5",
            "-o", "BatchMode=yes",
            "-o", "StrictHostKeyChecking=accept-new",
            &host.ssh_target,
            &format!("bash -s -- {subcommand}"),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;

    // Write agent script to stdin, then close to signal EOF
    use tokio::io::AsyncWriteExt;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(AGENT_SCRIPT.as_bytes()).await;
        drop(stdin);
    }

    let output = child.wait_with_output().await.ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Some(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!("Agent command '{subcommand}' failed on {}: {stderr}", host.name);
        None
    }
}

/// Run agent with arguments (for spawn, kill, etc.)
async fn run_agent_with_args(host: &RemoteHost, args: &str) -> Option<String> {
    let mut child = tokio::process::Command::new("ssh")
        .args([
            "-o", "ConnectTimeout=5",
            "-o", "BatchMode=yes",
            "-o", "StrictHostKeyChecking=accept-new",
            &host.ssh_target,
            &format!("bash -s -- {args}"),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;

    use tokio::io::AsyncWriteExt;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(AGENT_SCRIPT.as_bytes()).await;
        drop(stdin);
    }

    let output = child.wait_with_output().await.ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!("Agent args '{args}' failed on {}: {stderr}", host.name);
        None
    }
}

// ─── Discovery ──────────────────────────────────────────────────────

/// JSON structure returned by the agent's `discover` command.
#[derive(Debug, Deserialize)]
struct AgentDiscovery {
    pid: u32,
    cmdline: String,
    cwd: String,
}

/// Discover Claude sessions on a remote host via the agent.
pub async fn discover_remote_sessions(host: &RemoteHost) -> Vec<ExternalSession> {
    if host.is_local {
        return Vec::new(); // local discovery handled by ProcessManager
    }

    let stdout = match run_agent(host, "discover").await {
        Some(s) => s,
        None => return Vec::new(),
    };

    let mut sessions = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            continue;
        }
        match serde_json::from_str::<AgentDiscovery>(line) {
            Ok(disc) => {
                sessions.push(ExternalSession {
                    pid: disc.pid,
                    project_dir: disc.cwd,
                    cmdline: disc.cmdline,
                    session_name: String::new(),
                    host: host.name.clone(),
                });
            }
            Err(e) => {
                debug!("Failed to parse agent discovery line from {}: {e}", host.name);
            }
        }
    }

    sessions
}

// ─── Reachability & Capabilities ────────────────────────────────────

/// Check if a remote host is reachable and probe its capabilities.
pub async fn check_host_reachable(host: &mut RemoteHost) {
    if host.is_local {
        host.reachable = true;
        return;
    }

    match run_agent(host, "check").await {
        Some(stdout) => {
            host.reachable = true;
            // Parse capabilities from the check output
            for line in stdout.lines() {
                let line = line.trim();
                if line.starts_with('{') {
                    if let Ok(caps) = serde_json::from_str::<HostCapabilities>(line) {
                        debug!("Host {} capabilities: os={}, mux={}, claude={}, gemini={}",
                            host.name, caps.os, caps.mux, caps.claude, caps.gemini);
                        host.capabilities = Some(caps);
                        break;
                    }
                }
            }
        }
        None => {
            host.reachable = false;
            host.capabilities = None;
        }
    }
}

// ─── Session Spawning ───────────────────────────────────────────────

/// Spawn a Claude session on a remote host via the agent.
///
/// The agent auto-detects tmux/screen/nohup and uses whatever is available.
/// Returns the session name on success.
pub async fn spawn_remote_session(
    host: &RemoteHost,
    project_name: &str,
    backend: &str,
    goal: &str,
    flags: &[String],
) -> anyhow::Result<String> {
    let flags_str = flags.join(" ");
    let args = format!(
        "spawn {} {} {} {}",
        shell_escape(project_name),
        shell_escape(backend),
        shell_escape(goal),
        flags_str,
    );

    match run_agent_with_args(host, &args).await {
        Some(stdout) => {
            let trimmed = stdout.trim();
            // Agent returns "OK:<session_name>:<mux>"
            if trimmed.starts_with("OK:") {
                let parts: Vec<&str> = trimmed.splitn(3, ':').collect();
                let session_name = parts.get(1).unwrap_or(&"unknown").to_string();
                let mux = parts.get(2).unwrap_or(&"?");
                debug!("Remote spawn on {} via {mux}: {session_name}", host.name);
                Ok(session_name)
            } else if trimmed.starts_with("ERROR:") {
                anyhow::bail!("Remote spawn failed: {trimmed}")
            } else {
                // Assume success if no error
                Ok(format!("orrch-{project_name}"))
            }
        }
        None => anyhow::bail!("Failed to reach {} for remote spawn", host.name),
    }
}

// ─── Session Management ─────────────────────────────────────────────

/// Kill a remote session by name.
pub async fn kill_remote_session(host: &RemoteHost, session_name: &str) -> bool {
    let args = format!("kill {}", shell_escape(session_name));
    run_agent_with_args(host, &args).await.is_some()
}

/// List orrchestrator-managed sessions on a remote host.
pub async fn list_remote_managed_sessions(host: &RemoteHost) -> Vec<String> {
    match run_agent(host, "list").await {
        Some(stdout) => stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && l.starts_with("orrch-"))
            .collect(),
        None => Vec::new(),
    }
}

// ─── Helpers ────────────────────────────────────────────────────────

fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if s.contains(|c: char| c.is_whitespace() || c == '\'' || c == '"' || c == '\\' || c == '$' || c == '`') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

fn get_hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}
