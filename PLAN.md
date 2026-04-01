# Orrchestrator — Master Development Plan

A full-service AI-powered software development hypervisor that unifies AI workflow management and enables design of node-based corporate emulation models for AI agent workforces.

## Open Questions

These must be resolved before their respective phases begin. Phases are blocked until their questions are answered.

### Q1 — Versioning Strategy ~~(blocks Phase 0)~~ RESOLVED
**Decision:** Tag `1.0.0` on the current codebase and build incrementally. The Rust workspace is modular enough — add new crates, restructure panels. No archive/scaffold needed.

### Q2 — Agent Execution Model ~~(blocks Phase 1)~~ RESOLVED
**Decision:** One Claude Code session per active workflow, NOT per agent. Token efficiency is a core design principle.

- **Hypervisor agent** — a new agent profile that orchestrates each workforce. It never speaks to the user directly. It receives the workflow definition, spawns subagents in order, pipes output between them via prompt injection, and gates verification agents from seeing prior results on the same task.
- **Subagent nesting** — unlimited depth. The hypervisor spawns subagents, which may spawn their own subagents. No artificial cap — constrained only by context window quality degradation.
- **Context isolation rule** — agents within a workflow must NOT share context about the current task being executed. Historical/completed work is shared via a core context file. Current-task isolation forces genuine independent verification (e.g., Beta Tester can't see Feature Tester already passed).
- **Core context file** — shorthand reference info (project summary, key decisions) available to all agents. Updated only when a workflow completes, never mid-execution.

**Communication is two-tier:**

| Layer | Mechanism | Example |
|-------|-----------|---------|
| Between operations (decoupled) | File inbox (append-only) | COO → `instructions_inbox.md` → PM ingests when ready |
| Within a workflow (tight coupling) | Prompt injection via subagents | PM ↔ Software Engineer ↔ Developer |

The file inbox separation enables throttling: COO can keep appending while PM is rate-limited. The prompt injection path is for agents that are part of the same execution flow.

### Q3 — Workforce Step Execution ~~(blocks Phase 2)~~ RESOLVED
**Decision:** Follows from Q2's subagent model.

- **Parallel branches** — the hypervisor agent spawns multiple subagents concurrently (e.g., Developer + Researcher + Feature Tester all as parallel Agent calls at the same step index).
- **Blocker polling** — checked by the hypervisor between steps, not mid-step. A running subagent finishes its current work before the workflow pauses.
- **Data handoff within workflow** — prompt injection between subagents (the hypervisor captures output and injects it into the next subagent's prompt).
- **Data handoff between operations** — file inbox. Separate operations are decoupled by design to enable independent throttling.

### Q4 — Native Window Mode Scope (blocks Phase 7)
The redesign acknowledges the node editor may not translate well to TUI. Options:
- a) TUI-only MVP with a simplified node list (no visual graph)
- b) Spawn a native window (GTK/egui) for the node editor only
- c) Web-based node editor served locally (like Node-RED)
- d) Defer entirely — design workforces via `.md` templates first, visual editor later

### Q5 — Ollama Integration (blocks Phase 4)
Ollama doesn't have a `claude`-like interactive CLI. Options:
- a) Wrap `ollama run <model>` in a PTY (it does have an interactive mode)
- b) Use the Ollama HTTP API directly (localhost:11434) — would need a thin CLI wrapper
- c) Support Ollama only via API provider integration, not as a PTY session

### Q6 — Token Optimization Specifics (blocks Phase 4)
"Layered abstraction and recompilation to minimize token length" — what gets compressed?
- a) The user's raw feedback before it enters fb2p.md (already partially done by COO)
- b) The accumulated PLAN.md / fb2p.md context that "continue development" loads
- c) Inter-agent handoff messages within a workforce
- d) All of the above, with different compression strategies per layer

### Q7 — Library Storage & Distribution (blocks Phase 5)
Skills, tools, agents, MCP configs — how are they stored and made available?
- a) Flat files in `~/.config/orrchestrator/library/` with subdirs per type
- b) SQLite database with full-text search
- c) Git-backed repository (versionable, shareable)
- Which agents get which tools? Auto-assigned by Mentor, or user-configured per workforce?

### Q8 — Workforce Design Input Format (blocks Phase 6)
Before the visual node editor exists, how does the user define custom workforces?
- a) YAML/TOML template files (machine-parseable, human-editable)
- b) Markdown with structured sections (matches existing .md patterns)
- c) Interactive TUI wizard (guided step-by-step)

---

## Architecture

### Core Concept
A terminal-based application (fully usable over SSH) that serves as an AI development pipeline hypervisor. Users design agent workforces, submit development plans, and manage parallel autonomous AI sessions across multiple projects simultaneously.

### Hard Requirements
- **SSH-first**: Fully usable over remote SSH — no GUI dependencies
- **Terminal multiplexing**: PTY allocation for embedded terminal sessions
- **Multi-provider AI**: Claude Code, Gemini CLI, Ollama, and raw API access (OpenAI, Anthropic, Google)
- **Unmanaged session awareness**: Discovers existing Claude/Gemini processes on the system
- **Focus/maximize**: Any session expandable to full screen for direct interaction
- **Dynamic throttling**: Respects API rate limits, auto-pauses/resumes workforces

### Panel Layout

```
[ Design ] [ Oversee ] [ Hypervise ] [ Library ]
```

| Panel | Purpose | Maps to v1 |
|-------|---------|------------|
| **Design** | Two sub-panels: Project Design (feedback intake + routing) and Workforce Design (node-based agent team editor) | Ideas + Feedback (merged + expanded) |
| **Oversee** | Project tracker — the existing projects dashboard with additions | Projects (largely unchanged) |
| **Hypervise** | Interactive multi-session management — live status, grouped workflows, inline prompts | Sessions (major expansion) |
| **Library** | Database of agents, skills, tools, MCP servers, API keys, workforce templates | New |

### Crate Structure

```
crates/
  orrch-core/        # existing — process manager, sessions, projects, feedback, backends
  orrch-tui/         # existing — ratatui panels, app state, UI rendering
  orrch-retrospect/  # existing — error capture, fingerprinting, protocols
  orrch-agents/      # NEW — agent profiles, department hierarchy, execution binding
  orrch-workforce/   # NEW — workforce templates, operation modules, step engine
  orrch-library/     # NEW — library storage, search, MCP server, tool distribution
src/                 # binary crate — ties everything together
```

### Agent Department Hierarchy

```
Admin Department
  Executive Assistant    — default user interface, routes non-dev input
  Chief Operations Officer (COO) — processes dev instructions → optimized prompts → project routing
  Intelligence Resources Manager — monitors API usage, decides workforce pause/resume
  Mentor                 — advises strategy, analyzes agent .md files, assigns tools/skills

Development Department
  Leadership
    Project Manager      — plan/build/test/break loop, delegates to team, cross-project awareness
    Talent Scout         — creates specialist agents, maintains specialist database
  Engineering
    Software Engineer    — architecture design, roadmap maintenance
    Developer            — implements code per supervisor instructions
    Feature Tester       — designs and runs tests in deployment environments
    Researcher           — comprehensive research on software solutions
    UI Designer          — interface design per UX specifications
    Specialist           — narrow-domain expert, built by Talent Scout + Researcher
  Quality Assurance
    Penetration Tester   — security testing, vulnerability reports
    Beta Tester          — destructive testing, failure point discovery
  DevOps
    Repository Manager   — git operations, semantic versioning, commit packaging

Marketing Department
  UX Specialist          — cross-platform interface audit + improvement reports
  Market Researcher      — target market investigation

Legal Department
  Licensing Auditor      — dependency license analysis
  Copyright Investigator — copyright conflict research
```

### Operation Module Template

```
Operation {
  name: String,
  trigger: TriggerCondition,      // event that starts this operation
  blocker: Option<BlockCondition>, // condition that prevents running
  steps: Vec<Step>,                // ordered operations
  interrupts: Vec<InterruptCondition>, // conditions that cancel mid-run
}

Step {
  index: u32,
  agent: AgentRef,
  tool_or_skill: Option<String>,
  operation: String,              // natural language instruction
  parallel_group: Option<u32>,    // steps with same group run concurrently
}
```

### Multi-Backend Configuration

```yaml
backends:
  claude:
    command: claude
    flags: ["--dangerously-skip-permissions"]
    kind: cli_pty
  gemini:
    command: gemini
    flags: []
    kind: cli_pty
  ollama:
    command: ollama
    model: "codellama:34b"
    kind: cli_pty  # or api_http
  anthropic_api:
    kind: api_http
    endpoint: "https://api.anthropic.com/v1/messages"
    key_env: ANTHROPIC_API_KEY
  openai_api:
    kind: api_http
    endpoint: "https://api.openai.com/v1/chat/completions"
    key_env: OPENAI_API_KEY
```

### Key Workflows

**User submits feedback:**
```
User writes in vim → orrchestrator saves draft →
  User sets type (instructions | plan | idea | knowledge) →
    Type-specific workforce processes it:
      instructions → COO optimizes → routes to project queues
      plan → COO + PM evaluate → may create project or trigger versioning
      idea → stored in Ideas vault as unattached
      knowledge → saved to Library
```

**Workforce executes a feature:**
```
Trigger: unprocessed instructions in project queue
  1. PM synthesizes instructions into project plan
  2. PM delegates to team (parallel):
     - Developer: coding tasks
     - Researcher: background research
     - Engineer: architecture design
     - UI Designer: interface elements
     - Feature Tester: test design
  3. PM runs dev-loop until testers report acceptable results
  4. PM compares deliverable to instructions
  5. PM writes dev log with version tag
  6. Repo Manager commits with semantic version
```

**Dynamic throttling:**
```
Intelligence Resources Manager polls API usage →
  If provider approaching rate limit:
    Pause queues using that provider →
    Shift work to available providers if possible →
    Resume when usage drops below threshold
```

---

## Roadmap — Agent Orchestration Platform (toward 2.0.0)

The completed feature set below constitutes `1.0.0`. The roadmap below builds toward `2.0.0` — a breaking architectural expansion from session manager to agent orchestration platform.

### Phase 0: Foundation Prep (1.1.0)
_Restructure the existing codebase to support the new architecture. No new features — just plumbing._

1. [ ] **Panel restructuring** — rename/remap panels: Ideas+Feedback → Design, Projects → Oversee, Sessions → Hypervise, new Library panel. Update `Panel` enum, tab bar labels, keybindings.
2. [ ] **Design panel sub-navigation** — split Design into two sub-panels: "Project Design" (feedback intake + type routing) and "Workforce Design" (placeholder). Left/right or tab toggles between them.
3. [ ] **New crate: `orrch-agents`** — cargo workspace member with agent profile struct, department enum, capability list. No execution yet — just the data model.
4. [ ] **New crate: `orrch-workforce`** — workforce template struct, operation module struct, step struct. Data model only.
5. [ ] **New crate: `orrch-library`** — library item types (agent, skill, tool, mcp_server, api_key, workforce_template), storage backend trait, filesystem implementation.
6. [ ] **Configuration migration** — move from hardcoded backend detection to `~/.config/orrchestrator/config.toml` with backends, library paths, agent directories.

### Phase 1: Agent Framework (1.2.0)
_Agents become first-class entities. Each agent is a `.md` profile that can be bound to a real AI session._

7. [ ] **Agent profile format** — `.md` files in `~/.config/orrchestrator/agents/` with YAML frontmatter: name, department, role, capabilities, preferred_backend, tool_requirements. Body is the agent's system prompt / identity instructions.
8. [ ] **Agent library — 19 predefined agents** — create `.md` profiles for all agents listed in the department hierarchy. Each with tailored system prompts, domain knowledge, and skill references.
9. [ ] **Agent execution binding** — `AgentRunner` that takes an agent profile + task instruction → spawns a session with the agent's system prompt prepended. Maps to existing `ProcessManager::spawn()`.
10. [ ] **Agent status tracking** — extend `Session` struct with `agent_id: Option<String>`, display agent role badge in Hypervise panel.
11. [ ] **COO instruction optimizer** — implement the token-efficient prompt compression pipeline. Takes raw user feedback → deduplicates, strips filler, normalizes references, outputs optimized instruction set. Replaces raw feedback passthrough in the routing pipeline.
12. [ ] **Mentor agent integration** — background task that periodically reviews agent profiles against the Library. Suggests tool/skill additions. User confirms before modifying agent `.md` files.

### Phase 2: Workforce Templates (1.3.0)
_Agents are organized into teams with defined operation flows._

13. [ ] **Workforce template data model** — `Workforce { name, agents: Vec<AgentRef>, connections: Vec<Connection>, operations: Vec<Operation> }`. Serialized as YAML/TOML (pending Q8).
14. [ ] **Built-in workforce templates** — create templates for: Personal Tech Support, General Software Development, Experimental Software Development, Commercial Software Development, Private Software Development, Curriculum Design.
15. [ ] **Template selector in spawn flow** — when spawning from Oversee panel, offer: "Single agent" (existing flow) or "Workforce: <template>" which instantiates the full team.
16. [ ] **Workforce-aware session management** — sessions spawned by a workforce are grouped. Hypervise panel shows the group as a collapsible entry displaying only the user-facing agent's output.

### Phase 3: Operation Modules (1.4.0)
_Workforces execute structured pipelines with triggers, blockers, and interrupts._

17. [ ] **Operation module engine** — runtime that executes operation steps in order, respects parallel groups, checks blockers before starting, monitors interrupt conditions.
18. [ ] **INSTRUCTION INTAKE module** — Executive Assistant → COO → Project Manager pipeline. Triggered by user feedback submission.
19. [ ] **DEVELOP FEATURE module** — PM-led heuristic loop: synthesize → delegate → parallel dev/test → dev-loop until pass → compare → log → commit. Triggered by unprocessed instructions in project queue.
20. [ ] **Module status display** — Hypervise panel shows running modules with step progress: "DEVELOP FEATURE [3/7] — Developer executing"
21. [ ] **Module editor** — view/edit operation modules in Design > Workforce Design sub-panel. Add/remove steps, change agents, set triggers.

### Phase 4: Multi-Provider & Resource Management (1.5.0)
_Expand beyond Claude+Gemini. Add API usage intelligence._

22. [ ] **Provider abstraction layer** — unified `Provider` trait with `cli_pty` and `api_http` variants. CLI providers use existing PTY spawn. API providers use `reqwest` with streaming response parsing.
23. [ ] **Ollama backend** — `ollama run <model>` PTY integration or HTTP API wrapper (pending Q5).
24. [ ] **Raw API backends** — Anthropic Messages API, OpenAI Chat Completions API. Direct HTTP, no CLI dependency.
25. [ ] **Intelligence Resources Manager** — background task tracking per-provider: requests/minute, tokens/minute, remaining quota (where APIs report it). Stores in `~/.config/orrchestrator/usage.jsonl`.
26. [ ] **Dynamic throttling** — when a provider approaches rate limits, the IRM pauses workforce queues using that provider. Shifts work to alternative providers when possible. Resumes automatically.
27. [ ] **Token optimization pipeline** — layered compression (pending Q6): feedback compression (COO), context compression (PLAN.md summarization for long projects), handoff compression (inter-agent message trimming).

### Phase 5: Library (1.6.0)
_Centralized database of reusable AI workflow components._

28. [ ] **Library storage backend** — filesystem-based (pending Q7): `~/.config/orrchestrator/library/{agents,skills,tools,mcp_servers,workforce_templates,api_keys}/`. Each item is a file with YAML frontmatter + content.
29. [ ] **Library panel UI** — browseable/searchable list in the Library tab. Filter by type. Preview pane. Create/edit/delete actions.
30. [ ] **Library MCP server** — orrchestrator hosts an MCP server that exposes library items as tools/resources. Managed sessions can query the library for available tools.
31. [ ] **AI-assisted creation** — "New skill/tool/agent" action spawns a Claude session that helps the user define the item interactively. Result is saved to the library.
32. [ ] **Auto-assignment via Mentor** — when an agent is bound to a session, the Mentor reviews its profile against Library contents and injects relevant tool/skill references into the agent's prompt.

### Phase 6: Node-Based Workforce Designer (1.7.0)
_Visual agent-as-node workflow editor._

33. [ ] **Workforce definition format** — finalize the file format for custom workforces (pending Q8). Must support: agents as nodes, directed connections between nodes, operation step sequences, nested sub-workforces.
34. [ ] **TUI node list view** — simplified non-visual representation: ordered agent list with connection arrows, step assignments, trigger/blocker display. Editable in the TUI.
35. [ ] **Nested workforces** — a workforce node can contain another workforce. The inner workforce runs as a unit, reporting only its designated output agent's results to the parent.
36. [ ] **Workforce import/export** — save/load workforce designs as files. Share between orrchestrator instances.

### Phase 7: Native Window Mode (1.8.0, pending Q4)
_Escape the TUI for features that need richer interfaces._

37. [ ] **Window spawning infrastructure** — ability for orrchestrator TUI to spawn a native window (egui or web-based) for specific sub-features. TUI continues running alongside.
38. [ ] **Visual node editor** — drag-and-drop agent nodes, draw connections, set properties. Full visual workforce designer in the native window.
39. [ ] **Non-TUI mode** — orrchestrator can optionally launch entirely as a windowed application for terminal-averse users.

### Phase 8: Intake Workforces (2.0.0-rc)
_The predefined workforces that process different types of user input. Completing this phase means the full agent orchestration pipeline is operational — tag `2.0.0` when stable._

40. [ ] **workforce:instruction-intake** — processes project dev instructions. COO optimizes → routes to project queues → PM incorporates into plan.
41. [ ] **workforce:plan-intake** — processes new project designs/plans. Evaluates scope, may create new projects, may trigger versioning. Distributes instructions.
42. [ ] **workforce:idea-intake** — processes incomplete ideas. Stores in Ideas vault as unattached. Tags with domain, potential project associations.
43. [ ] **workforce:knowledge-intake** — processes custom agents, skills, tools. Validates, formats, saves to Library. Updates relevant agent profiles.

### Carried Forward (from 1.0.0 queued items)
- [ ] **Agent profile management** — swappable CLAUDE.md/GEMINI.md profiles per project
- [ ] **Feedback pipeline redesign** — per-project persistent append-only feedback logs, vim split-view, diff-based extraction

---

## 1.0.0 Feature Roadmap (COMPLETE — 35/35)

1. [x] **Core process manager** — spawn/kill/monitor Claude Code sessions in PTYs with `--dangerously-skip-permissions`
2. [x] **External session discovery** — detect unmanaged Claude processes, display in dashboard
3. [x] **Dashboard view** — session list with status, info panel, project picker modal
4. [x] **Session maximize/focus** — full-screen a session for direct user interaction, Esc to return
5. [x] **External vim editor** — press `f` to spawn real vim in a new terminal window (falls back to same terminal over SSH)
6. [x] **Feedback routing** — parse feedback for project names, append to fb2p.md, save to .feedback/, routing summary overlay
7. [x] **Default "continue development" spawn** — empty goal = "continue development" prompt, the primary workflow
8. [x] **Parallel feature pipelines** — multiple Claude instances per project on different features
9. [x] **Live progress** — parse Claude Code output for task completion signals
10. [x] **Waiting-for-input detection** — highlight sessions that need user attention (yellow highlight + notification)
11. [x] **Error capture + fingerprinting** — parse session output for errors/stack traces, normalize and store per-project
12. [x] **Solution pairing** — detect when errors are resolved, pair fix with error fingerprint
13. [x] **Retrospective analyzer** — background process that mines dev logs for recurring patterns across projects
14. [x] **Protocol generation** — distill analysis into structured `.troubleshooting.md` decision trees per project
15. [x] **Active injection** — auto-inject known solutions into Claude sessions when recognized errors appear
16. [x] **Cross-project knowledge base** — shared `~/projects/.troubleshooting-global.md` for ecosystem-wide patterns
17. [x] **Multi-backend support** — Claude + Gemini CLI, auto-detection, Tab to switch in project picker
18. [x] **Interactive project picker** — modal overlay, arrow keys to navigate, Tab to toggle backend
19. [x] **Inline project editor** — e/f key in project detail spawns vim, routes directly to project fb2p.md on save
20. [x] **Master plan append mode** — m key spawns vim for append text, appends to MASTER_PLAN.md on save, routes to fb2p.md
21. [x] **Ideas vault** — "Plans" menu (p key) for undeveloped ideas, stored in orrchestrator/plans/
22. [x] **Better PLAN.md parsing** — descriptions from first paragraph, next priority item, improved format detection
23. [x] **Color tags** — manual green/yellow/red (t key cycles), stored in .orrtag, default sort by tag
24. [x] **Expandable project rows** — Tab toggles, shows sessions with goal tags inline
25. [x] **External session mapping** — map discovered sessions to projects by cwd match, show in expanded rows + detail
26. [x] **Session-goal inheritance** — auto-inherit goals, yellow warning on shared goals
27. [x] **Smart default actions** — "create plan" / "run queued" / "continue dev" shown per project state
28. [x] **"Personal" scope** — new scope level below private for user-only projects
29. [x] **Project file browser** — split-pane browser (b key), preview pane for text/metadata, Enter to edit/navigate, Backspace to go up
30. [x] **Deprecated browser** — read-only file browser for archived projects (d key), same split-pane layout
31. [x] **PLAN.md detection fix** — case-insensitive scan, finds plan.md/PLAN.md/DEVELOPMENT_PLAN.md, falls back to CLAUDE.md for descriptions
32. [x] **Project metadata line** — shows CLAUDE.md | Cargo.toml | master plan | v2 | Docker etc. under each project
33. [x] **Feedback pipeline tab** — new "Feedback" tab showing drafts/routed items with status, submit/resume/delete actions, routing target display
34. [x] **External vim replacement** — replaced custom inline editor with real vim spawned in new terminal windows (auto-detects $TERMINAL/alacritty/kitty/konsole)
35. [x] **Feedback lifecycle** — drafts saved to .feedback/, status tracked in .status.json, submit routes to projects, delete cleans up

---

## Technical Stack

- **Rust + Cargo** — native Linux binary, cargo workspace
  - `ratatui` + `crossterm` for TUI
  - `tokio` async runtime for PTY management, process spawning, background tasks
  - `nix` crate for Unix process control
  - `serde` + `serde_json` for serialization
  - `sha2` for error fingerprinting
  - `regex` for pattern matching
  - `reqwest` (future) for API provider HTTP calls
- **Cargo workspace**: `crates/orrch-core`, `crates/orrch-tui`, `crates/orrch-retrospect`, + 3 new crates
- **Development**: `cargo watch -x run` for live-reload, `cargo test` for testing
- **Distribution**: `cargo build --release` → single static binary
- **Target platforms**: orrion (CachyOS/Alacritty), any SSH client, orrpheus (macOS via orrch-agent.sh)

## Keybindings (tentative)

| Key | Context | Action |
|-----|---------|--------|
| `Tab` / `Shift+Tab` | Global | Next/prev panel |
| `Enter` | Any list | Expand/focus selected item |
| `Esc` | Any sub-view | Back to panel list |
| `f` | Global | New feedback (opens vim) |
| `n` | Oversee/Hypervise | New session (spawn wizard) |
| `N` | Oversee | Multi-spawn all open roadmap items |
| `w` | Design > Workforce | New/edit workforce template |
| `l` | Global | Jump to Library |
| `a` | Any item | Context action menu |
| `t` | Oversee | Cycle color tag |
| `p` | Design > Project | Toggle plan mode on feedback |
| `q` | Global | Quit |

## Recent Changes
- 2026-03-31: **FRESH PLAN.md written.** Processed redesign_plan into 8-phase roadmap toward 2.0.0 (43 items + 2 carried forward). 8 open questions identified for user resolution. Architecture expanded: 3 new crates, 4-panel layout, 19 agent roles, workforce templates, operation modules, multi-provider AI, node-based designer.
