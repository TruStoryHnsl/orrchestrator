use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::provider::{ProviderConfig, ProviderKind};

/// Supported AI backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    Claude,
    Gemini,
    Crush,
    OpenCode,
    #[serde(rename = "anthropic_api")]
    AnthropicApi,
    #[serde(rename = "openai_api")]
    OpenAiApi,
}

impl BackendKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Crush => "crush",
            Self::OpenCode => "opencode",
            Self::AnthropicApi => "anthropic-api",
            Self::OpenAiApi => "openai-api",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Claude => "[claude]",
            Self::Gemini => "[gemini]",
            Self::Crush => "[crush]",
            Self::OpenCode => "[opencode]",
            Self::AnthropicApi => "[anthropic-api]",
            Self::OpenAiApi => "[openai-api]",
        }
    }

    /// Whether this backend uses a CLI/PTY transport.
    pub fn is_cli(&self) -> bool {
        matches!(self, Self::Claude | Self::Gemini | Self::Crush | Self::OpenCode)
    }

    /// Whether this backend uses a direct HTTP API transport.
    pub fn is_api(&self) -> bool {
        matches!(self, Self::AnthropicApi | Self::OpenAiApi)
    }

    /// Map backend to the provider name used in valve store and model definitions.
    pub fn provider_name(&self) -> &'static str {
        match self {
            Self::Claude | Self::AnthropicApi => "Anthropic",
            Self::Gemini => "Google",
            Self::Crush | Self::OpenCode => "Local",
            Self::OpenAiApi => "OpenAI",
        }
    }

    /// Convert this backend kind into a unified ProviderConfig.
    /// For CLI backends, reads command/flags from BackendsConfig.
    /// For API backends, produces a stub config.
    pub fn to_provider(&self, config: &BackendsConfig) -> ProviderConfig {
        match self {
            Self::Claude | Self::Gemini | Self::Crush | Self::OpenCode => {
                if let Some(cfg) = config.backends.get(self) {
                    ProviderConfig {
                        name: self.label().to_string(),
                        kind: ProviderKind::CliPty {
                            command: cfg.command.clone(),
                            flags: cfg.flags.clone(),
                        },
                        available: cfg.available,
                    }
                } else {
                    ProviderConfig {
                        name: self.label().to_string(),
                        kind: ProviderKind::CliPty {
                            command: self.label().to_string(),
                            flags: vec![],
                        },
                        available: false,
                    }
                }
            }
            Self::AnthropicApi => ProviderConfig {
                name: "anthropic-api".to_string(),
                kind: ProviderKind::ApiHttp {
                    base_url: "https://api.anthropic.com".to_string(),
                    model_id: "claude-sonnet-4-20250514".to_string(),
                    api_key_env: "ANTHROPIC_API_KEY".to_string(),
                },
                available: std::env::var("ANTHROPIC_API_KEY").is_ok(),
            },
            Self::OpenAiApi => ProviderConfig {
                name: "openai-api".to_string(),
                kind: ProviderKind::ApiHttp {
                    base_url: "https://api.openai.com".to_string(),
                    model_id: "gpt-4o".to_string(),
                    api_key_env: "OPENAI_API_KEY".to_string(),
                },
                available: std::env::var("OPENAI_API_KEY").is_ok(),
            },
        }
    }

    /// All known backend variants.
    pub fn all() -> &'static [BackendKind] {
        &[
            Self::Claude,
            Self::Gemini,
            Self::Crush,
            Self::OpenCode,
            Self::AnthropicApi,
            Self::OpenAiApi,
        ]
    }

    /// Only CLI-based backends (for PTY spawning).
    pub fn cli_backends() -> &'static [BackendKind] {
        &[Self::Claude, Self::Gemini, Self::Crush, Self::OpenCode]
    }
}

impl Default for BackendKind {
    fn default() -> Self {
        Self::Claude
    }
}

/// Configuration for a single backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub command: String,
    pub flags: Vec<String>,
    #[serde(default = "default_true")]
    pub available: bool,
}

fn default_true() -> bool {
    true
}

/// Full backends configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendsConfig {
    pub backends: HashMap<BackendKind, BackendConfig>,
}

impl Default for BackendsConfig {
    fn default() -> Self {
        let mut backends = HashMap::new();
        backends.insert(
            BackendKind::Claude,
            BackendConfig {
                command: "claude".into(),
                flags: vec!["--dangerously-skip-permissions".into()],
                available: false,
            },
        );
        backends.insert(
            BackendKind::Gemini,
            BackendConfig {
                command: "gemini".into(),
                flags: vec![],
                available: false,
            },
        );
        backends.insert(
            BackendKind::Crush,
            BackendConfig {
                command: "crush".into(),
                flags: vec![],
                available: false,
            },
        );
        backends.insert(
            BackendKind::OpenCode,
            BackendConfig {
                command: "opencode".into(),
                flags: vec![],
                available: false,
            },
        );
        Self { backends }
    }
}

impl BackendsConfig {
    /// Load config from `~/.config/orrchestrator/backends.yaml`, or use defaults.
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(mut cfg) = serde_yaml_or_json(&contents) {
                    cfg.detect_availability();
                    return cfg;
                }
            }
        }
        let mut cfg = Self::default();
        cfg.detect_availability();
        cfg
    }

    /// Auto-detect which backends are available on this system.
    pub fn detect_availability(&mut self) {
        for (_, config) in self.backends.iter_mut() {
            config.available = which_exists(&config.command);
        }
    }

    /// Get the command + flags for a backend.
    pub fn get_command(&self, kind: BackendKind) -> Option<Vec<String>> {
        self.backends.get(&kind).and_then(|cfg| {
            if cfg.available {
                let mut cmd = vec![cfg.command.clone()];
                cmd.extend(cfg.flags.iter().cloned());
                Some(cmd)
            } else {
                None
            }
        })
    }

    /// List available backends.
    pub fn available(&self) -> Vec<BackendKind> {
        self.backends
            .iter()
            .filter(|(_, cfg)| cfg.available)
            .map(|(kind, _)| *kind)
            .collect()
    }
}

/// Check if a backend is available for spawning, considering both system availability
/// and external block status (e.g., valve state).
/// `valve_blocked` should be true if the provider's valve is closed.
pub fn is_provider_available(backends: &BackendsConfig, kind: BackendKind, valve_blocked: bool) -> (bool, &'static str) {
    if valve_blocked {
        return (false, "provider valve is closed");
    }
    match backends.backends.get(&kind) {
        Some(cfg) if cfg.available => (true, ""),
        Some(_) => (false, "backend binary not found"),
        None => (false, "backend not configured"),
    }
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
    PathBuf::from(home)
        .join(".config")
        .join("orrchestrator")
        .join("backends.yaml")
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Try parsing as YAML first (serde_json is a subset), fall back to JSON.
fn serde_yaml_or_json(contents: &str) -> Result<BackendsConfig, ()> {
    // We only have serde_json in deps — support JSON config
    serde_json::from_str(contents).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = BackendsConfig::default();
        assert!(cfg.backends.contains_key(&BackendKind::Claude));
        assert!(cfg.backends.contains_key(&BackendKind::Gemini));
    }

    #[test]
    fn test_backend_labels() {
        assert_eq!(BackendKind::Claude.label(), "claude");
        assert_eq!(BackendKind::Gemini.badge(), "[gemini]");
    }

    #[test]
    fn test_get_command_unavailable() {
        let cfg = BackendsConfig::default(); // available=false by default
        assert!(cfg.get_command(BackendKind::Claude).is_none());
    }
}
