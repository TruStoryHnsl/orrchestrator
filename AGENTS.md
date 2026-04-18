# Orrchestrator

AI-powered software development hypervisor. Rust TUI managing parallel AI coding sessions, agent workforces, and development pipelines.

## Build & Test

```bash
cargo build              # compile (warnings OK)
cargo test               # 88 tests across 6 crates
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

- **Design > Intentions**: Ideas vault with instruction intake pipeline. `n`=new, `s`=submit to pipeline, `Enter`=edit in nvim. Ideas track 0-100% progress through color gradient (yellow→default→green).
- **Design > Workforce**: Full-stack .md editor for all workflow components. `Tab` switches internal tabs. `n`=new from template, `Enter`=edit, `d`=delete.
- **Design > Library**: Read-only browser. `Tab` switches sub-tabs. `v`=toggle API valve on Models, `e`=toggle MCP server.
- **Oversee**: Project tracker with expandable rows, file browser, session management.
- **Hypervise**: Interactive multi-session management (tmux-based).
- **Analyze**: Placeholder — token efficiency stats.
- **Publish**: Placeholder — release packaging, legal analysis, marketing.

## Key Architecture Decisions

- **One session per workflow, NOT per agent** — token efficiency is a core design principle
- **Hypervisor agent** orchestrates workforces via subagent nesting (unlimited depth)
- **Context isolation** — verification agents (testers) never see other verifiers' results on the same task
- **File inbox between operations** (`instructions_inbox.md`), prompt injection within workflows
- **PLAN.md is deprecated** — replaced by per-project `instructions_inbox.md` managed by COO
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

## Current Priority

`instructions_inbox.md` has 9 unimplemented instructions. INS-001 (responsive tabs) and INS-002 (workforce nav fix) are bugs — fix first.

## Conventions

- Scope: `private` — iterate fast, no over-engineering
- Language: Rust for all new code
- Commits: conventional commit format (`feat:`, `fix:`, `refactor:`, etc.)
- Token efficiency is a design principle — minimize context size, compress handoffs
- User writes stream-of-consciousness feedback; COO agent processes it into optimized instructions
