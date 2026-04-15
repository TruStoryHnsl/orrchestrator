//! PI RPC session backend.
//!
//! Spawns `pi --mode rpc --no-session [--provider p] [--model m] [--thinking off]`
//! as a child process and communicates with it via JSONL on stdin/stdout.
//! The child's stdout is drained by a background thread into an `mpsc` channel
//! so callers can poll without blocking.

use std::io::Write as _;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::session::SessionState;

/// A live PI RPC session.
pub struct PiRpcSession {
    child: Child,
    stdin: ChildStdin,
    stdout_lines: Receiver<String>,
    state: SessionState,
    last_output: String,
}

/// Events emitted by the PI RPC protocol.
#[derive(Debug, Clone)]
pub enum PiEvent {
    TextDelta { text: String },
    AgentStart,
    AgentDone,
    ToolCall { name: String },
    ToolResult { name: String },
    Error { message: String },
    Unknown,
}

impl PiRpcSession {
    /// Spawn `pi --mode rpc --no-session [--provider p] [--model m] [--thinking off]`.
    pub fn spawn(
        provider: Option<&str>,
        model: Option<&str>,
        cwd: &Path,
    ) -> anyhow::Result<Self> {
        let mut cmd = Command::new("pi");
        cmd.arg("--mode").arg("rpc");
        cmd.arg("--no-session");
        cmd.arg("--thinking").arg("off");
        if let Some(p) = provider {
            cmd.arg("--provider").arg(p);
        }
        if let Some(m) = model {
            cmd.arg("--model").arg(m);
        }
        cmd.current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn pi: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("pi stdin not available"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("pi stdout not available"))?;

        let (tx, rx) = mpsc::channel::<String>();
        thread::spawn(move || {
            use std::io::BufRead as _;
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            stdout_lines: rx,
            state: SessionState::Idle,
            last_output: String::new(),
        })
    }

    /// Send a prompt command via JSONL stdin.
    pub fn prompt(&mut self, text: &str) -> anyhow::Result<()> {
        let msg = serde_json::json!({ "type": "prompt", "message": text });
        writeln!(self.stdin, "{msg}")
            .map_err(|e| anyhow::anyhow!("pi stdin write failed: {e}"))?;
        self.state = SessionState::Working;
        Ok(())
    }

    /// Send a steer command (inject mid-turn).
    pub fn steer(&mut self, text: &str) -> anyhow::Result<()> {
        let msg = serde_json::json!({ "type": "steer", "message": text });
        writeln!(self.stdin, "{msg}")
            .map_err(|e| anyhow::anyhow!("pi stdin write failed: {e}"))?;
        Ok(())
    }

    /// Send abort command.
    pub fn abort(&mut self) -> anyhow::Result<()> {
        let msg = serde_json::json!({ "type": "abort" });
        writeln!(self.stdin, "{msg}")
            .map_err(|e| anyhow::anyhow!("pi stdin write failed: {e}"))?;
        Ok(())
    }

    /// Non-blocking drain of pending stdout lines; parse events and update state.
    pub fn drain_events(&mut self) -> Vec<PiEvent> {
        let mut events = Vec::new();
        loop {
            match self.stdout_lines.try_recv() {
                Ok(line) => {
                    let event = parse_pi_event(&line);
                    match &event {
                        PiEvent::AgentStart | PiEvent::ToolCall { .. } => {
                            self.state = SessionState::Working;
                        }
                        PiEvent::AgentDone => {
                            self.state = SessionState::Idle;
                        }
                        PiEvent::TextDelta { text } => {
                            self.last_output.push_str(text);
                        }
                        _ => {}
                    }
                    events.push(event);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.state = SessionState::Dead;
                    break;
                }
            }
        }
        events
    }

    /// Current session state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Last meaningful text output accumulated from `text_delta` events.
    pub fn last_output(&self) -> &str {
        &self.last_output
    }

    /// Kill the child process.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        self.state = SessionState::Dead;
    }
}

/// Parse a single JSONL line from PI stdout into a `PiEvent`.
fn parse_pi_event(line: &str) -> PiEvent {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
        return PiEvent::Unknown;
    };

    let event_type = val
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match event_type {
        "agent_start" => PiEvent::AgentStart,
        "agent_done" => PiEvent::AgentDone,
        "tool_call" => {
            let name = val
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            PiEvent::ToolCall { name }
        }
        "tool_result" => {
            let name = val
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            PiEvent::ToolResult { name }
        }
        "message_update" => {
            // message_update carries assistantMessageEvent sub-object
            let delta = val
                .get("assistantMessageEvent")
                .and_then(|e| {
                    if e.get("type").and_then(|t| t.as_str()) == Some("text_delta") {
                        e.get("delta").and_then(|d| d.as_str()).map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if delta.is_empty() {
                PiEvent::Unknown
            } else {
                PiEvent::TextDelta { text: delta }
            }
        }
        "error" => {
            let message = val
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
                .to_string();
            PiEvent::Error { message }
        }
        _ => PiEvent::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_agent_start() {
        let ev = parse_pi_event(r#"{"type":"agent_start"}"#);
        assert!(matches!(ev, PiEvent::AgentStart));
    }

    #[test]
    fn test_parse_agent_done() {
        let ev = parse_pi_event(r#"{"type":"agent_done"}"#);
        assert!(matches!(ev, PiEvent::AgentDone));
    }

    #[test]
    fn test_parse_tool_call() {
        let ev = parse_pi_event(r#"{"type":"tool_call","name":"bash"}"#);
        match ev {
            PiEvent::ToolCall { name } => assert_eq!(name, "bash"),
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn test_parse_tool_result() {
        let ev = parse_pi_event(r#"{"type":"tool_result","name":"bash"}"#);
        match ev {
            PiEvent::ToolResult { name } => assert_eq!(name, "bash"),
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn test_parse_text_delta() {
        let ev = parse_pi_event(
            r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"hello"}}"#,
        );
        match ev {
            PiEvent::TextDelta { text } => assert_eq!(text, "hello"),
            _ => panic!("expected TextDelta"),
        }
    }

    #[test]
    fn test_parse_message_update_non_delta_is_unknown() {
        // A message_update with a different event type should yield Unknown
        let ev = parse_pi_event(
            r#"{"type":"message_update","assistantMessageEvent":{"type":"content_block_start"}}"#,
        );
        assert!(matches!(ev, PiEvent::Unknown));
    }

    #[test]
    fn test_parse_error() {
        let ev = parse_pi_event(r#"{"type":"error","message":"rate limited"}"#);
        match ev {
            PiEvent::Error { message } => assert_eq!(message, "rate limited"),
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn test_parse_unknown() {
        let ev = parse_pi_event(r#"{"type":"something_new"}"#);
        assert!(matches!(ev, PiEvent::Unknown));
    }

    #[test]
    fn test_parse_invalid_json_is_unknown() {
        let ev = parse_pi_event("not json at all");
        assert!(matches!(ev, PiEvent::Unknown));
    }
}
