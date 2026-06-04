use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tracing::warn;

use crate::control_loop::{
    VoiceActivityLog, VoiceActivityStatus, publish_activity_log, record_activity,
};
use crate::intent::VoiceAction;
use crate::protocol::Utterance;
use crate::tts::{SpeechSink, SystemTts};

const PI_BIN: &str = "/home/user/.npm-global/bin/pi";
const ORRCH_MCP_BIN: &str = "/home/user/.local/bin/orrch-mcp-server";

const DEFAULT_SYSTEM_PROMPT: &str = r#"You are the conversational orrchestrator admin portal.
You help the user create and submit feedback or implementation instructions, and you can dispatch heavier work to Codex.
Available tools are list_projects, submit_feedback, and dispatch_to_codex.
Before calling submit_feedback or dispatch_to_codex, ask the user for clear confirmation, then act only on the next turn when the user agrees.
Replies are spoken aloud by TTS: keep them concise, conversational, no markdown, no code, one to three short sentences."#;

#[derive(Debug, Clone)]
pub struct PortalConfig {
    pub provider: String,
    pub model: String,
    pub ollama_url: String,
    pub system_prompt: String,
    pub mcp_config_path: PathBuf,
    pub session_path: PathBuf,
}

impl PortalConfig {
    pub fn from_env() -> Result<Self> {
        let data_dir = orrchestrator_data_dir().join("voice-portal");
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("failed to create {}", data_dir.display()))?;

        Ok(Self {
            provider: std::env::var("ORRCH_VOICE_PORTAL_PROVIDER")
                .unwrap_or_else(|_| "local".to_string()),
            model: std::env::var("ORRCH_VOICE_PORTAL_MODEL")
                .unwrap_or_else(|_| "llama3:8b".to_string()),
            ollama_url: std::env::var("ORRCH_VOICE_PORTAL_OLLAMA_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
            system_prompt: std::env::var("ORRCH_VOICE_PORTAL_SYSTEM_PROMPT")
                .unwrap_or_else(|_| DEFAULT_SYSTEM_PROMPT.to_string()),
            mcp_config_path: std::env::var("ORRCH_VOICE_PORTAL_MCP_CONFIG")
                .map(PathBuf::from)
                .unwrap_or_else(|_| data_dir.join("mcp.json")),
            session_path: std::env::var("ORRCH_VOICE_PORTAL_SESSION")
                .map(PathBuf::from)
                .unwrap_or_else(|_| data_dir.join("session.jsonl")),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalTurn {
    pub reply: String,
    pub tool_summaries: Vec<String>,
}

pub trait PortalAgent: Send + Sync {
    fn send_turn(&self, user_text: &str) -> Result<PortalTurn>;
}

pub fn portal_agent_from_env() -> Result<Box<dyn PortalAgent>> {
    portal_agent_from_config(PortalConfig::from_env()?)
}

pub fn portal_agent_from_config(config: PortalConfig) -> Result<Box<dyn PortalAgent>> {
    let provider = config.provider.trim().to_ascii_lowercase();
    if let Some(name) = provider.strip_prefix("connection:") {
        let store = orrch_core::ConnectionStore::load()?;
        let connection = store
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("portal connection '{name}' not found/disabled"))?;
        if !connection.enabled {
            anyhow::bail!("portal connection '{name}' not found/disabled");
        }
        return Ok(Box::new(
            crate::portal_openai_compat::OpenAiCompatPortal::from_connection(connection, config)?,
        ));
    }

    match provider.as_str() {
        "local" | "ollama" => Ok(Box::new(crate::portal_local::OllamaPortal::from_config(
            config,
        )?)),
        "openai-compat" => {
            let store = orrch_core::ConnectionStore::load()?;
            let connection = store
                .list()
                .iter()
                .find(|connection| {
                    connection.enabled
                        && matches!(
                            connection.kind,
                            orrch_core::ConnectionKind::OpenAiCompatible
                        )
                })
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!("portal connection 'openai-compat' not found/disabled")
                })?;
            Ok(Box::new(
                crate::portal_openai_compat::OpenAiCompatPortal::from_connection(
                    connection, config,
                )?,
            ))
        }
        _ => Ok(Box::new(Portal::new(config)?)),
    }
}

pub struct Portal {
    config: PortalConfig,
    rpc: Mutex<Option<RpcProcess>>,
}

impl Portal {
    pub fn from_env() -> Result<Self> {
        Self::new(PortalConfig::from_env()?)
    }

    pub fn new(config: PortalConfig) -> Result<Self> {
        write_mcp_config(&config.mcp_config_path)?;
        Ok(Self {
            config,
            rpc: Mutex::new(None),
        })
    }

    pub fn send(&self, user_text: &str) -> Result<String> {
        Ok(self.send_turn(user_text)?.reply)
    }

    fn ensure_rpc<'a>(&self, guard: &'a mut Option<RpcProcess>) -> Result<&'a mut RpcProcess> {
        let needs_spawn = guard.as_mut().map(|rpc| rpc.child_exited()).unwrap_or(true);
        if needs_spawn {
            *guard = Some(RpcProcess::spawn(&self.config)?);
        }
        Ok(guard.as_mut().expect("rpc process just inserted"))
    }
}

impl PortalAgent for Portal {
    fn send_turn(&self, user_text: &str) -> Result<PortalTurn> {
        let mut guard = self.rpc.lock().unwrap();
        let rpc = self.ensure_rpc(&mut guard)?;
        rpc.prompt(user_text)
    }
}

impl Drop for Portal {
    fn drop(&mut self) {
        if let Some(mut rpc) = self.rpc.lock().ok().and_then(|mut guard| guard.take()) {
            rpc.shutdown();
        }
    }
}

struct RpcProcess {
    child: Child,
    stdin: ChildStdin,
    events: mpsc::Receiver<Value>,
}

impl RpcProcess {
    fn spawn(config: &PortalConfig) -> Result<Self> {
        // pi RPC protocol discovered from the installed pi type definitions and
        // a cheap-model probe: stdin accepts JSONL commands such as
        // {"id":"...","type":"prompt","message":"..."}. stdout emits JSONL
        // command responses plus session events. Assistant text is carried in
        // assistant message events, and the turn is complete on `agent_end`.
        // The process must keep stdin open; EOF requests shutdown.
        write_mcp_config(&config.mcp_config_path)?;
        let mut child = Command::new(PI_BIN)
            .arg("--mode")
            .arg("rpc")
            .arg("--provider")
            .arg(&config.provider)
            .arg("--model")
            .arg(&config.model)
            .arg("--no-session")
            // SAFETY: disable pi's built-in read/bash/edit/write so a voice-driven
            // agent cannot run shell or modify files from a (mis)heard utterance.
            // The portal acts ONLY through the controlled MCP tools (intake/dispatch).
            .arg("--no-tools")
            .arg("--thinking")
            .arg("off")
            .arg("--append-system-prompt")
            .arg(&config.system_prompt)
            .arg("--mcp-config")
            .arg(&config.mcp_config_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to start {PI_BIN} in RPC mode"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("pi RPC stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("pi RPC stdout unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("pi RPC stderr unavailable"))?;

        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("orrch-voice-portal-rpc-out".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
                    match serde_json::from_str::<Value>(&line) {
                        Ok(value) => {
                            let _ = tx.send(value);
                        }
                        Err(err) => warn!("pi RPC emitted non-JSON line: {err}: {line}"),
                    }
                }
            })
            .context("failed to spawn pi RPC stdout reader")?;

        thread::Builder::new()
            .name("orrch-voice-portal-rpc-err".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(|line| line.ok()) {
                    warn!("pi RPC stderr: {line}");
                }
            })
            .context("failed to spawn pi RPC stderr reader")?;

        Ok(Self {
            child,
            stdin,
            events: rx,
        })
    }

    fn child_exited(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }

    fn prompt(&mut self, user_text: &str) -> Result<PortalTurn> {
        let id = format!("voice-{}", now_ms());
        let command = json!({
            "id": id,
            "type": "prompt",
            "message": user_text,
        });
        writeln!(self.stdin, "{command}").context("failed to write pi RPC prompt")?;
        self.stdin
            .flush()
            .context("failed to flush pi RPC prompt")?;

        let mut events = Vec::new();
        let deadline = Duration::from_secs(120);
        loop {
            let event = self
                .events
                .recv_timeout(deadline)
                .context("timed out waiting for pi RPC response")?;

            if is_failed_prompt_response(&event, &id) {
                anyhow::bail!(
                    "pi RPC prompt failed: {}",
                    event
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error")
                );
            }

            let done = event.get("type").and_then(Value::as_str) == Some("agent_end");
            events.push(event);
            if done {
                return extract_turn_from_rpc_events(&events);
            }
        }
    }

    fn shutdown(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn start_portal_loop_from_env(
    receiver: mpsc::Receiver<Utterance>,
) -> Result<thread::JoinHandle<()>> {
    let portal: Arc<dyn PortalAgent> = Arc::from(portal_agent_from_env()?);
    let speaker = Arc::new(SystemTts);
    Ok(start_portal_loop(receiver, portal, speaker))
}

pub fn start_portal_loop(
    receiver: mpsc::Receiver<Utterance>,
    portal: Arc<dyn PortalAgent>,
    speaker: Arc<dyn SpeechSink>,
) -> thread::JoinHandle<()> {
    let activity_log: VoiceActivityLog = Arc::new(Mutex::new(Vec::new()));
    publish_activity_log(activity_log.clone());
    start_portal_loop_with_log(receiver, portal, speaker, activity_log)
}

pub fn start_portal_loop_with_log(
    receiver: mpsc::Receiver<Utterance>,
    portal: Arc<dyn PortalAgent>,
    speaker: Arc<dyn SpeechSink>,
    activity_log: VoiceActivityLog,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("orrch-voice-portal-loop".into())
        .spawn(move || {
            for utterance in receiver {
                handle_portal_utterance(
                    &activity_log,
                    portal.as_ref(),
                    speaker.as_ref(),
                    utterance,
                );
            }
        })
        .expect("failed to spawn orrch-voice-portal-loop")
}

fn handle_portal_utterance(
    activity_log: &VoiceActivityLog,
    portal: &dyn PortalAgent,
    speaker: &dyn SpeechSink,
    utterance: Utterance,
) {
    match portal.send_turn(&utterance.text) {
        Ok(turn) => {
            speaker.speak(&turn.reply);
            let detail = if turn.tool_summaries.is_empty() {
                Some("assistant reply".to_string())
            } else {
                Some(format!("tools: {}", turn.tool_summaries.join("; ")))
            };
            record_activity(
                activity_log,
                utterance.text,
                VoiceAction::Note {
                    text: format!("Assistant: {}", turn.reply),
                },
                VoiceActivityStatus::Dispatched,
                detail,
            );
        }
        Err(err) => {
            record_activity(
                activity_log,
                utterance.text,
                VoiceAction::None,
                VoiceActivityStatus::Error,
                Some(err.to_string()),
            );
        }
    }
}

pub fn extract_turn_from_rpc_events(events: &[Value]) -> Result<PortalTurn> {
    let tool_summaries = events
        .iter()
        .filter_map(tool_summary_from_event)
        .collect::<Vec<_>>();

    let assistant = events
        .iter()
        .rev()
        .find_map(|event| assistant_message_from_event(event))
        .ok_or_else(|| anyhow::anyhow!("pi RPC response did not include an assistant message"))?;

    if let Some(error) = assistant.get("errorMessage").and_then(Value::as_str) {
        anyhow::bail!("pi assistant error: {error}");
    }

    let reply = assistant_text(assistant).trim().to_string();
    if reply.is_empty() {
        anyhow::bail!("pi assistant reply was empty");
    }

    Ok(PortalTurn {
        reply,
        tool_summaries,
    })
}

fn is_failed_prompt_response(event: &Value, id: &str) -> bool {
    event.get("type").and_then(Value::as_str) == Some("response")
        && event.get("id").and_then(Value::as_str) == Some(id)
        && event.get("command").and_then(Value::as_str) == Some("prompt")
        && event.get("success").and_then(Value::as_bool) == Some(false)
}

fn assistant_message_from_event(event: &Value) -> Option<&Value> {
    match event.get("type").and_then(Value::as_str)? {
        "agent_end" => event
            .get("messages")?
            .as_array()?
            .iter()
            .rev()
            .find(|message| message.get("role").and_then(Value::as_str) == Some("assistant")),
        "message_end" | "message_update" | "turn_end" => {
            let message = event.get("message")?;
            (message.get("role").and_then(Value::as_str) == Some("assistant")).then_some(message)
        }
        _ => None,
    }
}

fn assistant_text(message: &Value) -> String {
    message
        .get("content")
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn tool_summary_from_event(event: &Value) -> Option<String> {
    match event.get("type").and_then(Value::as_str)? {
        "tool_execution_start" => {
            let name = event.get("toolName").and_then(Value::as_str)?;
            Some(format!("{name} started"))
        }
        "tool_execution_end" => {
            let name = event.get("toolName").and_then(Value::as_str)?;
            let status = if event.get("isError").and_then(Value::as_bool) == Some(true) {
                "failed"
            } else {
                "completed"
            };
            Some(format!("{name} {status}"))
        }
        _ => None,
    }
}

pub fn write_mcp_config(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let config = json!({
        "mcpServers": {
            "orrchestrator": {
                "command": ORRCH_MCP_BIN,
                "args": [],
                "env": {},
                "lifecycle": "eager",
                "directTools": [
                    "instruction_intake",
                    "inbox_append",
                    "incorporate_inbox",
                    "develop_feature"
                ]
            }
        }
    });
    std::fs::write(path, serde_json::to_vec_pretty(&config)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn orrchestrator_data_dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("orrchestrator")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
#[derive(Debug)]
pub struct MockPortal {
    replies: Mutex<Vec<PortalTurn>>,
}

#[cfg(test)]
impl MockPortal {
    pub fn new(replies: Vec<PortalTurn>) -> Self {
        Self {
            replies: Mutex::new(replies),
        }
    }
}

#[cfg(test)]
impl PortalAgent for MockPortal {
    fn send_turn(&self, _user_text: &str) -> Result<PortalTurn> {
        let mut replies = self.replies.lock().unwrap();
        if replies.is_empty() {
            anyhow::bail!("mock portal exhausted");
        }
        Ok(replies.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::MockSpeechSink;

    #[test]
    fn extracts_reply_and_tool_summaries_from_rpc_events() {
        let events = vec![
            json!({"type":"tool_execution_start","toolName":"mcp__orrchestrator__develop_feature","args":{}}),
            json!({"type":"tool_execution_end","toolName":"mcp__orrchestrator__develop_feature","isError":false,"result":"ok"}),
            json!({
                "type":"agent_end",
                "messages":[
                    {"role":"user","content":[{"type":"text","text":"hello"}]},
                    {"role":"assistant","content":[{"type":"text","text":"Ready when you are."}]}
                ]
            }),
        ];

        let turn = extract_turn_from_rpc_events(&events).unwrap();

        assert_eq!(turn.reply, "Ready when you are.");
        assert_eq!(
            turn.tool_summaries,
            vec![
                "mcp__orrchestrator__develop_feature started",
                "mcp__orrchestrator__develop_feature completed"
            ]
        );
    }

    #[test]
    fn portal_loop_records_reply_and_invokes_tts() {
        let (tx, rx) = mpsc::channel();
        let portal = Arc::new(MockPortal::new(vec![PortalTurn {
            reply: "Hello from the portal.".to_string(),
            tool_summaries: Vec::new(),
        }]));
        let speaker = Arc::new(MockSpeechSink::default());
        let speaker_read = speaker.clone();

        let activity_log: VoiceActivityLog = Arc::new(Mutex::new(Vec::new()));
        let handle = start_portal_loop_with_log(rx, portal, speaker, activity_log.clone());
        tx.send(Utterance {
            text: "say hello".to_string(),
            ts_ms: 1,
        })
        .unwrap();
        drop(tx);
        handle.join().unwrap();

        assert_eq!(speaker_read.spoken(), vec!["Hello from the portal."]);
        let activities = activity_log.lock().unwrap().clone();
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].utterance, "say hello");
        assert_eq!(activities[0].status, VoiceActivityStatus::Dispatched);
        assert_eq!(
            activities[0].action,
            VoiceAction::Note {
                text: "Assistant: Hello from the portal.".to_string()
            }
        );
    }

    #[test]
    fn writes_pi_mcp_config_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        write_mcp_config(&path).unwrap();
        let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();

        assert_eq!(
            value["mcpServers"]["orrchestrator"]["command"],
            ORRCH_MCP_BIN
        );
    }
}
