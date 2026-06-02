# MoE Offload Capacity Recognition Upgrade

## Summary

Findings dated 2026-06-02. `orrch-hwfit` now recognizes a MoE expert-offload run mode, `moe_offload`, for sparse models with meaningful active-parameter metadata. Dense model math remains on the previous `gpu` / `cpu_offload` / `cpu_only` / `no_fit` path.

The concrete 8 GB VRAM + 96 GB RAM fixture now rates a DeepSeek V4 Flash-class 158B total / 13B active Q4 MoE as runnable:

```text
run_mode: "moe_offload"
fit_level: "marginal"
context: 7812
required_gb: 7.8
speed_tps: 24.8
```

## Findings And Sources

Context7 was queried for `llama.cpp`, vLLM, and Hugging Face Transformers/Accelerate. Web search then cross-checked official docs/repos and project documentation.

- `llama.cpp`: mainline CLI documents `--cpu-moe`, `--n-cpu-moe`, `--override-tensor`, KV cache offload controls, GPU layer placement, and `--fit`. These controls let MoE expert tensors stay in CPU RAM while attention/KV/shared tensors use the GPU. Source: https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/cli/README.md
- `ik_llama.cpp`: the fork documents MoE/dense tensor auto-fit, tensor overrides, smart expert reduction, and CPU/GPU hybrid improvements. It is CUDA-oriented and more aggressive than mainline for MoE CPU/GPU placement. Source: https://github.com/ikawrakow/ik_llama.cpp and https://github.com/ikawrakow/ik_llama.cpp/blob/main/docs/parameters.md
- `ktransformers`: current docs describe CPU-GPU heterogeneous MoE inference, expert placement, GPU expert counts, deferred experts, and dynamic expert updates. DeepSeek V4 Flash is listed as an inference target but marked as needing smoke validation. Source: https://ktransformers.net/en/docs and https://ktransformers.net/docs/supported-models/deepseek
- vLLM: latest docs expose weight CPU offload via `--cpu-offload-gb`, `cpu_offload_params`, UVA/prefetch backends, and KV offloading. vLLM treats CPU offload as virtual GPU memory, but notes CPU-GPU transfer on each forward pass, so it should be rated slower than expert-specific MoE offload. Source: https://docs.vllm.ai/en/latest/cli/serve/ and https://docs.vllm.ai/en/v0.21.0/api/vllm/config/offload/
- Hugging Face Transformers/Accelerate: `device_map="auto"` and `max_memory` place modules across GPU, CPU, and disk; `offload_folder`/`offload_dir` are required for disk entries. Disk offload memory-maps parameters and pages them to the execution device as needed. This is generic module offload, not MoE-hot-expert residency. Source: https://huggingface.co/docs/accelerate/main/en/package_reference/big_modeling
- ExLlamaV3: GPU-first local inference with EXL3 quantization, tensor parallelism, expert parallelism, continuous batching, and quantized cache support. It does not present CPU expert streaming as the primary capacity strategy, so the rater should not model it like ktransformers/llama.cpp MoE CPU expert offload. Source: https://github.com/turboderp-org/exllamav3
- Apple MLX / mlx-lm: Apple Silicon has unified memory; arrays live in a shared CPU/GPU pool, and operations choose a device rather than copying arrays between separate RAM/VRAM pools. `mlx-lm` notes large models relative to total RAM can be slow and may need wired memory tuning. Sources: https://ml-explore.github.io/mlx/build/html/usage/unified_memory.html and https://github.com/ml-explore/mlx-lm

Conflicts/uncertainty:

- `DeepSeek-V4-Flash` public metadata is not stable across local catalogs and project notes. This crate's catalog row uses 158B total / 13B active. The upgrade depends on those existing fields.
- ktransformers current docs list DeepSeek V4 Flash support but say the path needs smoke testing. I modeled the architecture class, not a guaranteed one-command runtime setup.
- Exact tokens/sec varies heavily by CPU memory bandwidth, PCIe generation, KV quantization, prompt length, and expert routing locality. The new speed estimate is a fit/ranking heuristic, not a benchmark claim.

## What Changed In Code

- Added `RUN_MODE_MOE_OFFLOAD = "moe_offload"` in `src/fit.rs`.
- Added MoE-only resident-memory logic:
  - VRAM resident = active/hot parameter slice + active-parameter KV cache + runtime overhead.
  - RAM requirement = full quantized model weights.
  - Qualification requires resident footprint <= VRAM and full weights <= available RAM.
- Added MoE speed factor based on active params and expert selectivity. It avoids the old flat `0.5x` dense `cpu_offload` penalty and leaves dense `cpu_offload` unchanged.
- Changed MoE run-mode resolution to search the MoE-offload context ladder before falling back to dense-style CPU offload.
- Updated `FitResult.run_mode` comment to include `moe_offload`.
- Updated the vendored `deepseek-ai/DeepSeek-V4-Flash` row to local GGUF Q4 sizing hints: `quantization: "Q4_K_M"`, `min_ram_gb: 92.1`, `recommended_ram_gb: 128.0`, `min_vram_gb: 8.0`.
- Added tests for:
  - DeepSeek V4 Flash-class MoE on 8 GB VRAM / 96 GB RAM.
  - Vendored catalog DeepSeek V4 Flash row on the same fixture.
  - Dense 70B not taking the MoE or GPU path on the same fixture.

## Verification Evidence

```text
$ cargo build -p orrch-hwfit
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.67s
```

```text
$ cargo test -p orrch-hwfit
running 23 tests
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

Doc-tests orrch_hwfit
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Focused evidence with `--nocapture`:

```text
DeepSeek V4 Flash-class:
run_mode: "moe_offload"
fit_level: "marginal"
quant: "Q4_K_M"
context: 7812
required_gb: 7.8
speed_tps: 24.8

Dense 70B:
run_mode: "cpu_offload"
quant: "Q4_K_M"
context: 65536
required_gb: 77.8
speed_tps: 3.5
```

The dense 70B result confirms the MoE path did not create a false GPU/MoE fit for non-MoE models. It remains on the old dense CPU-offload route.

The existing parity example also compiles and runs:

```text
$ cargo run -p orrch-hwfit --example parity
[rust] catalog rows: 898
...
deepseek-ai/DeepSeek-V4-Flash: run_mode "no_fit" on the example's fixed 8 GB VRAM / 32 GB RAM fixture
```

## Caveats

- `FitResult` has one `required_gb` field. For `moe_offload`, it reports resident VRAM footprint. The separate full-weight RAM requirement is enforced internally but not surfaced as a second field.
- The resident active-slice estimate is conservative when `num_experts` / `active_experts` are absent and uses `active_parameters` as the hot slice.
- Long advertised context windows are still reduced until the resident KV cache fits. The 1M-context DeepSeek row fits the 8 GB fixture at 7,812 tokens, not at full context.
- Disk offload is researched here but not added as a runnable mode; disk-resident inference is too slow/variable for the current rater without a distinct mode and UI warning.
