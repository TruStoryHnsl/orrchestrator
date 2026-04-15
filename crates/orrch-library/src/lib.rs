pub mod item;
pub mod store;
pub mod model;
pub mod harness;
pub mod mcp;
pub mod sync;
pub mod templates;
pub mod pi_ext;

pub use item::{LibraryItem, ItemKind};
pub use store::LibraryStore;
pub use model::{ModelEntry, ModelTier, PricingModel, Valve, ValveStore, load_models};
pub use harness::{HarnessEntry, load_harnesses};
pub use mcp::{McpServerEntry, McpTransport, load_mcp_servers, save_mcp_server, load_mcp_servers_from_claude_configs, load_all_mcp_servers};
pub use sync::{clone_if_missing, sync_pull, sync_push};
pub use pi_ext::{load_pi_extensions, translate_skill_to_pi_extension, translate_tool_to_pi_extension};

/// Canonical path (relative to the project root) where per-harness / per-model
/// translated context files and syntax catalogs live. Downstream tooling (the
/// Syntax Translation Engine, PLAN.md item 63) should use this constant so the
/// location stays in one place.
pub const TRANSLATIONS_DIR: &str = "library/translations";
