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
    Pi,
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
            Self::Pi => "pi",
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
            Self::Pi => "[pi]",
            Self::AnthropicApi => "[anthropic-api]",
            Self::OpenAiApi => "[openai-api]",
        }
    }

    /// Whether this backend uses a CLI/PTY transport.
    pub fn is_cli(&self) -> bool {
        matches!(self, Self::Claude | Self::Gemini | Self::Crush | Self::OpenCode | Self::Pi)
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
            Self::Pi => "Multi",
            Self::OpenAiApi => "OpenAI",
        }
    }

    /// Convert this backend kind into a unified ProviderConfig.
    /// For CLI backends, reads command/flags from BackendsConfig.
    /// For API backends, produces a stub config.
    pub fn to_provider(&self, config: &BackendsConfig) -> ProviderConfig {
        match self {
            Self::Claude | Self::Gemini | Self::Crush | Self::OpenCode | Self::Pi => {
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
            Self::Pi,
            Self::AnthropicApi,
            Self::OpenAiApi,
        ]
    }

    /// Only CLI-based backends (for PTY spawning).
    pub fn cli_backends() -> &'static [BackendKind] {
        &[Self::Claude, Self::Gemini, Self::Crush, Self::OpenCode, Self::Pi]
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
        backends.insert(
            BackendKind::Pi,
            BackendConfig {
                command: "pi".into(),
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
///
/// CLI backends require their binary to be present on PATH (detected via `which`).
/// API backends require their API key env var to be set.
pub fn is_provider_available(backends: &BackendsConfig, kind: BackendKind, valve_blocked: bool) -> (bool, &'static str) {
    if valve_blocked {
        return (false, "provider valve is closed");
    }
    if kind.is_api() {
        let env_var = match kind {
            BackendKind::AnthropicApi => "ANTHROPIC_API_KEY",
            BackendKind::OpenAiApi => "OPENAI_API_KEY",
            _ => return (false, "backend not configured"),
        };
        return if std::env::var(env_var).is_ok() {
            (true, "")
        } else {
            (false, "api key env var not set")
        };
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

/// Send a single user message to an HTTP API backend and return the assistant's reply.
///
/// Supports `BackendKind::AnthropicApi` and `BackendKind::OpenAiApi`. The provider's
/// API key is read from the env var declared in its `ProviderConfig`. Uses blocking
/// reqwest so the call is safe to make from synchronous contexts (e.g.
/// `ProcessManager::spawn`). For private-scope iteration: no streaming, no retries.
pub fn send_api_message(backend: BackendKind, model_id: &str, prompt: &str) -> anyhow::Result<String> {
    match backend {
        BackendKind::AnthropicApi => send_anthropic(model_id, prompt),
        BackendKind::OpenAiApi => send_openai(model_id, prompt),
        BackendKind::Pi => send_pi(prompt),
        _ => anyhow::bail!("{} is not an HTTP API backend", backend.label()),
    }
}

/// Send a one-shot prompt via `pi --print --no-session --provider anthropic`.
fn send_pi(prompt: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("pi")
        .args(["--print", "--no-session", "--provider", "anthropic"])
        .arg(prompt)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| anyhow::anyhow!("pi command failed to spawn: {e}"))?;
    if !output.status.success() {
        anyhow::bail!("pi exited with status {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn http_client() -> anyhow::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build http client: {e}"))
}

fn send_anthropic(model_id: &str, prompt: &str) -> anyhow::Result<String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY is not set"))?;
    let body = serde_json::json!({
        "model": model_id,
        "max_tokens": 1024,
        "messages": [
            { "role": "user", "content": prompt }
        ]
    });
    let client = http_client()?;
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| anyhow::anyhow!("anthropic request failed: {e}"))?;
    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .map_err(|e| anyhow::anyhow!("anthropic response not json: {e}"))?;
    if !status.is_success() {
        anyhow::bail!("anthropic api error {}: {}", status, json);
    }
    // Response shape: { "content": [ { "type": "text", "text": "..." }, ... ], ... }
    let text = json
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.iter().find_map(|item| {
            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        }))
        .ok_or_else(|| anyhow::anyhow!("anthropic response missing text content"))?;
    Ok(text)
}

fn send_openai(model_id: &str, prompt: &str) -> anyhow::Result<String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY is not set"))?;
    let body = serde_json::json!({
        "model": model_id,
        "messages": [
            { "role": "user", "content": prompt }
        ]
    });
    let client = http_client()?;
    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| anyhow::anyhow!("openai request failed: {e}"))?;
    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .map_err(|e| anyhow::anyhow!("openai response not json: {e}"))?;
    if !status.is_success() {
        anyhow::bail!("openai api error {}: {}", status, json);
    }
    // Response shape: { "choices": [ { "message": { "content": "..." } }, ... ], ... }
    let text = json
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("openai response missing message content"))?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = BackendsConfig::default();
        assert!(cfg.backends.contains_key(&BackendKind::Claude));
        assert!(cfg.backends.contains_key(&BackendKind::Gemini));
        assert!(cfg.backends.contains_key(&BackendKind::Crush));
        assert!(cfg.backends.contains_key(&BackendKind::OpenCode));
        assert!(cfg.backends.contains_key(&BackendKind::Pi));
    }

    #[test]
    fn test_backend_labels() {
        assert_eq!(BackendKind::Claude.label(), "claude");
        assert_eq!(BackendKind::Gemini.badge(), "[gemini]");
        assert_eq!(BackendKind::Crush.label(), "crush");
        assert_eq!(BackendKind::OpenCode.badge(), "[opencode]");
        assert_eq!(BackendKind::Pi.label(), "pi");
        assert_eq!(BackendKind::Pi.badge(), "[pi]");
    }

    #[test]
    fn test_get_command_unavailable() {
        let cfg = BackendsConfig::default(); // available=false by default
        assert!(cfg.get_command(BackendKind::Claude).is_none());
    }

    #[test]
    fn test_crush_default_command() {
        let cfg = BackendsConfig::default();
        let crush_cfg = cfg.backends.get(&BackendKind::Crush).expect("crush entry");
        assert_eq!(crush_cfg.command, "crush");
        assert!(crush_cfg.flags.is_empty());
        assert!(!crush_cfg.available, "default should be unavailable until detection runs");
    }

    #[test]
    fn test_crush_availability_detection() {
        let mut cfg = BackendsConfig::default();
        // Force a known-bad command so detection sets available=false regardless of host
        if let Some(c) = cfg.backends.get_mut(&BackendKind::Crush) {
            c.command = "definitely_not_a_real_binary_xyz_123".into();
            c.available = true;
        }
        cfg.detect_availability();
        let crush_cfg = cfg.backends.get(&BackendKind::Crush).unwrap();
        assert!(!crush_cfg.available, "bogus binary must not be detected as available");

        // is_provider_available should report the binary as missing
        let (avail, reason) = is_provider_available(&cfg, BackendKind::Crush, false);
        assert!(!avail);
        assert_eq!(reason, "backend binary not found");

        // valve closed must take precedence
        let (avail, reason) = is_provider_available(&cfg, BackendKind::Crush, true);
        assert!(!avail);
        assert_eq!(reason, "provider valve is closed");
    }

    #[test]
    fn test_crush_to_provider_routes_cli_pty() {
        let cfg = BackendsConfig::default();
        let provider = BackendKind::Crush.to_provider(&cfg);
        assert!(provider.is_cli());
        assert!(!provider.is_api());
        assert_eq!(provider.name, "crush");
    }

    #[test]
    fn test_all_backends_route_to_correct_provider_kind() {
        // Exercises to_provider() for every BackendKind variant and asserts
        // CLI backends map to CliPty and API backends map to ApiHttp.
        let cfg = BackendsConfig::default();

        let cli_variants = [
            (BackendKind::Claude, "claude"),
            (BackendKind::Gemini, "gemini"),
            (BackendKind::Crush, "crush"),
            (BackendKind::OpenCode, "opencode"),
            (BackendKind::Pi, "pi"),
        ];
        for (kind, expected_name) in cli_variants {
            let provider = kind.to_provider(&cfg);
            assert!(provider.is_cli(), "{} should be CliPty", kind.label());
            assert!(!provider.is_api(), "{} must not be ApiHttp", kind.label());
            assert_eq!(provider.name, expected_name);
            assert!(kind.is_cli());
            assert!(!kind.is_api());
        }

        let api_variants = [
            (BackendKind::AnthropicApi, "anthropic-api"),
            (BackendKind::OpenAiApi, "openai-api"),
        ];
        for (kind, expected_name) in api_variants {
            let provider = kind.to_provider(&cfg);
            assert!(provider.is_api(), "{} should be ApiHttp", kind.label());
            assert!(!provider.is_cli(), "{} must not be CliPty", kind.label());
            assert_eq!(provider.name, expected_name);
            assert!(kind.is_api());
            assert!(!kind.is_cli());
        }

        // Sanity-check BackendKind::all() enumerates all seven.
        assert_eq!(BackendKind::all().len(), 7);
    }

    #[test]
    fn test_valve_blocked_overrides_all_backends() {
        // valve_blocked=true must short-circuit for every backend variant,
        // regardless of whether the binary or env var is present.
        let cfg = BackendsConfig::default();
        for kind in BackendKind::all() {
            let (avail, reason) = is_provider_available(&cfg, *kind, true);
            assert!(!avail, "{} must be unavailable when valve closed", kind.label());
            assert_eq!(reason, "provider valve is closed");
        }
    }

    #[test]
    fn test_api_backend_env_var_availability() {
        // Single test function covers both API backends and both set/unset states.
        // Doing it in one test avoids parallel races with other tests that read
        // these env vars. Save existing values, exercise all states, restore.
        let saved_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();
        let saved_openai = std::env::var("OPENAI_API_KEY").ok();

        let cfg = BackendsConfig::default();

        // --- Both unset ---
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
            std::env::remove_var("OPENAI_API_KEY");
        }
        let (avail, reason) = is_provider_available(&cfg, BackendKind::AnthropicApi, false);
        assert!(!avail);
        assert_eq!(reason, "api key env var not set");
        let (avail, reason) = is_provider_available(&cfg, BackendKind::OpenAiApi, false);
        assert!(!avail);
        assert_eq!(reason, "api key env var not set");

        // --- Anthropic set, OpenAI unset ---
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test-anthropic-key");
        }
        let (avail, reason) = is_provider_available(&cfg, BackendKind::AnthropicApi, false);
        assert!(avail, "AnthropicApi should be available when ANTHROPIC_API_KEY is set");
        assert_eq!(reason, "");
        let (avail, _) = is_provider_available(&cfg, BackendKind::OpenAiApi, false);
        assert!(!avail);

        // --- Both set ---
        unsafe {
            std::env::set_var("OPENAI_API_KEY", "test-openai-key");
        }
        let (avail, reason) = is_provider_available(&cfg, BackendKind::OpenAiApi, false);
        assert!(avail, "OpenAiApi should be available when OPENAI_API_KEY is set");
        assert_eq!(reason, "");

        // --- valve closed still wins even with keys set ---
        let (avail, reason) = is_provider_available(&cfg, BackendKind::AnthropicApi, true);
        assert!(!avail);
        assert_eq!(reason, "provider valve is closed");

        // Restore original environment.
        unsafe {
            match saved_anthropic {
                Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
                None => std::env::remove_var("ANTHROPIC_API_KEY"),
            }
            match saved_openai {
                Some(v) => std::env::set_var("OPENAI_API_KEY", v),
                None => std::env::remove_var("OPENAI_API_KEY"),
            }
        }
    }
}
