//! Target Registry — the fabric spine. A target is a point in a 3-axis space
//! (UI surface / capability flag-set / input modality) plus reachability + status.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where a target came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSource {
    Ephemeral,
    Mesh,
    Physical,
}

/// Axis 1 — which layout the build renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiSurface {
    Desktop,
    Mobile,
}

/// Axis 3 — input modality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputModality {
    Pointer,
    Touch,
    Both,
}

/// Axis 2 — capability flag-set. Presets are named bundles of these flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CapabilityFlags {
    pub camera: bool,
    pub mic: bool,
    pub gpu: bool,
    pub filesystem: bool,
    pub multi_window: bool,
    pub p2p_host: bool,
    pub background: bool,
    pub public_web_host: bool,
}

impl CapabilityFlags {
    /// `web-sandboxed` preset — only camera/mic guest access.
    pub fn web_sandboxed() -> Self {
        Self {
            camera: true,
            mic: true,
            ..Self::default()
        }
    }
}

/// How the harness reaches a target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "adapter", content = "addr")]
pub enum Reach {
    /// k8s pod, addressed by `namespace/job-name`.
    Pod(String),
    /// Reached over the tailnet by SSH, addressed by tailscale IP/host.
    TailnetSsh(String),
}

/// Probe + reachability outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetStatus {
    Reachable,
    Unreachable,
    ProbeFailed,
}

/// A normalized target — the unit everything else in the fabric operates on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub source: TargetSource,
    pub ui_surface: UiSurface,
    pub capabilities: CapabilityFlags,
    pub input: InputModality,
    pub arch: String,
    pub os: String,
    pub reach: Reach,
    pub status: TargetStatus,
    /// Human-readable note (e.g. the failure reason when status != Reachable).
    #[serde(default)]
    pub note: String,
}

/// The registry: a flat set of targets, persisted as JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    pub targets: Vec<Target>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, target: Target) {
        self.targets.push(target);
    }

    /// Path to the persisted registry: `~/.config/orrchestrator/orrdeal/registry.json`.
    pub fn path() -> PathBuf {
        orrch_core::config::config_dir()
            .join("orrdeal")
            .join("registry.json")
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
