# orrdeal — Walking-Skeleton Design

- **Date:** 2026-06-01
- **Status:** Approved design (pre-plan)
- **Home:** `orrchestrator` workspace (new crate `crates/orrch-orrdeal`)
- **Scope tier:** `private` (inherits orrchestrator)
- **First consumer:** `concord` (but the fabric is app-agnostic)

---

## 1. Context & why this exists

orrchestrator needs a **heterogeneous test fabric**: a way to provision, discover, and
drive a wide variety of execution targets (varied OSes, hardware limits, network
conditions, and input modalities) so that *any* application orrchestrator builds can be
tested and stress-tested against realistic, diverse scenarios.

The motivating consumer is **Concord**, a chat platform shipping a web client, native
desktop builds (Tauri: Windows/macOS/Linux), and mobile. The headline goals that drove
this design:

- Stress-test by spinning up ~15 peers that all join one voice room and broadcast video.
- Exercise builds across varied OSes and hardware limitations.
- Benchmark audio/image/text quality between instances using a corpus of deliberately
  boundary-pushing reference media.

That full vision is **far too large for one spec**. This document specifies only the
**walking skeleton** — the thinnest end-to-end slice that proves the fabric's spine on
local iron — after which every other capability layers onto a working backbone.

`orrdeal` (working name; "ordeal" — the trial devices are put through) is the name of
the fabric.

### 1.1 The full decomposition (for orientation; only the skeleton is in scope here)

The fabric decomposes into 7 sub-projects, each getting its own spec → plan → build:

1. **IaC Core** — extensible Terraform: pluggable provider modules (proxmox, aws, …),
   k3s bootstrap, OS images, cgroup hardware limits, netem network shaping.
2. **Device Fabric** — Tailscale discovery + capability registry; merges all target
   sources into one Target Registry + scheduler.
3. **Deploy Harness** — per-target adapters (pod image, VM installer, iOS `devicectl`,
   Android `adb`); app-agnostic "deploy recipes."
4. **Benchmark Media** — boundary-pushing AV + text corpus with ground-truth; reusable.
5. **Scenario Orchestrator** — e.g. "15 peers join room, broadcast item Y, hold N s";
   Concord scenario plugin atop the fabric.
6. **Metrics & Verdict** — VMAF/SSIM/PSNR, audio quality, frame drops, RTT/jitter,
   CPU-under-limit → pass/fail report.
7. **orrchestrator Integration** — TUI/WebUI surfaces, MCP tools, workflow steps.

### 1.2 Foundational decisions already made (apply to all sub-projects)

- **Hybrid backends.** Day-to-day testing runs on **local iron** (Proxmox host `orrbit`,
  192.168.1.10; `orrpheus` M1 Pro as the only legal macOS node). **AWS** is available
  for short bursts and genuine geo-distributed WAN latency (an AWS account + token already
  exists, used by `orr1on`).
- **Mesh devices are first-class targets.** Devices on the Tailscale tailnet
  (iPhone, iPad, a Linux Chromebook `cb17`, etc.) are usable testing surfaces. Tailscale
  provides discovery + network reachability; deploying *onto* locked-down OSes (iOS) still
  requires a USB→host adapter (e.g. `orrpheus` + signing + `devicectl`).
- **Three target sources** normalize into one Target Registry: ephemeral compute
  (Terraform-provisioned), mesh devices (Tailscale-discovered), physical-attached
  (USB-bound, host-driven).

### 1.3 The Target model — three orthogonal axes (registry schema, day one)

A target is **not** a single "platform" enum. It is a point in a 3-axis space, because a
build's UI surface, its runtime capabilities, and its input modality vary independently
(the iPad proves this: desktop UI + mobile capabilities + touch input).

- **Axis 1 — UI surface:** `desktop` | `mobile` (which layout the build renders).
- **Axis 2 — capability flag-set** (presets are named bundles of flags):
  `camera`, `mic`, `gpu/hw-decode`, `filesystem`, `multi-window`, `p2p-host`,
  `background-persistence`, **`public-web-host`**.
  - Presets: `full` = all flags (native desktop **and** Docker); `mobile` = camera/mic/
    p2p only; `web-sandboxed` = camera/mic guest.
- **Axis 3 — input modality:** `pointer` | `touch` | `both`.
  **Every desktop build must be testable under `touch`, not only `pointer`.**

The **`public-web-host`** flag is significant: a **desktop instance is a first-class
public host, peer to the Docker deployment.** It serves its WebUI and accepts a *virgin*
browser connecting by **URL, not peerID**, across three reachability modes:

1. `concordchat.net` shared hosting,
2. a one-time temporary URL,
3. a **bring-your-own custom `*.com` domain** (real DNS + TLS, exactly like a Docker deploy).

Example target plot:

| Target | UI surface | Preset | public-web-host | Input |
|---|---|---|---|---|
| Native desktop (Lin/Win/macOS) | desktop | full | ✓ | pointer + touch |
| Docker deployment | desktop | full | ✓ | pointer |
| iPad (the crossover) | **desktop** | **mobile** | ✗ | touch |
| iPhone / Android (device or emulator) | mobile | mobile | ✗ | touch |
| Web (headless pod / Chromebook) | either | web-sandboxed | ✗ (guest) | pointer/touch |

> The skeleton does **not** deploy iPads, emulators, or web-host scenarios. But the
> registry schema carries all three axes + all capability flags from day one so later
> sub-projects never force a schema rework.

---

## 2. Walking-skeleton goal & definition of done

**Goal:** prove the fabric end-to-end on local iron with the least code that is still
*real*. Two arms feed one Target Registry; one command prints a verifiable report.

```
> orrch orrdeal skeleton run

  ┌─ PROVISION ARM (ephemeral) ──────────────────────────────┐
  │ terraform apply → 1 k3s node on a Proxmox VM (orrbit)    │
  │   → kubectl apply probe Job                              │
  │   → harness reads probe JSON via `kubectl logs` ─────────┼──┐
  └──────────────────────────────────────────────────────────┘  │
                                                                 ▼
  ┌─ DISCOVER ARM (mesh) ────────────────────────────────────┐ ┌──────────────┐
  │ tailscale status --json → pick 1 online Linux device      │→│   TARGET     │
  │   → ssh probe over tailnet                                │ │   REGISTRY   │
  │   → returns probe JSON ───────────────────────────────────┼→│ (3-axis)     │
  └──────────────────────────────────────────────────────────┘ └──────┬───────┘
                                                                       ▼
        UNIFIED REPORT: 2 targets · each 3-axis classified · each reachable+probed ✓
```

**Definition of done (observable — per the repo's "WRITTEN IN BLOOD" testing rules):**

> You run **one command** (`orrch orrdeal skeleton run`) and **see** a report listing
> **2 real targets** — one ephemeral k3s pod, one live mesh device — each probed, each
> classified on the 3 axes, both marked reachable. Success is verified by *looking at the
> output*, not by "the code compiles" or "tests pass."

A teardown command (`orrch orrdeal skeleton down`) destroys the ephemeral target.

---

## 3. Architecture

### 3.1 Shape & philosophy

- New Rust crate **`crates/orrch-orrdeal`** in the orrchestrator workspace; surfaced via
  the existing `orrch` binary as the `orrch orrdeal …` subcommand. (Rust-first per
  workspace rules.)
- **Orchestrator, not reimplementer.** `orrch-orrdeal` shells out to `terraform`,
  `kubectl`, `tailscale`, and `ssh` — consistent with orrchestrator's thin-dispatcher
  ethos. No bespoke Kubernetes or Terraform API clients in the skeleton.
- **Config:** `~/.config/orrchestrator/orrdeal/` (local-first, self-hosted). Secrets via
  environment variables, never committed.

### 3.2 Components

| Component | Responsibility | Depends on |
|---|---|---|
| **Target Registry** | Core data model + JSON persistence. The spine. | (nothing) |
| **Probe agent** | One portable artifact that emits a capability self-report as JSON. | (nothing) |
| **Provision arm** | Run Terraform → k3s on Proxmox; apply probe Job; read its report. | Registry, Probe agent, `terraform`, `kubectl` |
| **Discover arm** | Enumerate tailnet; pick a Linux device; SSH-run the probe; read report. | Registry, Probe agent, `tailscale`, `ssh` |
| **Reporter** | Merge probed targets into the registry; print the unified report. | Registry |
| **CLI (`orrch orrdeal`)** | `skeleton run` / `skeleton down`; prereq checks; wire arms. | all of the above |

#### Target Registry — data model (illustrative Rust)

```rust
struct Target {
    id: String,
    source: TargetSource,        // Ephemeral | Mesh | Physical
    ui_surface: UiSurface,       // Desktop | Mobile
    capabilities: CapabilityFlags, // bitflags: camera, mic, gpu, fs, multi_window,
                                   //           p2p_host, background, public_web_host
    input: InputModality,        // Pointer | Touch | Both
    arch: String,                // x86_64 | aarch64 | …
    os: String,                  // distro/OS string from the probe
    reach: Reach,                // { addr, adapter: Pod | TailnetSsh | … }
    status: TargetStatus,        // Reachable | Unreachable | ProbeFailed
}
```

Persisted to `~/.config/orrchestrator/orrdeal/registry.json`. In-memory during a run.

#### Probe agent — one artifact, two delivery paths

- Emits JSON: `{ os, arch, capabilities[], ui_surface_hint, input_hint }`.
- **Delivery A (pod):** baked into / mounted in the probe Job's container; output captured
  via `kubectl logs`.
- **Delivery B (mesh):** copied/streamed over SSH and executed; stdout captured.
- For the skeleton the agent may be a portable POSIX `sh` script (detects `uname`, CPU,
  presence of camera/mic/GPU nodes). It is the embryo of the future **Deploy Harness**.

> Skeleton simplification: the pod reports via `kubectl logs` of a one-shot **Job** (no
> inbound networking back to the harness). This is simpler and more robust than an HTTP
> callback and is sufficient to prove the spine.

### 3.3 Data flow

`orrch orrdeal skeleton run` → prereq checks → **provision arm** and **discover arm** run
**in parallel** → each yields one probed `Target` → Reporter merges into the Registry →
prints the unified report → exit code reflects DoD.

### 3.4 Terraform module (provision arm)

- `crates/orrch-orrdeal/terraform/proxmox-k3s/` (HCL), using the **bpg/proxmox** provider.
- Creates **1 VM** on `orrbit`; cloud-init installs **single-node k3s**.
- Outputs: node IP + kubeconfig path (consumed by the harness for `kubectl`).
- **State:** local `terraform.tfstate` under a gitignored path for the skeleton. (Remote
  backend is deferred to the IaC Core sub-project.)

---

## 4. Error handling, teardown, secrets

- **Independently fallible arms.** A failed arm marks *its* target `Unreachable` /
  `ProbeFailed` in the report rather than aborting the other arm. The run's exit code
  reflects whether the DoD (both targets probed) was met.
- **Prereq fail-fast.** Verify `terraform`, `kubectl`, `tailscale`, `ssh` are present;
  the Proxmox API token env var is set; the tailnet is authenticated. Missing prereq →
  clear, actionable error message, no partial provisioning.
- **Teardown.** `orrch orrdeal skeleton down` runs `terraform destroy`. Ephemeral stays
  ephemeral (cost + hygiene). The skeleton should never leave a VM running silently.
- **Secrets.** Proxmox API token + Tailscale auth live in env vars / local config, never
  in the repo. (`private` scope; self-hosted.)

---

## 5. Verification & testing

Per the repo's mandatory testing rules:

- **Done = observed.** The author verifies by running `skeleton run` and **looking** at a
  report showing 2 real targets probed and classified. Status language must be "I ran it
  and saw X," never "it should work."
- **Tests are authored in a separate cold session**, not the session that builds the
  feature. A cold reader rediscovers the behavior from the outside and asserts on
  user-visible output (the report contents), not abstract internal values.
- **Regression tests** may be written this session only for bugs *empirically proven*
  during this session.
- Suggested cold-session assertions (for the test author, not built here): run
  `skeleton run` and assert the report contains exactly 2 targets with non-empty os/arch
  and `Reachable` status; then break one arm (revoke token / down the mesh device) and
  assert the report **degrades gracefully** (one target `Unreachable`, the other still
  `Reachable`) with a non-zero exit code.

---

## 6. Out of scope (YAGNI — deferred to later sub-projects)

AWS burst provisioning; native-OS VMs (Windows/macOS); iOS / Android / emulator targets;
touch-input injection; the benchmark media corpus; voice-swarm / multi-peer scenarios;
AV-quality scoring (VMAF/SSIM/PSNR/audio); cgroup hardware-limit + netem shaping;
TUI/WebUI surfaces and MCP tools; the custom-domain / temp-URL web-host scenarios; remote
Terraform state; the scheduler. The registry schema already carries the axes/flags these
need, so none of them force a rework of the skeleton.

---

## 7. Prerequisites (must be real before build)

- A **Proxmox API token** on `orrbit` (192.168.1.10) with rights to create a VM.
- A **Linux device on the tailnet** reachable via SSH (the Chromebook `cb17`).
- **Tailscale up** and authenticated on the orchestrating host.
- `terraform` and `kubectl` installed on the orchestrating host.

---

## 8. Open questions for the implementation plan

- Exact probe-agent capability-detection heuristics (how `camera`/`mic`/`gpu` presence is
  inferred portably across a pod and a Chromebook).
- Whether the skeleton's CLI lives behind a Cargo feature flag in `orrch` until stable.
- Pinning the bpg/proxmox provider + k3s version for reproducibility.
