use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::portal::{PortalAgent, PortalConfig, PortalTurn};

pub(crate) const MAX_TOOL_ROUNDS: usize = 4;
pub(crate) const DEFAULT_DISPATCH_CAP: usize = 3;

pub(crate) const LOCAL_SYSTEM_PROMPT: &str = r#"You are the conversational orrchestrator admin portal.
You help the user create and submit feedback or implementation instructions, and you can dispatch heavier work to Codex.
Available tools are list_projects, submit_feedback, and dispatch_to_codex.
Before calling submit_feedback or dispatch_to_codex, ask the user for clear confirmation, then act only on the next turn when the user agrees.
Replies are spoken aloud by TTS: keep them concise, conversational, no markdown, no code, one to three short sentences."#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<OllamaToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub(crate) fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            name: None,
            tool_call_id: None,
        }
    }

    pub(crate) fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            name: None,
            tool_call_id: None,
        }
    }

    fn tool(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            name: Some(name.into()),
            tool_call_id: None,
        }
    }

    pub(crate) fn tool_with_id(
        name: impl Into<String>,
        content: impl Into<String>,
        tool_call_id: impl Into<String>,
    ) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            name: Some(name.into()),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<Value>,
    pub stream: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaChatResponse {
    pub message: ChatMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub function: OllamaToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaToolFunction {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

pub trait OllamaChatClient: Send + Sync {
    fn chat(&self, request: OllamaChatRequest) -> Result<OllamaChatResponse>;
}

pub trait PortalTools: Send + Sync {
    fn list_projects(&self) -> Result<Vec<String>>;
    fn submit_feedback(&self, project: &str, text: &str) -> Result<String>;
    fn dispatch_to_codex(&self, project: &str, goal: &str) -> Result<String>;
}

pub struct OllamaPortal {
    model: String,
    messages: Mutex<Vec<ChatMessage>>,
    chat_client: Arc<dyn OllamaChatClient>,
    tools: Arc<dyn PortalTools>,
    dispatch_count: Mutex<usize>,
    dispatch_cap: usize,
}

impl OllamaPortal {
    pub fn from_config(config: PortalConfig) -> Result<Self> {
        Self::with_real_tools(
            config.model,
            config.ollama_url,
            if config.system_prompt.trim().is_empty() {
                LOCAL_SYSTEM_PROMPT.to_string()
            } else {
                config.system_prompt
            },
        )
    }

    pub fn from_ollama_url(
        model: impl Into<String>,
        ollama_url: impl Into<String>,
    ) -> Result<Self> {
        Self::with_real_tools(
            model.into(),
            ollama_url.into(),
            LOCAL_SYSTEM_PROMPT.to_string(),
        )
    }

    pub fn new(
        model: impl Into<String>,
        system_prompt: impl Into<String>,
        chat_client: Arc<dyn OllamaChatClient>,
        tools: Arc<dyn PortalTools>,
    ) -> Self {
        Self::new_with_dispatch_cap(
            model,
            system_prompt,
            chat_client,
            tools,
            DEFAULT_DISPATCH_CAP,
        )
    }

    pub fn new_with_dispatch_cap(
        model: impl Into<String>,
        system_prompt: impl Into<String>,
        chat_client: Arc<dyn OllamaChatClient>,
        tools: Arc<dyn PortalTools>,
        dispatch_cap: usize,
    ) -> Self {
        Self {
            model: model.into(),
            messages: Mutex::new(vec![ChatMessage::system(system_prompt)]),
            chat_client,
            tools,
            dispatch_count: Mutex::new(0),
            dispatch_cap,
        }
    }

    fn with_real_tools(model: String, ollama_url: String, system_prompt: String) -> Result<Self> {
        Ok(Self::new(
            model,
            system_prompt,
            Arc::new(ReqwestOllamaClient::new(ollama_url)?),
            Arc::new(RealPortalTools::from_env()),
        ))
    }

    fn execute_tool(&self, call: &OllamaToolCall, user_text: &str) -> Result<String> {
        execute_portal_tool(
            self.tools.as_ref(),
            &self.dispatch_count,
            self.dispatch_cap,
            &call.function.name,
            &call.function.arguments,
            user_text,
        )
    }
}

impl PortalAgent for OllamaPortal {
    fn send_turn(&self, user_text: &str) -> Result<PortalTurn> {
        let mut messages = self.messages.lock().unwrap();
        messages.push(ChatMessage::user(user_text));
        let mut tool_summaries = Vec::new();

        for _ in 0..=MAX_TOOL_ROUNDS {
            let response = self.chat_client.chat(OllamaChatRequest {
                model: self.model.clone(),
                messages: messages.clone(),
                tools: portal_tool_schema(),
                stream: false,
            })?;

            let assistant = response.message;
            if assistant.tool_calls.is_empty() {
                let reply = assistant.content.trim().to_string();
                if reply.is_empty() {
                    anyhow::bail!("Ollama assistant reply was empty");
                }
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: reply.clone(),
                    tool_calls: Vec::new(),
                    name: None,
                    tool_call_id: None,
                });
                return Ok(PortalTurn {
                    reply,
                    tool_summaries,
                });
            }

            let tool_calls = assistant.tool_calls.clone();
            messages.push(assistant);
            for call in &tool_calls {
                let result = self.execute_tool(call, user_text)?;
                tool_summaries.push(format!("{}: {}", call.function.name, result));
                messages.push(ChatMessage::tool(&call.function.name, result));
            }
        }

        anyhow::bail!("Ollama portal exceeded maximum tool rounds");
    }
}

struct ReqwestOllamaClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl ReqwestOllamaClient {
    fn new(base_url: String) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .context("failed to build Ollama HTTP client")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        })
    }
}

impl OllamaChatClient for ReqwestOllamaClient {
    fn chat(&self, mut request: OllamaChatRequest) -> Result<OllamaChatResponse> {
        let response = self.post_chat(&request)?;
        let status = response.status();
        if status.is_success() {
            return response
                .json::<OllamaChatResponse>()
                .context("failed to decode Ollama chat response");
        }

        let body = response
            .text()
            .unwrap_or_else(|err| format!("failed to read error body: {err}"));
        if status.as_u16() == 400
            && !request.tools.is_empty()
            && body.contains("does not support tools")
        {
            warn!(
                model = %request.model,
                "Ollama model does not support tools; retrying this turn without tools"
            );
            request.tools.clear();
            let response = self.post_chat(&request)?;
            let status = response.status();
            if status.is_success() {
                return response
                    .json::<OllamaChatResponse>()
                    .context("failed to decode Ollama chat response");
            }
            let body = response
                .text()
                .unwrap_or_else(|err| format!("failed to read error body: {err}"));
            anyhow::bail!("Ollama returned {status} for fallback chat: {body}");
        }

        anyhow::bail!("Ollama returned {status}: {body}");
    }
}

impl ReqwestOllamaClient {
    fn post_chat(&self, request: &OllamaChatRequest) -> Result<reqwest::blocking::Response> {
        let url = format!("{}/api/chat", self.base_url);
        self.client
            .post(&url)
            .json(&request)
            .send()
            .with_context(|| format!("failed to call Ollama at {url}"))
    }
}

pub(crate) struct RealPortalTools {
    projects_dir: PathBuf,
    process_manager: Arc<Mutex<orrch_core::ProcessManager>>,
}

impl RealPortalTools {
    pub(crate) fn from_env() -> Self {
        let config = orrch_core::Config::load();
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            projects_dir: config.projects_dir,
            process_manager: Arc::new(Mutex::new(orrch_core::ProcessManager::new(event_tx))),
        }
    }

    fn project_dir(&self, project: &str) -> Result<PathBuf> {
        if project.contains('/') || project.contains('\\') || project.starts_with('.') {
            anyhow::bail!("project must be a project name, not a path");
        }

        let project_names = orrch_core::load_projects(&self.projects_dir)
            .into_iter()
            .map(|project| project.name)
            .collect::<Vec<_>>();

        if !project_names.iter().any(|name| name == project) {
            anyhow::bail!("project '{project}' not found");
        }

        let project_dir = self.projects_dir.join(project);
        if !project_dir.is_dir() {
            anyhow::bail!("project directory '{}' not found", project_dir.display());
        }
        Ok(project_dir)
    }
}

impl PortalTools for RealPortalTools {
    fn list_projects(&self) -> Result<Vec<String>> {
        Ok(orrch_core::load_projects(&self.projects_dir)
            .into_iter()
            .map(|project| project.name)
            .collect())
    }

    fn submit_feedback(&self, project: &str, text: &str) -> Result<String> {
        let project_dir = self.project_dir(project)?;
        let timestamp = orrch_core::feedback::chrono_lite_timestamp();
        orrch_core::feedback::append_to_inbox_direct(text, &project_dir, &timestamp)
            .with_context(|| format!("failed to append feedback to {}", project_dir.display()))?;
        Ok(format!("Submitted feedback to {project}."))
    }

    fn dispatch_to_codex(&self, project: &str, goal: &str) -> Result<String> {
        let project_dir = self.project_dir(project)?;
        info!(
            project = project,
            project_dir = %project_dir.display(),
            goal = goal,
            "voice portal dispatching Codex session"
        );
        let sid = self
            .process_manager
            .lock()
            .unwrap()
            .spawn(
                &project_dir,
                orrch_core::BackendKind::Codex,
                Some(goal),
                40,
                120,
            )
            .with_context(|| format!("failed to dispatch Codex in {}", project_dir.display()))?;
        Ok(format!("Dispatched Codex session {sid} for {project}."))
    }
}

pub(crate) fn portal_tool_schema() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "list_projects",
                "description": "List known orrchestrator project names.",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "submit_feedback",
                "description": "Append confirmed feedback or instructions to a project's instructions_inbox.md.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "project": { "type": "string", "description": "Exact project name." },
                        "text": { "type": "string", "description": "Feedback or instruction text to submit." }
                    },
                    "required": ["project", "text"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "dispatch_to_codex",
                "description": "Spawn a confirmed Codex coding session for heavier work.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "project": { "type": "string", "description": "Exact project name." },
                        "goal": { "type": "string", "description": "Concrete Codex session goal." }
                    },
                    "required": ["project", "goal"]
                }
            }
        }),
    ]
}

pub(crate) fn execute_portal_tool(
    tools: &dyn PortalTools,
    dispatch_count: &Mutex<usize>,
    dispatch_cap: usize,
    name: &str,
    arguments: &Value,
    user_text: &str,
) -> Result<String> {
    let args = normalized_arguments(arguments)?;
    match name {
        "list_projects" => {
            let projects = tools.list_projects()?;
            Ok(if projects.is_empty() {
                "No projects found.".to_string()
            } else {
                format!("Projects: {}", projects.join(", "))
            })
        }
        "submit_feedback" => {
            if !looks_like_confirmation(user_text) {
                return Ok("Confirmation required before submit_feedback. Ask the user to confirm, then call the tool on the next turn.".to_string());
            }
            let project = required_arg(&args, "project")?;
            let text = required_arg(&args, "text")?;
            tools.submit_feedback(project, text)
        }
        "dispatch_to_codex" => {
            if !looks_like_confirmation(user_text) {
                return Ok("Confirmation required before dispatch_to_codex. Ask the user to confirm, then call the tool on the next turn.".to_string());
            }
            {
                let mut count = dispatch_count.lock().unwrap();
                if *count >= dispatch_cap {
                    return Ok(format!(
                        "Dispatch cap reached for this conversation ({}/{}).",
                        *count, dispatch_cap
                    ));
                }
                *count += 1;
            }
            let project = required_arg(&args, "project")?;
            let goal = required_arg(&args, "goal")?;
            tools.dispatch_to_codex(project, goal)
        }
        other => Ok(format!("Unknown portal tool: {other}")),
    }
}

pub(crate) fn normalized_arguments(arguments: &Value) -> Result<Value> {
    match arguments {
        Value::String(raw) => serde_json::from_str(raw)
            .with_context(|| format!("tool arguments were not valid JSON: {raw}")),
        Value::Object(_) => Ok(arguments.clone()),
        Value::Null => Ok(json!({})),
        other => anyhow::bail!("tool arguments must be an object, got {other}"),
    }
}

pub(crate) fn required_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required tool argument '{name}'"))
}

pub(crate) fn looks_like_confirmation(text: &str) -> bool {
    let normalized = text
        .trim()
        .to_ascii_lowercase()
        .trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_string();
    matches!(
        normalized.as_str(),
        "yes"
            | "y"
            | "yeah"
            | "yep"
            | "ok"
            | "okay"
            | "confirm"
            | "confirmed"
            | "approve"
            | "approved"
            | "do it"
            | "go ahead"
            | "submit it"
            | "dispatch it"
            | "send it"
    ) || normalized.starts_with("yes ")
        || normalized.starts_with("okay ")
        || normalized.starts_with("ok ")
        || normalized.starts_with("confirmed ")
        || normalized.starts_with("go ahead")
        || normalized.starts_with("do it")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct MockChat {
        responses: Mutex<VecDeque<OllamaChatResponse>>,
    }

    impl MockChat {
        fn new(responses: Vec<OllamaChatResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
            }
        }
    }

    impl OllamaChatClient for MockChat {
        fn chat(&self, _request: OllamaChatRequest) -> Result<OllamaChatResponse> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("mock chat exhausted"))
        }
    }

    #[derive(Default)]
    struct MockTools {
        list_projects_calls: Mutex<usize>,
        submit_feedback_calls: Mutex<Vec<(String, String)>>,
        dispatch_calls: Mutex<Vec<(String, String)>>,
    }

    impl PortalTools for MockTools {
        fn list_projects(&self) -> Result<Vec<String>> {
            *self.list_projects_calls.lock().unwrap() += 1;
            Ok(vec!["orrchestrator".to_string()])
        }

        fn submit_feedback(&self, project: &str, text: &str) -> Result<String> {
            self.submit_feedback_calls
                .lock()
                .unwrap()
                .push((project.to_string(), text.to_string()));
            Ok(format!("Submitted feedback to {project}."))
        }

        fn dispatch_to_codex(&self, project: &str, goal: &str) -> Result<String> {
            self.dispatch_calls
                .lock()
                .unwrap()
                .push((project.to_string(), goal.to_string()));
            Ok("Dispatched Codex session s1.".to_string())
        }
    }

    fn assistant(content: &str) -> OllamaChatResponse {
        OllamaChatResponse {
            message: ChatMessage {
                role: "assistant".to_string(),
                content: content.to_string(),
                tool_calls: Vec::new(),
                name: None,
                tool_call_id: None,
            },
        }
    }

    fn tool_response(name: &str, arguments: Value) -> OllamaChatResponse {
        OllamaChatResponse {
            message: ChatMessage {
                role: "assistant".to_string(),
                content: String::new(),
                tool_calls: vec![OllamaToolCall {
                    id: None,
                    kind: Some("function".to_string()),
                    function: OllamaToolFunction {
                        name: name.to_string(),
                        arguments,
                    },
                }],
                name: None,
                tool_call_id: None,
            },
        }
    }

    #[test]
    fn plain_reply_returns_assistant_content() {
        let portal = OllamaPortal::new(
            "llama3:8b",
            LOCAL_SYSTEM_PROMPT,
            Arc::new(MockChat::new(vec![assistant("Hello there.")])),
            Arc::new(MockTools::default()),
        );

        let turn = portal.send_turn("Say hello.").unwrap();

        assert_eq!(turn.reply, "Hello there.");
        assert!(turn.tool_summaries.is_empty());
    }

    #[test]
    fn submit_feedback_tool_call_executes_after_confirmation() {
        let tools = Arc::new(MockTools::default());
        let portal = OllamaPortal::new(
            "llama3:8b",
            LOCAL_SYSTEM_PROMPT,
            Arc::new(MockChat::new(vec![
                tool_response(
                    "submit_feedback",
                    json!({"project":"orrchestrator","text":"Fix the tabs."}),
                ),
                assistant("Submitted."),
            ])),
            tools.clone(),
        );

        let turn = portal.send_turn("yes").unwrap();

        assert_eq!(turn.reply, "Submitted.");
        assert_eq!(
            *tools.submit_feedback_calls.lock().unwrap(),
            vec![("orrchestrator".to_string(), "Fix the tabs.".to_string())]
        );
        assert_eq!(turn.tool_summaries.len(), 1);
    }

    #[test]
    fn submit_feedback_tool_call_is_blocked_without_confirmation() {
        let tools = Arc::new(MockTools::default());
        let portal = OllamaPortal::new(
            "llama3:8b",
            LOCAL_SYSTEM_PROMPT,
            Arc::new(MockChat::new(vec![
                tool_response(
                    "submit_feedback",
                    json!({"project":"orrchestrator","text":"Fix the tabs."}),
                ),
                assistant("Please confirm first."),
            ])),
            tools.clone(),
        );

        let turn = portal.send_turn("Add this feedback.").unwrap();

        assert_eq!(turn.reply, "Please confirm first.");
        assert!(tools.submit_feedback_calls.lock().unwrap().is_empty());
        assert!(turn.tool_summaries[0].contains("Confirmation required"));
    }

    #[test]
    fn dispatch_cap_is_enforced() {
        let tools = Arc::new(MockTools::default());
        let portal = OllamaPortal::new_with_dispatch_cap(
            "llama3:8b",
            LOCAL_SYSTEM_PROMPT,
            Arc::new(MockChat::new(vec![
                OllamaChatResponse {
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content: String::new(),
                        tool_calls: vec![
                            OllamaToolCall {
                                id: None,
                                kind: Some("function".to_string()),
                                function: OllamaToolFunction {
                                    name: "dispatch_to_codex".to_string(),
                                    arguments: json!({"project":"orrchestrator","goal":"Fix bug one."}),
                                },
                            },
                            OllamaToolCall {
                                id: None,
                                kind: Some("function".to_string()),
                                function: OllamaToolFunction {
                                    name: "dispatch_to_codex".to_string(),
                                    arguments: json!({"project":"orrchestrator","goal":"Fix bug two."}),
                                },
                            },
                        ],
                        name: None,
                        tool_call_id: None,
                    },
                },
                assistant("Done."),
            ])),
            tools.clone(),
            1,
        );

        let turn = portal.send_turn("yes").unwrap();

        assert_eq!(turn.reply, "Done.");
        assert_eq!(tools.dispatch_calls.lock().unwrap().len(), 1);
        assert!(turn.tool_summaries[1].contains("Dispatch cap reached"));
    }
}
