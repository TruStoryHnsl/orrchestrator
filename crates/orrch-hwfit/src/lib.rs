// ── crate orrch-hwfit (src/lib.rs) ───────────────────────────────────────
// Hardware-fit analysis: detect a system's RAM/VRAM/backend, then rank an
// LLM catalog by how well each model fits. Port of odysseus hwfit / llmfit.

pub mod fit;
pub mod machines;
pub mod models;
pub mod probe;
pub mod refresh;
pub mod types;

// Re-export key items at crate root.
pub use fit::{RankOptions, analyze_model, rank_models};
pub use machines::{Machine, MachineRegistry, probe_machine, rank_for_machine};
pub use models::{estimate_memory_gb, infer_use_case, is_prequantized, load_catalog, params_b};
pub use probe::{detect_local, detect_system};
pub use refresh::{MergeReport, OverrideMap, apply_override, merge_catalog, merge_into_file};
pub use types::*;
