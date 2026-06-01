// Independent parity harness for the verifier. Loads the vendored catalog,
// picks representative models by exact name, runs analyze_model against a FIXED
// RTX 3070 fixture, emits one JSON line per model so it can be diffed against
// the odysseus python reference. NOT written by the crate author.

use orrch_hwfit::{
    analyze_model, estimate_memory_gb, load_catalog, models::default_catalog_path, SystemInfo,
};

fn fixture() -> SystemInfo {
    SystemInfo {
        has_gpu: true,
        gpu_name: Some("NVIDIA GeForce RTX 3070".to_string()),
        gpu_vram_gb: Some(8.0),
        gpu_count: 1,
        available_ram_gb: 32.0,
        backend: "cuda".to_string(),
        ..Default::default()
    }
}

const PICKS: &[&str] = &[
    "Qwen/Qwen2.5-7B-Instruct",
    "mistralai/Mistral-7B-Instruct-v0.3",
    "Qwen/Qwen2.5-Coder-7B-Instruct",
    "deepseek-ai/DeepSeek-V4-Flash",
    "deepseek-ai/DeepSeek-V4-Pro",
    "google/gemma-3n-E4B-it",
];

fn main() {
    let catalog = load_catalog(&default_catalog_path());
    eprintln!("[rust] catalog rows: {}", catalog.len());
    let sys = fixture();

    for name in PICKS {
        let m = catalog.iter().find(|m| m.name == *name);
        let m = match m {
            Some(m) => m,
            None => {
                println!("{{\"name\": {:?}, \"missing\": true}}", name);
                continue;
            }
        };
        // estimate_memory_gb at the model's native quant + native ctx (parity probe)
        let native_q = if m.quantization.is_empty() {
            "Q4_K_M".to_string()
        } else {
            m.quantization.clone()
        };
        let ctx = m.context_length.filter(|c| *c != 0).unwrap_or(4096);
        let est = estimate_memory_gb(m, &native_q, ctx);

        match analyze_model(m, &sys, None) {
            Some(r) => {
                println!(
                    "{{\"name\": {:?}, \"est_native_q\": {:.4}, \"run_mode\": {:?}, \"fit_level\": {:?}, \"required_gb\": {:.4}, \"quant\": {:?}, \"context\": {}, \"params_b\": {:.4}, \"speed_tps\": {:.4}, \"score\": {:.4}}}",
                    r.name, est, r.run_mode, r.fit_level, r.required_gb, r.quant, r.context, r.params_b, r.speed_tps, r.score
                );
            }
            None => {
                println!(
                    "{{\"name\": {:?}, \"est_native_q\": {:.4}, \"analyze\": null}}",
                    name, est
                );
            }
        }
    }
}
