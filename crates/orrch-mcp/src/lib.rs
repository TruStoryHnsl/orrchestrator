//! Library face of the orrch-mcp crate.
//!
//! Exists primarily so integration tests under `tests/` can call into the
//! MCP `dispatch()` entry point and construct an `OrrchMcpServer` against
//! fixture directories. The main binary at `src/main.rs` continues to use
//! these modules directly via `mod` declarations.

pub mod protocol;
pub mod server;
pub mod tools;
