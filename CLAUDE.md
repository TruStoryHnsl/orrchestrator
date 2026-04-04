# Orrchestrator

AI-powered software development hypervisor. Rust TUI managing parallel AI coding sessions, agent workforces, and development pipelines.

## Build & Test

```bash
cargo build              # compile (warnings OK)
cargo test               # 119 tests across 7 crates
cargo watch -x run       # live-reload dev
cargo build --release    # ~5MB native binary
```

## Crate Structure

```
crates/
  orrch-agents/      — agent profiles, department hierarchy (21 roles), AgentRunner
  orrch-core/        — process manager, sessions, projects, feedback, backends, config, vault
  orrch-library/     — models, harnesses, MCP configs, valves, templates
  orrch-retrospect/  — error capture, fingerprinting, troubleshooting protocols
  orrch-tui/         — ratatui panels, app state, UI rendering, editor integration
  orrch-workforce/   — workforce templates, operation modules, step engine, markdown parser
src/                 — binary crate (main.rs)
```

## Panel Layout

```
Top bar:  [ Design ] [ Oversee ] [ Hypervise ] [ Analyze ] [ Publish ]

Design sub-bar:  Intentions | Workforce | Library

Workforce tabs:  Workflows | Teams | Agents | Skills | Tools | MCP | Profiles | Training | Models
```

- **Design > Intentions**: Ideas vault with instruction intake pipeline. `n`=new, `s`=submit to pipeline, `Enter`=edit in vim. Ideas track 0-100% progress through color gradient (yellow→default→green).
- **Design > Workforce**: Full-stack .md editor for all workflow components. `Tab` switches internal tabs. `n`=new from template, `Enter`=edit, `d`=delete.
- **Design > Library**: Read-only browser. `Tab` switches sub-tabs. `v`=toggle API valve on Models, `e`=toggle MCP server.
- **Oversee**: Project tracker with expandable rows, file browser, session management.
- **Hypervise**: Interactive multi-session management (tmux-based).
- **Analyze**: Placeholder — token efficiency stats.
- **Publish**: Placeholder — release packaging, legal analysis, marketing.

## Key Architecture Decisions

- **One session per workflow, NOT per agent** — token efficiency is a core design principle
- **Hypervisor is a thin dispatcher, NOT an LLM agent** — it mechanically executes a step table, spawning agents and piping compressed output. Zero reasoning overhead. All orchestration logic lives in the workflow definition and deterministic tools.
- **Context isolation** — verification agents (testers) never see other verifiers' results on the same task
- **File-cluster batching** — tasks grouped by shared files, not agent role. One agent reads each file. Duplicate roles (3 Developers) are fine if they reduce file overlap.
- **Deterministic tools between steps** — `codebase_brief.sh` (API surface extraction), `compress_output.sh` (structured data extraction from agent output), `cluster_tasks.sh` (union-find file clustering). These replace LLM-based "compression" and "batching" reasoning.
- **File inbox between operations** (`instructions_inbox.md`)
- **fb2p.md is deprecated** — replaced by per-project `instructions_inbox.md` managed by COO
- **Three-tier model system**: enterprise (Claude/GPT-4o), mid-tier (Mistral Large), local (Ollama)
- **Workforce format**: structured markdown with pipe-delimited step tables (auto-detects parallel groups)
- **API valves**: per-provider on/off toggle persisted in `~/.config/orrchestrator/valves.json`

## Key Files

| File | Purpose |
|------|---------|
| `PLAN.md` | Master dev plan — 64+ roadmap items, all design decisions |
| `instructions_inbox.md` | Queued instructions for implementation (INS-001 through INS-009) |
| `agents/*.md` | 21 agent profiles with YAML frontmatter |
| `workforces/*.md` | Workforce templates (team compositions) |
| `operations/*.md` | Operation modules (step pipelines) |
| `library/models/*.md` | 8 AI model definitions with tier/pricing |
| `library/harnesses/*.md` | 5 harness definitions with auto-detection |
| `plans/` | Ideas vault, pipeline state in `plans/.pipeline/` |
| `HANDOFF.md` | Session handoff document (detailed context dump from 2026-03-31) |

## Agent Department Hierarchy

Admin: Executive Assistant, COO, Intelligence Resources Manager, Mentor, Hypervisor
Dev/Leadership: Project Manager, Talent Scout, Resource Optimizer
Dev/Engineering: Software Engineer, Developer, Feature Tester, Researcher, UI Designer, Specialist
Dev/QA: Penetration Tester, Beta Tester
Dev/DevOps: Repository Manager
Marketing: UX Specialist, Market Researcher
Legal: Licensing Auditor, Copyright Investigator

## Instruction Intake Pipeline

User writes idea → `s` submits → COO optimizes into discrete instructions → routes to project `instructions_inbox.md` → PM incorporates into plan. Idea stays in place, color tracks progress: 0-4% default, 5% yellow (processing), 5-50% yellow→default (intake), 50-100% default→green (implementation).

## Workflow Execution (MANDATORY)

Every session spawned by orrchestrator MUST follow the workforce pipelines defined in this project. You are the **Hypervisor agent**. Read `agents/hypervisor.md` for your full behavioral profile.

### Architecture: Skills and Tools (NOT prompt injection)

Workflow execution uses **skills** and **tools**, NOT prompt blob injection into sessions.

- **Skills** (`.md` prompt files in `library/skills/`): LLM judgment required — workflow orchestration (`/develop-feature`), agent roles (`/agent:pm`, `/agent:developer`), instruction optimization. Harness-agnostic.
- **Tools** (shell scripts in `library/tools/`): deterministic repeatable operations — file routing, git packaging, version tagging. No LLM judgment.

The `/develop-feature` skill IS the Hypervisor — it procedurally spawns agents via Agent tool calls, pipes results, enforces isolation, manages the dev loop. Sessions start with a skill invocation, not a prompt blob.

**Deprecated:** `build_workforce_context()`, SpawnWorkforce prompt injection, composite prompt construction. These approaches were tested and failed — sessions ignored the injected instructions.

### Instruction intake pipeline

```
EA separates → COO optimizes → USER REVIEWS (side-by-side, editable) → COO distributes → PM incorporates
```

The user audit step (raw vs optimized, side-by-side in Design > Intentions) is mandatory. Instructions do not route to project inboxes until the user confirms.

## Current Priority

**Critical Path (CP-1 through CP-6):** Build the skill-based workflow execution system. See PLAN.md CRITICAL PATH section for details. CP-1 (workflow skills) and CP-2 (agent role skills) are the immediate next items.

`instructions_inbox.md` has 9 instructions. INS-001 through INS-003 are done. Remaining: INS-004 through INS-009.

**Critical priority:** Implement the CRITICAL PATH items in PLAN.md (CP-1 workforce-aware spawning, CP-2 template selector, CP-3 COO intake wiring). These are the highest priority items in the entire roadmap — without them, every spawned session ignores the orchestration architecture. The "Workflow Execution" section above is a stopgap until CP-1/CP-2 make it automatic.

## Conventions

- Scope: `private` — iterate fast, no over-engineering
- Language: Rust for all new code
- Commits: conventional commit format (`feat:`, `fix:`, `refactor:`, etc.)
- Token efficiency is a design principle — minimize context size, compress handoffs
- User writes stream-of-consciousness feedback; COO agent processes it into optimized instructions
