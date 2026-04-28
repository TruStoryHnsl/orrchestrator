# orrchestrator

AI-powered software development hypervisor — a Rust TUI that runs many parallel coding sessions, organizes them into agent workforces, and routes raw user thought into a managed dev pipeline.

## What it is

orrchestrator is a single Rust binary (with a workspace of nine crates) that sits between you and the army of AI coding sessions you'd otherwise be juggling by hand. It does four things:

1. **Manages parallel sessions.** tmux-controlled local Claude Code (and other harness) sessions, grouped by project, displayed in a TUI.
2. **Runs agent workforces.** A workforce is a team of agent profiles (Project Manager, Developer, Researcher, Penetration Tester, Repository Manager, etc.) wired together as a step pipeline. The hypervisor mechanically dispatches each step — no LLM reasoning in the dispatcher itself.
3. **Routes raw input.** Stream-of-consciousness ideas, plans, bug reports, and instructions get dropped into an editor; an intake workforce optimizes them into discrete instructions, you audit the result, and they get routed to the right project's inbox.
4. **Tracks the dev pipeline.** Multi-project plan view, hot/cold session tracking, token-usage analytics, release packaging — all in one TUI, with a web UI mirror for terminal-averse use.

It is the canonical user-voice tool in the TruStoryHnsl ecosystem. Most of the other repos that exist were planned or instructed *through* orrchestrator.

## Why

I was running five Claude Code sessions in five tmux windows, copying the same context into each one, losing track of which session was working on what, and wasting an obscene number of tokens reloading state every time something compacted. The hypervisor model is the answer: many parallel sessions, each on its own branch, coordinated by a thin dispatcher and reviewed by a PM agent before merge.

The design has a few non-negotiable principles. They show up in every architectural decision:

- **Token efficiency is the constraint, not a feature.** Every interface — workforce step tables, file-cluster batching, the deterministic compression tools — exists because context size is the cost center. One session per workflow, not per agent. Compressed handoffs between operations. No prompt-blob injection.
- **Stream-of-consciousness intake, structured execution.** I write whenever, in whatever shape — fragments, walls of text, voice-to-text dumps. The COO agent optimizes that into instructions; the user audits side-by-side; only then does it route to a project. The visionary writes; the agents execute; nothing in between drops on the floor.
- **Hypervisor model, not autonomous swarm.** The user is still the visionary. The hypervisor is a dispatcher. Agents do work; PM agent reviews PRs; user reconciles. Every session works on its own branch (regression-cost lessons learned the expensive way).
- **Self-hosted by default.** Local-first config (`~/.config/orrchestrator/`). Native TLS in the WebUI so no nginx-proxy-manager sidecar. Library is git-backed — your agents and skills are yours, not a vendor's catalog.
- **Function over form, but polish where it matters.** TUI is the daily driver. The web UI and native egui window exist for the specific things terminals can't do well (the node editor; terminal-averse users). The same binary serves all three.
- **Rust-first.** New code is Rust. The Python prototype lives archived in `python-prototype/`; that's where it stays.

## Architecture

```
                              ┌──────────────────────────────────────────┐
                              │            orrchestrator (binary)        │
                              └──────────────────────────────────────────┘
                                                  │
            ┌─────────────────────────────────────┼─────────────────────────────────────┐
            │                                     │                                     │
        TUI front-end                       WebUI (8484/TLS)                    egui native window
        (orrch-tui)                         (orrch-webui)                       (orrch-tui --egui)
            │                                     │                                     │
            └─────────────────────────────────────┴─────────────────────────────────────┘
                                                  │
   ┌──────────────────────────────────────────────┼──────────────────────────────────────────────┐
   │                                              │                                              │
[ Design ]  [ Oversee ]  [ Hypervise ]  [ Analyze ]  [ Publish ]
   │             │             │             │              │
   │             │             │             │              └─ packaging / distribution / compliance
   │             │             │             └─ token usage, throughput, cost
   │             │             └─ tmux-managed parallel sessions, workflow grouping
   │             └─ projects, sessions, file browser, hot/cold tracking
   │
   ├── Intentions   (raw idea editor → submission gate → intake workforce)
   ├── Workforce    (workflows · teams · agents · skills · tools · MCP · profiles · models)
   ├── Library      (read-only browser of models, harnesses, MCP servers, valves)
   └── Plans        (cross-project PLAN.md browser & editor)


            ─── Agent department hierarchy (agents/*.md, 21 roles) ───

         ┌─────────────────────┬─────────────────────┬─────────────────┐
         │       Admin         │     Development     │    Marketing    │   Legal
         │ Executive Assistant │   ┌─ Leadership ─┐  │ UX Specialist   │ Licensing Auditor
         │ COO                 │   PM             │  │ Market Researcher│ Copyright Investigator
         │ IR Manager          │   Talent Scout   │  │
         │ Mentor              │   Resource Optim │  │
         │ Hypervisor          │   ┌─ Engineering ┐  │
         │                     │   SW Engineer    │
         │                     │   Developer      │
         │                     │   Researcher     │
         │                     │   UI Designer    │
         │                     │   Feature Tester │
         │                     │   Specialist     │
         │                     │   ┌─ QA ─────────┐
         │                     │   Pen Tester     │
         │                     │   Beta Tester    │
         │                     │   ┌─ DevOps ─────┐
         │                     │   Repository Mgr │


            ─── Workforce → Operation → Step (the dispatch pipeline) ───

   workforces/<name>.md              operations/<name>.md            (each row spawns a session)
   ┌──────────────────┐              ┌──────────────────────┐        ┌──────────────────────┐
   │ team composition │  references  │ step table           │  runs  │ agent + skill/tool   │
   │ (which agents)   │ ───────────► │ (pipe-delimited md)  │ ─────► │ + compressed inputs  │
   │ + connections    │              │ trigger / blocker    │        │ + isolated context   │
   └──────────────────┘              └──────────────────────┘        └──────────────────────┘
                                                                                │
                                                                  compress_output.sh
                                                                  cluster_tasks.sh
                                                                  codebase_brief.sh
                                                                                │
                                                                                ▼
                                                                  next step receives only
                                                                  the structured fields it needs


            ─── Three-tier model layer (library/models/*.md) ───

   ┌─────────────────────────┬─────────────────────────┬─────────────────────────┐
   │  Enterprise tier        │  Mid tier               │  Local tier             │
   │  Claude Sonnet/Opus     │  Mistral Large          │  Ollama (Crush/OpenCode)│
   │  GPT-4o / GPT-5         │  Mixtral, etc.          │  any locally hosted     │
   │  long context, dense    │  cheap bulk reasoning   │  free, private, slower  │
   └─────────────────────────┴─────────────────────────┴─────────────────────────┘
                                       │
                              Resource Optimizer agent
                              annotates each step with
                              tier + harness + rationale
                                       │
                              API valve system enforces
                              per-provider on/off + rate limits
                              (~/.config/orrchestrator/valves.json)
```

### Components

| Crate | Responsibility |
|---|---|
| `src/` | Binary entrypoint. Argument parsing, dispatch to TUI / webedit / egui / MCP server. |
| `crates/orrch-agents` | 21 agent profiles, department hierarchy, `AgentRunner`. |
| `crates/orrch-core` | Process manager, sessions, projects, feedback, backends, config, vault. |
| `crates/orrch-library` | Models, harnesses, MCP server configs, API valves, templates. |
| `crates/orrch-mcp` | MCP server exposing orrchestrator's own tools to running agents. |
| `crates/orrch-retrospect` | Error capture, fingerprinting, troubleshooting protocols. |
| `crates/orrch-tui` | ratatui panels, app state, UI rendering, editor integration. |
| `crates/orrch-webedit` | Local HTTP server + JS canvas for the workforce node editor. |
| `crates/orrch-webui` | Always-on WebUI server (HTTP + native TLS), Esc-menu accessible. |
| `crates/orrch-workforce` | Workforce templates, operation modules, step engine, markdown parser. |
| `agents/*.md` | 21 agent profile definitions (YAML frontmatter + system prompt). |
| `workforces/*.md` | Workforce templates — team compositions and operation lists. |
| `operations/*.md` | Operation modules — step tables that drive the dispatcher. |
| `library/models/*.md` | Model definitions with tier and pricing. |
| `library/harnesses/*.md` | Harness definitions with auto-detection rules. |
| `library/skills/*.md` | LLM-judgment skill prompts (harness-agnostic). |
| `library/tools/` | Deterministic shell tools called between steps. |
| `branding/` | Logo and brand palette (terminal truecolor). |

## Quickstart

```bash
git clone <this-repo> orrchestrator
cd orrchestrator

# build
cargo build              # debug, warnings OK
cargo build --release    # ~5MB native binary at target/release/orrchestrator

# test (~119 tests across 9 crates)
cargo test

# run the TUI
cargo run                # debug
./target/release/orrchestrator

# run live-reload during development
cargo watch -x run
```

### Alternate front-ends

```bash
# headless web node editor (no real terminal needed)
orrchestrator --webedit

# native egui workforce viewer (rebuild with feature)
cargo build --release --features orrch-tui/egui-window
orrchestrator --egui
```

### WebUI (always on)

The WebUI listener is always running. Press `Esc` from any TUI panel to see the URLs (local HTTP, public HTTP, public TLS — depending on your env).

```bash
# bare local HTTP only (default)
orrchestrator                                       # http://127.0.0.1:8484

# add native TLS termination — bring your own cert
ORRCH_WEBUI_TLS_CERT=/path/fullchain.pem \
ORRCH_WEBUI_TLS_KEY=/path/privkey.pem \
ORRCH_WEBUI_TLS_PORT=8443 \
orrchestrator

# expose port 80/443 directly without root
sudo setcap cap_net_bind_service=+ep ./target/release/orrchestrator

# tailnet-only (recommended): trust 100.64.0.0/10, no token needed for tailnet peers
ORRCH_WEBUI_PUBLIC_HTTP_PORT=80 \
ORRCH_WEBUI_TRUSTED_CIDRS=100.64.0.0/10 \
orrchestrator
```

Full WebUI env reference is in [CLAUDE.md](CLAUDE.md#native-tls-for-the-webui).

### Config

`~/.config/orrchestrator/config.json` — unified config struct. API valves (per-provider on/off) at `~/.config/orrchestrator/valves.json`.

## Features

- **5-panel TUI**: Design (Intentions / Workforce / Library / Plans), Oversee, Hypervise, Analyze, Publish.
- **Parallel session management** via tmux. Hot/cold project tracking. Workflow sessions group their child sessions into a single expandable row.
- **Stream-of-consciousness intake** — global hotkey opens `nvim`; submission routes through an intake workforce; you audit raw-vs-optimized side-by-side before instructions hit a project's inbox.
- **Workforce + operation system** — pipe-delimited markdown tables that the dispatcher executes mechanically. Auto-detects parallel groups (same step index = run concurrently).
- **21 agent profiles** organized into Admin / Development (Leadership · Engineering · QA · DevOps) / Marketing / Legal departments.
- **Three-tier model layer** with API valves and a Resource Optimizer agent annotating each task with tier + harness + rationale.
- **Deterministic compression tools** (`codebase_brief.sh`, `compress_output.sh`, `cluster_tasks.sh`) that replace LLM-based "compression" reasoning between steps.
- **WebUI mirror** with native TLS termination, token auth, trusted-CIDR bypass for tailnet peers, and a single-process dual-listener mode (serve `127.0.0.1:8484` and `orrchestrator.com:80` from one binary).
- **Web node editor** for workforce design (launch with `Ctrl+w` from Workforce panel or `--webedit`).
- **Cross-project plan browser** in Design > Plans with status cycling, deprecation, reorder, edit-in-nvim.
- **Library MCP server** (`orrch-mcp`) exposing skills, tools, agent invocation, project state, and codebase briefing to running agents.
- **Self-extending library** — agents can create new agents, skills, tools, and workflows at runtime via MCP and save them back to the library.
- **Conventional commits + SemVer + branch isolation** baked into the development workflow (every session works on its own branch; PM agent reviews; merge tool reconciles back to main).

## Status

**Single-user beta.** Runs daily on the maintainer's primary dev machine (orrion). Roughly 119 tests across 9 crates, ~5MB release binary on Linux x86_64. Scope is `commercial` (intent: monetizable; currently distributed as source).

Audience right now: solo developers running multiple AI coding sessions on a single workstation.

Not yet supported / known unfinished:
- Native installers for distributions other than source-build.
- Some Phase-9 (Publish) sub-features are scaffolded but not wired end-to-end.
- Token-budget tracking subsystem (TOK-001/002/003) is in design.
- The Hypervise panel's session brief navigation surface is a work-in-progress.
- Cross-machine session orchestration (orrpheus, mbp15, cb17 compatibility sessions) classification is partial.

This is also not yet a "drop into your team's dev process" tool. It assumes a single-user home-lab posture — Syncthing-based file sync between machines, tmux as the session substrate, and your own LLM API keys / Ollama install. Multi-user / multi-tenant deployment is not on the roadmap yet.

## Related projects

orrchestrator coordinates work across the rest of the TruStoryHnsl ecosystem; it doesn't replace any of them. The repos it touches most directly:

- **concord** — self-hosted Matrix chat. orrchestrator can post to a concord room from a Hypervisor-managed agent (e.g., OpenClaw multi-account agents on `concorrd.com`).
- **concord-extensions** — pluggable concord features. Often a target *for* orrchestrator-managed dev sessions.
- **orrtellite** — self-hosted Headscale/WireGuard mesh. The "tailnet-only" WebUI mode targets orrtellite peers.
- **orrbeam** — bidirectional remote-desktop mesh. Useful for visually-supervising orrchestrator sessions running on a different machine.
- **orracle** — AI model training stack. Models trained there can be registered as a `local`-tier entry in orrchestrator's library.
- **orrguard** — health-check + auto-remediation for orrgate / orrion. Keeps the underlying machines orrchestrator runs on alive.
- **borrk**, **orrapus**, **omnipus** — most of these projects' instruction queues are populated *through* orrchestrator's intake pipeline.

## License

[MIT](LICENSE).
