// =====================================================================
// HWF-006  src/refresh.rs  — catalog merge (pure) + optional HF pull (network)
// =====================================================================
//
// Network is feature-isolated (`hf-pull`) so the MERGE logic unit-tests with
// ZERO network. Ports add_hwfit_models.py main(): by_name = {m.name: m}; for
// each incoming entry, skip if name already present AND !overwrite, else
// insert; apply the per-repo override map on top of each incoming entry before
// merge.

use crate::types::CatalogModel;
use std::collections::BTreeMap;

/// Per-repo hand-corrections, keyed by model NAME (== HF repo id), each a sparse
/// map of field→JSON value. Mirrors EXTRA_REPOS' override dicts
/// ({"parameter_count":"168B","quantization":"Q4_K_M"} etc.). Applied to an
/// incoming CatalogModel before it is merged. Pure data; unit-testable.
pub type OverrideMap = BTreeMap<String, serde_json::Map<String, serde_json::Value>>;

/// Outcome of a merge — counts + the merged catalog, so callers can report
/// "added/updated N (was M, now K)" like the Python script's stdout.
#[derive(Debug, Clone, Default)]
pub struct MergeReport {
    pub added: Vec<String>,   // names newly inserted
    pub updated: Vec<String>, // names overwritten (only when overwrite=true)
    pub was: usize,           // catalog len before
    pub now: usize,           // catalog len after
}

/// Apply `overrides[name]` (if any) onto a single incoming entry by round-
/// tripping through serde_json::Value and patching top-level fields. Keeps
/// parameters_raw consistent when an override sets parameter_count (mirrors the
/// Python `_parse_params("x/"+pc)` reconciliation): if override sets
/// parameter_count but not parameters_raw, parameters_raw is cleared so
/// params_b() re-derives from the label.
pub fn apply_override(entry: &CatalogModel, overrides: &OverrideMap) -> CatalogModel {
    let Some(ov) = overrides.get(&entry.name) else {
        return entry.clone();
    };
    let mut v = serde_json::to_value(entry).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(obj) = &mut v {
        for (k, val) in ov {
            obj.insert(k.clone(), val.clone());
        }
        if ov.contains_key("parameter_count") && !ov.contains_key("parameters_raw") {
            obj.insert("parameters_raw".into(), serde_json::Value::Null);
        }
    }
    serde_json::from_value(v).unwrap_or_else(|_| entry.clone())
}

/// PURE merge — NO network. This is the unit-test target.
///   * `existing` = current catalog (loaded from data/hf_models.json).
///   * `incoming` = candidate entries (from a live pull or a test fixture).
///   * `overrides` applied to every incoming entry before merge.
///   * `overwrite=false` ⇒ keep existing entry when names collide (DEFAULT).
///   * `overwrite=true`  ⇒ incoming (post-override) replaces existing.
/// Returns (merged_catalog, report). Order: existing order preserved; new
/// entries appended in `incoming` order (Python dict-insertion order).
pub fn merge_catalog(
    existing: &[CatalogModel],
    incoming: &[CatalogModel],
    overrides: &OverrideMap,
    overwrite: bool,
) -> (Vec<CatalogModel>, MergeReport) {
    let was = existing.len();
    let mut order: Vec<String> = existing.iter().map(|m| m.name.clone()).collect();
    let mut by_name: std::collections::HashMap<String, CatalogModel> = existing
        .iter()
        .map(|m| (m.name.clone(), m.clone()))
        .collect();
    let mut report = MergeReport {
        was,
        ..Default::default()
    };

    for raw in incoming {
        let entry = apply_override(raw, overrides);
        let exists = by_name.contains_key(&entry.name);
        if exists && !overwrite {
            continue;
        }
        if exists {
            report.updated.push(entry.name.clone());
        } else {
            report.added.push(entry.name.clone());
            order.push(entry.name.clone());
        }
        by_name.insert(entry.name.clone(), entry);
    }

    let merged: Vec<CatalogModel> = order
        .into_iter()
        .filter_map(|n| by_name.remove(&n))
        .collect();
    report.now = merged.len();
    (merged, report)
}

/// Merge into the on-disk catalog at `path`, writing a `.bak` first (Python
/// parity). No-op write when nothing added/updated. Returns the report.
pub fn merge_into_file(
    path: &std::path::Path,
    incoming: &[CatalogModel],
    overrides: &OverrideMap,
    overwrite: bool,
) -> anyhow::Result<MergeReport> {
    let existing = crate::models::load_catalog(path);
    let (merged, report) = merge_catalog(&existing, incoming, overrides, overwrite);
    if report.added.is_empty() && report.updated.is_empty() {
        return Ok(report);
    }
    std::fs::write(
        path.with_extension("json.bak"),
        serde_json::to_string_pretty(&existing)?,
    )?;
    std::fs::write(path, serde_json::to_string_pretty(&merged)?)?;
    Ok(report)
}

// =====================================================================
// LIVE HF Hub pull (network; reqwest blocking). Gated behind `hf-pull` so
// `cargo test -p orrch-hwfit` never touches the network.
// =====================================================================

/// Parse "(parameters_raw, active_parameters)" from a repo name. Port of the
/// Python `_parse_params`: handles dense ("27B") and MoE ("235B-A22B") naming.
/// The `-A<num>B` active-experts token (with a negative lookahead so "8bit" is
/// not read as "8B") is stripped before the first plausible size token is read.
#[cfg(feature = "hf-pull")]
pub(crate) fn parse_params(name: &str) -> (Option<u64>, Option<u64>) {
    let base = name.rsplit('/').next().unwrap_or(name);

    // Find "-A<num>B" not followed by a letter. We scan manually (no regex dep).
    let find_size = |s: &str, require_dash_a: bool| -> Option<(u64, usize, usize)> {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            // optional "-A" prefix
            let start = i;
            if require_dash_a {
                if bytes[i] != b'-' {
                    i += 1;
                    continue;
                }
                if i + 1 >= bytes.len() || (bytes[i + 1] != b'A' && bytes[i + 1] != b'a') {
                    i += 1;
                    continue;
                }
                i += 2;
            }
            // number [\d.]+
            let num_start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i == num_start {
                i = start + 1;
                continue;
            }
            // suffix B/b
            if i < bytes.len() && (bytes[i] == b'B' || bytes[i] == b'b') {
                // negative lookahead: next char must not be a letter
                let after = i + 1;
                let ok = after >= bytes.len() || !(bytes[after] as char).is_ascii_alphabetic();
                if ok {
                    if let Ok(val) = s[num_start..i].parse::<f64>() {
                        return Some(((val * 1e9) as u64, start, after));
                    }
                }
            }
            i = start + 1;
        }
        None
    };

    let mut active = None;
    let base_wo: String = match find_size(base, true) {
        Some((val, start, end)) => {
            active = Some(val);
            format!("{}{}", &base[..start], &base[end..])
        }
        None => base.to_string(),
    };

    let total = find_size(&base_wo, false).map(|(v, _, _)| v);
    (total, active)
}

/// Quant label from a repo name. Port of `_quant_from_name`.
#[cfg(feature = "hf-pull")]
pub(crate) fn quant_from_name(name: &str) -> String {
    let n = name.to_lowercase();
    let is8 = n.contains("8bit") || n.contains("8-bit") || n.contains("int8");
    if n.contains("awq") {
        return if is8 { "AWQ-8bit" } else { "AWQ-4bit" }.to_string();
    }
    if n.contains("gptq") {
        return if is8 { "GPTQ-Int8" } else { "GPTQ-Int4" }.to_string();
    }
    if n.contains("mlx") {
        if n.contains("6bit") {
            return "mlx-6bit".to_string();
        }
        return if is8 { "mlx-8bit" } else { "mlx-4bit" }.to_string();
    }
    if n.contains("fp8") {
        return "FP8".to_string();
    }
    if n.contains("int4") || n.contains("4bit") || n.contains("4-bit") {
        return "AWQ-4bit".to_string();
    }
    "Q4_K_M".to_string()
}

/// First tag that looks like an architecture name. Port of `_arch_from_tags`.
#[cfg(feature = "hf-pull")]
pub(crate) fn arch_from_tags(tags: &[String]) -> String {
    const GENERIC: &[&str] = &[
        "transformers",
        "safetensors",
        "conversational",
        "text-generation",
        "image-text-to-text",
        "text-generation-inference",
        "endpoints_compatible",
        "autotrain_compatible",
        "compressed-tensors",
        "gguf",
        "mlx",
        "vllm",
        "4-bit",
        "8-bit",
        "awq",
        "gptq",
        "fp8",
        "quantized",
        "chat",
    ];
    for t in tags {
        if t.contains(':') || GENERIC.contains(&t.as_str()) {
            continue;
        }
        let charsok = t
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        let has_alpha = t.chars().any(|c| c.is_ascii_alphabetic());
        if charsok && has_alpha {
            return t.clone();
        }
    }
    String::new()
}

/// Map one HF modelinfo JSON object to a CatalogModel. Port of
/// `_entry_from_modelinfo` (the safetensors / base_model fallbacks that require
/// a second API call are omitted; an unsizable repo is skipped → None).
#[cfg(feature = "hf-pull")]
fn entry_from_modelinfo(mi: &serde_json::Value) -> Option<CatalogModel> {
    let name = mi.get("id").and_then(|v| v.as_str())?.to_string();
    let provider = name.split('/').next().unwrap_or("").to_string();
    let (mut total, active) = parse_params(&name);

    // base_model: tag fallback for size.
    let tags: Vec<String> = mi
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if total.is_none() {
        for t in &tags {
            if let Some(bm) = t.strip_prefix("base_model:") {
                let (bt, _) = parse_params(bm);
                if bt.is_some() {
                    total = bt;
                    break;
                }
            }
        }
    }

    let total = total?; // can't size it — skip
    let pb = total as f64 / 1e9;
    let quant = quant_from_name(&name);

    // Rough RAM/VRAM hints (fit.rs recomputes the real requirement).
    let bpp = match quant.as_str() {
        "AWQ-4bit" | "GPTQ-Int4" => 0.58,
        "mlx-4bit" => 0.55,
        "mlx-6bit" => 0.85,
        "AWQ-8bit" | "GPTQ-Int8" | "mlx-8bit" | "FP8" => 1.1,
        _ => 0.6,
    };
    let round1 = |x: f64| (x * 10.0).round() / 10.0;
    let vram = round1(pb * bpp + 0.5);

    let pipeline_tag = mi
        .get("pipeline_tag")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("text-generation")
        .to_string();

    let mut model = CatalogModel {
        name,
        provider,
        parameter_count: format!("{}B", round1(pb)),
        parameters_raw: Some(total),
        min_ram_gb: f64::max(1.0, round1(vram * 0.6)),
        recommended_ram_gb: Some(f64::max(2.0, round1(vram * 1.2))),
        min_vram_gb: vram,
        quantization: quant,
        context_length: Some(32768),
        use_case: "General purpose".to_string(),
        capabilities: Vec::new(),
        pipeline_tag,
        architecture: arch_from_tags(&tags),
        hf_downloads: mi.get("downloads").and_then(|v| v.as_i64()).unwrap_or(0),
        hf_likes: mi.get("likes").and_then(|v| v.as_i64()).unwrap_or(0),
        ..Default::default()
    };
    if let Some(a) = active {
        model.is_moe = true;
        model.active_parameters = Some(a);
    }
    Some(model)
}

/// LIVE HF Hub pull (network; reqwest blocking). Fetches model metadata for the
/// given authors/repos and maps each to a CatalogModel. Returns the `incoming`
/// vec to feed merge_catalog — pull and merge are SEPARATE so merge stays pure.
/// Gated so `cargo test -p orrch-hwfit` never touches the network.
#[cfg(feature = "hf-pull")]
pub fn fetch_hf_models(authors: &[&str], repos: &[&str]) -> anyhow::Result<Vec<CatalogModel>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("orrch-hwfit/0.1")
        .build()?;
    let mut out: Vec<CatalogModel> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for author in authors {
        let url =
            format!("https://huggingface.co/api/models?author={author}&full=true&cardData=true");
        let resp = client.get(&url).send()?.error_for_status()?;
        let arr: serde_json::Value = resp.json()?;
        if let Some(models) = arr.as_array() {
            for mi in models {
                if let Some(entry) = entry_from_modelinfo(mi) {
                    if seen.insert(entry.name.clone()) {
                        out.push(entry);
                    }
                }
            }
        }
    }

    for repo in repos {
        if seen.contains(*repo) {
            continue;
        }
        let url = format!("https://huggingface.co/api/models/{repo}");
        let resp = match client.get(&url).send() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let Ok(resp) = resp.error_for_status() else {
            continue;
        };
        let Ok(mi) = resp.json::<serde_json::Value>() else {
            continue;
        };
        if let Some(entry) = entry_from_modelinfo(&mi) {
            if seen.insert(entry.name.clone()) {
                out.push(entry);
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(name: &str, params: &str) -> CatalogModel {
        CatalogModel {
            name: name.to_string(),
            parameter_count: params.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn merge_keeps_existing_when_no_overwrite() {
        let existing = vec![model("a", "7B"), model("b", "13B")];
        // incoming collides on "a" (different params) and adds "c".
        let incoming = vec![model("a", "70B"), model("c", "3B")];
        let (merged, report) = merge_catalog(&existing, &incoming, &OverrideMap::new(), false);

        // "a" is NOT overwritten; original 7B preserved.
        let a = merged.iter().find(|m| m.name == "a").unwrap();
        assert_eq!(a.parameter_count, "7B");
        // "c" appended.
        assert!(merged.iter().any(|m| m.name == "c"));
        assert_eq!(report.was, 2);
        assert_eq!(report.now, 3);
        assert_eq!(report.added, vec!["c".to_string()]);
        assert!(report.updated.is_empty());

        // existing order preserved, new appended last.
        let names: Vec<&str> = merged.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn merge_overwrites_when_flagged() {
        let existing = vec![model("a", "7B")];
        let incoming = vec![model("a", "70B")];
        let (merged, report) = merge_catalog(&existing, &incoming, &OverrideMap::new(), true);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].parameter_count, "70B");
        assert_eq!(report.updated, vec!["a".to_string()]);
        assert!(report.added.is_empty());
        assert_eq!(report.was, 1);
        assert_eq!(report.now, 1);
    }

    #[test]
    fn override_applied_to_incoming_before_merge() {
        let existing: Vec<CatalogModel> = vec![];
        let incoming = vec![model("deepseek-ai/DeepSeek-V4-Flash", "")];

        let mut ov = OverrideMap::new();
        let mut fields = serde_json::Map::new();
        fields.insert("parameter_count".into(), serde_json::json!("168B"));
        fields.insert("quantization".into(), serde_json::json!("Q4_K_M"));
        ov.insert("deepseek-ai/DeepSeek-V4-Flash".into(), fields);

        let (merged, report) = merge_catalog(&existing, &incoming, &ov, false);
        assert_eq!(merged.len(), 1);
        let m = &merged[0];
        assert_eq!(m.parameter_count, "168B");
        assert_eq!(m.quantization, "Q4_K_M");
        assert_eq!(report.added.len(), 1);
    }

    #[test]
    fn override_clears_parameters_raw_when_param_count_set() {
        // entry comes in with a stale parameters_raw; override sets a new
        // parameter_count but NOT parameters_raw → raw must be cleared so
        // params_b re-derives from the "168B" label.
        let mut entry = model("x/y", "7B");
        entry.parameters_raw = Some(7_000_000_000);

        let mut ov = OverrideMap::new();
        let mut fields = serde_json::Map::new();
        fields.insert("parameter_count".into(), serde_json::json!("168B"));
        ov.insert("x/y".into(), fields);

        let patched = apply_override(&entry, &ov);
        assert_eq!(patched.parameter_count, "168B");
        assert_eq!(patched.parameters_raw, None);
    }

    #[test]
    fn override_keeps_parameters_raw_when_both_set() {
        let mut entry = model("x/y", "7B");
        entry.parameters_raw = Some(7_000_000_000);

        let mut ov = OverrideMap::new();
        let mut fields = serde_json::Map::new();
        fields.insert("parameter_count".into(), serde_json::json!("168B"));
        fields.insert(
            "parameters_raw".into(),
            serde_json::json!(168_000_000_000u64),
        );
        ov.insert("x/y".into(), fields);

        let patched = apply_override(&entry, &ov);
        assert_eq!(patched.parameters_raw, Some(168_000_000_000));
    }

    #[test]
    fn apply_override_noop_when_name_absent() {
        let entry = model("a", "7B");
        let patched = apply_override(&entry, &OverrideMap::new());
        assert_eq!(patched.parameter_count, "7B");
        assert_eq!(patched.name, "a");
    }

    #[test]
    fn merge_into_file_round_trips_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hf_models.json");
        let existing = vec![model("a", "7B")];
        std::fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        let incoming = vec![model("b", "13B")];
        let report = merge_into_file(&path, &incoming, &OverrideMap::new(), false).unwrap();
        assert_eq!(report.added, vec!["b".to_string()]);

        // .bak written with the ORIGINAL catalog.
        let bak = path.with_extension("json.bak");
        assert!(bak.exists());
        let bak_models: Vec<CatalogModel> =
            serde_json::from_str(&std::fs::read_to_string(&bak).unwrap()).unwrap();
        assert_eq!(bak_models.len(), 1);
        assert_eq!(bak_models[0].name, "a");

        // catalog now has both.
        let now = crate::models::load_catalog(&path);
        assert_eq!(now.len(), 2);
    }

    #[test]
    fn merge_into_file_noop_when_nothing_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hf_models.json");
        let existing = vec![model("a", "7B")];
        std::fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        // incoming collides only; overwrite=false ⇒ nothing added/updated ⇒ no .bak.
        let incoming = vec![model("a", "70B")];
        let report = merge_into_file(&path, &incoming, &OverrideMap::new(), false).unwrap();
        assert!(report.added.is_empty() && report.updated.is_empty());
        assert!(!path.with_extension("json.bak").exists());
    }
}
