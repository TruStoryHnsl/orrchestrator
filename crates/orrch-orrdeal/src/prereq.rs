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
