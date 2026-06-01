// ── module `fit` (src/fit.rs) — scoring + ranking ──
// Faithful port of odysseus services/hwfit/fit.py. Formulas and table values
// are VERBATIM from the Python; parity with odysseus is the point.

use crate::models::{
    active_params_b, estimate_memory_gb, infer_use_case, is_prequantized, params_b,
    quant_bytes_per_param, quant_quality_penalty, quant_speed_mult, QUANT_HIERARCHY,
};
use crate::types::{CatalogModel, FitResult, FitScores, SystemInfo};

// Verbatim tables (same keys/values/order as Python):
pub const GPU_BANDWIDTH: &[(&str, u32)] = &[
    ("5090", 1792),
    ("5080", 960),
    ("5070 ti", 896),
    ("5070", 672),
    ("5060 ti", 448),
    ("5060", 256),
    ("4090", 1008),
    ("4080 super", 736),
    ("4080", 717),
    ("4070 ti super", 672),
    ("4070 ti", 504),
    ("4070 super", 504),
    ("4070", 504),
    ("4060 ti", 288),
    ("4060", 272),
    ("3090 ti", 1008),
    ("3090", 936),
    ("3080 ti", 912),
    ("3080", 760),
    ("3070 ti", 608),
    ("3070", 448),
    ("3060 ti", 448),
    ("3060", 360),
    ("2080 ti", 616),
    ("2080 super", 496),
    ("2080", 448),
    ("2070 super", 448),
    ("2070", 448),
    ("2060 super", 448),
    ("2060", 336),
    ("1660 ti", 288),
    ("1660 super", 336),
    ("1660", 192),
    ("1650 super", 192),
    ("1650", 128),
    ("h100 sxm", 3350),
    ("h100", 2039),
    ("h200", 4800),
    ("a100 sxm", 2039),
    ("a100", 1555),
    ("l40s", 864),
    ("l40", 864),
    ("l4", 300),
    ("a10g", 600),
    ("a10", 600),
    ("t4", 320),
    ("v100 sxm", 900),
    ("v100", 897),
    ("a6000", 768),
    ("a5000", 768),
    ("a4000", 448),
    ("7900 xtx", 960),
    ("7900 xt", 800),
    ("7900 gre", 576),
    ("7800 xt", 624),
    ("7700 xt", 432),
    ("7600", 288),
    ("6950 xt", 576),
    ("6900 xt", 512),
    ("6800 xt", 512),
    ("6800", 512),
    ("6700 xt", 384),
    ("6600 xt", 256),
    ("6600", 224),
    ("mi300x", 5300),
    ("mi300", 5300),
    ("mi250x", 3277),
    ("mi250", 3277),
    ("mi210", 1638),
    ("mi100", 1229),
    ("9070 xt", 624),
    ("9070", 488),
];

pub const FALLBACK_K: &[(&str, u32)] =
    &[("cuda", 220), ("rocm", 180), ("cpu_x86", 70), ("cpu_arm", 90)];

// (wq, ws, wf, wc); default (0.45, 0.30, 0.15, 0.10)
pub const USE_CASE_WEIGHTS: &[(&str, (f64, f64, f64, f64))] = &[
    ("general", (0.45, 0.30, 0.15, 0.10)),
    ("coding", (0.50, 0.20, 0.15, 0.15)),
    ("reasoning", (0.55, 0.15, 0.15, 0.15)),
    ("chat", (0.40, 0.35, 0.15, 0.10)),
    ("multimodal", (0.50, 0.20, 0.15, 0.15)),
    ("embedding", (0.30, 0.40, 0.20, 0.10)),
    ("tts", (0.40, 0.35, 0.15, 0.10)),
    ("stt", (0.40, 0.35, 0.15, 0.10)),
];

pub const SPEED_TARGET: &[(&str, u32)] = &[
    ("general", 40),
    ("coding", 40),
    ("multimodal", 40),
    ("chat", 40),
    ("reasoning", 25),
    ("embedding", 200),
    ("tts", 40),
    ("stt", 40),
];

pub const CONTEXT_TARGET: &[(&str, u32)] = &[
    ("general", 4096),
    ("chat", 4096),
    ("coding", 8192),
    ("reasoning", 8192),
    ("multimodal", 4096),
    ("embedding", 512),
    ("tts", 2048),
    ("stt", 2048),
];

fn lookup_u32(table: &[(&str, u32)], key: &str, default: u32) -> u32 {
    table
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or(default)
}

/// _lookup_bandwidth: lowercase gpu_name; iterate GPU_BANDWIDTH keys sorted by
/// len DESC; first substring hit wins. None when no match / no name.
pub fn lookup_bandwidth(gpu_name: Option<&str>) -> Option<u32> {
    let gn = gpu_name?;
    if gn.is_empty() {
        return None;
    }
    let gn = gn.to_lowercase();
    // Pre-sort keys by length descending for correct substring matching.
    let mut keys: Vec<&(&str, u32)> = GPU_BANDWIDTH.iter().collect();
    keys.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (key, val) in keys {
        if gn.contains(key) {
            return Some(*val);
        }
    }
    None
}

/// _estimate_speed: tok/s. Uses active params for MoE.
pub fn estimate_speed(
    model: &CatalogModel,
    quant: &str,
    run_mode: &str,
    system: &SystemInfo,
) -> f64 {
    let pb = active_params_b(model);
    let is_moe = model.is_moe;
    let bw = lookup_bandwidth(system.gpu_name.as_deref());
    let backend = if system.backend.is_empty() {
        "cpu_x86"
    } else {
        system.backend.as_str()
    };

    if let Some(bw) = bw {
        if run_mode == "gpu" || run_mode == "cpu_offload" {
            let bpp = quant_bytes_per_param(quant);
            let model_gb = pb * bpp;
            if model_gb <= 0.0 {
                return 0.0;
            }
            let efficiency = 0.55;
            let raw_tps = (bw as f64 / model_gb) * efficiency;
            let mode_factor = if run_mode == "cpu_offload" {
                0.5
            } else if is_moe {
                0.8
            } else {
                1.0
            };
            return raw_tps * mode_factor;
        }
    }

    let k = lookup_u32(FALLBACK_K, backend, 70) as f64;
    if pb <= 0.0 {
        return 0.0;
    }
    let sm = quant_speed_mult(quant);
    k / pb * sm
}

/// _quality_score
pub fn quality_score(model: &CatalogModel, quant: &str, use_case: &str) -> f64 {
    let pb = params_b(model);
    let mut base: f64 = if pb < 1.0 {
        30.0
    } else if pb < 3.0 {
        45.0
    } else if pb < 7.0 {
        60.0
    } else if pb < 10.0 {
        75.0
    } else if pb < 20.0 {
        82.0
    } else if pb < 40.0 {
        89.0
    } else {
        95.0
    };

    let name_lower = model.name.to_lowercase();
    if name_lower.contains("qwen") {
        base += 2.0;
    }
    if name_lower.contains("deepseek") {
        base += 3.0;
    }
    if name_lower.contains("llama") {
        base += 2.0;
    }
    if name_lower.contains("mistral") || name_lower.contains("mixtral") {
        base += 1.0;
    }
    if name_lower.contains("gemma") {
        base += 1.0;
    }

    base += quant_quality_penalty(quant);

    let model_uc = infer_use_case(model);
    if model_uc == "coding" && use_case == "coding" {
        base += 6.0;
    }
    if model_uc == "reasoning" && use_case == "reasoning" && pb >= 13.0 {
        base += 5.0;
    }
    if model_uc == "multimodal" && use_case == "multimodal" {
        base += 6.0;
    }

    base.clamp(0.0, 100.0)
}

/// _speed_score
pub fn speed_score(tps: f64, use_case: &str) -> f64 {
    let target = lookup_u32(SPEED_TARGET, use_case, 40) as f64;
    ((tps / target) * 100.0).clamp(0.0, 100.0)
}

/// _fit_score
pub fn fit_score(required: f64, available: f64) -> f64 {
    if required > available {
        return 0.0;
    }
    if available <= 0.0 {
        return 0.0;
    }
    let ratio = required / available;
    if ratio <= 0.5 {
        return 60.0 + (ratio / 0.5) * 40.0;
    }
    if ratio <= 0.8 {
        return 100.0;
    }
    if ratio <= 0.9 {
        return 70.0;
    }
    50.0
}

/// _context_score
pub fn context_score(ctx: u64, use_case: &str) -> f64 {
    let target = lookup_u32(CONTEXT_TARGET, use_case, 4096) as f64;
    let ctx = ctx as f64;
    if ctx >= target {
        return 100.0;
    }
    if ctx >= target / 2.0 {
        return 70.0;
    }
    30.0
}

/// _try_quant_at: try a specific quant at a given context.
/// Returns (run_mode, quant, ctx, mem) or None.
pub fn try_quant_at(
    model: &CatalogModel,
    quant: &str,
    ctx: u64,
    gpu_vram: f64,
    available_ram: f64,
) -> Option<(String, String, u64, f64)> {
    let mem = estimate_memory_gb(model, quant, ctx);
    if gpu_vram > 0.0 && mem <= gpu_vram {
        return Some(("gpu".to_string(), quant.to_string(), ctx, mem));
    }
    if gpu_vram > 0.0 && mem <= available_ram {
        return Some(("cpu_offload".to_string(), quant.to_string(), ctx, mem));
    }
    if gpu_vram <= 0.0 && mem <= available_ram {
        return Some(("cpu_only".to_string(), quant.to_string(), ctx, mem));
    }
    // Try halving context
    let mut cur_ctx = ctx / 2;
    while cur_ctx >= 1024 {
        let mem = estimate_memory_gb(model, quant, cur_ctx);
        if gpu_vram > 0.0 && mem <= gpu_vram {
            return Some(("gpu".to_string(), quant.to_string(), cur_ctx, mem));
        }
        if mem <= available_ram {
            let mode = if gpu_vram > 0.0 {
                "cpu_offload"
            } else {
                "cpu_only"
            };
            return Some((mode.to_string(), quant.to_string(), cur_ctx, mem));
        }
        cur_ctx /= 2;
    }
    None
}

/// _quant_bits: approximate bit-width of a quant label. 0 = unknown.
pub fn quant_bits(q: &str) -> u32 {
    let qu = q
        .to_uppercase()
        .replace('-', "")
        .replace('_', "")
        .replace(' ', "");
    // GGUF k-quants + float formats
    if qu.starts_with("Q8") || qu.contains("FP8") {
        return 8;
    }
    if qu.starts_with("Q4") || qu.starts_with("IQ4") {
        return 4;
    }
    if qu.starts_with("Q2") || qu.starts_with("IQ2") {
        return 2;
    }
    if qu.starts_with("Q3") || qu.starts_with("IQ3") {
        return 3;
    }
    if qu.starts_with("Q5") {
        return 5;
    }
    if qu.starts_with("Q6") {
        return 6;
    }
    if qu.starts_with("F16") || qu.starts_with("BF16") || qu.starts_with("F32") {
        return 16;
    }
    // Prequantized formats: pull the bit-width digit.
    // Python: re.search(r"(?:AWQ|GPTQ|MLX|EXL2|BNB|INT|W)(\d{1,2})", qu)
    //         or re.search(r"(\d{1,2})BIT", qu)
    let prefixes = ["AWQ", "GPTQ", "MLX", "EXL2", "BNB", "INT", "W"];
    let mut found: Option<u32> = None;
    'outer: for p in prefixes {
        let mut start = 0usize;
        while let Some(pos) = qu[start..].find(p) {
            let after = start + pos + p.len();
            if let Some(b) = parse_leading_digits(&qu[after..]) {
                found = Some(b);
                break 'outer;
            }
            start = start + pos + 1;
        }
    }
    if found.is_none() {
        // (\d{1,2})BIT
        if let Some(pos) = qu.find("BIT") {
            // grab up to 2 trailing digits before "BIT"
            let pre = &qu[..pos];
            let digits: String = pre
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            let digits = if digits.len() > 2 {
                digits[digits.len() - 2..].to_string()
            } else {
                digits
            };
            if let Ok(b) = digits.parse::<u32>() {
                if !digits.is_empty() {
                    found = Some(b);
                }
            }
        }
    }
    if let Some(b) = found {
        if (2..=16).contains(&b) {
            return b;
        }
    }
    0
}

/// Parse 1-2 leading digits (Python \d{1,2}). Returns None if no leading digit.
fn parse_leading_digits(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).take(2).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// analyze_model
pub fn analyze_model(
    model: &CatalogModel,
    system: &SystemInfo,
    target_quant: Option<&str>,
) -> Option<FitResult> {
    let pb = params_b(model);
    if pb <= 0.0 {
        return None;
    }

    let use_case = infer_use_case(model);
    let has_gpu = system.has_gpu;
    let gpu_vram = if has_gpu {
        system.gpu_vram_gb.unwrap_or(0.0)
    } else {
        0.0
    };
    let gpu_count = if system.gpu_count == 0 {
        1
    } else {
        system.gpu_count
    };
    let single_gpu_vram = if gpu_count > 1 {
        gpu_vram / gpu_count as f64
    } else {
        gpu_vram
    };
    let available_ram = system.available_ram_gb;
    let gpu_only = system.gpu_only && has_gpu && gpu_vram > 0.0;
    let eff_ram = if gpu_only { 0.0 } else { available_ram };
    let is_moe = model.is_moe;
    let ctx = match model.context_length {
        Some(c) if c != 0 => c,
        _ => 4096,
    };

    let native_quant = if model.quantization.is_empty() {
        "Q4_K_M".to_string()
    } else {
        model.quantization.clone()
    };
    let preq = is_prequantized(model);

    // GGUF models can't be sharded across GPUs — use single GPU VRAM
    let is_gguf = !model.gguf_sources.is_empty();
    let quant_upper = native_quant.to_uppercase();
    let is_gguf_quant = ["Q2", "Q3", "Q4", "Q5", "Q6", "Q8", "IQ", "F16", "F32"]
        .iter()
        .any(|p| quant_upper.starts_with(p));
    let effective_vram = if (is_gguf || is_gguf_quant) && !preq {
        single_gpu_vram
    } else {
        gpu_vram
    };

    // Determine which quant to evaluate at
    let quant_to_try: String = if preq {
        if let Some(tq) = target_quant {
            let tb = quant_bits(tq);
            let nb = quant_bits(&native_quant);
            if tb != 0 && nb != 0 && tb != nb {
                return None;
            }
        }
        native_quant.clone()
    } else if let Some(tq) = target_quant {
        tq.to_string()
    } else {
        "Q4_K_M".to_string()
    };

    let mut result = try_quant_at(model, &quant_to_try, ctx, effective_vram, eff_ram);

    // If target quant doesn't fit and it's not pre-quantized, try lower quants
    if result.is_none() && !preq {
        if let Some(tq) = target_quant {
            let idx = QUANT_HIERARCHY.iter().position(|&q| q == tq);
            let start = match idx {
                Some(i) => i + 1,
                None => 0, // Python: index(-1) -> -1, slice [0:] = whole list
            };
            for q in &QUANT_HIERARCHY[start..] {
                result = try_quant_at(model, q, ctx, effective_vram, eff_ram);
                if result.is_some() {
                    break;
                }
            }
        }
    }

    let (run_mode, quant, fit_ctx, required_gb) = match result {
        Some(r) => r,
        None => {
            // Model doesn't fit — surface with too_tight badge.
            let oversized_required = estimate_memory_gb(model, &quant_to_try, ctx);
            return Some(FitResult {
                name: model.name.clone(),
                provider: model.provider.clone(),
                parameter_count: model.parameter_count.clone(),
                params_b: round1(pb),
                is_moe,
                use_case: use_case.clone(),
                fit_level: "too_tight".to_string(),
                run_mode: "no_fit".to_string(),
                quant: quant_to_try.clone(),
                context: ctx,
                required_gb: round1(oversized_required),
                speed_tps: 0.0,
                score: 0.0,
                scores: FitScores {
                    quality: 0.0,
                    speed: 0.0,
                    fit: 0.0,
                    context: 0.0,
                },
                gguf_sources: model.gguf_sources.clone(),
                context_length: model.context_length.unwrap_or(4096),
            });
        }
    };

    // Determine fit level
    let budget = if run_mode == "gpu" {
        effective_vram
    } else {
        available_ram
    };
    if required_gb > budget {
        return None;
    }
    let fit_level = if run_mode == "gpu" {
        let rec = model.recommended_ram_gb.unwrap_or(required_gb);
        if rec <= gpu_vram {
            "perfect"
        } else if gpu_vram >= required_gb * 1.2 {
            "good"
        } else {
            "marginal"
        }
    } else if run_mode == "cpu_offload" {
        if available_ram >= required_gb * 1.2 {
            "good"
        } else {
            "marginal"
        }
    } else {
        "marginal"
    };

    let tps = estimate_speed(model, &quant, &run_mode, system);

    let q_score = quality_score(model, &quant, &use_case);
    let s_score = speed_score(tps, &use_case);
    let f_score = fit_score(required_gb, budget);
    let c_score = context_score(fit_ctx, &use_case);

    let (wq, ws, wf, wc) = USE_CASE_WEIGHTS
        .iter()
        .find(|(k, _)| *k == use_case)
        .map(|(_, w)| *w)
        .unwrap_or((0.45, 0.30, 0.15, 0.10));
    let composite = q_score * wq + s_score * ws + f_score * wf + c_score * wc;

    Some(FitResult {
        name: model.name.clone(),
        provider: model.provider.clone(),
        parameter_count: model.parameter_count.clone(),
        params_b: round1(pb),
        is_moe,
        use_case,
        fit_level: fit_level.to_string(),
        run_mode,
        quant,
        context: fit_ctx,
        required_gb: round1(required_gb),
        speed_tps: round1(tps),
        score: round1(composite),
        scores: FitScores {
            quality: round1(q_score),
            speed: round1(s_score),
            fit: round1(f_score),
            context: round1(c_score),
        },
        gguf_sources: model.gguf_sources.clone(),
        context_length: model.context_length.unwrap_or(4096),
    })
}

/// SORT_KEYS: score->score, speed->speed_tps, vram->required_gb,
/// params->params_b, context->context.
pub fn sort_key(result: &FitResult, key: &str) -> f64 {
    match key {
        "speed" => result.speed_tps,
        "vram" => result.required_gb,
        "params" => result.params_b,
        "context" => result.context as f64,
        _ => result.score,
    }
}

#[derive(Debug, Clone, Default)]
pub struct RankOptions<'a> {
    pub use_case: Option<&'a str>,
    pub limit: usize,
    pub search: Option<&'a str>,
    pub sort: &'a str,
    pub quant: Option<&'a str>,
}

/// rank_models. (image_gen branch from Python is OUT OF SCOPE for v1 — TODO.)
pub fn rank_models(
    models: &[CatalogModel],
    system: &SystemInfo,
    opts: &RankOptions,
) -> Vec<FitResult> {
    // image_gen is out of scope; ignore it.
    let mut results: Vec<FitResult> = Vec::new();

    // If user picked a prequantized format (AWQ/FP8/GPTQ), filter to only those.
    let filter_native = opts
        .quant
        .map(|q| q.starts_with("AWQ-") || q.starts_with("GPTQ-") || q.starts_with("FP8"))
        .unwrap_or(false);

    // MLX-quantized models only run on Apple Silicon (Metal).
    let system_backend = system.backend.to_lowercase();
    let apple_silicon =
        matches!(system_backend.as_str(), "mps" | "metal" | "apple");

    for m in models {
        let native_q = m.quantization.as_str();

        // Drop MLX models on non-Apple hardware
        if !apple_silicon && native_q.starts_with("mlx-") {
            continue;
        }

        // Format filter: AWQ tab → only AWQ models, FP8 tab → only FP8, etc.
        if filter_native {
            let quant = opts.quant.unwrap_or("");
            if quant == "FP8" && native_q != "FP8" {
                continue;
            }
            if quant.starts_with("AWQ") && !native_q.starts_with("AWQ") {
                continue;
            }
            if quant.starts_with("GPTQ") && !native_q.starts_with("GPTQ") {
                continue;
            }
        }

        if let Some(search) = opts.search {
            let name = m.name.to_lowercase();
            let provider = m.provider.to_lowercase();
            let s = search.to_lowercase();
            if !name.contains(&s) && !provider.contains(&s) {
                continue;
            }
        }

        let result = match analyze_model(m, system, opts.quant) {
            Some(r) => r,
            None => continue,
        };

        if let Some(use_case) = opts.use_case {
            let model_uc = infer_use_case(m);
            if use_case != model_uc && use_case != "general" {
                continue;
            }
        }

        results.push(result);
    }

    // Pick the visible SET by best fit (score) first (stable), truncate, THEN
    // order by the requested column.
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let limit = if opts.limit == 0 { 50 } else { opts.limit };
    results.truncate(limit);

    let sort = if opts.sort.is_empty() { "score" } else { opts.sort };
    // vram ascending (smallest first), everything else descending (biggest first)
    if sort == "vram" {
        results.sort_by(|a, b| {
            sort_key(a, sort)
                .partial_cmp(&sort_key(b, sort))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    } else {
        results.sort_by(|a, b| {
            sort_key(b, sort)
                .partial_cmp(&sort_key(a, sort))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    results
}
