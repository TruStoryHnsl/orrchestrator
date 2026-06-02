# orrch-relay — MoE-aware inference scheduler (design)

**Date:** 2026-06-02
**Status:** approved design, pre-implementation
**Scope:** `private`
**Host:** contained inside the `orrchestrator` binary (new crate `orrch-relay` + `src/main.rs` integration). NOT a standalone project.

---

## 1. Purpose

A reusable, engine-agnostic inference scheduler that orders a queue of LLM requests to keep a local Mixture-of-Experts (MoE) model's expert cache hot, improving realized throughput for "persistent agentic pressure" workloads (a steady stream of independent agent calls). Built as a foundation, with a concrete `deepseek-v4-flash` runner as its first consumer and end-to-end proof.

Two layers, one cargo crate:
- **Foundation** — the reusable scheduler (gateway, affinity, scheduler, engine adapter, metrics).
- **Consumer** — a `deepseek-v4-flash` runner (config + `llama-server` supervisor) built on the foundation.

### Why this exists / what it is NOT

The freedom this system exploits lives at the **request** level, not the **token** level. Within a single autoregressive generation, expert routing is forced by the model — you cannot reorder or minimize active-parameter changes without changing the output. But *independent requests in a queue* CAN be reordered freely, because reordering them does not change any single request's result. The scheduler reorders the queue so semantically-similar requests (which route through overlapping experts) run back-to-back, keeping the engine's mmap/page cache hot instead of thrashing cold experts off NVMe.

**This is NOT** a token-level expert scheduler, an inference-engine fork, or a system that changes model outputs. It is a request-ordering proxy in front of unmodified black-box engines.

### Honest bound on value

With black-box engines the scheduler's only lever is request *ordering*; it cannot see or control expert routing. The throughput benefit is therefore **real but workload-dependent**: large for a queue of similar tasks (e.g. 200 code-review calls), near-zero for a maximally diverse queue. Ordering is always an optimization, never a correctness requirement — if every affinity mechanism fails, requests still complete in FIFO order.

---

## 2. Architecture

```
client → POST /v1/chat/completions  (orrchestrator binary's listener)
  → gateway   : assign id, enqueue
  → affinity  : embed prompt (Ollama) OR use affinity_hint → descriptor
  → scheduler : cluster + order (max_wait + aging guards) → pick next
  → engine    : LlamaCppAdapter streams from llama-server
  → gateway   : stream SSE back to client
```

- **Containment:** one new crate `orrch-relay` in `crates/`, matching the `orrch-*` convention (`orrch-core`, `orrch-tui`, `orrch-hwfit`, …). Internal modules instead of multiple crates, to keep the workspace tidy and the work self-contained.
- **Binary integration:** launched by `src/main.rs` as a subsystem behind an env toggle (`ORRCH_RELAY_ENABLE=1`), reusing the existing WebUI server's auth/token/TLS infrastructure rather than standing up a parallel HTTP stack. Its OpenAI-compatible URL is surfaced in the Esc menu alongside the other WebUI URLs.
- **No new launcher:** adds keys to orrchestrator's existing `~/.config/orrchestrator/launch.env`; no separate `install.sh`.
- **Execution model:** a single in-flight request against the engine at a time (one big local model is all a RAM-constrained box can hold). All the leverage is in queue ordering, not concurrency.

---

## 3. Components (modules of `orrch-relay`)

Each module has one job and a typed interface; gateway↔scheduler↔engine communicate by plain messages, and affinity + engine are swappable behind traits.

### 3.1 `gateway`
OpenAI-compatible HTTP front (axum, reusing the WebUI server stack): `/v1/chat/completions` (streaming SSE + non-stream), `/v1/models`, `/health`. Assigns a request id, enqueues the request, streams the engine's output back to the client. Knows nothing about affinity or engines — just HTTP ↔ queue. Auth via orrchestrator's loopback-trusted / bearer-token pattern.

### 3.2 `affinity`
Turns a request into an affinity descriptor.
- **Path A (core):** embed the prompt via a local embedder (Ollama bge/e5-class over HTTP) → vector.
- **Path B (free override):** if the request carries an `affinity_hint` field, use it directly and skip embedding.
- Embedder is behind a trait so it can be mocked/swapped. Pure function of request → descriptor.

### 3.3 `scheduler`
The heart. Holds pending requests, does incremental similarity clustering on descriptors, emits requests in cluster-contiguous order subject to two guards:
- **`max_wait_ms`** — latency cap; a request cannot wait longer than this for a cluster.
- **Aging** — a request's effective priority rises with wait time, so no cluster can starve a lone/odd request.
Pure logic over (tagged requests + policy + clock); no I/O. Output is an ordering. Testable in isolation with a fake clock and fake descriptors.

Clustering for v1: greedy incremental nearest-cluster assignment with a cosine-similarity threshold; hint-tagged requests group by exact-match key. Keep it simple — no learned/online policy (that is deferred Approach C).

### 3.4 `engine`
Trait `Engine { async fn complete(req) -> TokenStream }` plus adapters:
- **`LlamaCppAdapter`** (v1) — drives `llama-server`'s OpenAI-compatible API.
- **`GenericOpenAiAdapter`** — any OpenAI-compatible endpoint.
- **`KTransformersAdapter`** — slot for post-RAM-upgrade; not implemented in v1.
The only module that talks to a real engine. Serialized execution.

### 3.5 `metrics`
Queue depth, realized tok/s, wait-time histogram, and an affinity-contiguity proxy (how often consecutive executed requests shared a cluster) as a cache-hit-quality signal. Lays the observability groundwork a future Approach C would need.

### 3.6 `runner` (deepseek-v4-flash consumer)
Config + process supervisor, NOT novel inference code. Launches/supervises `llama-server` with the unsloth V4-Flash GGUF and the right flags for the target box (`--n-cpu-moe`, mmap on the NVMe path, GPU layer count, a sane operating context), registers it as a served model, restarts it on crash. The foundation's first real client and the end-to-end proof.

---

## 4. Error handling

Ordering is an optimization, never a correctness requirement.

| Failure | Behavior |
|---|---|
| Embedder down/unreachable | Fall back to FIFO ordering; requests still complete. Degrade, don't fail. |
| Engine crash | `runner` health-checks and restarts `llama-server`; in-flight request retried once; queue held; `503 + Retry-After` only if unrecoverable. |
| Overload | Queue-depth cap → `429`. |
| Client disconnect mid-stream | Cancel the in-flight request, free the engine immediately. |
| Starvation pressure | Aging guarantees a lone request runs within `max_wait_ms`. |

---

## 5. Testing strategy

Per the project's testing rules: user-observable assertions, authored in a cold/separate session (not the one that built the feature, except for regression tests of empirically-proven bugs), no abstract-value tests.

- **Scheduler (isolated):** a fake engine adapter records the *order* it received a known mixed queue → assert clustering actually reordered it. A fake clock asserts a lone request executes within `max_wait_ms`. The assertion is on observable ordering/timing, not an internal flag.
- **Graceful degradation:** kill the embedder → assert completions still return (FIFO path).
- **End-to-end (the real proof):** actually launch `llama-server` with V4-Flash on the target box, send two real completion requests through the proxy, and OBSERVE real tokens returned. Status language: "it works because I watched deepseek-v4-flash generate through the scheduler."
- **Affinity benefit:** clustered vs shuffled workload, measure realized tok/s. Honest assertion = "reordering occurred + no throughput regression," NOT a fabricated speedup — on a disk-bound box the win may be modest.

---

## 6. Scope / YAGNI (v1 cuts)

Explicitly out of scope for v1, notable as future phases:
- Multi-engine concurrency (one big local model at a time).
- Multi-node / distributed scheduling.
- Approach C (measured-feedback / online-learning affinity).
- In-engine expert prefetch or pinned expert cache (requires an engine fork; confirmed black-box-only depth).
- Cross-restart queue persistence (in-memory queue; agentic callers re-submit on restart).
- A dedicated TUI panel (control via env/config + Esc-menu URL for v1).

---

## 7. Open implementation questions (resolve during planning)

- Exact `llama-server` flag set + GGUF quant for the 8GB-VRAM / 27GB-RAM / NVMe target (the runner's launch config). The orrch-hwfit `disk_stream` rating informs expectations (~0.4 tok/s on NVMe at the model's operating context).
- Which embedding model to standardize on (bge-small vs e5 vs nomic) and its dimensionality vs clustering cost.
- Whether the OpenAI listener shares the WebUI port with path-based routing or takes its own port.
- Default `max_wait_ms`, cluster similarity threshold, and queue-depth cap values (tune empirically).
