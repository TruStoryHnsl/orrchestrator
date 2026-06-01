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
