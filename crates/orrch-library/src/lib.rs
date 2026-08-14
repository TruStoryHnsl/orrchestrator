pub mod harness;
pub mod item;
pub mod mcp;
pub mod model;
pub mod pi_ext;
pub mod store;
pub mod sync;
pub mod templates;

pub use harness::{
    FlexibilityMatrix, HarnessEntry, ToolKind, ToolPolicy, is_tool_allowed, load_harnesses,
    policy_enforceable_on,
};
pub use item::{ItemKind, LibraryItem};
pub use mcp::{
    McpServerEntry, McpTransport, load_all_mcp_servers, load_mcp_servers,
    load_mcp_servers_from_claude_configs, save_mcp_server,
};
pub use model::{
    ApiFormat, EngineLocation, ModelEntry, ModelTier, PricingModel, Valve, ValveStore, load_models,
};
pub use pi_ext::{
    load_pi_extensions, translate_skill_to_pi_extension, translate_tool_to_pi_extension,
};
pub use store::LibraryStore;
pub use sync::{clone_if_missing, sync_pull, sync_push};

/// Canonical path (relative to the project root) where per-harness / per-model
/// translated context files and syntax catalogs live. Downstream tooling (the
/// Syntax Translation Engine, PLAN.md item 63) should use this constant so the
/// location stays in one place.
pub const TRANSLATIONS_DIR: &str = "library/translations";
