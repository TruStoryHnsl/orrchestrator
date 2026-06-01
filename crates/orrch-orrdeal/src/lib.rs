//! orrdeal — heterogeneous test fabric (walking skeleton).
//!
//! Orchestrates external tools (terraform, kubectl, tailscale, ssh) to
//! provision + discover + probe targets, normalizing them into one registry.

pub mod cli;
pub mod config;
pub mod discover;
pub mod prereq;
pub mod probe;
pub mod provision;
pub mod registry;
pub mod report;

/// Entry point invoked by the `orrchestrator orrdeal …` subcommand.
pub async fn run_cli(args: &[String]) -> anyhow::Result<()> {
    cli::run_cli(args).await
}
