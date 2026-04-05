use std::path::Path;

use serde::{Deserialize, Serialize};

/// The transport mechanism for communicating with an AI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderKind {
    /// CLI-based provider using PTY (Claude CLI, Gemini CLI, Crush, OpenCode).
    CliPty {
        command: String,
        flags: Vec<String>,
    },
    /// Direct HTTP API provider (Anthropic API, OpenAI API).
    /// Stub — actual HTTP implementation comes in a future task.
    ApiHttp {
        base_url: String,
        model_id: String,
        api_key_env: String,
    },
}

/// Unified provider configuration that backends resolve into.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub kind: ProviderKind,
    pub available: bool,
}

impl ProviderConfig {
    /// Check if this provider's binary/API is available on the system.
    pub fn check_available(&mut self) {
        match &self.kind {
            ProviderKind::CliPty { command, .. } => {
                self.available = which_exists(command);
            }
            ProviderKind::ApiHttp { api_key_env, .. } => {
                // Available if the env var holding the API key is set
                self.available = std::env::var(api_key_env).is_ok();
            }
        }
    }

    /// Build the full command args for CLI providers.
    /// Returns `None` for API providers (they don't use CLI spawning).
    pub fn cli_args(&self, _project_dir: &Path, goal: Option<&str>) -> Option<Vec<String>> {
        match &self.kind {
            ProviderKind::CliPty { command, flags } => {
                if !self.available {
                    return None;
                }
                let mut args = vec![command.clone()];
                args.extend(flags.iter().cloned());
                if let Some(g) = goal {
                    args.push(g.to_string());
                }
                Some(args)
            }
            ProviderKind::ApiHttp { .. } => None,
        }
    }

    /// Returns true if this is a CLI/PTY-based provider.
    pub fn is_cli(&self) -> bool {
        matches!(self.kind, ProviderKind::CliPty { .. })
    }

    /// Returns true if this is an HTTP API provider.
    pub fn is_api(&self) -> bool {
        matches!(self.kind, ProviderKind::ApiHttp { .. })
    }
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
