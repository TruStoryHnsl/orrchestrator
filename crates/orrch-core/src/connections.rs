use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::config_dir;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionKind {
    OpenAiCompatible,
    Anthropic,
    Ollama,
}

impl ConnectionKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai-compatible",
            Self::Anthropic => "anthropic",
            Self::Ollama => "ollama",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub name: String,
    pub kind: ConnectionKind,
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub rate_limit_rpm: u32,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionStore {
    pub connections: Vec<Connection>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl ConnectionStore {
    pub fn load() -> Result<Self> {
        Self::load_from_path(connections_path())
    }

    pub fn load_from_path(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self {
                connections: Vec::new(),
                path: Some(path),
            });
        }

        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut store: Self = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        store.path = Some(path);
        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let path = self.path.clone().unwrap_or_else(connections_path);
        self.save_to_path(path)
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self).context("failed to serialize connections")?;
        std::fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn add(&mut self, connection: Connection) -> Result<()> {
        if self.get(&connection.name).is_some() {
            anyhow::bail!("connection '{}' already exists", connection.name);
        }
        self.connections.push(connection);
        self.connections.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }

    pub fn update(&mut self, original_name: &str, connection: Connection) -> Result<()> {
        if original_name != connection.name && self.get(&connection.name).is_some() {
            anyhow::bail!("connection '{}' already exists", connection.name);
        }
        let Some(existing) = self
            .connections
            .iter_mut()
            .find(|item| item.name == original_name)
        else {
            anyhow::bail!("connection '{}' not found", original_name);
        };
        *existing = connection;
        self.connections.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.connections.len();
        self.connections
            .retain(|connection| connection.name != name);
        before != self.connections.len()
    }

    pub fn get(&self, name: &str) -> Option<&Connection> {
        self.connections
            .iter()
            .find(|connection| connection.name == name)
    }

    pub fn list(&self) -> &[Connection] {
        &self.connections
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        let Some(connection) = self
            .connections
            .iter_mut()
            .find(|connection| connection.name == name)
        else {
            anyhow::bail!("connection '{}' not found", name);
        };
        connection.enabled = enabled;
        Ok(())
    }
}

pub fn connections_path() -> PathBuf {
    config_dir().join("connections.json")
}

pub fn presets() -> Vec<Connection> {
    vec![
        Connection {
            name: "nvidia".to_string(),
            kind: ConnectionKind::OpenAiCompatible,
            base_url: "https://integrate.api.nvidia.com/v1".to_string(),
            api_key: String::new(),
            default_model: "meta/llama-3.1-70b-instruct".to_string(),
            rate_limit_rpm: 40,
            enabled: true,
        },
        Connection {
            name: "openai".to_string(),
            kind: ConnectionKind::OpenAiCompatible,
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            default_model: "gpt-4o-mini".to_string(),
            rate_limit_rpm: 0,
            enabled: true,
        },
        Connection {
            name: "groq".to_string(),
            kind: ConnectionKind::OpenAiCompatible,
            base_url: "https://api.groq.com/openai/v1".to_string(),
            api_key: String::new(),
            default_model: "llama-3.3-70b-versatile".to_string(),
            rate_limit_rpm: 0,
            enabled: true,
        },
        Connection {
            name: "openrouter".to_string(),
            kind: ConnectionKind::OpenAiCompatible,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key: String::new(),
            default_model: "openai/gpt-4o-mini".to_string(),
            rate_limit_rpm: 0,
            enabled: true,
        },
        Connection {
            name: "together".to_string(),
            kind: ConnectionKind::OpenAiCompatible,
            base_url: "https://api.together.xyz/v1".to_string(),
            api_key: String::new(),
            default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo".to_string(),
            rate_limit_rpm: 0,
            enabled: true,
        },
        Connection {
            name: "ollama".to_string(),
            kind: ConnectionKind::Ollama,
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: String::new(),
            default_model: "llama3:8b".to_string(),
            rate_limit_rpm: 0,
            enabled: true,
        },
    ]
}

pub fn mask_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return "(none)".to_string();
    }
    let last4 = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    let prefix = trimmed
        .split_once('-')
        .map(|(head, _)| format!("{head}-"))
        .unwrap_or_else(|| trimmed.chars().take(3).collect::<String>());
    format!("{prefix}...{last4}")
}

pub async fn test_connection(connection: &Connection) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed to build connection test HTTP client")?;
    let base_url = connection.base_url.trim_end_matches('/');
    let primary = format!("{base_url}/models");

    match test_models_url(&client, &primary, connection).await {
        Ok(count) => Ok(format!("reachable, {count} models")),
        Err(primary_error) => {
            if matches!(connection.kind, ConnectionKind::Ollama)
                || connection.api_key.trim().is_empty()
            {
                let alt = if base_url.ends_with("/v1") {
                    let root = base_url.trim_end_matches("/v1");
                    format!("{root}/models")
                } else {
                    format!("{base_url}/v1/models")
                };
                if alt != primary
                    && let Ok(count) = test_models_url(&client, &alt, connection).await
                {
                    return Ok(format!("reachable, {count} models"));
                }
                if head_reachable(&client, base_url).await {
                    return Ok("reachable, models endpoint unavailable".to_string());
                }
            }
            Err(primary_error)
        }
    }
}

async fn test_models_url(
    client: &reqwest::Client,
    url: &str,
    connection: &Connection,
) -> Result<usize> {
    let mut request = client.get(url);
    if !connection.api_key.trim().is_empty() {
        request = request.bearer_auth(connection.api_key.trim());
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("failed to reach {url}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("connection returned {status}: {}", body.trim());
    }
    let value: serde_json::Value = response
        .json()
        .await
        .context("failed to decode models response")?;
    Ok(value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .or_else(|| {
            value
                .get("models")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
        })
        .unwrap_or(0))
}

async fn head_reachable(client: &reqwest::Client, url: &str) -> bool {
    client
        .head(url)
        .send()
        .await
        .map(|response| response.status().is_success() || response.status().is_redirection())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_connection() -> Connection {
        Connection {
            name: "nvidia".to_string(),
            kind: ConnectionKind::OpenAiCompatible,
            base_url: "https://integrate.api.nvidia.com/v1".to_string(),
            api_key: "nvapi-secret-last".to_string(),
            default_model: "meta/llama-3.1-70b-instruct".to_string(),
            rate_limit_rpm: 40,
            enabled: true,
        }
    }

    #[test]
    fn add_save_load_round_trip_preserves_api_key() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("connections.json");
        let mut store = ConnectionStore::load_from_path(&path).unwrap();

        store.add(sample_connection()).unwrap();
        store.save().unwrap();

        let loaded = ConnectionStore::load_from_path(&path).unwrap();
        let connection = loaded.get("nvidia").unwrap();
        assert_eq!(connection.api_key, "nvapi-secret-last");
        assert_eq!(connection.rate_limit_rpm, 40);
    }

    #[test]
    fn serde_round_trip() {
        let connection = sample_connection();
        let json = serde_json::to_string(&connection).unwrap();
        let parsed: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, connection);
    }

    #[test]
    fn presets_include_expected_providers() {
        let names = presets()
            .into_iter()
            .map(|connection| connection.name)
            .collect::<Vec<_>>();
        for expected in [
            "nvidia",
            "openai",
            "groq",
            "openrouter",
            "together",
            "ollama",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn mask_key_hides_secret_material() {
        assert_eq!(mask_key(""), "(none)");
        assert_eq!(mask_key("sk-abcdef1234"), "sk-...1234");
        let masked = mask_key("abcdef1234");
        assert!(masked.ends_with("1234"));
        assert!(!masked.contains("abcdef"));
    }
}
