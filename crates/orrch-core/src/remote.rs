//! Remote host management — discover and manage Claude sessions across the network.
//!
//! Uses orrch-agent.sh (piped via SSH stdin) for cross-platform discovery and
//! session management. The agent handles Linux/macOS differences transparently.

use serde::Deserialize;
use tracing::debug;

use crate::session::ExternalSession;

// ─── Output sanitization ─────────────────────────────────────────────
//
// Remote shells (especially fish with themed prompts) tend to emit ANSI
// CSI and OSC escape sequences on stdout at startup — even for
// non-interactive `ssh host 'bash -s -- …'` invocations. Those bytes get
// prepended to the agent's JSON output on the same line, so naive
// `line.starts_with('{')` parsing fails silently. We strip escape
// sequences up front, then scan for balanced JSON objects within each
// line.

/// Strip ANSI CSI (`ESC [ ... final`) and OSC (`ESC ] ... ST`) escape
/// sequences from a string. Also strips bare BEL (`\x07`) and stray
/// DCS/APC/PM/SOS sequences. Non-escape bytes pass through unchanged.
pub(crate) fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1B && i + 1 < bytes.len() {
            // ESC — look at the introducer byte.
            let next = bytes[i + 1];
            match next {
                b'[' => {
                    // CSI: parameters/intermediate bytes, then a final byte
                    // in 0x40–0x7E. Skip until we hit the final byte.
                    i += 2;
                    while i < bytes.len() && !(0x40..=0x7E).contains(&bytes[i]) {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                }
                b']' | b'P' | b'X' | b'^' | b'_' => {
                    // OSC/DCS/SOS/PM/APC: terminated by ST (ESC \) or BEL.
                    i += 2;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1B && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    // Two-byte ESC sequence (e.g. ESC M, ESC 7, ESC (B).
                    i += 2;
                    // Some sequences have an additional charset byte.
                    if next == b'(' || next == b')' || next == b'*' || next == b'+' {
                        if i < bytes.len() {
                            i += 1;
                        }
                    }
                }
            }
            continue;
        }
        // Strip bare BEL too — some shells emit it from OSC terminators
        // when the ESC got eaten elsewhere.
        if b == 0x07 {
            i += 1;
            continue;
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Find the first balanced JSON object (`{…}`) inside a string and
/// return it as a slice. Quoted strings and escape characters inside
/// JSON values are handled correctly. Returns None if no complete
/// object is found.
pub(crate) fn find_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = s.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&s[start..=i]);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Iterate JSON objects inside `text`, handling shell noise (escape
/// sequences, prompt bytes) and mixed output. Each `{…}` object found
/// via balanced-brace scanning is yielded.
pub(crate) fn iter_json_objects(text: &str) -> Vec<String> {
    let clean = strip_ansi(text);
    let mut out: Vec<String> = Vec::new();
    let mut rest = clean.as_str();
    while let Some(obj) = find_json_object(rest) {
        out.push(obj.to_string());
        let obj_end_offset = (obj.as_ptr() as usize + obj.len()) - rest.as_ptr() as usize;
        if obj_end_offset >= rest.len() {
            break;
        }
        rest = &rest[obj_end_offset..];
    }
    out
}

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
    for obj in iter_json_objects(&stdout) {
        match serde_json::from_str::<AgentDiscovery>(&obj) {
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
                debug!("Failed to parse agent discovery object from {}: {e}", host.name);
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
            // Parse capabilities tolerantly — remote shells (fish with
            // themed prompts, in particular) emit terminal escape codes
            // on the first line of output, so naive startswith('{')
            // scanning silently fails. `iter_json_objects` strips
            // ANSI/OSC noise and scans for balanced JSON objects.
            for obj in iter_json_objects(&stdout) {
                if let Ok(caps) = serde_json::from_str::<HostCapabilities>(&obj) {
                    debug!("Host {} capabilities: os={}, mux={}, claude={}, gemini={}",
                        host.name, caps.os, caps.mux, caps.claude, caps.gemini);
                    host.capabilities = Some(caps);
                    break;
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
    let flags_str = flags.iter().map(|f| shell_escape(f)).collect::<Vec<_>>().join(" ");
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
        Some(stdout) => {
            // Same shell-noise problem as check/discover — the first line
            // can be preceded by OSC color codes from a themed prompt.
            // Strip escapes before scanning for session names.
            let clean = strip_ansi(&stdout);
            clean
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && l.starts_with("orrch-"))
                .collect()
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_leaves_plain_text() {
        assert_eq!(strip_ansi("hello world"), "hello world");
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn strip_ansi_removes_csi_sequences() {
        // Red-colored "err" followed by reset + plain text.
        let raw = "\x1b[31merr\x1b[0m ok";
        assert_eq!(strip_ansi(raw), "err ok");
    }

    #[test]
    fn strip_ansi_removes_osc_color_setup() {
        // Classic Catppuccin OSC 10/11/12 prompt startup noise.
        let raw = "\x1b]10;#cdd6f4\x07\x1b]11;#1e1e2e\x07{\"os\":\"macos\"}";
        assert_eq!(strip_ansi(raw), "{\"os\":\"macos\"}");
    }

    #[test]
    fn strip_ansi_handles_st_terminated_osc() {
        // OSC terminated by ST (ESC \\) instead of BEL.
        let raw = "\x1b]4;15;#a6adc8\x1b\\after";
        assert_eq!(strip_ansi(raw), "after");
    }

    #[test]
    fn find_json_object_simple() {
        let s = "prefix {\"a\":1,\"b\":2} suffix";
        assert_eq!(find_json_object(s), Some("{\"a\":1,\"b\":2}"));
    }

    #[test]
    fn find_json_object_nested() {
        let s = "{\"outer\":{\"inner\":[1,2,3]},\"end\":true}";
        assert_eq!(find_json_object(s), Some(s));
    }

    #[test]
    fn find_json_object_string_with_braces() {
        // Braces inside a JSON string should NOT close the outer object.
        let s = "{\"msg\":\"not a } close\",\"ok\":true}";
        assert_eq!(find_json_object(s), Some(s));
    }

    #[test]
    fn find_json_object_escaped_quote() {
        let s = "{\"msg\":\"a \\\"quoted\\\" word\",\"ok\":true}";
        assert_eq!(find_json_object(s), Some(s));
    }

    #[test]
    fn find_json_object_none() {
        assert_eq!(find_json_object("no object here"), None);
        assert_eq!(find_json_object("{incomplete"), None);
    }

    #[test]
    fn iter_json_objects_strips_shell_noise() {
        // This is the actual failure mode that broke orrpheus:
        // fish prompt OSC escapes prepended to the JSON line from the
        // agent's `check` command.
        let raw = "\x1b]10;#cdd6f4\x07\x1b]11;#1e1e2e\x07{\"os\":\"macos\",\"mux\":\"screen\",\"claude\":true,\"gemini\":false,\"projects_dir\":\"/Users/x/projects\",\"hostname\":\"orrpheus\"}\n\x1b]4;15;#a6adc8\x07";
        let objects = iter_json_objects(raw);
        assert_eq!(objects.len(), 1, "objects: {objects:?}");
        let parsed: HostCapabilities = serde_json::from_str(&objects[0]).expect("JSON parses");
        assert_eq!(parsed.os, "macos");
        assert_eq!(parsed.mux, "screen");
        assert!(parsed.claude);
        assert!(!parsed.gemini);
        assert_eq!(parsed.hostname, "orrpheus");
    }

    #[test]
    fn iter_json_objects_multiple_lines() {
        // `discover` emits one JSON object per line.
        let raw = "\x1b]10;#cdd6f4\x07{\"pid\":42,\"cmdline\":\"claude\",\"cwd\":\"/a\"}\n{\"pid\":43,\"cmdline\":\"claude\",\"cwd\":\"/b\"}\n";
        let objects = iter_json_objects(raw);
        assert_eq!(objects.len(), 2);
        assert!(objects[0].contains("\"pid\":42"));
        assert!(objects[1].contains("\"pid\":43"));
    }
}
