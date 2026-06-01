//! Discover arm: find a Linux device on the tailnet by hostname, SSH in, run the
//! probe, and produce a Target. Independently fallible — failure yields a Target
//! with a non-Reachable status rather than aborting the run.

use std::process::Stdio;

use anyhow::{Context, anyhow};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::config::MeshConfig;
use crate::probe::{PROBE_SH, ProbeReport};
use crate::registry::{
    CapabilityFlags, InputModality, Reach, Target, TargetSource, TargetStatus, UiSurface,
};

/// Find the first online tailnet peer whose HostName matches `device`,
/// returning its tailscale IP.
async fn find_device_ip(device: &str) -> anyhow::Result<String> {
    let out = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await
        .context("running `tailscale status --json`")?;
    if !out.status.success() {
        return Err(anyhow!("`tailscale status --json` exited non-zero"));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let peers = v
        .get("Peer")
        .and_then(|p| p.as_object())
        .ok_or_else(|| anyhow!("no Peer map in tailscale status"))?;
    for peer in peers.values() {
        let hostname = peer.get("HostName").and_then(|h| h.as_str()).unwrap_or("");
        let online = peer.get("Online").and_then(|o| o.as_bool()).unwrap_or(false);
        if hostname.eq_ignore_ascii_case(device)
            && online
            && let Some(ip) = peer
                .get("TailscaleIPs")
                .and_then(|i| i.as_array())
                .and_then(|a| a.first())
                .and_then(|s| s.as_str())
        {
            return Ok(ip.to_string());
        }
    }
    Err(anyhow!("tailnet device `{device}` not found or offline"))
}

/// Run probe.sh on the device over SSH and return its raw stdout.
async fn ssh_probe(cfg: &MeshConfig, ip: &str) -> anyhow::Result<String> {
    let mut child = Command::new("ssh")
        .args([
            "-o", "BatchMode=yes",
            "-o", "StrictHostKeyChecking=accept-new",
            "-o", "ConnectTimeout=15",
            "-i", &cfg.ssh_private_key_path,
            &format!("{}@{}", cfg.ssh_user, ip),
            "sh -s",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning ssh")?;

    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("ssh stdin unavailable"))?
        .write_all(PROBE_SH.as_bytes())
        .await?;

    let out = child.wait_with_output().await?;
    if !out.status.success() {
        return Err(anyhow!(
            "ssh probe failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run the discover arm. Always returns a Target (Reachable on success, otherwise
/// Unreachable/ProbeFailed with a note). Errors only on genuinely unexpected faults.
pub async fn run(cfg: &MeshConfig) -> Target {
    let base = |status: TargetStatus, note: String, os: String, arch: String, caps: CapabilityFlags, addr: String| Target {
        id: format!("mesh:{}", cfg.device),
        source: TargetSource::Mesh,
        ui_surface: UiSurface::Desktop, // source default — refined later
        capabilities: caps,
        input: InputModality::Pointer,  // source default — refined later
        arch,
        os,
        reach: Reach::TailnetSsh(addr),
        status,
        note,
    };

    let ip = match find_device_ip(&cfg.device).await {
        Ok(ip) => ip,
        Err(e) => {
            return base(
                TargetStatus::Unreachable,
                e.to_string(),
                String::new(),
                String::new(),
                CapabilityFlags::default(),
                String::new(),
            );
        }
    };

    match ssh_probe(cfg, &ip).await {
        Ok(stdout) => match ProbeReport::parse(&stdout) {
            Ok(r) => base(
                TargetStatus::Reachable,
                String::new(),
                r.os.clone(),
                r.arch.clone(),
                r.capabilities(),
                ip,
            ),
            Err(e) => base(
                TargetStatus::ProbeFailed,
                e.to_string(),
                String::new(),
                String::new(),
                CapabilityFlags::default(),
                ip,
            ),
        },
        Err(e) => base(
            TargetStatus::Unreachable,
            e.to_string(),
            String::new(),
            String::new(),
            CapabilityFlags::default(),
            ip,
        ),
    }
}
