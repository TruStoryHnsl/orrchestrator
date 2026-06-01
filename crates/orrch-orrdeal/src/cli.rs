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
