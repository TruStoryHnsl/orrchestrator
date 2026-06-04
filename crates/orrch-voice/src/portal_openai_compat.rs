use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::portal::{PortalAgent, PortalConfig, PortalTurn};
use crate::portal_local::{
    ChatMessage, DEFAULT_DISPATCH_CAP, LOCAL_SYSTEM_PROMPT, MAX_TOOL_ROUNDS, OllamaToolCall,
    PortalTools, RealPortalTools, execute_portal_tool, portal_tool_schema,
};

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<Value>,
    pub stream: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChatResponse {
    pub choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChoice {
    pub message: OpenAiResponseMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiResponseMessage {
    pub role: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<OllamaToolCall>,
}

impl OpenAiResponseMessage {
    fn into_chat_message(self) -> ChatMessage {
        ChatMessage {
            role: self.role,
            content: self.content.unwrap_or_default(),
            tool_calls: self.tool_calls,
            name: None,
            tool_call_id: None,
        }
    }
}

pub trait OpenAiCompatChatClient: Send + Sync {
    fn chat(&self, request: OpenAiChatRequest) -> Result<OpenAiChatResponse>;
}

pub struct OpenAiCompatPortal {
    model: String,
    messages: Mutex<Vec<ChatMessage>>,
    chat_client: Arc<dyn OpenAiCompatChatClient>,
    tools: Arc<dyn PortalTools>,
    dispatch_count: Mutex<usize>,
    dispatch_cap: usize,
    last_call: Mutex<Option<Instant>>,
    min_interval: Option<Duration>,
}

impl OpenAiCompatPortal {
    pub fn from_connection(
        connection: orrch_core::Connection,
        config: PortalConfig,
    ) -> Result<Self> {
        let model = std::env::var("ORRCH_VOICE_PORTAL_MODEL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(connection.default_model);
        let system_prompt = if config.system_prompt.trim().is_empty() {
            LOCAL_SYSTEM_PROMPT.to_string()
        } else {
            config.system_prompt
        };
        Self::with_real_tools(
            model,
            connection.base_url,
            connection.api_key,
            connection.rate_limit_rpm,
            system_prompt,
        )
    }

    pub fn with_real_tools(
        model: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        rate_limit_rpm: u32,
        system_prompt: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self::new(
            model,
            system_prompt,
            Arc::new(ReqwestOpenAiCompatClient::new(base_url, api_key)?),
            Arc::new(RealPortalTools::from_env()),
            rate_limit_rpm,
        ))
    }

    pub fn new(
        model: impl Into<String>,
        system_prompt: impl Into<String>,
        chat_client: Arc<dyn OpenAiCompatChatClient>,
        tools: Arc<dyn PortalTools>,
        rate_limit_rpm: u32,
    ) -> Self {
        Self::new_with_dispatch_cap(
            model,
            system_prompt,
            chat_client,
            tools,
            rate_limit_rpm,
            DEFAULT_DISPATCH_CAP,
        )
    }

    pub fn new_with_dispatch_cap(
        model: impl Into<String>,
        system_prompt: impl Into<String>,
        chat_client: Arc<dyn OpenAiCompatChatClient>,
        tools: Arc<dyn PortalTools>,
        rate_limit_rpm: u32,
        dispatch_cap: usize,
    ) -> Self {
        let min_interval = if rate_limit_rpm == 0 {
            None
        } else {
            Some(Duration::from_millis(60_000 / u64::from(rate_limit_rpm)))
        };
        Self {
            model: model.into(),
            messages: Mutex::new(vec![ChatMessage::system(system_prompt)]),
            chat_client,
            tools,
            dispatch_count: Mutex::new(0),
            dispatch_cap,
            last_call: Mutex::new(None),
            min_interval,
        }
    }

    fn throttle(&self) {
        let Some(min_interval) = self.min_interval else {
            return;
        };
        let mut last_call = self.last_call.lock().unwrap();
        if let Some(last) = *last_call {
            let elapsed = last.elapsed();
            if elapsed < min_interval {
                std::thread::sleep(min_interval - elapsed);
            }
        }
        *last_call = Some(Instant::now());
    }
}

impl PortalAgent for OpenAiCompatPortal {
    fn send_turn(&self, user_text: &str) -> Result<PortalTurn> {
        let mut messages = self.messages.lock().unwrap();
        messages.push(ChatMessage::user(user_text));
        let mut tool_summaries = Vec::new();

        for _ in 0..=MAX_TOOL_ROUNDS {
            self.throttle();
            let response = self.chat_client.chat(OpenAiChatRequest {
                model: self.model.clone(),
                messages: messages.clone(),
                tools: portal_tool_schema(),
                stream: false,
            })?;
            let assistant = response
                .choices
                .into_iter()
                .next()
                .map(|choice| choice.message.into_chat_message())
                .ok_or_else(|| anyhow::anyhow!("OpenAI-compatible response had no choices"))?;

            if assistant.tool_calls.is_empty() {
                let reply = assistant.content.trim().to_string();
                if reply.is_empty() {
                    anyhow::bail!("OpenAI-compatible assistant reply was empty");
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
                let result = execute_portal_tool(
                    self.tools.as_ref(),
                    &self.dispatch_count,
                    self.dispatch_cap,
                    &call.function.name,
                    &call.function.arguments,
                    user_text,
                )?;
                tool_summaries.push(format!("{}: {}", call.function.name, result));
                let tool_call_id = call
                    .id
                    .clone()
                    .unwrap_or_else(|| call.function.name.clone());
                messages.push(ChatMessage::tool_with_id(
                    &call.function.name,
                    result,
                    tool_call_id,
                ));
            }
        }

        anyhow::bail!("OpenAI-compatible portal exceeded maximum tool rounds");
    }
}

struct ReqwestOpenAiCompatClient {
    base_url: String,
    api_key: String,
    client: reqwest::blocking::Client,
}

impl ReqwestOpenAiCompatClient {
    fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .context("failed to build OpenAI-compatible HTTP client")?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            client,
        })
    }
}

impl OpenAiCompatChatClient for ReqwestOpenAiCompatClient {
    fn chat(&self, request: OpenAiChatRequest) -> Result<OpenAiChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut builder = self.client.post(&url).json(&request);
        if !self.api_key.trim().is_empty() {
            builder = builder.bearer_auth(self.api_key.trim());
        }
        let response = builder
            .send()
            .with_context(|| format!("failed to call OpenAI-compatible endpoint at {url}"))?;
        let status = response.status();
        if status.is_success() {
            return response
                .json::<OpenAiChatResponse>()
                .context("failed to decode OpenAI-compatible chat response");
        }
        let body = response
            .text()
            .unwrap_or_else(|err| format!("failed to read error body: {err}"));
        anyhow::bail!("OpenAI-compatible endpoint returned {status}: {body}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portal_local::PortalTools;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[derive(Default)]
    struct MockTools {
        list_projects_calls: Mutex<usize>,
    }

    impl PortalTools for MockTools {
        fn list_projects(&self) -> Result<Vec<String>> {
            *self.list_projects_calls.lock().unwrap() += 1;
            Ok(vec!["orrchestrator".to_string()])
        }

        fn submit_feedback(&self, _project: &str, _text: &str) -> Result<String> {
            Ok("Submitted.".to_string())
        }

        fn dispatch_to_codex(&self, _project: &str, _goal: &str) -> Result<String> {
            Ok("Dispatched.".to_string())
        }
    }

    struct MockServer {
        base_url: String,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl MockServer {
        fn new(responses: Vec<String>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let handle = thread::spawn(move || {
                for response in responses {
                    let Ok((mut stream, _)) = listener.accept() else {
                        break;
                    };
                    let mut buf = [0_u8; 8192];
                    let _ = stream.read(&mut buf);
                    let http = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        response.len(),
                        response
                    );
                    let _ = stream.write_all(http.as_bytes());
                }
            });
            Self {
                base_url,
                handle: Some(handle),
            }
        }
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    #[test]
    fn openai_compat_plain_completion_returns_reply() {
        let server = MockServer::new(vec![
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Hello from the portal."
                    }
                }]
            })
            .to_string(),
        ]);
        let portal = OpenAiCompatPortal::new(
            "test-model",
            LOCAL_SYSTEM_PROMPT,
            Arc::new(ReqwestOpenAiCompatClient::new(server.base_url.clone(), "").unwrap()),
            Arc::new(MockTools::default()),
            0,
        );

        let turn = portal.send_turn("hello").unwrap();

        assert_eq!(turn.reply, "Hello from the portal.");
        assert!(turn.tool_summaries.is_empty());
    }

    #[test]
    fn openai_compat_tool_call_invokes_tool_executor() {
        let server = MockServer::new(vec![
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "list_projects",
                                "arguments": "{}"
                            }
                        }]
                    }
                }]
            })
            .to_string(),
            json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "I found one project."
                    }
                }]
            })
            .to_string(),
        ]);
        let tools = Arc::new(MockTools::default());
        let portal = OpenAiCompatPortal::new(
            "test-model",
            LOCAL_SYSTEM_PROMPT,
            Arc::new(ReqwestOpenAiCompatClient::new(server.base_url.clone(), "sk-test").unwrap()),
            tools.clone(),
            0,
        );

        let turn = portal.send_turn("list projects").unwrap();

        assert_eq!(turn.reply, "I found one project.");
        assert_eq!(*tools.list_projects_calls.lock().unwrap(), 1);
        assert_eq!(turn.tool_summaries.len(), 1);
        assert!(turn.tool_summaries[0].contains("Projects: orrchestrator"));
    }
}
