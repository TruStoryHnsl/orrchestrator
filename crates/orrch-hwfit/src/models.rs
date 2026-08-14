// ── module `models` (src/models.rs) — pure functions + const tables ──
// Faithful Rust port of odysseus services/hwfit/models.py. Quant tables and
// the memory formula carry the Python values VERBATIM — parity with odysseus
// is the point, not "improving" the math.

use crate::types::CatalogModel;

// Raw verbatim tables (exact Python values, same key set & order):
pub const QUANT_BPP: &[(&str, f64)] = &[
    ("F32", 4.0),
    ("F16", 2.0),
    ("BF16", 2.0),
    ("FP8", 1.0),
    ("Q8_0", 1.05),
    ("Q6_K", 0.80),
    ("Q5_K_M", 0.68),
    ("Q4_K_M", 0.58),
    ("Q4_0", 0.58),
    ("Q3_K_M", 0.48),
    ("Q2_K", 0.37),
    ("AWQ-4bit", 0.50),
    ("AWQ-8bit", 1.0),
    ("GPTQ-Int4", 0.50),
    ("GPTQ-Int8", 1.0),
    ("mlx-4bit", 0.55),
    ("mlx-8bit", 1.0),
    ("mlx-6bit", 0.75),
];

pub const QUANT_SPEED_MULT: &[(&str, f64)] = &[
    ("F16", 0.6),
    ("BF16", 0.6),
    ("FP8", 0.85),
    ("Q8_0", 0.8),
    ("Q6_K", 0.95),
    ("Q5_K_M", 1.0),
    ("Q4_K_M", 1.15),
    ("Q4_0", 1.15),
    ("Q3_K_M", 1.25),
    ("Q2_K", 1.35),
    ("AWQ-4bit", 1.2),
    ("AWQ-8bit", 0.85),
    ("GPTQ-Int4", 1.2),
    ("GPTQ-Int8", 0.85),
    ("mlx-4bit", 1.15),
    ("mlx-8bit", 0.85),
    ("mlx-6bit", 1.0),
];

pub const QUANT_QUALITY_PENALTY: &[(&str, f64)] = &[
    ("F16", 0.0),
    ("BF16", 0.0),
    ("FP8", 0.0),
    ("Q8_0", 0.0),
    ("Q6_K", -1.0),
    ("Q5_K_M", -2.0),
    ("Q4_K_M", -5.0),
    ("Q4_0", -5.0),
    ("Q3_K_M", -8.0),
    ("Q2_K", -12.0),
    ("AWQ-4bit", -3.0),
    ("AWQ-8bit", 0.0),
    ("GPTQ-Int4", -3.0),
    ("GPTQ-Int8", 0.0),
    ("mlx-4bit", -4.0),
    ("mlx-8bit", 0.0),
    ("mlx-6bit", -1.0),
];

pub const QUANT_BYTES_PER_PARAM: &[(&str, f64)] = &[
    ("F16", 2.0),
    ("BF16", 2.0),
    ("FP8", 1.0),
    ("Q8_0", 1.0),
    ("Q6_K", 0.75),
    ("Q5_K_M", 0.625),
    ("Q4_K_M", 0.5),
    ("Q4_0", 0.5),
    ("Q3_K_M", 0.375),
    ("Q2_K", 0.25),
    ("AWQ-4bit", 0.5),
    ("AWQ-8bit", 1.0),
    ("GPTQ-Int4", 0.5),
    ("GPTQ-Int8", 1.0),
    ("mlx-4bit", 0.5),
    ("mlx-8bit", 1.0),
    ("mlx-6bit", 0.75),
];

pub const QUANT_HIERARCHY: &[&str] = &["Q8_0", "Q6_K", "Q5_K_M", "Q4_K_M", "Q3_K_M", "Q2_K"];
pub const PREQUANTIZED_PREFIXES: &[&str] = &["AWQ-", "GPTQ-", "mlx-", "FP8"];

fn table_lookup(table: &[(&str, f64)], q: &str, default: f64) -> f64 {
    for (k, v) in table {
        if *k == q {
            return *v;
        }
    }
    default
}

// Lookups with Python's defaulting baked in where callers used .get(k, default):
pub fn quant_bpp(q: &str) -> f64 {
    table_lookup(QUANT_BPP, q, 0.58)
} // default 0.58 (matches estimate_memory_gb)
pub fn quant_bytes_per_param(q: &str) -> f64 {
    table_lookup(QUANT_BYTES_PER_PARAM, q, 0.5)
} // default 0.5  (matches _estimate_speed)
pub fn quant_speed_mult(q: &str) -> f64 {
    table_lookup(QUANT_SPEED_MULT, q, 1.0)
} // default 1.0
pub fn quant_quality_penalty(q: &str) -> f64 {
    table_lookup(QUANT_QUALITY_PENALTY, q, 0.0)
} // default 0.0

// Core model math:

/// Port of params_b(model). parameters_raw>0 wins (raw/1e9); else parse the
/// parameter_count string against ^([\d.]+)\s*([BKMGT]?)$ after trim+upper.
pub fn params_b(model: &CatalogModel) -> f64 {
    if let Some(raw) = model.parameters_raw
        && raw > 0
    {
        return raw as f64 / 1_000_000_000.0;
    }

    let pc = model.parameter_count.trim().to_uppercase();
    if pc.is_empty() {
        return 0.0;
    }

    if let Some((num_str, suffix)) = parse_param_count(&pc)
        && let Ok(val) = num_str.parse::<f64>()
    {
        return match suffix {
            'B' => val,
            'M' => val / 1000.0,
            'K' => val / 1_000_000.0,
            'T' => val * 1000.0,
            _ => {
                // No unit. A bare number is conventionally a millions count
                // (e.g. "355" = 355M). Very large bare values are raw counts.
                if val >= 1_000_000.0 {
                    val / 1_000_000_000.0 // raw count
                } else if val >= 1000.0 {
                    val / 1000.0 // thousands of millions? treat as millions
                } else {
                    val / 1000.0 // e.g. "355" → 0.355B
                }
            }
        };
    }
    0.0
}

/// Match ^([\d.]+)\s*([BKMGT]?)$ on an already trim+uppercased string.
/// Returns (digits, suffix_char). suffix '\0' = no unit. None = no match.
fn parse_param_count(pc: &str) -> Option<(&str, char)> {
    let bytes = pc.as_bytes();
    // Leading [\d.]+ run.
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }
    if i == 0 {
        return None; // need at least one [\d.] char
    }
    let digits = &pc[..i];
    // Optional \s* run.
    let mut j = i;
    while j < bytes.len() && (bytes[j] as char).is_whitespace() {
        j += 1;
    }
    // Optional single [BKMGT].
    let suffix = if j < bytes.len() {
        let c = bytes[j] as char;
        if matches!(c, 'B' | 'K' | 'M' | 'G' | 'T') {
            j += 1;
            c
        } else {
            return None;
        }
    } else {
        '\0'
    };
    // Must consume the whole string (anchored $).
    if j != bytes.len() {
        return None;
    }
    Some((digits, suffix))
}

/// Python _active_params_b, renamed public (no leading _).
pub fn active_params_b(model: &CatalogModel) -> f64 {
    if model.is_moe
        && let Some(active) = model.active_parameters
    {
        // Python: `if is_moe and active_parameters` — 0 is falsy.
        if active != 0 {
            return active as f64 / 1_000_000_000.0;
        }
    }
    params_b(model)
}

pub fn is_prequantized(model: &CatalogModel) -> bool {
    let q = &model.quantization;
    PREQUANTIZED_PREFIXES.iter().any(|p| q.starts_with(p))
}

/// VERBATIM: pb*bpp + 0.000008*active_pb*ctx + 0.5
pub fn estimate_memory_gb(model: &CatalogModel, quant: &str, ctx: u64) -> f64 {
    let pb = params_b(model);
    let bpp = quant_bpp(quant);
    let kv_params = active_params_b(model);
    pb * bpp + 0.000008 * kv_params * (ctx as f64) + 0.5
}

/// Find best quant that fits in budget_gb of VRAM.
/// Pre-quantized models (AWQ/GPTQ/MLX) use their native quant only.
/// Returns (quant, ctx, mem_gb) or None.
pub fn best_quant_for_budget(
    model: &CatalogModel,
    budget_gb: f64,
    ctx: u64,
) -> Option<(String, u64, f64)> {
    if is_prequantized(model) {
        let q = if model.quantization.is_empty() {
            "Q4_K_M".to_string()
        } else {
            model.quantization.clone()
        };
        let mem = estimate_memory_gb(model, &q, ctx);
        if mem <= budget_gb {
            return Some((q, ctx, mem));
        }
        // Try halving context.
        let mut cur_ctx = ctx / 2;
        while cur_ctx >= 1024 {
            let mem = estimate_memory_gb(model, &q, cur_ctx);
            if mem <= budget_gb {
                return Some((q, cur_ctx, mem));
            }
            cur_ctx /= 2;
        }
        return None;
    }

    // GGUF: try best quality first, then fall back.
    for q in QUANT_HIERARCHY {
        let mem = estimate_memory_gb(model, q, ctx);
        if mem <= budget_gb {
            return Some(((*q).to_string(), ctx, mem));
        }
    }

    let mut cur_ctx = ctx / 2;
    while cur_ctx >= 1024 {
        for q in QUANT_HIERARCHY {
            let mem = estimate_memory_gb(model, q, cur_ctx);
            if mem <= budget_gb {
                return Some(((*q).to_string(), cur_ctx, mem));
            }
        }
        cur_ctx /= 2;
    }

    None
}

pub fn infer_use_case(model: &CatalogModel) -> String {
    let name = model.name.to_lowercase();
    let uc = model.use_case.to_lowercase();
    let combined = format!("{} {}", name, uc);

    let any = |keys: &[&str]| keys.iter().any(|k| combined.contains(k));

    if any(&["embedding", "embed", "bge"]) {
        return "embedding".to_string();
    }
    if any(&[
        "tts",
        "text-to-speech",
        "speech-synthesis",
        "cosyvoice",
        "parler",
    ]) {
        return "tts".to_string();
    }
    if any(&["stt", "speech-to-text", "whisper", "transcri", "asr"]) {
        return "stt".to_string();
    }
    if combined.contains("code") {
        return "coding".to_string();
    }
    if any(&["vision", "multimodal", "vlm", "vl-"]) {
        return "multimodal".to_string();
    }
    if any(&["reason", "chain-of-thought", "deepseek-r1"]) {
        return "reasoning".to_string();
    }
    if any(&["chat", "instruction"]) {
        return "chat".to_string();
    }
    "general".to_string()
}

// Catalog loader (serde). On parse/IO error returns empty Vec (mirrors Python's
// FileNotFoundError / JSONDecodeError fallback to []).
pub fn load_catalog(path: &std::path::Path) -> Vec<CatalogModel> {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

/// data/hf_models.json under CARGO_MANIFEST_DIR (the vendored sibling catalog).
pub fn default_catalog_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("hf_models.json")
}
