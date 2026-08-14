//! orrdeal configuration, loaded from `~/.config/orrchestrator/orrdeal/config.json`.
//! Secrets (the Proxmox API token) are NEVER stored here — only the *name* of the
//! environment variable that holds the token.

use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxmoxConfig {
    /// e.g. "https://orrbit:8006/"
    pub endpoint: String,
    /// PVE node name to create the VM on.
    pub node: String,
    /// Name of the env var holding the API token (e.g. "ORRDEAL_PVE_TOKEN").
    pub api_token_env: String,
    /// Cloud-init-enabled template VM to clone.
    pub template: String,
    pub ssh_user: String,
    pub ssh_public_key_path: String,
    pub ssh_private_key_path: String,
    pub cores: u32,
    pub memory_mb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    /// Tailscale HostName to target (e.g. "cb17").
    pub device: String,
    pub ssh_user: String,
    pub ssh_private_key_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrrdealConfig {
    pub proxmox: ProxmoxConfig,
    pub mesh: MeshConfig,
}

impl OrrdealConfig {
    pub fn path() -> PathBuf {
        orrch_core::config::config_dir()
            .join("orrdeal")
            .join("config.json")
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = Self::path();
        let contents = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "orrdeal config not found at {}. Copy crates/orrch-orrdeal/config.example.json there and edit it.",
                path.display()
            )
        })?;
        let cfg: OrrdealConfig = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(cfg)
    }
}
