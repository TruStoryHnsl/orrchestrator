# Session Handoff — Orrchestrator Development

**Date**: 2026-03-31
**Outgoing model**: Claude Opus 4.6 (usage limit reached)
**Incoming model**: Gemini CLI
**Resume instruction**: `gemini` from `/home/corr/projects/orrchestrator/`

## What This Project Is

Orrchestrator is an AI-powered software development hypervisor — a Rust TUI application that manages parallel AI coding sessions, organizes agent workforces, and provides a visual interface for the user's entire development pipeline.

**Tech stack**: Rust + ratatui + tokio + crossterm. Cargo workspace with 6 crates.

## What Was Built This Session

This was a massive planning + implementation session. Starting from a completed v1 (35/35 features), we processed a redesign plan and built the foundation for a 2.0.0 agent orchestration platform.

### Completed work:
1. **Fresh PLAN.md** with 64+ roadmap items across 8 phases, all design decisions resolved
2. **Panel restructure**: 5 top-level panels (Design | Oversee | Hypervise | Analyze | Publish)
3. **Design panel**: 3 sub-panels (Intentions | Workforce | Library)
4. **Workforce editor**: 9 tabs (Workflows | Teams | Agents | Skills | Tools | MCP | Profiles | Training | Models)
5. **3 new crates**: orrch-agents (21 agent roles, department hierarchy, AgentRunner), orrch-workforce (templates, operations, engine, markdown parser), orrch-library (models, harnesses, MCP configs, valves, templates)
6. **21 agent profiles** in `agents/` directory (.md files with YAML frontmatter)
7. **3 workforce templates** in `workforces/` (General SW Dev, Personal Tech Support, Commercial SW Dev)
8. **2 operation modules** in `operations/` (INSTRUCTION INTAKE, DEVELOP FEATURE)
9. **8 model definitions + 5 harness definitions** in `library/`
10. **API valve system** — per-provider on/off toggle with timed auto-reopen, persisted to valves.json
11. **Hypervisor agent** — orchestrates workforces via subagent nesting
12. **Resource Optimizer agent** — annotates tasks with model/harness cost recommendations
13. **Intentions pipeline** — ideas with 100-step color gradient tracking (yellow→default→green)
14. **Agent profile support in spawn wizard** (goal → agent → backend → host)
15. **Template system** — `n` key creates blank templates for any category
16. **Config system** — unified Config struct at `~/.config/orrchestrator/config.json`
17. **88 tests pass** across all crates

### Key design decisions (all recorded in PLAN.md):
- **One session per workflow**, NOT per agent (token efficiency)
- **Hypervisor agent** orchestrates via Claude's Agent/subagent tool
- **Context isolation** — verification agents never see other verifiers' results
- **File inbox between operations**, prompt injection within workflows
- **fb2p.md deprecated** → per-project `instructions_inbox.md` managed by COO
- **Ollama via Crush/OpenCode**, not raw Ollama
- **Library is git-backed GitHub repo** + single orrch-mcp server
- **Workforce format is structured markdown** (pipe-delimited step tables)
- **Three-tier model system**: enterprise/mid-tier/local with different instruction density

### Unprocessed instruction inbox:
`/home/corr/projects/orrchestrator/instructions_inbox.md` has 9 instructions (INS-001 through INS-009) from the latest feedback processing. Key items:
- INS-001: Responsive tab bar width (tabs extend beyond narrow windows)
- INS-002: Fix Workforce sub-tab navigation (Tab key not cycling)
- INS-003: Left-justify Library sub-panel (remove right-justification)
- INS-004: Add Harnesses editor as leftmost Workforce tab
- INS-005: Rich markdown preview renderer
- INS-006: Fix orphaned tmux sessions
- INS-007: Custom tmux status bar
- INS-008: Unified vim tmux window
- INS-009: Instruction audit trail with hash coordinates

### What to work on next:
1. **INS-001 and INS-002 are bugs** — fix those first
2. **INS-003** is a quick style fix
3. Then continue with the instruction inbox items
4. The Anthropic valve auto-reopens Friday at 15:00

## Crate Structure

```
crates/
  orrch-agents/      — agent profiles, department hierarchy, AgentRunner
  orrch-core/        — process manager, sessions, projects, feedback, backends, config, vault
  orrch-library/     — models, harnesses, MCP configs, valves, templates, item store
  orrch-retrospect/  — error capture, fingerprinting, protocols
  orrch-tui/         — ratatui panels, app state, UI rendering, editor
  orrch-workforce/   — workforce templates, operation modules, step engine, markdown parser
src/                 — binary crate (main.rs)
```

## Key Files

- `PLAN.md` — master development plan with all design decisions and roadmap
- `instructions_inbox.md` — queued instructions for implementation
- `agents/` — 21 agent profile .md files
- `workforces/` — workforce template .md files
- `operations/` — operation module .md files
- `library/models/` — 8 AI model definitions
- `library/harnesses/` — 5 harness definitions
- `plans/` — ideas vault with pipeline state in `plans/.pipeline/`
- `fb2p.md` — historical feedback log (deprecated as active system)

## Memory (for Claude sessions)

Memory files at `~/.claude/projects/-home-corr-projects-orrchestrator/memory/`:
- `feedback_agent_execution_model.md` — one session per workflow, not per agent
- `feedback_fb2p_deprecated.md` — use instructions_inbox.md instead
- `project_orrchestrator_redesign.md` — 8-phase roadmap overview

## Build & Test

```bash
cargo build          # should compile clean (warnings only)
cargo test           # 88 tests should pass
cargo watch -x run   # live-reload dev loop
cargo build --release  # ~5MB native binary
```

## User Preferences

- Scope: private (iterate fast, no over-engineering)
- Language: Rust for everything new
- Commits: conventional commit format
- The user writes stream-of-consciousness feedback that needs to be processed by the COO into optimized instructions
- Token efficiency is a core design principle
- The user prefers terse responses and dislikes summaries of work they can see in diffs
