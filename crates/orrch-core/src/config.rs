use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::backend::BackendsConfig;

/// Top-level orrchestrator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// AI backend configuration (Claude, Gemini, Crush, etc.).
    #[serde(default)]
    pub backends: BackendsConfig,
    /// Directory containing agent profile `.md` files.
    #[serde(default = "default_agents_dir")]
    pub agents_dir: PathBuf,
    /// Root directory of the library (git-backed repo).
    #[serde(default = "default_library_dir")]
    pub library_dir: PathBuf,
    /// Root directory for projects.
    #[serde(default = "default_projects_dir")]
    pub projects_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backends: BackendsConfig::default(),
            agents_dir: default_agents_dir(),
            library_dir: default_library_dir(),
            projects_dir: default_projects_dir(),
        }
    }
}

impl Config {
    /// Load config from `~/.config/orrchestrator/config.json`.
    /// Falls back to legacy `backends.yaml`, then defaults.
    pub fn load() -> Self {
        let config_path = config_dir().join("config.json");

        // Try new unified config
        if config_path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&config_path) {
                if let Ok(mut cfg) = serde_json::from_str::<Config>(&contents) {
                    cfg.backends.detect_availability();
                    return cfg;
                }
            }
        }

        // Fall back to legacy backends.yaml
        let mut cfg = Self::default();
        cfg.backends = BackendsConfig::load();
        cfg
    }

    /// Save config to `~/.config/orrchestrator/config.json`.
    pub fn save(&self) -> std::io::Result<()> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("config.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }
}

/// Configuration directory: ~/.config/orrchestrator/
pub fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
    PathBuf::from(home).join(".config").join("orrchestrator")
}

fn default_agents_dir() -> PathBuf {
    // Check for project-local agents/ first
    let local = PathBuf::from("agents");
    if local.is_dir() {
        return local;
    }
    config_dir().join("agents")
}

fn default_library_dir() -> PathBuf {
    config_dir().join("library")
}

fn default_projects_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
    PathBuf::from(home).join("projects")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert!(cfg.projects_dir.to_string_lossy().contains("projects"));
        assert!(cfg.library_dir.to_string_lossy().contains("library"));
    }

    #[test]
    fn test_config_serialization() {
        let cfg = Config::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.projects_dir, parsed.projects_dir);
    }
}
