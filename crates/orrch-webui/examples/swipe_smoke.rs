//! Standalone smoke harness for the dual-page swipe terminal.
//!
//! Launches `WebUiServer` on `SMOKE_PORT` (or an OS-assigned port) and
//! prints the bound port to stdout so test scripts can scrape it.
//! Idles on Ctrl-C. Used for live verification — exercises the same
//! code path as the real binary's WebUI but without any of the TUI.

use orrch_webui::{WebUiConfig, WebUiServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = std::env::var("SMOKE_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let cfg = WebUiConfig {
        local_port: port,
        ..Default::default()
    };
    let srv = WebUiServer::start_with_config(cfg).await?;
    println!("LISTENING_ON_PORT {}", srv.port);
    tokio::signal::ctrl_c().await?;
    Ok(())
}
