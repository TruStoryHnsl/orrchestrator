//! The probe agent (a single portable shell script) and the typed report it emits.

use serde::Deserialize;

use crate::registry::CapabilityFlags;

/// The one probe artifact, embedded at compile time. Delivered two ways:
/// piped to `sh -s` over SSH (mesh), and embedded into a k8s Job (ephemeral).
pub const PROBE_SH: &str = include_str!("../probe/probe.sh");

/// JSON shape emitted by `probe.sh`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProbeReport {
    pub os: String,
    pub arch: String,
    pub camera: bool,
    pub mic: bool,
    pub gpu: bool,
    pub filesystem: bool,
}

impl ProbeReport {
    /// Parse the probe's stdout. The probe prints exactly one JSON line; we take
    /// the last non-empty line to tolerate any leading SSH/login banner noise.
    pub fn parse(stdout: &str) -> anyhow::Result<Self> {
        let line = stdout
            .lines()
            .rev()
            .find(|l| l.trim_start().starts_with('{'))
            .ok_or_else(|| anyhow::anyhow!("no JSON object in probe output:\n{stdout}"))?;
        Ok(serde_json::from_str(line.trim())?)
    }

    /// Map detected hardware to capability flags. UI surface / input modality are
    /// NOT host-detectable in the skeleton and are set by source defaults at the
    /// call site (see provision/discover arms).
    pub fn capabilities(&self) -> CapabilityFlags {
        CapabilityFlags {
            camera: self.camera,
            mic: self.mic,
            gpu: self.gpu,
            filesystem: self.filesystem,
            ..CapabilityFlags::default()
        }
    }
}
