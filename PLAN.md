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
| Within a workflow (tight coupling) | ~~Prompt injection via subagents~~ **Skill-based Agent tool calls** (updated 2026-04-03) | PM ↔ Software Engineer ↔ Developer |

The file inbox separation enables throttling: COO can keep appending while PM is rate-limited. ~~The prompt injection path is for agents that are part of the same execution flow.~~ **Update (2026-04-03):** Within-workflow communication now uses Agent tool calls from self-managing workflow skills, not prompt injection into a single session. See CRITICAL PATH section.

### Q3 — Workforce Step Execution ~~(blocks Phase 2)~~ RESOLVED
**Decision:** Follows from Q2's subagent model.

- **Parallel branches** — the hypervisor agent spawns multiple subagents concurrently (e.g., Developer + Researcher + Feature Tester all as parallel Agent calls at the same step index).
- **Blocker polling** — checked by the hypervisor between steps, not mid-step. A running subagent finishes its current work before the workflow pauses.
- **Data handoff within workflow** — ~~prompt injection between subagents (the hypervisor captures output and injects it into the next subagent's prompt)~~ **Agent tool call return values** (updated 2026-04-03). The workflow skill captures each subagent's output and passes it to the next subagent's invocation context.
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

### CRITICAL PATH — Skill-Based Workflow Execution (blocks all orchestration)

> **Why this section exists:** Two foundational discoveries drove this section to the top of the roadmap.
>
> **Discovery 1 (2026-04-03):** A session spawned by orrchestrator to implement bug fixes operated as a solo developer — no architecture review, no testing agents, no PM synthesis loop. The DEVELOP FEATURE workflow was never followed because the spawn flow did not inject workforce context into sessions.
>
> **Discovery 2 (2026-04-03):** Two live tests of the prompt injection approach (CP-1/CP-2/CP-3) confirmed that even with 15k tokens of workforce instructions injected into a session prompt, sessions ignored the orchestration architecture and operated as solo developers. Prompt injection is fundamentally the wrong mechanism — LLM sessions treat injected workforce definitions as informational context, not as executable procedure. The entire `build_workforce_context()` / SpawnWorkforce prompt injection approach is scrapped.
>
> **New architecture:** Skill-based workflow execution. Workflow definitions become executable skills (`.md` prompt files invoked via Claude Code's `/skill` or Agent tool mechanism). The `/develop-feature` skill IS the Hypervisor — it procedurally spawns agents via Agent tool calls, pipes results between steps, enforces context isolation, and manages the dev loop. Orrchestrator provides visibility (live agent tree in Hypervise) and intervention controls (pause, redirect, inject). Sessions start with a skill invocation, not a massive prompt blob.

These items replace the former CP-1/CP-2/CP-3 (prompt injection approach, now deprecated). They are the highest priority because they gate the value of everything else already built.

**Deprecated:** `build_workforce_context()`, the SpawnWorkforce wizard step that injected prompt blobs, and the entire concept of constructing composite prompts from Hypervisor profile + workforce template + operation steps. These are replaced by skill invocation on session spawn.

CP-1. [x] **Workflow skills (`.md` prompt files)** — Convert workflow definitions (DEVELOP FEATURE, INSTRUCTION INTAKE, etc.) into self-managing skill files. Implemented: `develop-feature.md` (10-step pipeline) and `instruction-intake.md` (7-step pipeline with user audit) created as Claude Code commands.

CP-2. [x] **Agent role skills** — Convert agent profiles into invocable skills (`/agent:pm`, `/agent:developer`, `/agent:coo`, etc.). Each agent skill loads the agent's `.md` profile as its system context. Workflow skills spawn agent role skills as subagents. This replaces the `AgentRunner::build_prompt()` approach of constructing composite prompts. **Status: DONE — 13 agent skill files created (pm, developer, engineer, coo, tester, repo, executive-assistant, resource-optimizer, feature-tester, penetration-tester, beta-tester, ui-designer, researcher). 8 non-workflow agents deferred.**

CP-3. [x] **Tool scripts for deterministic operations** — Implemented: `route_instructions.sh`, `workflow_status.sh`, `intake_review.sh`, `session_log.sh` in `library/tools/`. All defensive bash with `set -euo pipefail`.

CP-4. [x] **Skill invocation on session spawn** — Replace the prompt injection spawn flow. When a session is spawned from orrchestrator, it starts with a skill invocation (e.g., `/develop-feature <goal>`) instead of a raw goal string or a massive composite prompt. The spawn wizard selects the skill; orrchestrator passes the invocation command as the session's initial input. Solo-developer mode (current behavior) remains as the fallback when no skill is selected. **Status: DONE — spawn flow now passes `/develop-feature <goal>` as initial command when workforce is selected.**

CP-5. [x] **Hypervise live agent tree** — When a workflow skill executes, the Hypervise panel displays a real-time nested agent tree: which agents are active, what step the pipeline is on, agent status (running/waiting/complete/failed), and truncated output. The tree updates as the skill spawns and completes subagents. The user can pause, intervene (inject a message), or redirect from the TUI. This is the visibility layer — orrchestrator does not control execution, but it observes and can interrupt. **Status: DONE — TUI polls `.orrch/workflow.json` from active sessions, renders agent tree with step progress and color-coded statuses. Visibility for all workflow states (running/paused/failed/complete). Pause/intervene/redirect deferred.**

CP-6. [x] **Instruction intake audit UI** — Add a user review step to the INSTRUCTION INTAKE pipeline: EA separates → COO optimizes → **user reviews (raw vs optimized side-by-side, editable)** → COO splits and routes → PM incorporates. The audit UI lives in Design > Intentions. When the COO finishes optimization, the result appears in a review queue. The user sees their original text alongside the optimized version, can edit the optimization, then confirms distribution. Instructions do not route to project inboxes until the user confirms. **Status: DONE — TUI polls `.orrch/intake_review.json` from project dirs when viewing Intentions. Auto-populates review overlay (side-by-side raw vs optimized). Confirm/reject writes decision back to filesystem.**

CP-7. [x] **Unified orrch-mcp server** *(was items 30, 61)* — Implemented: `crates/orrch-mcp/` standalone binary, 10 MCP tools over stdio JSON-RPC, 19 tests. Registered at user scope via `claude mcp add -s user`. 1.3MB release binary.

---

### UI Polish & Infrastructure (from Instruction Inbox)

_Queued work from `instructions_inbox.md` (INS-001 through INS-009). Formally incorporated 2026-04-03._

65. [x] **INS-001: Responsive tab bar width** — Panel tabs now use fixed-width cells with collapsing labels (full → short → tiny). Dividers override text. Tab bar never extends beyond terminal width.
66. [x] **INS-002: Fix Workforce sub-tab navigation** — Replaced `tab_focused: bool` with `focus_depth: usize` multi-level vertical navigation system. Each bar level is independently navigable with Left/Right, Up/Down moves between levels.
67. [x] **INS-003: Left-justify Library sub-panel** — Library tab is now left-justified next to Workforce in the Design sub-bar. Right-justification logic removed.
68. [x] **INS-004: Add Harnesses editor as leftmost Workforce tab** — Add a "Harnesses" tab as the first tab in Design > Workforce (before Workflows). Placeholder page listing harness source directories and repo links. Long-term: visual harness aggregator indexing features across open-source harnesses (Claude Code, OpenCode, Crush, Codex, Gemini) for a custom fork. *(Note: distinct from item 56 — that is the read-only Library > Harnesses browser; this is an editable Workforce tab for harness feature cataloguing.)*
69. [x] **INS-005: Rich markdown preview renderer** — TUI rendering layer for `.md` files with rich formatting (headers, bold, lists, code blocks, links). Replaces plaintext preview in Library, Workforce, and Intentions panels. Must be scrollable and embeddable as a widget. Research: yazi image protocol, mdcat, glow, bat.
70. [x] **INS-006: Fix orphaned tmux sessions on exit** — On quit, enumerate all orrchestrator-managed tmux windows and kill them. On startup, detect orphaned sessions from a previous run and offer cleanup. Store managed session names in a state file for cross-run tracking.
71. [x] **INS-007: Custom tmux status bar for managed sessions** — Custom tmux status bar for the "orrch" session: window name, busy/waiting/idle status with color coding, sorted by urgency (waiting first). Custom hotkey to jump to most urgent window.
72. [x] **INS-008: Unified vim/nvim tmux window** — All vim/nvim editing sessions open in a single tmux window with the custom status bar. Users can split off individual vim sessions; orrchestrator tracks the change and represents lone windows in the Intentions menu. The tmux+vim window should feel like a native orrchestrator editing interface.
73. [x] **INS-009: Instruction audit trail with hash coordinates** — When COO splits user feedback into discrete instructions, each instruction is indexed with a hash derived from coordinate data (line range, character offsets) of the source text chunk. Bidirectional audit trail: trace any instruction back to the exact source text and see how it was interpreted/optimized. Audit log stored in `.feedback/audit.jsonl`. Translation mapping displayed in Intentions panel when an idea is expanded. *(Note: complements CP-6 audit UI — CP-6 is the user review step; this is the coordinate-level tracing and persistence layer.)*

---

### UI Polish & Infrastructure (from Instruction Inbox — 2026-04-23)

_Queued work from `instructions_inbox.md` (INS-001 through INS-007 in the 2026-04-23 batch). Source idea file: `plans/2026-04-23-20-34.md`. Formally incorporated 2026-04-09._

**Open Conflicts / Cross-References:**
- **INS-005 ↔ item 65 (2026-04-03 INS-001):** Item 65 implemented responsive tab-bar width with collapsing labels. INS-005 adds right-pane focusability and auto-scroll wrappers at the panel level. No structural conflict — INS-005 operates on the panel body, item 65 on the tab bar. INS-005 must not regress the tab-bar collapsing behavior when refactoring panels to the new wrapper.
- **INS-001 → INS-005 (hard dependency):** The new Design > Plans panel needs the automatic scroll architecture from INS-005 to render arbitrarily long PLAN.md trees without clipping. INS-005 should land first (or in the same PR).
- **INS-007 ↔ items 21 / 33 / 34 / 37:** Item 33 defines the on-disk pipe-delimited workforce format, item 34 is the TUI node list view, item 37 is the web node editor, item 21 is the operations module editor. INS-007 adds a NEW edit mode (TUI or vim) that expands the compressed table into a human-readable form and recompresses on save. It is adjacent to, not in conflict with, items 33/34/37 — the web editor edits a graph, INS-007 edits the expanded markdown. Parser/formatter work lives in `orrch-workforce` and should be reused by both paths. Cross-reference rather than conflict.
- **INS-002 / INS-003 / INS-004:** No conflicts with existing roadmap. INS-002 and INS-003 are bugfixes to the Workforce sub-tabs surfaced after item 60 (MCP panel) and whatever shipped the Skills tab. INS-004 generalizes that bugfix into an acceptance test for every sub-tab.

74. [x] **INS-001: Add Design > Plans panel with interactive plan management** — New Design sub-tab labeled "Plans" alongside Intentions, Workforce, Library. Enumerates projects with `PLAN.md` files (reuse Oversee project discovery), parses `[ ]` / `[x]` items and section headers, renders as expandable tree/table with per-feature actions: mark as delivery milestone, defer/delay, prioritize up/down, edit in place. Changes persist back to `PLAN.md` preserving markdown structure and trigger the PM agent (reuse item 51 pm-plan-edit skill) to reconcile. Maximizes user visibility into dev plan state across all projects. **Depends on INS-005** for the scroll infrastructure — PLAN.md trees are arbitrarily tall and must auto-scroll in the panel body. (source: plans/2026-04-23-20-34.md)

75. [x] **INS-002: Fix skill name display in Workforce > Skills tab** — Skills tab currently displays the first line of each skill `.md` file as the name, which is wrong because skill files do not start with a name line. Fix the skill loader in `orrch-library` (or wherever skills are parsed) to derive the display name in precedence order: (a) YAML frontmatter `name:` field, (b) filename stem, (c) first `# Heading` in body. Update existing `library/skills/*.md` files that lack a name source. Acceptance: every skill in the Skills tab shows a name that corresponds to its actual content. (source: plans/2026-04-23-20-34.md)

76. [x] **INS-003: Wire Workforce > MCP tab to live MCP server list** — Design > Workforce > MCP tab currently shows only "github" hardcoded. Replace with a live enumeration of all MCP servers available to orrchestrator. Data sources to scan: `~/.claude.json`, `~/.config/claude/mcp.json`, project-local `.mcp.json`, and orrchestrator-managed configs under `library/mcp/`. Display per server: name, connection status (connected/disconnected), transport type (stdio/sse), and tool count. Refresh on panel focus. (source: plans/2026-04-23-20-34.md)

77. [x] **INS-004: Make Library and Workforce panels dynamic file-system-backed views** — Audit every sub-tab in Design > Library and Design > Workforce (Workflows, Teams, Agents, Skills, Tools, MCP, Profiles, Training, Models, plus Library sub-tabs). Confirm each panel reads its source directory at runtime and reflects actual files present, not a hardcoded or stale in-memory list. Rewire any panel that is not live-backed to scan its source directory on open/refresh. Add a refresh keybind (`r`) to each panel to force re-scan. Acceptance: adding/removing a file in `library/skills/`, `workforces/`, `operations/`, `library/models/`, etc., is reflected in the UI without rebuild. (source: plans/2026-04-23-20-34.md)

78. [x] **INS-005: Implement automatic scroll architecture for all panels** — Foundational fix: the TUI panel framework must automatically enable scrolling whenever rendered content exceeds viewport height. NOT an opt-in per-panel fix — implement as a trait/wrapper in `orrch-tui` that every panel uses. Requirements: detect content overflow at render time, auto-attach a vertical scroll state to overflowing panels, allow focus to move to right-half panels (fix panel focus navigation so the right pane is selectable), standard keybinds (`Up`/`Down`, `PgUp`/`PgDn`, `Home`/`End`, mouse wheel), scroll indicator (`▲▼` or position bar) when content overflows. Refactor existing panels onto the new wrapper and remove ad-hoc scroll logic. Acceptance: no panel displays clipped content without a working scroll path; right-half panels are focusable and scrollable. **Blocks INS-001.** (source: plans/2026-04-23-20-34.md)

79. [x] **INS-006: Audit and remove dead keybind hints** — Audit all panels for displayed shortcut hints that do not actually trigger the advertised action. Reported case: `Shift+Tab` in Design > Workforce > MCP does nothing. For each dead hint: either wire up the keybind to do what the hint claims, or remove the hint from the footer/help line. Add a sanity test (unit or integration) that every footer-advertised keybind resolves to a registered handler in the panel's key handler. (source: plans/2026-04-23-20-34.md)

80. [x] **INS-007: Human-readable workflow edit mode with roundtrip compression** — In Design > Workforce > Workflows, when the user enters edit mode on a workflow, expand the compressed pipe-delimited step table into a human-readable markdown form (e.g., numbered step list with labeled fields: agent, input, output, parallel group). On save/exit, re-compress back into the canonical machine-readable pipe-delimited format used by `orrch-workforce` parser. On-disk format stays compressed; only the edit buffer expands. Requirements: lossless roundtrip (compressed → expanded → compressed produces byte-identical output for unchanged content); expansion/compression handled by `orrch-workforce` parser/formatter; edit mode launches vim or in-TUI editor with the expanded form; validate re-compression before writing to disk; show errors if malformed. *(Cross-reference: items 33, 34, 37 — this is a new edit mode, not a replacement for the web node editor.)* (source: plans/2026-04-23-20-34.md)

---

### UI/UX Optimization Sprint (from Instruction Inbox — 2026-04-14)

_OPT and TOK items queued from inbox. Source: plans/2026-04-24-09-03.md and subsequent feedback sessions._

81. [ ] **OPT-001: Focus new ideation editor window on creation** — When a new file is created from Design > Intentions, the spawned editor window must receive desktop focus immediately. Audit `spawn_vim_window` in `editor.rs` — ensure terminal emulator command includes focus-stealing flags (`wmctrl -a` post-spawn, or emulator-specific `--focus` flags). Test on orrion (CachyOS/KWin).

82. [x] **OPT-002: Remove dev map from project detail, show only roadmap** — Project detail pages render both a "dev map" and a "feature roadmap." Remove the dev map display. Show only the roadmap. Audit all project detail rendering paths in `ui.rs` to confirm a single roadmap view.

83. [x] **OPT-003: Fix roadmap scrollability in project detail** — Roadmap section in project detail does not scroll when content overflows. Wrap the roadmap widget in the automatic scroll infrastructure. Audit all panels/widgets for others that also missed the scroll wrapper.

84. [x] **OPT-004: Fix navigation traps across all pages** — Project menu (and potentially other pages) traps the user — Up/Back/Esc does not exit. Audit every page/panel/sub-view for navigation traps. Every view must be exitable via Left-arrow, Esc, or Up (vertical focus navigation model). Fix all instances.

85. [ ] **OPT-005: Fix right-arrow project navigation to show project details** — Right-arrow on a project in Oversee should drill into project detail view. Audit `key_oversee` handler.

86. [ ] **OPT-006: Track all projects including newly created ones** — Projects without tracked tasks (e.g., borrk) don't show dev completion in Oversee. Fix: project discovery must include all projects under `~/projects/` with any trackable state (PLAN.md, active sessions, recent git activity). Projects with no PLAN.md show "no plan" indicator.

87. [ ] **OPT-007: Dynamic tip line based on project completion state** — When a project has 100% roadmap complete, tip/action line displays "submit feedback" and "construct packages" instead of default actions. Conditional in smart default actions logic (item 27): `project.roadmap_complete() -> bool` overrides tip text.

88. [x] **OPT-008: Write platform porting workflow** — Create `operations/platform_port.md` — multi-step workflow for porting to any target platform (PyPI, crates.io, npm, Docker Hub, Flathub, AUR, Homebrew, apt/deb, etc.). Pipe-delimited step table format. Agents: Researcher, Developer, Repo Manager, Licensing Auditor, Feature Tester, Beta Tester.

89. [ ] **OPT-009: Self-extending agent library via MCP** — Add MCP tools: `create_agent`, `create_skill`, `create_tool`, `create_workflow` for runtime library extension. Behavioral contract: agents encountering problems outside expertise ask for an expert; if none exists, create the expert agent + skills via MCP, save to library, invoke.

90. [ ] **OPT-010: Session lifecycle management** — Unified system: (a) hot/cold project tracking in Oversee; (b) stale session cleanup; (c) session status indicators; (d) session close protocol; (e) session brief navigation in project details TUI.

91. [x] **OPT-011: Use nvim instead of vim** — Audit all editor invocations. Replace all `vim` with `nvim`. Window titles and docs must say `nvim`.

92. [ ] **OPT-012: Tmux session management overhaul** — Tab naming: `<project>:<short-goal>`. Window splitting support. Custom tmux config shipped at `config/tmux.conf`. Do NOT use system tmux config.

93. [ ] **OPT-013: Project classification and lifecycle tools** — Tools to classify projects by lifecycle stage (active, maintenance, archived, deprecated) and surface appropriate actions per stage.

94. [ ] **OPT-014: In-TUI file and entity renaming** — Rename action (`r` or `F2`) on any nameable entity in any panel: files, projects, intentions, plans, agents, skills, tools, workflows.

95. [ ] **OPT-015: Publish page plan** — Detailed plan for Phase 9 (Publish panel). Covers: release packaging, version tagging, changelog, platform distribution, license audit, copyright verification, release notes, marketing material, pre-release checklist, post-release monitoring, rollback.

96. [ ] **OPT-016: Expand the Analyze page with comprehensive metrics** — Token usage per project/session/step/agent, cost breakdown, session throughput, agent performance, error frequency, workflow efficiency, project velocity, historical trends.

97. [ ] **OPT-017: Hypervise panel feature expansion** — Session list: rename inline, expand for 2 most recent messages, show host + cwd. Session detail: live message stream, prompt input, workflow dashboard, interrupt/pause.

98. [ ] **Publish panel skeleton** — Add `Publish` as a top-level panel. Sub-tabs: Packaging, Distribution, Compliance, Marketing, History. Placeholder rendering for each tab.

99. [ ] **Release packaging engine** — Build artifacts for the selected project: cargo build, pip wheel, npm pack, Docker image, platform-specific archives. Configurable per-project build matrix in `.orrch/publish.toml`.

100. [ ] **Version tagging and changelog generation** — Integrate with `/release` flow: bump version (SemVer), generate CHANGELOG.md entries from conventional commits since last tag, create annotated git tag. Preview changelog in TUI before confirming.

101. [ ] **Platform distribution** — Publish to target platforms: crates.io, PyPI, npm, Docker Hub, GitHub Releases, AUR, Homebrew, Flathub. Distribution tab shows per-platform publish status.

102. [ ] **License compliance audit** — Invoke Licensing Auditor agent. Report: dependency tree with license per dep, flagged conflicts, missing licenses, SPDX identifiers. Display in Compliance tab. Block publish if critical conflicts detected.

103. [ ] **Copyright verification** — Invoke Copyright Investigator agent. Verify source file headers, attribution, trademark conflicts. Report in Compliance tab.

104. [ ] **Release notes generation** — Auto-draft release notes from conventional commits. Group by type. Editable in nvim before publishing.

105. [ ] **Marketing material generation** — Invoke Market Researcher + UX Specialist agents. Generate: project description, feature highlights, README badges, social media announcement draft. Display in Marketing tab.

106. [ ] **Pre-release checklist enforcement** — Automated checklist before publish: all tests pass, no open blockers, user verification complete, CHANGELOG up to date, license file present, no secrets. Block publish until critical items pass.

107. [ ] **Post-release monitoring** — Track download counts, new issue reports, GitHub release engagement. Display in History tab. Alert on post-release issue spike.

108. [ ] **Rollback capability** — Yank/unpublish from platforms that support it. Revert git tag. Generate rollback advisory. Confirmation required.

109. [ ] **TOK-001: Token delivery audit and controlled injection system** — Systematic audit of all context delivery mechanisms for token waste. Design a controlled context injection layer. Add token budget tracker to Analyze panel. Document the controlled delivery protocol.

110. [ ] **TOK-002: Per-project concurrent session limits with warning gate** — Configurable max concurrent session count per project (default: 3). Warning modal on limit exceeded. `N sessions / max` counter in Oversee.

111. [ ] **TOK-003: Multi-device session classification (primary vs compatibility)** — Extend session metadata with device role: primary (orrion) vs compatibility (orrpheus, etc.). TOK-002 limits apply separately per class.

---

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
11. [x] **COO instruction optimizer** — **Status: DONE (superseded by CP-1 + CP-6)** — the prompt injection approach was scrapped; the Instruction Intake skill (`library/skills/instruction-intake.md`) handles COO optimization, and CP-6 ships the side-by-side audit UI (`crates/orrch-tui/src/intake_review`).
12. [x] **Mentor agent integration** — **Status: DONE (superseded by item 32)** — Library storage (item 28) and `mentor_review_profile` + `as_preamble_with_library` in `crates/orrch-agents/src/runner.rs` are wired. Mentor profile lives at `agents/mentor.md`. Periodic background scheduler is the task-58 follow-up, not blocking.

### Phase 2: Workforce Templates (1.3.0)
_Agents are organized into teams with defined operation flows._

13. [x] **Workforce template data model** — Workforce, AgentNode, Connection, DataFlow structs in orrch-workforce crate. Supports agent teams with directed connections and operation references.
14. [x] **Built-in workforce templates** — 3 created as structured markdown in `workforces/`: Personal Tech Support, General Software Development, Commercial Software Development.
15. [x] **Template selector in spawn flow** — **Status: DONE (superseded by CP-4)** — the prompt injection approach is scrapped; spawn flow now passes `/develop-feature <goal>` as initial command when a workforce is selected.
16. [x] **Workforce-aware session management** — **Status: DONE (superseded by CP-4 + CP-5)** — skill invocation on session spawn replaces the prompt injection model, and the Hypervise live agent tree provides workforce-aware monitoring via `.orrch/workflow.json`.

### Phase 3: Operation Modules (1.4.0)
_Workforces execute structured pipelines with triggers, blockers, and interrupts._

17. [x] **Operation module engine** — OperationExecution runtime: tracks state (Idle/Blocked/Running/Complete/Interrupted), next_steps() returns parallel batches, advance() moves pointer, progress_display() for UI. load_operations() reads .md files.
18. [x] **INSTRUCTION INTAKE module** — `operations/instruction_intake.md`. EA → COO (clarify, parse) → PM (synthesize). Parser auto-detects parallel group at step 1 (EA handles two tasks concurrently).
19. [x] **DEVELOP FEATURE module** — `operations/develop_feature.md`. 9-step pipeline with parallel group at step 3 (Dev+Researcher+Engineer+UIDesigner+FeatureTester) and step 4 (PenTester+BetaTester).
20. [x] **Module status display** — **Status: DONE (superseded by CP-5)** — `OperationExecution::progress_display()` exists in `crates/orrch-workforce/src/engine.rs`, and the Hypervise panel polls `.orrch/workflow.json` via `orrch_core::load_workflow_status` and renders the live agent tree (step counter, color-coded statuses, agent roster) in `crates/orrch-tui/src/ui.rs:1849+`.
21. [x] **Module editor** — **Status: DONE (superseded by items 34 + 37)** — Design > Workforce > Teams tab in `crates/orrch-tui/src/app.rs` lists `operations/*.md` and supports `n` (new from template), `Enter` (edit in vim via `vim_request`), `d` (delete). The web node editor (item 37) and standalone egui scaffold (item 38) cover the visual-edit slice.

### Phase 4: Multi-Provider & Resource Management (1.5.0)
_Expand beyond Claude+Gemini. Add API usage intelligence._

22. [x] **Provider abstraction layer** — unified `Provider` trait with `cli_pty` and `api_http` variants. CLI providers use existing PTY spawn. API providers use `reqwest` with streaming response parsing.
23. [x] **Ollama backend via Crush/OpenCode** — Crush and OpenCode are wired into `BackendKind` and `BackendsConfig::default()`, auto-detected via `which`, route through the same PTY spawn path as Claude/Gemini in `process_manager`. Test coverage via `test_crush_*` in `backend.rs`.
24. [x] **Raw API backends** — `AnthropicApi` and `OpenAiApi` dispatch via `backend::send_api_message` (reqwest blocking, rustls-tls). `process_manager::spawn_api_oneshot` runs the HTTP call on `tokio::task::spawn_blocking` and emits the response as `SessionEvent::Output` + `Died`. Availability gated by `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`. **Known follow-ups:** model_id is hardcoded in `to_provider()` (needs Library wiring); API sessions are not tracked in `ProcessManager.sessions` (one-shot only — needs persistent session model for multi-turn).
25. [x] **Intelligence Resources Manager** — background task tracking per-provider: requests/minute, tokens/minute, remaining quota (where APIs report it). Stores in `~/.config/orrchestrator/usage.jsonl`. **Status: DONE — UsageTracker with append-only JSONL persistence, SessionStart/SessionEnd recording on spawn/death, Analyze panel replaced with per-provider usage summary table.**
26. [x] **Dynamic throttling** — when a provider approaches rate limits, the IRM pauses workforce queues using that provider. Shifts work to alternative providers when possible. Resumes automatically.
27. [x] **Token optimization pipeline** — three layers: (a) COO one-time feedback compression during intake, (b) COO manages `instructions_inbox.md` lifecycle (trim on version publish, truncate long files), (c) hypervisor trims verbose reasoning from inter-agent handoffs, keeps actionable conclusions only. **Layer (a) DONE:** `feedback::append_to_inbox` auto-truncates on write via `INBOX_WRITE_SOFT_CAP` (64 KiB); `intake_review::distribute_to_inbox_from_intake` helper provides explicit intake path. **Layer (b) DONE:** primitives + walker (`feedback::truncate_inbox_if_large`, `trim_completed_entries`, `maintain_all_project_inboxes`); periodic tick runs every 60s on `spawn_blocking` from `run_loop()` in `src/src/main.rs`, bounded by `App.last_inbox_maintenance`. **Layer (c) DONE:** `runner::compress_handoff` in `crates/orrch-agents/src/runner.rs`, wired into `build_handoff_prompt`.

### Phase 5: Library (1.6.0)
_Centralized database of reusable AI workflow components._

28. [x] **Library storage backend** — git-backed GitHub repository. Structure: `{agents,skills,tools,mcp_servers,workforce_templates}/`. Each item is a `.md` file with YAML frontmatter + content. Synced across machines via git. **Status: DONE** — `crates/orrch-library/src/sync.rs` with `clone_if_missing`, `sync_pull`, `sync_push` (git shell-out). `Config::library_repo_url: Option<String>` in `crates/orrch-core/src/config.rs`. TUI surface: `key_library()` handles `P` (sync_pull) and `U` (sync_push) with not-a-repo fallback; `App::library_clone_if_missing()` called from `main()` before `run_loop`. Unblocks items 31, 32, 58.
29. [x] **Library panel UI** — 4 sub-panels (Agents/Models/Harnesses/MCP) with Shift+Tab navigation. Split-pane layout (40/60 list+preview). Models show tier badge (color-coded enterprise/mid-tier/local), pricing, context size, capabilities, limitations. Harnesses show availability status (auto-detected via `which`), supported models, flags. 8 model definitions + 5 harness definitions seeded. Enter opens items in vim for editing.
30. [x] **Library MCP server** — MOVED to Critical Path (CP-7). Done: orrch-mcp crate, 12 MCP tools, stdio JSON-RPC.
31. [x] **AI-assisted creation** — "New skill/tool/agent" action spawns a Claude session that helps the user define the item interactively. Result is saved to the library. **Status: DONE (scaffold)** — Shift+N in Design > Workforce panel spawns the creation flow; header hint wired in `ui.rs`. Full Claude-session spawn is a follow-up.
32. [x] **Auto-assignment via Mentor** — when an agent is bound to a session, the Mentor reviews its profile against Library contents and injects relevant tool/skill references into the agent's prompt. **Status: DONE** — `mentor_review_profile` + `as_preamble_with_library` in `orrch-agents::runner`; profile.rs carries library reference fields with `serde(default, skip_serializing_if)` for backward compat.

### Phase 6: Node-Based Workforce Designer (1.7.0)
_Visual agent-as-node workflow editor._

33. [x] **Workforce definition format** — structured markdown with pipe-delimited step tables. Parser in orrch-workforce handles trigger/blocker/steps/interrupts. Auto-detects parallel groups from duplicate step indices.
34. [x] **TUI node list view** — Workforce Design sub-panel in Design tab shows workforce templates and operation modules as navigable lists. Enter opens in vim for editing. Shift+Tab toggles between Project Design and Workforce Design.
35. [x] **Nested workforces** — a workforce node can contain another workforce. The inner workforce runs as a unit, reporting only its designated output agent's results to the parent. **Status: DONE** — `AgentNode.nested_workforce: Option<String>` (template.rs), 4-column parser (parser.rs), `engine::expand_nested_workforce`, `engine::resolve_step_for_dispatch` producing `ResolvedStep`, and `OperationExecution::next_resolved_steps(workforce, all_workforces)` Rust-side dispatch helper. Runtime wiring: `mcp__orrchestrator__agent_invoke` accepts `workforce` + `operation` + `step_index` params, loads workforce + operation catalogs, calls `resolve_step_for_dispatch`, and feeds the result into `AgentRunner::build_prompt_for_resolved_step` — nested workforce expansion now propagates through the dispatch boundary.
36. [x] **Workforce import/export** — save/load workforce designs as files. `serialize_workforce_markdown` in orrch-workforce parser (round-trip lossless). `export_workforce_to_path`/`import_workforce_from_path` helpers in engine. TUI keybinds `x` (export to `~/Downloads/<name>.md`) and `i` (import from `~/Downloads/import.md`) in Design > Workforce > Workflows tab.

### Phase 7: Visual Editors & Native Window Mode (1.8.0)
_Web-based visual editing, then native window integration._

37. [x] **Web-based node editor** — local HTTP server serving a drag-and-drop node graph UI (JS canvas library). TUI opens browser. Reads/writes structured markdown workforce files. TUI continues running alongside. **Status: DONE (2026-04-08)** — `orrch-webedit` crate ships an embedded JS canvas editor (HTML + JS + CSS), tiny_http server with full GET/POST API for workforce CRUD, 11 unit tests including a real-socket POST→GET roundtrip. Two launch paths: standalone via `orrchestrator --webedit` (headless, Ctrl-C to stop), or `Ctrl+w` from inside the TUI's Design > Workforce panel which spawns the server in a background thread and opens the browser via `xdg-open`. Server stops automatically when the parent process exits.
38. [x] **Native egui window** — orrchestrator TUI can spawn a native egui window for the node editor. Fully integrated, no browser dependency. **Status: DONE (scaffold)** — feature-gated `egui_window` module in `orrch-tui`, `--egui` CLI flag dispatch in `src/src/main.rs`, default build pulls zero new deps. Full node editor is a follow-up slice.
39. [x] **Non-TUI mode** — orrchestrator can optionally launch entirely as a windowed application for terminal-averse users. **Status: DONE (2026-04-08)** — two non-TUI entry points landed: `--egui` (native window scaffold, item 38) and `--webedit` (HTTP node editor, item 37). Both bypass the terminal-capability check in `main.rs` so they run headless. `--help` lists both. Documented in CLAUDE.md "Non-TUI entry points" section.

### Phase 8: Intake Workforces (2.0.0-rc)
_The predefined workforces that process different types of user input. Completing this phase means the full agent orchestration pipeline is operational — tag `2.0.0` when stable._

40. [x] **workforce:instruction-intake** — processes project dev instructions. COO optimizes → routes to project queues → PM incorporates into plan. **Status: DONE — `operations/instruction_intake.md` expanded to 7-step pipe-delimited table (Triage → Optimize → Write Review → Blocking user confirm → Route → Append → PM Incorporate) mirroring `library/skills/instruction-intake.md`. Parses via `parse_operation_markdown`, tests green.**
41. [x] **workforce:plan-intake** — processes new project designs/plans. Evaluates scope, may create new projects, may trigger versioning. Distributes instructions.
42. [x] **workforce:idea-intake** — processes incomplete ideas. Stores in Ideas vault as unattached. Tags with domain, potential project associations.
43. [x] **workforce:knowledge-intake** — processes custom agents, skills, tools. Validates, formats, saves to Library. Updates relevant agent profiles.

### Cross-Cutting: Interactive Dev Map (Oversee panel)
_Feature tracking interface in the project focus view. Plan.md becomes a live, interactive development map._

44. [x] **Plan.md syntax parser for dev map** — parse Plan.md into a structured feature tree: phases → features → status (planned/in-progress/tested/verified/removed). Display as interactive list in Oversee project detail view.
45. [x] **Feature state machine** — each feature tracks: planned → implementing → implemented → testing → verified → user-confirmed. Visual indicators for each state. Removed features show as strikethrough with removal context (removed-before-impl vs removed-after-impl vs failing-verification). **Status: DONE — FeatureStatus expanded to 9 states with parse markers ([~], [=], [t], [v], [✓]), color-coded TUI rendering, s/S keybindings for status cycling with Plan.md write-back.**
46. [x] **Reorder features** — user can move features up/down within a phase, or between phases. Changes persist back to Plan.md.
47. [x] **Add feature popup** — syntax-controlling text input that guides the user to write a feature request matching COO optimization format. Appended to Plan.md.
48. [x] **Quick-spawn for feature** — select a feature on the map, press Enter to spawn a dedicated session targeting that feature specifically with priority.
49. [x] **Diff log persistence** — all changes to the dev map are tracked. Features display visual badges: modified since last verification, newly added, reordered, removed. **Status: DONE — `orrch-core::diff_log` module with append-only `plans/.diff_log.json` storage; `+N` cyan badge in Oversee dev map.**
50. [x] **User verification tracking** — user marks features as manually tested/confirmed. Confirmation persists until any code change affects that feature. Previously confirmed features are re-flagged when implementation changes. Release readiness view shows unconfirmed features. **Status: DONE — `user_verified` field on `PlanFeature`, `mark_verified_in_plan` mutator, `V` keybinding in Oversee, `✓` green badge.**
51. [x] **Direct PM interaction** — open a live session with the Project Manager agent for natural-language plan changes. PM updates Plan.md, dev map reflects changes in real time. **Status: DONE** — `library/skills/pm-plan-edit.md` defines the PM plan-edit operating mode; `KeyCode::Char('P')` in `key_project_detail` (`crates/orrch-tui/src/app.rs:~2694`) spawns a Claude session bound to the project_manager agent with the pm-plan-edit skill invocation. Real-time dev map refresh follows from existing PLAN.md reload mechanics.
52. [x] **Git commit grouping display** — show how the workforce intends to package commits by phase. Repository Manager reviews and advises on the PM's chosen feature grouping for optimal git workflow. **Status: DONE (core) — dev map feature rows now render up to 3 packaged commit lines (short-sha + subject) using `commits_for_feature()`. Implemented as multi-line `ListItem` to preserve selection index invariants. RepoManager advisory layer is a future follow-up.**

### Cross-Cutting: AI Tooling Management (Library)
_Model hierarchy, harness management, and cost-optimized workforce assignment._

53. [x] **Model registry in Library** — ModelEntry struct with tier, pricing, capabilities, limitations, max_context, API key env. 8 models seeded. Loaded from `library/models/*.md`. Displayed in Library > Models sub-panel with tier color badges and preview pane.
54. [x] **Three-tier workforce profiles** — enterprise (Claude, GPT-4o — existing workflows), mid-tier (Mistral Large API — more structured instructions), local/free (Ollama Mistral, Gemini free — rigid logic, scope-limited tasks). Each tier has its own workforce micromanagement template adjusting instruction directness and scope boundaries. **Status: DONE** — `workforces/enterprise_tier.md` (5 agents, loose), `workforces/mid_tier.md` (6 agents, adds Specialist reviewer), `workforces/local_tier.md` (4 agents, dual verifier with Penetration Tester for narrow rigid scope). All three parse via existing 4-column workforce parser.
55. [x] **Resource Optimizer agent** — profile created (`agents/resource_optimizer.md`), AgentRole added, DEVELOP FEATURE module updated with step 2 (optimize) and step 7 (commit review). Prompt includes model tier guidelines and harness awareness.
56. [x] **Harness registry in Library** — HarnessEntry struct with auto-detection (`which`), supported models, flags, capabilities. 5 harnesses seeded. Displayed in Library > Harnesses sub-panel with availability status badge.
57. [x] **Mixed-model workflow support** — a single workforce execution can use different models for different steps. The hypervisor passes the model/harness assignment from the Resource Optimizer to each subagent spawn. **Status: DONE** — `Step::model_override: Option<String>` (operation.rs); parser accepts optional 5th `Model` column (legacy 4-col still parses); `AgentRunner::build_prompt_for_resolved_step` threads `ResolvedStep.model_override` into a `## Model Override` prompt directive block. Runtime wiring: `mcp__orrchestrator__agent_invoke` accepts either a direct `model_override` shortcut or resolves the override from `workforce` + `operation` + `step_index` via `resolve_step_for_dispatch`, and always produces prompts via `build_prompt_for_resolved_step` when any dispatch context is supplied.
58. [x] **Mentor resource updates** — Mentor periodically dispatches Researcher to investigate changes to available models, harnesses, and tools. Researcher reports back, Mentor updates Library entries. PM uses these updated entries for optimization decisions. **Status: DONE (scaffold)** — `ResourceUpdateRequest`, `build_researcher_resource_prompt`, and `last_checked` fields on models/harnesses. Periodic scheduler is a follow-up.

### Cross-Cutting: API Valves & MCP Management

59. [x] **API valves (manual provider shutoff)** — per-provider on/off toggle in Library > Models. `v` = instant toggle, `V` = timed close (24h default). ValveStore persisted to `~/.config/orrchestrator/valves.json`. Visual `BLOCKED` badges on affected models. Auto-reopen ticker with countdown display.
60. [x] **MCP server config management** — McpServerEntry struct with stdio/sse transport, enable/disable toggle (`e` key), role assignment. Loaded from `library/mcp_servers/*.md`. Displayed in Library > MCP sub-panel.
61. [x] **orrch-mcp server** — MOVED to Critical Path (CP-7). Done: 12 tools including module_api, codebase_brief, develop_feature, agent_invoke.
62. [x] **External MCP server management** — configure connections to user's existing MCP servers (github, context7, etc.) through the Library > MCP panel. Assign servers to agent roles.
63. [x] **Syntax translation engine** — research session to catalog prompt/tool-call syntax differences across models and harnesses. Generate translated versions of context files (agent profiles, CLAUDE.md equivalents) per model/harness combination. Stored in Library. **Status: DONE (first slice)** — `TRANSLATIONS_DIR` constant exported from `orrch-library`, catalog file at `library/translations/harness_syntax_catalog.md` covering all 5 harnesses across 6 syntax columns with TBDs marking follow-up research.
64. [x] **Valve integration with Resource Optimizer** — Resource Optimizer checks valve state before recommending a model. Blocked providers are excluded from optimization suggestions. IRM auto-closes valves when rate limits are detected.

### Carried Forward (from 1.0.0 queued items)
- [x] **Agent profile management** — swappable CLAUDE.md/GEMINI.md profiles per project. **Status: DONE** — `Project.agent_profile: Option<String>` read from `.agent_profile` dotfile via `load_agent_profile`, wired into `Project::load()`; helpers `Project::agent_profile_filename()` (fallback `CLAUDE.md`), `agent_profile_path()`, `save_agent_profile()`; `orrch_agents::load_project_core_context(project_root, profile_filename) -> Option<String>` re-exported at the crate root. Runtime wiring: `mcp__orrchestrator__agent_invoke` accepts a `project_dir` parameter that reads the project's `.agent_profile` dotfile (or uses an explicit `profile_filename` override), loads the named file via `load_project_core_context`, and injects it as the core context in the resulting prompt — so swapping a project between Claude and Gemini profiles is now a one-line dotfile edit.
- [x] **Instruction inbox migration** — replace `fb2p.md` with per-project `instructions_inbox.md` managed by COO. Deprecate `/interpret-user-instructions` skill. Intake handled by `workforce:instruction-intake` module (EA → COO → PM). COO trims on version publish, truncates long files. Status: DONE — feedback.rs now writes per-project instructions_inbox.md; fb2p function names retained as deprecated wrappers; TUI call sites updated.

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
- 2026-04-09: **Instruction inbox refreshed — 7 new items (INS-001 through INS-007) routed from idea 2026-04-23-20-34 and incorporated as roadmap items 74-80.** Focus: Design > Plans panel, dynamic Library/Workforce views, foundational scroll architecture, dead-keybind audit, human-readable workflow edit mode. Hard dependency flagged: INS-001 (Plans panel) requires INS-005 (automatic scroll architecture). Cross-references added: INS-005 to item 65 (responsive tab bar), INS-007 to items 21/33/34/37 (workforce editors). No structural conflicts with existing roadmap.
- 2026-04-09: **2.0.0 roadmap closure sprint.** Closed the last 6 unchecked items on the 2.0.0 roadmap (11, 12, 15, 16, 20, 21) — all were architecturally superseded by completed Critical Path / Phase 5 / Phase 6 work. No code changes required: items 11/15/16 superseded by CP-1/CP-4/CP-5/CP-6, item 12 by item 32 (mentor_review_profile + Library wiring), item 20 by CP-5 (live agent tree polling .orrch/workflow.json), item 21 by items 34 + 37 (Workforce > Teams tab in TUI + web node editor). 2.0.0 roadmap is now 100% complete pending Phase 8 acceptance.
- 2026-04-09: **Instruction inbox migration complete.** `crates/orrch-core/src/feedback.rs` now writes per-project `instructions_inbox.md` instead of `fb2p.md`. `append_to_fb2p` and `append_to_fb2p_direct` retained as `#[deprecated]` wrappers forwarding to `append_to_inbox`/`append_to_inbox_direct`. TUI call sites in `app.rs` (5 filesystem literals + 2 function calls + 6 comments) and `ui.rs` (1 user-visible message) updated. All 90 orrch-core tests pass; cargo build clean with no new warnings.
- 2026-04-03: **MCP server elevated to Critical Path (CP-7).** Items 30 (Library MCP server) and 61 (orrch-mcp server) consolidated into CP-7 — a unified MCP server (stdio transport) exposing all library contents and pipeline operations as tools. Skill distribution via symlinked files is unsustainable; a single MCP server is the distribution layer that makes CP-1 through CP-6 accessible to all harnesses without file duplication.
- 2026-04-03: **Instruction inbox incorporated into roadmap.** INS-001 through INS-009 from `instructions_inbox.md` formally added to PLAN.md as "UI Polish & Infrastructure (from Instruction Inbox)" section (items 65-73). INS-001 (responsive tabs), INS-002 (sub-tab navigation), INS-003 (Library left-justification) marked complete. INS-004 through INS-009 queued as pending. No duplicates with existing roadmap items; INS-004 distinguished from item 56 (Library harnesses vs Workforce harnesses editor), INS-009 noted as complementary to CP-6.
- 2026-04-03: **Architecture pivot: skill-based workflow execution.** The prompt injection approach (old CP-1/CP-2/CP-3) is scrapped after two live tests confirmed sessions ignore 15k tokens of injected workforce instructions. New architecture: workflow definitions become executable skills (`.md` prompt files). The `/develop-feature` skill IS the Hypervisor — it spawns agents via Agent tool calls, pipes results, enforces context isolation. Orrchestrator provides visibility (live agent tree in Hypervise) and intervention. Skills vs Tools separation: skills = LLM judgment (workflows, agent roles), tools = deterministic ops (file routing, git, scaffolding). Instruction intake gets a user audit step (raw vs optimized side-by-side review before distribution). `build_workforce_context()` and SpawnWorkforce prompt injection deprecated. Critical Path rewritten with 6 new items (CP-1 through CP-6).
- 2026-04-03: **CRITICAL PATH reprioritization.** Items 15 (template selector), 16 (workforce-aware spawning), and 11 (COO optimizer) moved to a new Critical Path section above Phase 0. Root cause: a spawned session operated as a solo developer, ignoring the DEVELOP FEATURE workflow entirely. The spawn flow does not inject workforce context — `AgentRunner::build_prompt()` exists but is never called in the actual spawn path. All orchestration architecture is dead code until CP-1/CP-2/CP-3 ship. Items marked unblocked for immediate implementation.
- 2026-03-31: **FRESH PLAN.md written.** Processed redesign_plan into 8-phase roadmap toward 2.0.0 (43 items + 2 carried forward). All 8 design decisions resolved. Architecture expanded: 3 new crates, 4-panel layout, 19 agent roles, workforce templates, operation modules, multi-provider AI, node-based designer. Hypervisor agent profile created + agent profile system implemented (new spawn wizard step). fb2p.md model deprecated in favor of per-project `instructions_inbox.md` managed by COO via intake workforces. Ollama via Crush/OpenCode, library as git-backed GitHub repo + MCP server, workforce format is structured markdown.
