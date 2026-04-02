# Orrchestrator — Master Development Plan

A full-service AI-powered software development hypervisor that unifies AI workflow management and enables design of node-based corporate emulation models for AI agent workforces.

## Design Decisions (all resolved 2026-03-31)

Resolved during initial planning session. Preserved here as architectural reference.

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

### Q4 — Native Window Mode Scope ~~(blocks Phase 7)~~ RESOLVED
**Decision:** All four approaches, implemented in order:
1. **TUI node list** (Phase 6) — simplified non-visual representation, proves the data model
2. **`.md` template files** (Phase 6) — structured markdown for workforce definitions, power-user editing path
3. **Web-based editor** (Phase 7) — local web UI with drag-and-drop node graph, rich visual editing
4. **Native egui window** (Phase 7+) — fully integrated native window, no browser dependency

Each layer validates the next. The TUI list and templates ship first; visual editors come after the data model is proven.

### Q5 — Ollama Integration ~~(blocks Phase 4)~~ RESOLVED
**Decision:** Use existing agentic coding tools that support Ollama natively, NOT raw Ollama.

1. **Crush** — primary. Agentic coding CLI powered by Ollama. Added as `BackendKind::Crush`, same PTY spawn model as Claude/Gemini. Provides file editing, tool use, codebase awareness.
2. **OpenCode** — fallback if Crush doesn't work.
3. **Native modules** — last resort. Build Claude Code-like agentic capabilities (file read/write, shell exec, codebase search) directly in orrchestrator as modules wrapping Ollama's API.

The goal is full agentic coding from Ollama-powered sessions, not just chat.

### Q6 — Token Optimization Specifics ~~(blocks Phase 4)~~ RESOLVED
**Decision:** Three compression layers:

- **(a) User feedback → optimized instructions** — COO processes raw feedback into optimized instructions once during intake. This is a one-time transformation, not repeated.
- **(b) Project instruction queue management** — `fb2p.md` is deprecated, replaced by `instructions_inbox.md` per project. The COO manages these inboxes: appends optimized instructions, trims completed/outdated entries on version publish, truncates excessively long files. The intake process is formalized as four workforce modules (instruction-intake, plan-intake, idea-intake, knowledge-intake) per the redesign plan.
- **(c) Inter-agent handoff compression** — the hypervisor trims verbose reasoning from agent output before injecting into the next subagent's prompt. Keeps only actionable conclusions.

**Deprecated:** `fb2p.md` model, `/interpret-user-instructions` skill. Both replaced by the `workforce:instruction-intake` module (Executive Assistant → COO → PM pipeline).

### Q7 — Library Storage & Distribution ~~(blocks Phase 5)~~ RESOLVED
**Decision:**
- **Git-backed GitHub repo** for the library — versionable, shareable, synced across machines.
- **MCP server** hosted by orrchestrator exposes library contents to all managed sessions for easy access.
- **Tool assignment:** Workforce template defines the baseline tool set per agent role. Mentor agent suggests additions on top of the baseline by reviewing agent profiles against library contents.

### Q8 — Workforce Design Input Format ~~(blocks Phase 6)~~ RESOLVED
**Decision:** Structured markdown matching the pipe-delimited format from the redesign plan.

```markdown
## INSTRUCTION INTAKE

Trigger: user submits a prompt
Blocker: none

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1  | Executive Assistant | * | separate dev instructions from other input
1B | Executive Assistant | * | immediately address non-dev input
2  | COO | skill:clarify | process raw instructions into optimized instructions
3  | COO | skill:parse | determine which project each instruction goes to
4  | COO | tool:copy-file | append to appropriate project instruction_inbox.md
5  | Project Manager | skill:synthesize_instructions | incorporate into project plan

Interrupts: none
```

This format is human-writable, Claude-readable, and already proven by the redesign plan document itself. The workforce engine parses these structured sections.

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
    Resource Optimizer   — assesses task complexity, annotates plan with model/harness recommendations
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

1. [x] **Panel restructuring** — Design (Project+Workforce sub-panels), Oversee, Hypervise, Library. Panel enum + all match arms + tab labels + key handlers + UI rendering updated.
2. [x] **Design panel sub-navigation** — DesignSub enum with Shift+Tab toggle. ProjectDesign shows ideas (feedback intake). WorkforceDesign is placeholder.
3. [x] **New crate: `orrch-agents`** — agent profile struct (extracted from core), Department enum, AgentRole enum (20 roles across 4 departments), department→role mapping.
4. [x] **New crate: `orrch-workforce`** — Workforce template, AgentNode, Connection, Operation, Step, trigger/blocker/interrupt types. Markdown parser for pipe-delimited step tables with auto-detected parallel groups.
5. [x] **New crate: `orrch-library`** — ItemKind (6 types), LibraryItem struct, LibraryStore with filesystem backend, frontmatter parsing.
6. [x] **Configuration migration** — Config struct wrapping backends + agents_dir + library_dir + projects_dir. Loads from config.json, falls back to legacy backends.yaml.

### Phase 1: Agent Framework (1.2.0)
_Agents become first-class entities. Each agent is a `.md` profile that can be bound to a real AI session._

7. [x] **Agent profile format** — `.md` files with YAML frontmatter (name, department, role, description, capabilities, preferred_backend). Body is the agent's system prompt. Profile loader in orrch-agents crate.
8. [x] **Agent library — 20 agent profiles** — all 19 roles + Hypervisor created in `agents/` directory with tailored system prompts, behavioral rules, and domain constraints.
9. [x] **Agent execution binding** — `AgentRunner` with `build_prompt()`, `build_verification_prompt()` (context isolation), and `build_handoff_prompt()` (inter-agent data flow). `is_verification_role()` helper for isolation gating.
10. [x] **Agent status tracking** — `Session.agent_profile: Option<String>` field. Agent selection step in spawn wizard (SpawnAgent). Profile prepended to goal on spawn.
11. [ ] **COO instruction optimizer** — *requires live AI session to test*. Profile created (`chief_operations_officer.md`). Optimization logic is embedded in the COO's prompt instructions.
12. [ ] **Mentor agent integration** — *requires Library (Phase 5) + live sessions*. Profile created (`mentor.md`). Background task deferred until Library storage exists.

### Phase 2: Workforce Templates (1.3.0)
_Agents are organized into teams with defined operation flows._

13. [x] **Workforce template data model** — Workforce, AgentNode, Connection, DataFlow structs in orrch-workforce crate. Supports agent teams with directed connections and operation references.
14. [x] **Built-in workforce templates** — 3 created as structured markdown in `workforces/`: Personal Tech Support, General Software Development, Commercial Software Development.
15. [ ] **Template selector in spawn flow** — *deferred: needs workforce-aware session spawning (item 16) to be meaningful*.
16. [ ] **Workforce-aware session management** — *deferred: requires live hypervisor execution to test grouped session display*.

### Phase 3: Operation Modules (1.4.0)
_Workforces execute structured pipelines with triggers, blockers, and interrupts._

17. [x] **Operation module engine** — OperationExecution runtime: tracks state (Idle/Blocked/Running/Complete/Interrupted), next_steps() returns parallel batches, advance() moves pointer, progress_display() for UI. load_operations() reads .md files.
18. [x] **INSTRUCTION INTAKE module** — `operations/instruction_intake.md`. EA → COO (clarify, parse) → PM (synthesize). Parser auto-detects parallel group at step 1 (EA handles two tasks concurrently).
19. [x] **DEVELOP FEATURE module** — `operations/develop_feature.md`. 9-step pipeline with parallel group at step 3 (Dev+Researcher+Engineer+UIDesigner+FeatureTester) and step 4 (PenTester+BetaTester).
20. [ ] **Module status display** — *deferred: needs live operation execution to display*. progress_display() method ready.
21. [ ] **Module editor** — *deferred: needs Workforce Design sub-panel implementation (Phase 6 TUI node list)*.

### Phase 4: Multi-Provider & Resource Management (1.5.0)
_Expand beyond Claude+Gemini. Add API usage intelligence._

22. [ ] **Provider abstraction layer** — unified `Provider` trait with `cli_pty` and `api_http` variants. CLI providers use existing PTY spawn. API providers use `reqwest` with streaming response parsing.
23. [ ] **Ollama backend via Crush/OpenCode** — integrate Crush (primary) or OpenCode (fallback) as `BackendKind::Crush`, same PTY model. If neither works, build native agentic modules wrapping Ollama's API.
24. [ ] **Raw API backends** — Anthropic Messages API, OpenAI Chat Completions API. Direct HTTP, no CLI dependency.
25. [ ] **Intelligence Resources Manager** — background task tracking per-provider: requests/minute, tokens/minute, remaining quota (where APIs report it). Stores in `~/.config/orrchestrator/usage.jsonl`.
26. [ ] **Dynamic throttling** — when a provider approaches rate limits, the IRM pauses workforce queues using that provider. Shifts work to alternative providers when possible. Resumes automatically.
27. [ ] **Token optimization pipeline** — three layers: (a) COO one-time feedback compression during intake, (b) COO manages `instructions_inbox.md` lifecycle (trim on version publish, truncate long files), (c) hypervisor trims verbose reasoning from inter-agent handoffs, keeps actionable conclusions only.

### Phase 5: Library (1.6.0)
_Centralized database of reusable AI workflow components._

28. [ ] **Library storage backend** — git-backed GitHub repository. Structure: `{agents,skills,tools,mcp_servers,workforce_templates}/`. Each item is a `.md` file with YAML frontmatter + content. Synced across machines via git.
29. [x] **Library panel UI** — 4 sub-panels (Agents/Models/Harnesses/MCP) with Shift+Tab navigation. Split-pane layout (40/60 list+preview). Models show tier badge (color-coded enterprise/mid-tier/local), pricing, context size, capabilities, limitations. Harnesses show availability status (auto-detected via `which`), supported models, flags. 8 model definitions + 5 harness definitions seeded. Enter opens items in vim for editing.
30. [ ] **Library MCP server** — orrchestrator hosts an MCP server that exposes library items as tools/resources. Managed sessions can query the library for available tools.
31. [ ] **AI-assisted creation** — "New skill/tool/agent" action spawns a Claude session that helps the user define the item interactively. Result is saved to the library.
32. [ ] **Auto-assignment via Mentor** — when an agent is bound to a session, the Mentor reviews its profile against Library contents and injects relevant tool/skill references into the agent's prompt.

### Phase 6: Node-Based Workforce Designer (1.7.0)
_Visual agent-as-node workflow editor._

33. [x] **Workforce definition format** — structured markdown with pipe-delimited step tables. Parser in orrch-workforce handles trigger/blocker/steps/interrupts. Auto-detects parallel groups from duplicate step indices.
34. [x] **TUI node list view** — Workforce Design sub-panel in Design tab shows workforce templates and operation modules as navigable lists. Enter opens in vim for editing. Shift+Tab toggles between Project Design and Workforce Design.
35. [ ] **Nested workforces** — a workforce node can contain another workforce. The inner workforce runs as a unit, reporting only its designated output agent's results to the parent.
36. [ ] **Workforce import/export** — save/load workforce designs as files. Share between orrchestrator instances.

### Phase 7: Visual Editors & Native Window Mode (1.8.0)
_Web-based visual editing, then native window integration._

37. [ ] **Web-based node editor** — local HTTP server serving a drag-and-drop node graph UI (JS canvas library). TUI opens browser. Reads/writes structured markdown workforce files. TUI continues running alongside.
38. [ ] **Native egui window** — orrchestrator TUI can spawn a native egui window for the node editor. Fully integrated, no browser dependency.
39. [ ] **Non-TUI mode** — orrchestrator can optionally launch entirely as a windowed application for terminal-averse users.

### Phase 8: Intake Workforces (2.0.0-rc)
_The predefined workforces that process different types of user input. Completing this phase means the full agent orchestration pipeline is operational — tag `2.0.0` when stable._

40. [ ] **workforce:instruction-intake** — processes project dev instructions. COO optimizes → routes to project queues → PM incorporates into plan.
41. [ ] **workforce:plan-intake** — processes new project designs/plans. Evaluates scope, may create new projects, may trigger versioning. Distributes instructions.
42. [ ] **workforce:idea-intake** — processes incomplete ideas. Stores in Ideas vault as unattached. Tags with domain, potential project associations.
43. [ ] **workforce:knowledge-intake** — processes custom agents, skills, tools. Validates, formats, saves to Library. Updates relevant agent profiles.

### Cross-Cutting: Interactive Dev Map (Oversee panel)
_Feature tracking interface in the project focus view. Plan.md becomes a live, interactive development map._

44. [ ] **Plan.md syntax parser for dev map** — parse Plan.md into a structured feature tree: phases → features → status (planned/in-progress/tested/verified/removed). Display as interactive list in Oversee project detail view.
45. [ ] **Feature state machine** — each feature tracks: planned → implementing → implemented → testing → verified → user-confirmed. Visual indicators for each state. Removed features show as strikethrough with removal context (removed-before-impl vs removed-after-impl vs failing-verification).
46. [ ] **Reorder features** — user can move features up/down within a phase, or between phases. Changes persist back to Plan.md.
47. [ ] **Add feature popup** — syntax-controlling text input that guides the user to write a feature request matching COO optimization format. Appended to Plan.md.
48. [ ] **Quick-spawn for feature** — select a feature on the map, press Enter to spawn a dedicated session targeting that feature specifically with priority.
49. [ ] **Diff log persistence** — all changes to the dev map are tracked. Features display visual badges: modified since last verification, newly added, reordered, removed.
50. [ ] **User verification tracking** — user marks features as manually tested/confirmed. Confirmation persists until any code change affects that feature. Previously confirmed features are re-flagged when implementation changes. Release readiness view shows unconfirmed features.
51. [ ] **Direct PM interaction** — open a live session with the Project Manager agent for natural-language plan changes. PM updates Plan.md, dev map reflects changes in real time.
52. [ ] **Git commit grouping display** — show how the workforce intends to package commits by phase. Repository Manager reviews and advises on the PM's chosen feature grouping for optimal git workflow.

### Cross-Cutting: AI Tooling Management (Library)
_Model hierarchy, harness management, and cost-optimized workforce assignment._

53. [x] **Model registry in Library** — ModelEntry struct with tier, pricing, capabilities, limitations, max_context, API key env. 8 models seeded. Loaded from `library/models/*.md`. Displayed in Library > Models sub-panel with tier color badges and preview pane.
54. [ ] **Three-tier workforce profiles** — enterprise (Claude, GPT-4o — existing workflows), mid-tier (Mistral Large API — more structured instructions), local/free (Ollama Mistral, Gemini free — rigid logic, scope-limited tasks). Each tier has its own workforce micromanagement template adjusting instruction directness and scope boundaries.
55. [x] **Resource Optimizer agent** — profile created (`agents/resource_optimizer.md`), AgentRole added, DEVELOP FEATURE module updated with step 2 (optimize) and step 7 (commit review). Prompt includes model tier guidelines and harness awareness.
56. [x] **Harness registry in Library** — HarnessEntry struct with auto-detection (`which`), supported models, flags, capabilities. 5 harnesses seeded. Displayed in Library > Harnesses sub-panel with availability status badge.
57. [ ] **Mixed-model workflow support** — a single workforce execution can use different models for different steps. The hypervisor passes the model/harness assignment from the Resource Optimizer to each subagent spawn.
58. [ ] **Mentor resource updates** — Mentor periodically dispatches Researcher to investigate changes to available models, harnesses, and tools. Researcher reports back, Mentor updates Library entries. PM uses these updated entries for optimization decisions.

### Cross-Cutting: API Valves & MCP Management

59. [x] **API valves (manual provider shutoff)** — per-provider on/off toggle in Library > Models. `v` = instant toggle, `V` = timed close (24h default). ValveStore persisted to `~/.config/orrchestrator/valves.json`. Visual `BLOCKED` badges on affected models. Auto-reopen ticker with countdown display.
60. [x] **MCP server config management** — McpServerEntry struct with stdio/sse transport, enable/disable toggle (`e` key), role assignment. Loaded from `library/mcp_servers/*.md`. Displayed in Library > MCP sub-panel.
61. [ ] **orrch-mcp server** — unified MCP server exposing orrchestrator internals: library_search, library_get, project_state, inbox_append, operation_status, session_list. Single server, connected to all agent sessions.
62. [ ] **External MCP server management** — configure connections to user's existing MCP servers (github, context7, etc.) through the Library > MCP panel. Assign servers to agent roles.
63. [ ] **Syntax translation engine** — research session to catalog prompt/tool-call syntax differences across models and harnesses. Generate translated versions of context files (agent profiles, CLAUDE.md equivalents) per model/harness combination. Stored in Library.
64. [ ] **Valve integration with Resource Optimizer** — Resource Optimizer checks valve state before recommending a model. Blocked providers are excluded from optimization suggestions. IRM auto-closes valves when rate limits are detected.

### Carried Forward (from 1.0.0 queued items)
- [ ] **Agent profile management** — swappable CLAUDE.md/GEMINI.md profiles per project
- [ ] **Instruction inbox migration** — replace `fb2p.md` with per-project `instructions_inbox.md` managed by COO. Deprecate `/interpret-user-instructions` skill. Intake handled by `workforce:instruction-intake` module (EA → COO → PM). COO trims on version publish, truncates long files.

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
- 2026-03-31: **FRESH PLAN.md written.** Processed redesign_plan into 8-phase roadmap toward 2.0.0 (43 items + 2 carried forward). All 8 design decisions resolved. Architecture expanded: 3 new crates, 4-panel layout, 19 agent roles, workforce templates, operation modules, multi-provider AI, node-based designer. Hypervisor agent profile created + agent profile system implemented (new spawn wizard step). fb2p.md model deprecated in favor of per-project `instructions_inbox.md` managed by COO via intake workforces. Ollama via Crush/OpenCode, library as git-backed GitHub repo + MCP server, workforce format is structured markdown.
