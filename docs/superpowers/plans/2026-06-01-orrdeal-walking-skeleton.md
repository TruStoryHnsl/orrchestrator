# orrdeal Walking-Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the thinnest real end-to-end slice of the `orrdeal` test fabric: one command provisions a k3s pod on Proxmox AND discovers a Linux mesh device over Tailscale, probes both, and prints a unified Target Registry report.

**Architecture:** A new Rust workspace crate `crates/orrch-orrdeal` that *orchestrates* external tools (`terraform`, `kubectl`, `tailscale`, `ssh`) rather than reimplementing them. Two independently-fallible arms (provision / discover) feed one in-memory Target Registry; a reporter prints it. Surfaced via the existing `orrchestrator` binary as the `orrdeal` subcommand (hand-rolled arg dispatch, matching the repo's no-clap convention).

**Tech Stack:** Rust (edition 2024), serde/serde_json, anyhow, tokio (process + join), Terraform (bpg/proxmox provider), k3s, a portable POSIX `sh` probe script. Config as JSON under `~/.config/orrchestrator/orrdeal/`.

---

## CRITICAL: testing discipline for the executor

This repo's `CLAUDE.md` ("WRITTEN IN BLOOD") forbids authoring tests in the same session that builds a feature, and forbids abstract-value tests. **This plan therefore does NOT contain `write-failing-test-first` TDD loops.** Verification in each task is **build + observe**: `cargo build`/`cargo clippy` for compile-correctness, and for the integration tasks, **running the real command and looking at the output**. Automated tests are a separate cold-session deliverable (see the closing handoff). Do not write speculative unit tests while implementing.

## File structure (created/modified by this plan)

| Path | Responsibility |
|---|---|
| `crates/orrch-orrdeal/Cargo.toml` | Crate manifest |
| `crates/orrch-orrdeal/src/lib.rs` | Module wiring + `run_cli` entry |
| `crates/orrch-orrdeal/src/registry.rs` | Target model (3 axes), CapabilityFlags, Registry persistence |
| `crates/orrch-orrdeal/src/config.rs` | `OrrdealConfig` JSON load |
| `crates/orrch-orrdeal/src/probe.rs` | Parse probe JSON → `ProbeReport`; embed `probe.sh` |
| `crates/orrch-orrdeal/src/prereq.rs` | Binary + env prerequisite checks |
| `crates/orrch-orrdeal/src/discover.rs` | Discover arm: tailscale + ssh probe |
| `crates/orrch-orrdeal/src/provision.rs` | Provision arm: terraform + kubectl probe Job |
| `crates/orrch-orrdeal/src/report.rs` | Print unified report table |
| `crates/orrch-orrdeal/src/cli.rs` | Subcommand parse + orchestration |
| `crates/orrch-orrdeal/probe/probe.sh` | The single portable probe agent |
| `crates/orrch-orrdeal/terraform/proxmox-k3s/main.tf` | VM + cloud-init k3s |
| `crates/orrch-orrdeal/terraform/proxmox-k3s/variables.tf` | Module inputs |
| `crates/orrch-orrdeal/terraform/proxmox-k3s/outputs.tf` | node IP + kubeconfig path |
| `crates/orrch-orrdeal/config.example.json` | Seed config template |
| `crates/orrch-orrdeal/.gitignore` | Ignore TF state + fetched kubeconfig |
| `Cargo.toml` (workspace) | Add crate to members |
| `src/Cargo.toml` | Add dep on `orrch-orrdeal` |
| `src/src/main.rs` | Dispatch the `orrdeal` subcommand |

> **Skeleton honesty note:** a headless pod / SSH host cannot self-report *UI surface* or *input modality*. The probe reports `os`, `arch`, and hardware capability flags (camera/mic/gpu/filesystem). `ui_surface` and `input` are set to source-based defaults (refined by later sub-projects). This is deliberate, not a gap.

---

### Task 1: Scaffold the crate and wire it into the workspace

**Files:**
- Create: `crates/orrch-orrdeal/Cargo.toml`
- Create: `crates/orrch-orrdeal/src/lib.rs`
- Create: `crates/orrch-orrdeal/.gitignore`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Create the crate manifest**

`crates/orrch-orrdeal/Cargo.toml`:
```toml
[package]
name = "orrch-orrdeal"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "orrdeal — heterogeneous test fabric for orchestrator-built apps (walking skeleton)"

[dependencies]
orrch-core = { path = "../orrch-core" }
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
tracing.workspace = true
```

- [ ] **Step 2: Create a minimal lib.rs so the crate compiles**

`crates/orrch-orrdeal/src/lib.rs`:
```rust
//! orrdeal — heterogeneous test fabric (walking skeleton).
//!
//! Orchestrates external tools (terraform, kubectl, tailscale, ssh) to
//! provision + discover + probe targets, normalizing them into one registry.

pub mod registry;

/// Placeholder entry point; real CLI lands in Task 11.
pub async fn run_cli(_args: &[String]) -> anyhow::Result<()> {
    println!("orrdeal: not yet implemented");
    Ok(())
}
```

- [ ] **Step 3: Create a stub registry module so lib.rs resolves**

`crates/orrch-orrdeal/src/registry.rs`:
```rust
//! Target Registry — filled in Task 2.
```

- [ ] **Step 4: Add the crate to the workspace members**

Modify `Cargo.toml` (workspace root). In the `members = [ ... ]` array, add the line `"crates/orrch-orrdeal",` immediately after `"crates/orrch-mcp",` so the list stays alphabetical:
```toml
    "crates/orrch-mcp",
    "crates/orrch-orrdeal",
    "crates/orrch-retrospect",
```

- [ ] **Step 5: Create the crate .gitignore**

`crates/orrch-orrdeal/.gitignore`:
```gitignore
# Terraform working state — never commit
terraform/proxmox-k3s/.terraform/
terraform/proxmox-k3s/.terraform.lock.hcl
terraform/proxmox-k3s/terraform.tfstate
terraform/proxmox-k3s/terraform.tfstate.backup
terraform/proxmox-k3s/kubeconfig.yaml
```

- [ ] **Step 6: Build to verify the crate compiles and is in the workspace**

Run: `cargo build -p orrch-orrdeal`
Expected: `Compiling orrch-orrdeal v0.1.0` then `Finished`. No errors.

- [ ] **Step 7: Commit**

```bash
git add crates/orrch-orrdeal/Cargo.toml crates/orrch-orrdeal/src/lib.rs \
        crates/orrch-orrdeal/src/registry.rs crates/orrch-orrdeal/.gitignore Cargo.toml
git commit -m "feat(orrdeal): scaffold orrch-orrdeal crate"
```

---

### Task 2: Target Registry data model + persistence

**Files:**
- Modify: `crates/orrch-orrdeal/src/registry.rs` (replace stub)

- [ ] **Step 1: Write the full registry module**

Replace the entire contents of `crates/orrch-orrdeal/src/registry.rs` with:
```rust
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
        Self { camera: true, mic: true, ..Self::default() }
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
        orrch_core::config::config_dir().join("orrdeal").join("registry.json")
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
```

- [ ] **Step 2: Build to verify the model compiles**

Run: `cargo build -p orrch-orrdeal`
Expected: `Finished`. No errors. (`orrch_core::config::config_dir` is already public.)

- [ ] **Step 3: Lint**

Run: `cargo clippy -p orrch-orrdeal -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/orrch-orrdeal/src/registry.rs
git commit -m "feat(orrdeal): Target Registry data model + JSON persistence"
```

---

### Task 3: Config loading

**Files:**
- Create: `crates/orrch-orrdeal/src/config.rs`
- Create: `crates/orrch-orrdeal/config.example.json`
- Modify: `crates/orrch-orrdeal/src/lib.rs` (add `pub mod config;`)

- [ ] **Step 1: Write the config module**

`crates/orrch-orrdeal/src/config.rs`:
```rust
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
        orrch_core::config::config_dir().join("orrdeal").join("config.json")
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
```

- [ ] **Step 2: Write the example config**

`crates/orrch-orrdeal/config.example.json`:
```json
{
  "proxmox": {
    "endpoint": "https://orrbit:8006/",
    "node": "orrbit",
    "api_token_env": "ORRDEAL_PVE_TOKEN",
    "template": "debian-12-cloudinit",
    "ssh_user": "orrdeal",
    "ssh_public_key_path": "~/.ssh/orrdeal.pub",
    "ssh_private_key_path": "~/.ssh/orrdeal",
    "cores": 2,
    "memory_mb": 2048
  },
  "mesh": {
    "device": "cb17",
    "ssh_user": "corr",
    "ssh_private_key_path": "~/.ssh/orrdeal"
  }
}
```

- [ ] **Step 3: Register the module**

In `crates/orrch-orrdeal/src/lib.rs`, add below `pub mod registry;`:
```rust
pub mod config;
```

- [ ] **Step 4: Build + lint**

Run: `cargo build -p orrch-orrdeal && cargo clippy -p orrch-orrdeal -- -D warnings`
Expected: `Finished`, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/orrch-orrdeal/src/config.rs crates/orrch-orrdeal/config.example.json crates/orrch-orrdeal/src/lib.rs
git commit -m "feat(orrdeal): JSON config loader + example config"
```

---

### Task 4: The probe agent (single source) + probe parsing

**Files:**
- Create: `crates/orrch-orrdeal/probe/probe.sh`
- Create: `crates/orrch-orrdeal/src/probe.rs`
- Modify: `crates/orrch-orrdeal/src/lib.rs` (add `pub mod probe;`)

- [ ] **Step 1: Write the portable probe script**

`crates/orrch-orrdeal/probe/probe.sh`:
```sh
#!/bin/sh
# orrdeal probe agent — emits ONE JSON object describing the host.
# Must run on minimal images (alpine/busybox) and over SSH. POSIX sh only.
os="$(uname -s)"
if [ -r /etc/os-release ]; then
  # shellcheck disable=SC1091
  . /etc/os-release
  os="${ID:-$os} ${VERSION_ID:-}"
fi
arch="$(uname -m)"

cam=false
for d in /dev/video0 /dev/video1; do
  [ -e "$d" ] && cam=true && break
done
mic=false
[ -e /dev/snd ] && mic=true
gpu=false
{ [ -e /dev/dri ] || [ -e /dev/nvidia0 ]; } && gpu=true

printf '{"os":"%s","arch":"%s","camera":%s,"mic":%s,"gpu":%s,"filesystem":true}\n' \
  "$os" "$arch" "$cam" "$mic" "$gpu"
```

- [ ] **Step 2: Write the probe parser**

`crates/orrch-orrdeal/src/probe.rs`:
```rust
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
```

- [ ] **Step 3: Register the module**

In `crates/orrch-orrdeal/src/lib.rs`, add:
```rust
pub mod probe;
```

- [ ] **Step 4: Verify the probe script runs on this host and emits valid JSON**

Run: `sh crates/orrch-orrdeal/probe/probe.sh`
Expected: a single line like `{"os":"cachyos ","arch":"x86_64","camera":false,"mic":true,"gpu":true,"filesystem":true}` (values depend on the host). It MUST be parseable JSON — pipe through a validator to confirm:
Run: `sh crates/orrch-orrdeal/probe/probe.sh | python3 -c "import json,sys; print(json.load(sys.stdin))"`
Expected: a Python dict printed, no exception.

- [ ] **Step 5: Build + lint**

Run: `cargo build -p orrch-orrdeal && cargo clippy -p orrch-orrdeal -- -D warnings`
Expected: `Finished`, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/orrch-orrdeal/probe/probe.sh crates/orrch-orrdeal/src/probe.rs crates/orrch-orrdeal/src/lib.rs
git commit -m "feat(orrdeal): portable probe agent + typed report parser"
```

---

### Task 5: Prerequisite checks

**Files:**
- Create: `crates/orrch-orrdeal/src/prereq.rs`
- Modify: `crates/orrch-orrdeal/src/lib.rs` (add `pub mod prereq;`)

- [ ] **Step 1: Write the prereq module**

`crates/orrch-orrdeal/src/prereq.rs`:
```rust
//! Fail-fast prerequisite checks. Run before any provisioning so we never leave
//! half-created infrastructure behind.

use std::io::ErrorKind;
use std::process::Command;

use crate::config::OrrdealConfig;

/// Returns true if `bin` exists on PATH (NotFound => false; any other outcome,
/// including a non-zero `--version` exit, counts as present).
fn has_bin(bin: &str) -> bool {
    match Command::new(bin).arg("--version").output() {
        Ok(_) => true,
        Err(e) if e.kind() == ErrorKind::NotFound => false,
        Err(_) => true,
    }
}

/// Verify every external dependency and required secret is present.
/// Returns a list of human-readable problems (empty == good to go).
pub fn check(cfg: &OrrdealConfig) -> Vec<String> {
    let mut problems = Vec::new();

    for bin in ["terraform", "kubectl", "tailscale", "ssh"] {
        if !has_bin(bin) {
            problems.push(format!("required binary `{bin}` not found on PATH"));
        }
    }

    if std::env::var(&cfg.proxmox.api_token_env).is_err() {
        problems.push(format!(
            "Proxmox API token env var `{}` is not set",
            cfg.proxmox.api_token_env
        ));
    }

    // Tailscale must be up for discovery to work.
    match Command::new("tailscale").arg("status").output() {
        Ok(out) if out.status.success() => {}
        Ok(_) => problems.push("`tailscale status` failed — is the tailnet up?".into()),
        Err(_) => problems.push("could not run `tailscale status`".into()),
    }

    problems
}
```

- [ ] **Step 2: Register the module**

In `crates/orrch-orrdeal/src/lib.rs`, add:
```rust
pub mod prereq;
```

- [ ] **Step 3: Build + lint**

Run: `cargo build -p orrch-orrdeal && cargo clippy -p orrch-orrdeal -- -D warnings`
Expected: `Finished`, no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/orrch-orrdeal/src/prereq.rs crates/orrch-orrdeal/src/lib.rs
git commit -m "feat(orrdeal): fail-fast prerequisite checks"
```

---

### Task 6: Discover arm (Tailscale + SSH probe)

**Files:**
- Create: `crates/orrch-orrdeal/src/discover.rs`
- Modify: `crates/orrch-orrdeal/src/lib.rs` (add `pub mod discover;`)

- [ ] **Step 1: Write the discover arm**

`crates/orrch-orrdeal/src/discover.rs`:
```rust
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
        if hostname.eq_ignore_ascii_case(device) && online {
            if let Some(ip) = peer
                .get("TailscaleIPs")
                .and_then(|i| i.as_array())
                .and_then(|a| a.first())
                .and_then(|s| s.as_str())
            {
                return Ok(ip.to_string());
            }
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
```

- [ ] **Step 2: Register the module**

In `crates/orrch-orrdeal/src/lib.rs`, add:
```rust
pub mod discover;
```

- [ ] **Step 3: Build + lint**

Run: `cargo build -p orrch-orrdeal && cargo clippy -p orrch-orrdeal -- -D warnings`
Expected: `Finished`, no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/orrch-orrdeal/src/discover.rs crates/orrch-orrdeal/src/lib.rs
git commit -m "feat(orrdeal): discover arm — tailnet device probe over ssh"
```

---

### Task 7: Terraform module (Proxmox → k3s)

**Files:**
- Create: `crates/orrch-orrdeal/terraform/proxmox-k3s/variables.tf`
- Create: `crates/orrch-orrdeal/terraform/proxmox-k3s/main.tf`
- Create: `crates/orrch-orrdeal/terraform/proxmox-k3s/outputs.tf`

- [ ] **Step 1: Write variables.tf**

`crates/orrch-orrdeal/terraform/proxmox-k3s/variables.tf`:
```hcl
variable "pve_endpoint"        { type = string }
variable "pve_node"            { type = string }
variable "pve_api_token"       { type = string, sensitive = true }
variable "template"            { type = string }
variable "vm_name"             { type = string, default = "orrdeal-skeleton" }
variable "cores"               { type = number, default = 2 }
variable "memory_mb"           { type = number, default = 2048 }
variable "ssh_user"            { type = string }
variable "ssh_public_key_path" { type = string }
variable "ssh_private_key_path"{ type = string }
```

- [ ] **Step 2: Write main.tf**

`crates/orrch-orrdeal/terraform/proxmox-k3s/main.tf`:
```hcl
terraform {
  required_providers {
    proxmox = {
      source  = "bpg/proxmox"
      version = "~> 0.66"
    }
  }
}

provider "proxmox" {
  endpoint  = var.pve_endpoint
  api_token = var.pve_api_token
  insecure  = true
}

resource "proxmox_virtual_environment_vm" "k3s" {
  name      = var.vm_name
  node_name = var.pve_node

  clone {
    vm_id = tonumber(var.template) # numeric template VM id; or use a data source for name lookup
  }

  cpu { cores = var.cores }
  memory { dedicated = var.memory_mb }
  agent { enabled = true }

  initialization {
    user_account {
      username = var.ssh_user
      keys     = [trimspace(file(pathexpand(var.ssh_public_key_path)))]
    }
    user_data_file_id = null
    # cloud-init runs the inline user data below via the datastore-provided drive.
  }

  # Install k3s on first boot, then expose a readable kubeconfig.
  initialization {
    dns {}
  }
}

# Fetch the kubeconfig once the agent reports an IP, localizing the server address.
resource "null_resource" "kubeconfig" {
  depends_on = [proxmox_virtual_environment_vm.k3s]

  connection {
    type        = "ssh"
    host        = proxmox_virtual_environment_vm.k3s.ipv4_addresses[1][0]
    user        = var.ssh_user
    private_key = file(pathexpand(var.ssh_private_key_path))
    timeout     = "5m"
  }

  provisioner "remote-exec" {
    inline = [
      "curl -sfL https://get.k3s.io | sudo sh -",
      "sudo install -m 644 /etc/rancher/k3s/k3s.yaml /home/${var.ssh_user}/k3s.yaml",
      "sudo chown ${var.ssh_user} /home/${var.ssh_user}/k3s.yaml",
    ]
  }

  provisioner "local-exec" {
    command = <<-EOT
      scp -o StrictHostKeyChecking=accept-new -i ${pathexpand(var.ssh_private_key_path)} \
        ${var.ssh_user}@${proxmox_virtual_environment_vm.k3s.ipv4_addresses[1][0]}:k3s.yaml \
        ${path.module}/kubeconfig.yaml
      sed -i 's#https://127.0.0.1:6443#https://${proxmox_virtual_environment_vm.k3s.ipv4_addresses[1][0]}:6443#' \
        ${path.module}/kubeconfig.yaml
    EOT
  }
}
```

> **Note for the executor:** the exact `proxmox_virtual_environment_vm` cloud-init wiring varies with the bpg provider version and how the template was built. The template referenced by `var.template` MUST be a cloud-init-enabled image with the qemu-guest-agent installed (so `ipv4_addresses` is populated). If the inline `initialization` blocks conflict in your provider version, consolidate them into one `initialization { user_account { … } }` block — the load-bearing parts are: clone the template, inject the SSH key, boot, then the `null_resource` installs k3s and fetches the kubeconfig. Do not silently skip the kubeconfig localization `sed` — the provision arm depends on `kubeconfig.yaml` pointing at the VM's real IP.

- [ ] **Step 3: Write outputs.tf**

`crates/orrch-orrdeal/terraform/proxmox-k3s/outputs.tf`:
```hcl
output "node_ipv4" {
  value = proxmox_virtual_environment_vm.k3s.ipv4_addresses[1][0]
}

output "kubeconfig_path" {
  value = "${path.module}/kubeconfig.yaml"
}
```

- [ ] **Step 4: Validate HCL syntax (no apply yet)**

Run: `cd crates/orrch-orrdeal/terraform/proxmox-k3s && terraform init -backend=false && terraform validate`
Expected: `Success! The configuration is valid.` (Provider download happens during init; `validate` checks syntax/refs without contacting Proxmox.)
If `validate` reports a provider-version-specific schema error on the `initialization` blocks, fix per the note in Step 2 until `terraform validate` passes. Then `cd` back to repo root.

- [ ] **Step 5: Commit**

```bash
git add crates/orrch-orrdeal/terraform/proxmox-k3s/
git commit -m "feat(orrdeal): terraform proxmox-k3s module (single-node k3s)"
```

---

### Task 8: Provision arm (Terraform + kubectl probe Job)

**Files:**
- Create: `crates/orrch-orrdeal/src/provision.rs`
- Modify: `crates/orrch-orrdeal/src/lib.rs` (add `pub mod provision;`)

- [ ] **Step 1: Write the provision arm**

`crates/orrch-orrdeal/src/provision.rs`:
```rust
//! Provision arm: terraform apply (k3s on Proxmox) → kubectl apply a probe Job →
//! read its logs → Target. Independently fallible.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, anyhow};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::config::ProxmoxConfig;
use crate::probe::{PROBE_SH, ProbeReport};
use crate::registry::{
    CapabilityFlags, InputModality, Reach, Target, TargetSource, TargetStatus, UiSurface,
};

/// Path to the bundled terraform module (relative to the crate at runtime).
fn module_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("terraform/proxmox-k3s")
}

async fn run_checked(mut cmd: Command, what: &str) -> anyhow::Result<std::process::Output> {
    let out = cmd.output().await.with_context(|| format!("spawning {what}"))?;
    if !out.status.success() {
        return Err(anyhow!(
            "{what} failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out)
}

/// terraform init + apply; returns (node_ipv4, kubeconfig_path).
async fn terraform_apply(cfg: &ProxmoxConfig) -> anyhow::Result<(String, String)> {
    let dir = module_dir();
    let token = std::env::var(&cfg.api_token_env)
        .with_context(|| format!("env var {} not set", cfg.api_token_env))?;

    run_checked(
        {
            let mut c = Command::new("terraform");
            c.arg(format!("-chdir={}", dir.display()))
                .args(["init", "-input=false"]);
            c
        },
        "terraform init",
    )
    .await?;

    run_checked(
        {
            let mut c = Command::new("terraform");
            c.arg(format!("-chdir={}", dir.display()))
                .args(["apply", "-auto-approve", "-input=false"])
                .arg(format!("-var=pve_endpoint={}", cfg.endpoint))
                .arg(format!("-var=pve_node={}", cfg.node))
                .arg(format!("-var=pve_api_token={token}"))
                .arg(format!("-var=template={}", cfg.template))
                .arg(format!("-var=cores={}", cfg.cores))
                .arg(format!("-var=memory_mb={}", cfg.memory_mb))
                .arg(format!("-var=ssh_user={}", cfg.ssh_user))
                .arg(format!("-var=ssh_public_key_path={}", cfg.ssh_public_key_path))
                .arg(format!("-var=ssh_private_key_path={}", cfg.ssh_private_key_path));
            c
        },
        "terraform apply",
    )
    .await?;

    let out = run_checked(
        {
            let mut c = Command::new("terraform");
            c.arg(format!("-chdir={}", dir.display()))
                .args(["output", "-json"]);
            c
        },
        "terraform output",
    )
    .await?;

    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let ip = v["node_ipv4"]["value"]
        .as_str()
        .ok_or_else(|| anyhow!("terraform output missing node_ipv4"))?
        .to_string();
    let kubeconfig = v["kubeconfig_path"]["value"]
        .as_str()
        .ok_or_else(|| anyhow!("terraform output missing kubeconfig_path"))?
        .to_string();
    Ok((ip, kubeconfig))
}

/// Build the probe Job manifest with probe.sh embedded as the container command.
fn probe_job_manifest() -> String {
    // probe.sh is indented under a YAML block scalar.
    let indented: String = PROBE_SH
        .lines()
        .map(|l| format!("              {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"apiVersion: batch/v1
kind: Job
metadata:
  name: orrdeal-probe
spec:
  backoffLimit: 0
  ttlSecondsAfterFinished: 120
  template:
    spec:
      restartPolicy: Never
      containers:
        - name: probe
          image: alpine:3.20
          command: ["sh", "-c"]
          args:
            - |
{indented}
"#
    )
}

/// kubectl apply the probe Job, wait for completion, return its logs.
async fn kubectl_probe(kubeconfig: &str) -> anyhow::Result<String> {
    let manifest = probe_job_manifest();

    // apply via stdin
    let mut child = Command::new("kubectl")
        .args(["--kubeconfig", kubeconfig, "apply", "-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning kubectl apply")?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("kubectl stdin unavailable"))?
        .write_all(manifest.as_bytes())
        .await?;
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        return Err(anyhow!(
            "kubectl apply failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    run_checked(
        {
            let mut c = Command::new("kubectl");
            c.args([
                "--kubeconfig", kubeconfig,
                "wait", "--for=condition=complete", "job/orrdeal-probe",
                "--timeout=120s",
            ]);
            c
        },
        "kubectl wait",
    )
    .await?;

    let logs = run_checked(
        {
            let mut c = Command::new("kubectl");
            c.args(["--kubeconfig", kubeconfig, "logs", "job/orrdeal-probe"]);
            c
        },
        "kubectl logs",
    )
    .await?;
    Ok(String::from_utf8_lossy(&logs.stdout).into_owned())
}

/// Run the provision arm. Always returns a Target.
pub async fn run(cfg: &ProxmoxConfig) -> Target {
    let make = |status: TargetStatus, note: String, os: String, arch: String, caps: CapabilityFlags| Target {
        id: "ephemeral:proxmox-k3s".into(),
        source: TargetSource::Ephemeral,
        ui_surface: UiSurface::Desktop,                  // source default — refined later
        capabilities: caps,
        input: InputModality::Pointer,                   // source default — refined later
        arch,
        os,
        reach: Reach::Pod("default/orrdeal-probe".into()),
        status,
        note,
    };

    let kubeconfig = match terraform_apply(cfg).await {
        Ok((_ip, kc)) => kc,
        Err(e) => {
            return make(
                TargetStatus::Unreachable,
                format!("provision failed: {e}"),
                String::new(),
                String::new(),
                CapabilityFlags::default(),
            );
        }
    };

    match kubectl_probe(&kubeconfig).await {
        Ok(logs) => match ProbeReport::parse(&logs) {
            Ok(r) => make(
                TargetStatus::Reachable,
                String::new(),
                r.os.clone(),
                r.arch.clone(),
                r.capabilities(),
            ),
            Err(e) => make(
                TargetStatus::ProbeFailed,
                e.to_string(),
                String::new(),
                String::new(),
                CapabilityFlags::default(),
            ),
        },
        Err(e) => make(
            TargetStatus::ProbeFailed,
            e.to_string(),
            String::new(),
            String::new(),
            CapabilityFlags::default(),
        ),
    }
}

/// terraform destroy — teardown for `skeleton down`.
pub async fn destroy(cfg: &ProxmoxConfig) -> anyhow::Result<()> {
    let dir = module_dir();
    let token = std::env::var(&cfg.api_token_env)
        .with_context(|| format!("env var {} not set", cfg.api_token_env))?;
    run_checked(
        {
            let mut c = Command::new("terraform");
            c.arg(format!("-chdir={}", dir.display()))
                .args(["destroy", "-auto-approve", "-input=false"])
                .arg(format!("-var=pve_endpoint={}", cfg.endpoint))
                .arg(format!("-var=pve_node={}", cfg.node))
                .arg(format!("-var=pve_api_token={token}"))
                .arg(format!("-var=template={}", cfg.template))
                .arg(format!("-var=cores={}", cfg.cores))
                .arg(format!("-var=memory_mb={}", cfg.memory_mb))
                .arg(format!("-var=ssh_user={}", cfg.ssh_user))
                .arg(format!("-var=ssh_public_key_path={}", cfg.ssh_public_key_path))
                .arg(format!("-var=ssh_private_key_path={}", cfg.ssh_private_key_path));
            c
        },
        "terraform destroy",
    )
    .await?;
    Ok(())
}
```

> **Executor note:** both `terraform_apply` and `destroy` read the Proxmox token the same way — `std::env::var(&cfg.api_token_env)`. Keep them identical.

- [ ] **Step 2: Register the module**

In `crates/orrch-orrdeal/src/lib.rs`, add:
```rust
pub mod provision;
```

- [ ] **Step 3: Build + lint**

Run: `cargo build -p orrch-orrdeal && cargo clippy -p orrch-orrdeal -- -D warnings`
Expected: `Finished`, no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/orrch-orrdeal/src/provision.rs crates/orrch-orrdeal/src/lib.rs
git commit -m "feat(orrdeal): provision arm — terraform k3s + kubectl probe job"
```

---

### Task 9: Reporter

**Files:**
- Create: `crates/orrch-orrdeal/src/report.rs`
- Modify: `crates/orrch-orrdeal/src/lib.rs` (add `pub mod report;`)

- [ ] **Step 1: Write the reporter**

`crates/orrch-orrdeal/src/report.rs`:
```rust
//! Human-readable report of a Registry — the thing you LOOK AT to know the run worked.

use crate::registry::{Registry, Target, TargetStatus};

fn caps_summary(t: &Target) -> String {
    let c = &t.capabilities;
    let mut on = Vec::new();
    if c.camera { on.push("cam"); }
    if c.mic { on.push("mic"); }
    if c.gpu { on.push("gpu"); }
    if c.filesystem { on.push("fs"); }
    if c.public_web_host { on.push("web-host"); }
    if on.is_empty() { "-".into() } else { on.join("+") }
}

fn status_mark(s: TargetStatus) -> &'static str {
    match s {
        TargetStatus::Reachable => "OK  ✓",
        TargetStatus::Unreachable => "UNREACHABLE ✗",
        TargetStatus::ProbeFailed => "PROBE-FAILED ✗",
    }
}

/// Print the registry as a table; return how many targets are Reachable.
pub fn print(reg: &Registry) -> usize {
    println!("\n  orrdeal — skeleton run report");
    println!("  ─────────────────────────────────────────────────────────────────────");
    println!(
        "  {:<28} {:<10} {:<8} {:<12} {}",
        "TARGET", "OS", "ARCH", "CAPS", "STATUS"
    );
    let mut reachable = 0;
    for t in &reg.targets {
        if t.status == TargetStatus::Reachable {
            reachable += 1;
        }
        println!(
            "  {:<28} {:<10} {:<8} {:<12} {}",
            t.id,
            if t.os.is_empty() { "-" } else { &t.os },
            if t.arch.is_empty() { "-" } else { &t.arch },
            caps_summary(t),
            status_mark(t.status),
        );
        if !t.note.is_empty() {
            println!("      └─ {}", t.note);
        }
    }
    println!("  ─────────────────────────────────────────────────────────────────────");
    println!("  {reachable}/{} targets reachable\n", reg.targets.len());
    reachable
}
```

- [ ] **Step 2: Register the module**

In `crates/orrch-orrdeal/src/lib.rs`, add:
```rust
pub mod report;
```

- [ ] **Step 3: Build + lint**

Run: `cargo build -p orrch-orrdeal && cargo clippy -p orrch-orrdeal -- -D warnings`
Expected: `Finished`, no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/orrch-orrdeal/src/report.rs crates/orrch-orrdeal/src/lib.rs
git commit -m "feat(orrdeal): unified report printer"
```

---

### Task 10: CLI orchestration + run_cli

**Files:**
- Create: `crates/orrch-orrdeal/src/cli.rs`
- Modify: `crates/orrch-orrdeal/src/lib.rs` (add `pub mod cli;`, replace `run_cli`)

- [ ] **Step 1: Write the CLI module**

`crates/orrch-orrdeal/src/cli.rs`:
```rust
//! Subcommand dispatch + orchestration. `skeleton run` fans the two arms out
//! concurrently, merges into the registry, prints the report, and sets the exit
//! code by DoD. `skeleton down` tears the ephemeral target down.

use anyhow::{Context, anyhow};

use crate::config::OrrdealConfig;
use crate::registry::Registry;
use crate::{discover, prereq, provision, report};

const USAGE: &str = "\
orrdeal — heterogeneous test fabric (walking skeleton)

USAGE:
  orrchestrator orrdeal skeleton run    Provision + discover + probe; print report
  orrchestrator orrdeal skeleton down   Tear down the ephemeral (k3s) target
";

pub async fn run_cli(args: &[String]) -> anyhow::Result<()> {
    match args {
        [a, b, ..] if a == "skeleton" && b == "run" => skeleton_run().await,
        [a, b, ..] if a == "skeleton" && b == "down" => skeleton_down().await,
        _ => {
            print!("{USAGE}");
            Ok(())
        }
    }
}

async fn skeleton_run() -> anyhow::Result<()> {
    let cfg = OrrdealConfig::load()?;

    let problems = prereq::check(&cfg);
    if !problems.is_empty() {
        eprintln!("orrdeal: prerequisites not met:");
        for p in &problems {
            eprintln!("  - {p}");
        }
        return Err(anyhow!("prerequisite check failed"));
    }

    // Both arms run concurrently; each always resolves to a Target.
    let (ephemeral, mesh) =
        tokio::join!(provision::run(&cfg.proxmox), discover::run(&cfg.mesh));

    let mut reg = Registry::new();
    reg.add(ephemeral);
    reg.add(mesh);
    reg.save().context("saving registry.json")?;

    let reachable = report::print(&reg);

    if reachable < reg.targets.len() {
        // DoD not fully met — surface a non-zero exit without panicking.
        std::process::exit(1);
    }
    Ok(())
}

async fn skeleton_down() -> anyhow::Result<()> {
    let cfg = OrrdealConfig::load()?;
    provision::destroy(&cfg.proxmox).await?;
    println!("orrdeal: ephemeral target destroyed.");
    Ok(())
}
```

- [ ] **Step 2: Replace `run_cli` in lib.rs to delegate to cli**

In `crates/orrch-orrdeal/src/lib.rs`, add `pub mod cli;` with the other module declarations, and replace the placeholder `run_cli` function with:
```rust
/// Entry point invoked by the `orrchestrator orrdeal …` subcommand.
pub async fn run_cli(args: &[String]) -> anyhow::Result<()> {
    cli::run_cli(args).await
}
```

- [ ] **Step 3: Build + lint**

Run: `cargo build -p orrch-orrdeal && cargo clippy -p orrch-orrdeal -- -D warnings`
Expected: `Finished`, no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/orrch-orrdeal/src/cli.rs crates/orrch-orrdeal/src/lib.rs
git commit -m "feat(orrdeal): skeleton run/down CLI orchestration"
```

---

### Task 11: Wire the subcommand into the orrchestrator binary

**Files:**
- Modify: `src/Cargo.toml` (add dependency)
- Modify: `src/src/main.rs` (dispatch `orrdeal`)

- [ ] **Step 1: Add the dependency**

In `src/Cargo.toml`, under `[dependencies]`, add after the `orrch-webui` line:
```toml
orrch-orrdeal = { path = "../crates/orrch-orrdeal" }
```

- [ ] **Step 2: Dispatch the subcommand in main.rs**

In `src/src/main.rs`, find the block that handles `--web` (it ends with `return open_webui_in_browser();` followed by a `}`). Immediately AFTER that closing brace and BEFORE the `if !io::stdout().is_terminal() {` check, insert:
```rust
    // `orrdeal …` — heterogeneous test fabric subcommand. Dispatched before the
    // terminal-capability check because it's a non-TUI command-line tool.
    if args.first().map(String::as_str) == Some("orrdeal") {
        return orrch_orrdeal::run_cli(&args[1..]).await;
    }
```

- [ ] **Step 3: Add an `orrdeal` line to the `--help` output**

In the `--help` block in `src/src/main.rs`, after the `--webedit` help line, add:
```rust
        println!("  orrchestrator orrdeal …  Heterogeneous test fabric (try: orrdeal skeleton run)");
```

- [ ] **Step 4: Build the whole binary**

Run: `cargo build -p orrchestrator`
Expected: `Finished`. No errors.

- [ ] **Step 5: Verify dispatch works (usage path, no infra needed)**

Run: `cargo run -p orrchestrator -- orrdeal`
Expected: prints the orrdeal USAGE text (the `skeleton run`/`skeleton down` lines), exits 0. This confirms the subcommand is wired without touching Proxmox/Tailscale.

- [ ] **Step 6: Commit**

```bash
git add src/Cargo.toml src/src/main.rs
git commit -m "feat(orrdeal): wire orrdeal subcommand into orchestrator binary"
```

---

### Task 12: End-to-end observation run + teardown (the real DoD)

This task has no code — it is the **observed verification** the repo's testing rules require. Do it on a host that has the prerequisites real (this machine, with `terraform`/`kubectl`/`tailscale`/`ssh`, a Proxmox token, and `cb17` online).

- [ ] **Step 1: Seed the config**

Run:
```bash
mkdir -p ~/.config/orrchestrator/orrdeal
cp crates/orrch-orrdeal/config.example.json ~/.config/orrchestrator/orrdeal/config.json
```
Then edit `~/.config/orrchestrator/orrdeal/config.json` for real values (Proxmox endpoint/node/template, SSH keypair, `mesh.device` = `cb17`, `mesh.ssh_user`). Export the token: `export ORRDEAL_PVE_TOKEN='user@pam!tokenid=secret'`.

- [ ] **Step 2: Run the skeleton and LOOK at the output**

Run: `cargo run -p orrchestrator -- orrdeal skeleton run`
Expected (observed, not assumed): a report table listing **2 targets** —
`ephemeral:proxmox-k3s` and `mesh:cb17` — each with a non-empty OS + arch, a CAPS summary, and `OK ✓` status; final line `2/2 targets reachable`.
State the result as observed (e.g. "the report shows mesh:cb17 as debian/aarch64 OK and ephemeral:proxmox-k3s as alpine/x86_64 OK — I am looking at it").

- [ ] **Step 3: Confirm the registry was persisted**

Run: `cat ~/.config/orrchestrator/orrdeal/registry.json`
Expected: JSON with two target objects matching what the report showed.

- [ ] **Step 4: Observe graceful degradation (negative check)**

Temporarily set `mesh.device` to a bogus hostname in the config, then:
Run: `cargo run -p orrchestrator -- orrdeal skeleton run; echo "exit=$?"`
Expected: report shows `ephemeral:proxmox-k3s` `OK ✓` and `mesh:<bogus>` `UNREACHABLE ✗` with a note; final line `1/2 targets reachable`; `exit=1`. Restore the real `cb17` afterward.

- [ ] **Step 5: Tear down the ephemeral target**

Run: `cargo run -p orrchestrator -- orrdeal skeleton down`
Expected: `orrdeal: ephemeral target destroyed.` Verify in the Proxmox UI that the `orrdeal-skeleton` VM is gone.

- [ ] **Step 6: Record the observed outcome**

Append a dated entry to `orrchestrator/.orrch/DEVLOG.md` describing what you observed in Steps 2–5 (the actual OS/arch values reported, that 2/2 was reached, that degradation produced exit=1, that teardown removed the VM). No commit of secrets or registry.json.

---

## Closing handoff — automated tests (separate cold session)

Per `CLAUDE.md`, automated tests are authored by a **different session / cold reader**, asserting on user-visible behavior. Hand that session this checklist (do NOT implement here):

- A test that drives `orrdeal skeleton run` against a disposable target set and asserts the **report contains two targets with non-empty os/arch and Reachable status** (parse stdout or `registry.json`, not internal mocks).
- A test that breaks one arm (bogus mesh hostname / revoked token) and asserts the **run degrades gracefully**: the other arm still Reachable, exit code 1.
- A unit-level supplement: feed `ProbeReport::parse` a real captured probe line and a banner-prefixed line, assert both parse; feed garbage, assert it errors. (This asserts observable parsing behavior against real probe output, not an abstract value.)

## Self-review notes (author)

- **Spec coverage:** 3-axis registry (Task 2) ✓; Tailscale discovery (Task 6) ✓; Proxmox/k3s provisioning (Tasks 7–8) ✓; single probe agent, two delivery paths (Task 4 + used in 6/8) ✓; independently-fallible arms + exit code (Tasks 6/8/10) ✓; teardown (Task 8 `destroy` + Task 12 down) ✓; observed DoD (Task 12) ✓; config under `~/.config/orrchestrator/orrdeal/` JSON (Task 3) ✓; out-of-scope items untouched ✓.
- **Spec deviation (intentional):** spec §3.1 said "config.toml"; the repo uses JSON config (`orrch_core::config`), so this plan uses `config.json` to match convention.
- **Type consistency:** `Target`/`CapabilityFlags`/`Reach`/`TargetStatus` defined once (Task 2) and used unchanged by `probe.rs`, `discover.rs`, `provision.rs`, `report.rs`. `run_cli(&[String])` signature consistent between lib, cli, and the main.rs call site.
