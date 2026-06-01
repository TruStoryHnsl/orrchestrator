// ── crate orrch-hwfit (src/lib.rs) ───────────────────────────────────────
// Hardware-fit analysis: detect a system's RAM/VRAM/backend, then rank an
// LLM catalog by how well each model fits. Port of odysseus hwfit / llmfit.

pub mod types;
pub mod models;
pub mod probe;
pub mod fit;

// Re-export key items at crate root.
pub use types::*;
pub use models::{
    estimate_memory_gb, infer_use_case, is_prequantized, load_catalog, params_b,
};
pub use probe::{detect_local, detect_system};
pub use fit::{analyze_model, rank_models, RankOptions};
