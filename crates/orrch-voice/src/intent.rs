use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum VoiceAction {
    Dispatch {
        project: String,
        instruction: String,
    },
    SpawnSession {
        project: String,
        goal: String,
    },
    Note {
        text: String,
    },
    Confirm,
    Cancel,
    None,
}

pub trait VoiceInterpreter: Send + Sync {
    fn interpret(&self, utterance: &str, recent_context: &[String]) -> Result<VoiceAction>;
}

#[derive(Debug, Clone)]
pub struct OllamaInterpreter {
    pub model: String,
    pub base_url: String,
}

impl OllamaInterpreter {
    pub fn from_env() -> Self {
        Self {
            model: std::env::var("ORRCH_VOICE_LOOP_MODEL")
                .unwrap_or_else(|_| "llama3.2".to_string()),
            base_url: std::env::var("ORRCH_VOICE_LOOP_OLLAMA_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.base_url.trim_end_matches('/'))
    }
}

impl VoiceInterpreter for OllamaInterpreter {
    fn interpret(&self, utterance: &str, recent_context: &[String]) -> Result<VoiceAction> {
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt()
                },
                {
                    "role": "user",
                    "content": serde_json::json!({
                        "utterance": utterance,
                        "recent_context": recent_context,
                    }).to_string()
                }
            ],
            "format": "json"
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("failed to build Ollama HTTP client")?;
        let response = client
            .post(self.chat_url())
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .context("Ollama intent request failed")?;
        let status = response.status();
        let json: serde_json::Value = response
            .json()
            .context("Ollama intent response was not JSON")?;
        if !status.is_success() {
            anyhow::bail!("Ollama intent error {status}: {json}");
        }
        let content = json
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .ok_or_else(|| anyhow::anyhow!("Ollama response missing message.content"))?;

        parse_action_json(content)
    }
}

fn parse_action_json(content: &str) -> Result<VoiceAction> {
    let trimmed = content.trim();
    match serde_json::from_str::<VoiceAction>(trimmed) {
        Ok(action) => Ok(action),
        Err(_) => {
            let start = trimmed
                .find('{')
                .ok_or_else(|| anyhow::anyhow!("intent response did not contain JSON"))?;
            let end = trimmed
                .rfind('}')
                .ok_or_else(|| anyhow::anyhow!("intent response did not contain JSON"))?;
            serde_json::from_str::<VoiceAction>(&trimmed[start..=end])
                .context("failed to parse voice action JSON")
        }
    }
}

fn system_prompt() -> &'static str {
    r#"You are the local voice intent interpreter for orrchestrator.
Classify the user's speech into exactly one action and return only strict JSON.
Never invent project names or instructions. If the utterance is ambiguous, return {"action":"none"}.
Heavy actions must be classified but are not executed by you.

Allowed JSON:
{"action":"dispatch","project":"project-slug","instruction":"clear implementation instruction"}
{"action":"spawn_session","project":"project-slug","goal":"session goal"}
{"action":"note","text":"short note"}
{"action":"confirm"}
{"action":"cancel"}
{"action":"none"}

Use "confirm" only for explicit approvals such as yes, confirm, do it, proceed.
Use "cancel" only for explicit rejection or stop commands.
Use "dispatch" when the user asks to route implementation work to a project inbox.
Use "spawn_session" only when the user explicitly asks to start a background work session.
"#
}

#[cfg(test)]
#[derive(Debug)]
pub struct MockInterpreter {
    actions: std::sync::Mutex<std::collections::VecDeque<VoiceAction>>,
}

#[cfg(test)]
impl MockInterpreter {
    pub fn new(actions: Vec<VoiceAction>) -> Self {
        Self {
            actions: std::sync::Mutex::new(actions.into()),
        }
    }
}

#[cfg(test)]
impl VoiceInterpreter for MockInterpreter {
    fn interpret(&self, _utterance: &str, _recent_context: &[String]) -> Result<VoiceAction> {
        Ok(self
            .actions
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(VoiceAction::None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_schema_round_trips_tagged_json() {
        let action = VoiceAction::Dispatch {
            project: "orrchestrator".to_string(),
            instruction: "Fix responsive tabs.".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(
            json,
            r#"{"action":"dispatch","project":"orrchestrator","instruction":"Fix responsive tabs."}"#
        );
        assert_eq!(serde_json::from_str::<VoiceAction>(&json).unwrap(), action);
    }

    #[test]
    fn parses_json_embedded_in_model_text() {
        let action = parse_action_json("```json\n{\"action\":\"confirm\"}\n```").unwrap();
        assert_eq!(action, VoiceAction::Confirm);
    }
}
