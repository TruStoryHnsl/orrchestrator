// ── crate orrch-hwfit, module `types` (src/types.rs) ──────────────────────
// Shared structs ALL three modules compile against. No logic here beyond
// serde derives + Default. Re-exported at crate root: `pub use types::*;`
//
// Cargo.toml deps: serde = { workspace = true, features=["derive"] },
//   serde_json = { workspace = true }, anyhow = { workspace = true }.
// (workspace serde already enables "derive"; add the feature explicitly to be safe.)

use serde::{Deserialize, Serialize};

/// One model row from data/hf_models.json. serde-loaded.
/// `#[serde(default)]` on the struct so any missing field gets its type default,
/// matching Python's dict.get(k, default) semantics. Unknown JSON fields ignored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CatalogModel {
    pub name: String,
    pub provider: String,
    pub parameter_count: String, // e.g. "7B", "355", "80K"  (may be empty)
    pub parameters_raw: Option<u64>, // raw param count when present (>0 wins over parameter_count)
    pub min_ram_gb: f64,
    pub recommended_ram_gb: Option<f64>, // None → analyze falls back to required_gb
    pub min_vram_gb: f64,
    pub quantization: String,        // native quant label; "" when absent
    pub context_length: Option<u64>, // None/0 → treated as 4096 by callers
    pub use_case: String,
    pub capabilities: Vec<String>,
    pub pipeline_tag: String,
    pub architecture: String,
    pub is_moe: bool,
    pub active_parameters: Option<u64>, // MoE active params (per-token)
    pub num_experts: Option<u32>,
    pub active_experts: Option<u32>,
    pub gguf_sources: Vec<serde_json::Value>, // opaque list; only emptiness is read in logic
    pub hf_downloads: i64,
    pub hf_likes: i64,
    pub release_date: Option<String>,
    pub format: String,
}

/// One physical GPU. Mirrors Python gpu dict {index,name,vram_gb}.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Gpu {
    pub index: Option<u32>,
    pub name: String,
    pub vram_gb: f64,
}

/// A homogeneous GPU pool (identical name + rounded VRAM). Mirrors _group_gpus output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuGroup {
    pub name: String,
    pub vram_each: f64, // round(vram_gb,1) of one card
    pub count: u32,
    pub indices: Vec<Option<u32>>,
    pub vram_total: f64, // round(vram_each*count,1)
}

/// Detected (or SSH-probed) system hardware. Carries exactly the fields the
/// task mandates. `Default` = an all-zero CPU-only box.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemInfo {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub cpu_cores: u32,
    pub cpu_name: String,
    pub has_gpu: bool,
    pub gpu_name: Option<String>,
    pub gpu_vram_gb: Option<f64>,
    pub gpu_count: u32,
    pub gpus: Vec<Gpu>,
    pub gpu_groups: Vec<GpuGroup>,
    pub backend: String, // "cuda" | "rocm" | "cpu_x86" | "cpu_arm"
    // ── optional/diagnostic fields (Python carried these on the dict) ──
    #[serde(default)]
    pub homogeneous: bool, // gpu_groups.len() <= 1
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unified_memory: Option<bool>, // AMD APU UMA flag
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_error: Option<String>, // nvidia-smi present but driver error
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>, // "Cannot connect to <host>"
    #[serde(default)]
    pub gpu_only: bool, // user picked explicit GPU config → no RAM offload
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_rand_read_gbps: Option<f64>, // conservative random-read GB/s for disk-backed MoE
}

/// Per-component scores 0..=100. Mirrors the nested "scores" object.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FitScores {
    pub quality: f64,
    pub speed: f64,
    pub fit: f64,
    pub context: f64,
}

/// Result of analyzing one model against one system. Mirrors analyze_model dict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitResult {
    pub name: String,
    pub provider: String,
    pub parameter_count: String,
    pub params_b: f64, // round(pb,1)
    pub is_moe: bool,
    pub use_case: String,
    pub fit_level: String, // "perfect"|"good"|"marginal"|"too_tight"
    pub run_mode: String,  // "gpu"|"moe_offload"|"cpu_offload"|"cpu_only"|"disk_stream"|"no_fit"
    pub quant: String,
    pub context: u64,
    pub required_gb: f64, // round(...,1)
    pub speed_tps: f64,   // round(...,1)
    pub score: f64,       // round(composite,1)
    pub scores: FitScores,
    pub gguf_sources: Vec<serde_json::Value>,
    pub context_length: u64,
}
