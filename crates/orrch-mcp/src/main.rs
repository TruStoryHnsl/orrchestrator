mod server;
mod tools;
mod protocol;

use server::OrrchMcpServer;

#[tokio::main]
async fn main() {
    // All diagnostic output goes to stderr — stdout is reserved for JSON-RPC.
    eprintln!("orrch-mcp-server starting");

    let server = OrrchMcpServer::from_defaults();
    if let Err(e) = protocol::run_stdio(server).await {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}
