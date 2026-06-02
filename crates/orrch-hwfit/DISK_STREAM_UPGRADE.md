# Disk Stream Upgrade

Date: 2026-06-02

## Research Findings

Sources checked through Context7 and web:

- Context7: `/ggml-org/llama.cpp`, `/kvcache-ai/ktransformers`
- llama.cpp CLI docs: https://github.com/ggml-org/llama.cpp/blob/master/tools/cli/README.md
- KTransformers DeepSeek injection docs: https://github.com/kvcache-ai/ktransformers/blob/main/doc/en/deepseek-v2-injection.md
- KTransformers DeepSeek R1/V3 guide mirror: https://deepwiki.com/kvcache-ai/ktransformers/9.1-deepseek-r1-and-v3
- mmap/page-cache explanation: https://moderncpp.dev/articles/anatomy-of-llamacpp/
- User-reported DeepSeek disk-backed throughput: https://www.reddit.com/r/LocalLLaMA/comments/1ikprg7/trouble_with_running_llamacpp_with_deepseekr1_on/
- Additional DeepSeek V3 disk/swap throughput reports: https://www.reddit.com/r/LocalLLaMA/comments/1idseqb/deepseek_r1_671b_over_2_toksec_without_gpu_on/

Confirmed model:

- llama.cpp enables `--mmap` by default, offers `--mlock` to force RAM residency, and exposes `-ot/--override-tensor`, `--cpu-moe`, and `--n-cpu-moe` for tensor/expert placement.
- mmap means GGUF bytes are mapped into virtual memory; pages are faulted from disk into the OS page cache on first touch, then reused from RAM while resident. Under memory pressure, cold file-backed pages can be evicted and faulted again.
- KTransformers supports heterogeneous MoE execution: docs show experts loaded/generated on CPU with output on CUDA, while DeepSeek guides place MLA/shared work on GPU and routed experts on CPU.
- DeepSeek-class MoE disk paging is random/page-fault shaped, not sequential streaming. User reports show high sequential NVMe/RAID bandwidth did not translate to proportional tok/s because inference touched small discontinuous pages.
- RAM is therefore an expert page cache, not a hard full-weight capacity wall. More available RAM increases cache hit rate; low RAM still can run, but slowly.
- Real reported disk-backed DeepSeek-class speeds are low but non-zero: examples include roughly 1.2 tok/s from a single NVMe for DeepSeek-R1 Q2 with no GPU offload, 0.5-0.7 tok/s through swap, and 1.0-2.8 tok/s for DeepSeek V3 Q8 depending on disk/swap setup. KTransformers reports much higher decode on large-RAM dual-Xeon systems, which is a different RAM-resident CPU-expert tier.

## What Changed

- Added `SystemInfo.storage_rand_read_gbps: Option<f64>` with serde default and `skip_serializing_if`.
- Added deterministic storage probing in `src/probe.rs`:
  - `ORRCH_HWFIT_STORAGE_GBPS` override.
  - backing block device from `findmnt`/`df`.
  - `/sys/block/*/queue/rotational`.
  - NVMe PCI link-speed heuristic when available.
  - conservative class map: NVMe Gen4 `2.0`, NVMe Gen3/unknown `1.0`, SATA/non-rotational `0.4`, HDD `0.02` GB/s.
- Added `RUN_MODE_DISK_STREAM = "disk_stream"` in `src/fit.rs`.
- Qualification is MoE-only and does not require full quantized weights in RAM:
  - resident active slice plus KV must fit VRAM.
  - per-token active working set must fit available RAM.
  - full weights may exceed available RAM.
- Disk-stream speed estimates storage random-read GB/s divided by uncached active bytes, with RAM/extra VRAM increasing cache credit. It is capped and floored so qualifying systems report slow non-zero speed.
- MoE offload/disk-stream KV sizing now uses an operating context target instead of max catalog context, while dense-model handling remains unchanged.

## Verification Evidence

OBSERVED `cargo build -p orrch-hwfit`:

```text
   Compiling orrch-hwfit v0.1.0 (/home/user/projects/orrchestrator/crates/orrch-hwfit)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.53s
```

OBSERVED `cargo test -p orrch-hwfit -- --nocapture`:

```text
running 25 tests
FitResult {
    name: "deepseek-ai/DeepSeek-V4-Flash",
    provider: "DeepSeek",
    parameter_count: "158B",
    params_b: 158.0,
    is_moe: true,
    use_case: "reasoning",
    fit_level: "marginal",
    run_mode: "disk_stream",
    quant: "Q4_K_M",
    context: 8192,
    required_gb: 7.9,
    speed_tps: 0.4,
    context_length: 1000000,
}
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

Doc-tests orrch_hwfit
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Guard coverage:

- 8GB VRAM / 17GB available RAM / NVMe fixture rates `deepseek-ai/DeepSeek-V4-Flash` as `disk_stream`, `required_gb = 7.9`, `speed_tps = 0.4`.
- Same DeepSeek fixture with 96GB available RAM still uses `moe_offload`.
- Dense 70B on 8GB VRAM / 17GB available RAM returns `no_fit`, not `disk_stream`.
- Existing catalog DeepSeek MoE offload test remains green.

OBSERVED `cargo run -p orrch-hwfit --example parity`:

```text
[rust] catalog rows: 898
{"name": "Qwen/Qwen2.5-7B-Instruct", "run_mode": "gpu", "fit_level": "perfect", "required_gb": 6.9000, "speed_tps": 64.7000}
{"name": "mistralai/Mistral-7B-Instruct-v0.3", "run_mode": "gpu", "fit_level": "perfect", "required_gb": 6.6000, "speed_tps": 68.0000}
{"name": "Qwen/Qwen2.5-Coder-7B-Instruct", "run_mode": "gpu", "fit_level": "perfect", "required_gb": 6.9000, "speed_tps": 64.7000}
{"name": "deepseek-ai/DeepSeek-V4-Flash", "run_mode": "disk_stream", "fit_level": "marginal", "required_gb": 7.9000, "context": 8192, "speed_tps": 0.1000}
{"name": "deepseek-ai/DeepSeek-V4-Pro", "run_mode": "no_fit", "fit_level": "too_tight", "required_gb": 1320.5000, "speed_tps": 0.0000}
{"name": "google/gemma-3n-E4B-it", "run_mode": "cpu_offload", "fit_level": "good", "required_gb": 13.5000, "speed_tps": 30.8000}
```
