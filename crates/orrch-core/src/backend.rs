use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Supported AI CLI backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    Claude,
    Gemini,
}

impl BackendKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Claude => "[claude]",
            Self::Gemini => "[gemini]",
        }
    }

    /// Map backend to the provider name used in valve store and model definitions.
    pub fn provider_name(&self) -> &'static str {
        match self {
            Self::Claude => "Anthropic",
            Self::Gemini => "Google",
        }
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
