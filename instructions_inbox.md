# Orrchestrator — Instruction Inbox

<!-- Queued: 17 instructions from idea 2026-04-24-20-16.md. Pending PM incorporation into PLAN.md. -->
<!-- Queued: 3 instructions from idea 2026-04-25-01-15.md. Pending PM incorporation into PLAN.md. -->

### TOK-001: Token-Minimization as Core Organizational Principle
Implement a systematic token optimization layer in orrchestrator. This is the primary mission-critical objective. Deliverables:
- Audit all context delivery mechanisms (tool calls, session prompts, workforce context) for token waste
- Design a controlled context injection system that delivers only what each agent step requires — no more
- Evaluate and integrate any applicable token-reduction tooling (e.g., prompt compression, output summarization, file-cluster batching, semantic deduplication)
- Add a token budget tracker visible in the TUI (Analyze panel or persistent status bar metric)
- Document the controlled delivery protocol so all future workforce/operation design follows it

Constraint: token efficiency is not a feature — it is the design constraint that governs all other architectural decisions.

Source: Raw instruction 1 (2026-04-25-01-15.md)

### TOK-002: Per-Project Concurrent Session Limits with Warning Gate
Add session concurrency limits to orrchestrator's session management:
- Each project gets a configurable max concurrent session count (default: 3)
- When a user attempts to open a session beyond the project's limit, display a warning modal explaining the token cost risk of parallel sessions and require explicit confirmation to proceed
- Store the limit per project in project config (alongside other project metadata in `orrch-core`)
- Expose the limit in the Oversee panel's project view (current sessions / max)
- Rationale: keeping existing sessions alive is more token-efficient than spawning new ones; this gate enforces that discipline

Source: Raw instruction 2 (2026-04-25-01-15.md)

### TOK-003: Multi-Device Session Classification (Primary vs Compatibility)
Extend session metadata to distinguish session origin by device role:
- **Primary sessions**: orrion (main Linux dev machine)
- **Compatibility sessions**: orrpheus, mbp15, cb17, or any non-primary device
- Classification is per-project; cross-platform projects are allowed higher session counts because compatibility sessions are a distinct workstream, not redundant parallelism
- The concurrent session warning gate (TOK-002) must apply limits separately per session class — primary limit and compatibility limit are independent counters
- Device identity should be detectable from hostname or user-configured in orrchestrator's machine config

Source: Raw instruction 3 (2026-04-25-01-15.md)

### OPT-001: Focus new ideation editor window on creation
When a new file is created from the Design > Intentions page (idea intake), the spawned editor window must receive desktop focus immediately so the user can start typing without clicking. Check the `spawn_vim_window` call path in `editor.rs` -- ensure the terminal emulator command includes focus-stealing flags (e.g., `wmctrl -a` post-spawn, or emulator-specific `--focus` flags for alacritty/kitty/konsole/gnome-terminal). Test on orrion (CachyOS/KWin).

Source: Raw instruction 1 (2026-04-24-20-16.md)

### OPT-002: Remove dev map from project detail, show only roadmap
Project detail pages currently render both a "dev map" and a "feature roadmap." Remove the dev map display. Show only the roadmap. Audit all project detail rendering paths in `ui.rs` to confirm there is a single roadmap view, not two competing feature-list views.

Source: Raw instruction 2 (2026-04-24-20-16.md)

### OPT-003: Fix roadmap scrollability in project detail
The roadmap section in project detail pages does not scroll when content overflows. This is a regression against the scroll architecture from INS-005 (item 78). The roadmap widget must be wrapped in the automatic scroll infrastructure. Audit all panels/widgets for any others that also missed the scroll wrapper -- INS-005 was meant to be universal, so any panel bypassing it is a bug.

Source: Raw instruction 3 (2026-04-24-20-16.md)

### OPT-004: Fix navigation traps across all pages
The project menu (and potentially other pages) traps the user -- pressing Up/Back/Esc does not exit the view. Audit every page/panel/sub-view for navigation traps where the user cannot return to the parent view. Every view must be exitable via Left-arrow, Esc, or Up (consistent with the vertical focus navigation model). Fix all instances found.

Source: Raw instruction 4 (2026-04-24-20-16.md)

### OPT-005: Fix right-arrow project navigation to show project details
Pressing Right arrow on a project in the Oversee list currently opens a deprecated project view instead of the project details page. Fix: Right arrow on a project must open the project details view. Left arrow from project details must return to the project list. Verify the deprecated browser (`d` key) is not being triggered by the Right arrow handler.

Source: Raw instruction 5 (2026-04-24-20-16.md)

### OPT-006: Track all projects including newly created ones
Projects whose tasks are not tracked (e.g., borrk, newly created today) do not show dev completion in the Oversee menu. Fix: project discovery must include all projects under `~/projects/` that have any trackable state (PLAN.md, active sessions, recent git activity). If a project exists but has no PLAN.md, show it with a "no plan" indicator rather than hiding it. Specifically verify borrk appears with its current dev state.

Source: Raw instruction 6 (2026-04-24-20-16.md)

### OPT-007: Dynamic tip line based on project completion state
When a project has completed all planned features (100% roadmap done), the tip/action line should display "submit feedback" and "construct packages" instead of the current default actions. Implement as a conditional in the smart default actions logic (item 27). Check: `project.roadmap_complete() -> bool`, then override the tip line text.

Source: Raw instruction 7 (2026-04-24-20-16.md)

### OPT-008: Write platform porting workflow
Create a new operation module `operations/platform_port.md` -- a detailed, multi-step workflow for porting a project to a target platform (PyPI, crates.io, npm, Docker Hub, Flathub, AUR, Homebrew, apt/deb, etc.). This is a first draft for the user to revise. Include steps for: platform research (requirements, conventions, licensing), dependency audit, build system adaptation, platform-specific packaging (manifest/config files), CI/CD pipeline for the target, test matrix (platform-native testing), documentation adaptation (platform README, install instructions), pre-release validation, publish, and post-publish verification. Use the existing pipe-delimited step table format. Assign appropriate agents (Researcher, Developer, Repo Manager, Licensing Auditor, Feature Tester, Beta Tester). The user will revise/replace this draft.

Source: Raw instruction 8 (2026-04-24-20-16.md)

### OPT-009: Self-extending agent library via MCP
Add MCP tools to orrchestrator that allow agents to dynamically create new agents, skills, tools, and workflows at runtime. Tools needed:
- `create_agent` -- writes a new agent `.md` profile to `agents/` with YAML frontmatter and system prompt
- `create_skill` -- writes a new skill `.md` to `library/skills/`
- `create_tool` -- writes a new tool script to `library/tools/`
- `create_workflow` -- writes a new workforce `.md` to `workforces/`

Behavioral contract: when an agent encounters a problem outside its expertise, it should ask (via prompt output) whether there is an expert agent that could help. If none exists, the agent creates the expert agent profile and any needed skills using these MCP tools, saves them to the library, then invokes the newly created agent to solve the problem. Add this protocol to the base agent instructions.

Source: Raw instruction 9 (2026-04-24-20-16.md)

### OPT-010: Session lifecycle management (hot/cold projects, stale cleanup, completion signaling, session briefs)
This is a unified session lifecycle system covering project activity tracking, stale session cleanup, completion detection, and session archival.

**Hot/cold project tracking (Oversee):**
- Projects with an active workflow running on them move to a "hot" section at the top of the Oversee project list.
- Projects stay hot while they have an open desktop window or tmux session. The idle timer starts when the window goes idle (not when the window closes). If the user closes a stale window that was keeping a project hot, the project goes cold immediately (timer was already running from idle start, not window close).

**Stale session cleanup:**
- Auto-close sessions that are finished and idle (auto-commit processes that completed, workflows that finished). No session should sit open after its work is done.
- Implement a completion key phrase that agents print when a workflow is fully complete and all work is committed. This phrase is orrchestrator's trigger to close the session.

**Session status indicators:**
- If a session is waiting for user input (question, approval) or has an issue to report, orrchestrator must surface an indicator in the TUI -- visible from the Oversee and Hypervise panels. Reuse/extend the existing waiting-for-input detection (item 10) to cover explicit agent questions and error reports.

**Session close protocol (agents must follow this before printing the completion phrase):**
1. Verify work is committed and pushed.
2. Run mempalace diary_write to record the session.
3. Write a session brief to the project's `.orrch/` data directory (structured: goal, changes made, files touched, test results, open questions).

**Session brief navigation:**
- Session briefs stored in `.orrch/session_briefs/` must be readable and navigable from the project details menu in the TUI.

Source: Raw instructions 10, 11, 12, 13, 14, 15 (2026-04-24-20-16.md)

### OPT-011: Use nvim instead of vim
Verify all editor invocations use `nvim`, not `vim`. Audit: check `vim_request` construction sites, `spawn_vim_window`, fallback paths, CLAUDE.md references, and any hardcoded `"vim"` strings in the codebase. Replace any remaining `vim` references with `nvim`. Ensure the user sees `nvim` in window titles and documentation.

Source: Raw instruction 16 (2026-04-24-20-16.md)

### OPT-012: Tmux session management overhaul (labels, splitting, custom config)
Unified tmux management improvements:

**Tab naming:**
- Tmux window/tab names are too long and bury the project name. Reformat: `<project>:<short-goal>` (e.g., `concord:fix-auth`). Truncate to fit. Project name must always be visible first.

**Window splitting:**
- Allow splitting an orrchestrator tmux tab into its own standalone tmux window. The user wants to view two orrchestrator session tabs side-by-side. Diagnose and remove whatever is currently blocking this (likely tmux session vs. window semantics, or a custom config restriction).

**Custom tmux config:**
- Orrchestrator must ship its own tmux config file (`config/tmux.conf`) and apply it via `tmux -f <path>` when creating managed sessions -- do NOT use the system tmux config. Include clear pane labels, orrchestrator branding, and sensible defaults. Create platform-specific configs for: Linux (CachyOS/orrion), macOS (orrpheus). Apply the correct config based on detected OS.

Source: Raw instructions 17, 18, 19 (2026-04-24-20-16.md)

### OPT-013: Project classification and lifecycle tools
Add project management tools to orrchestrator:
- Move projects between `admin/` and the regular `~/projects/` directory (bidirectional).
- Browse the `deprecated/` folder and move projects in/out of it.
- Delete projects from `deprecated/` (with confirmation).
- All operations update orrchestrator's project registry and refresh the Oversee panel.
- Accessible from a project context menu or dedicated keybind in the Oversee panel.

Source: Raw instruction 20 (2026-04-24-20-16.md)

### OPT-014: In-TUI file and entity renaming
Orrchestrator must support renaming without leaving the TUI: files, projects, intentions, plans, agents, skills, tools, workflows. Implement a rename action (`r` or `F2` keybind) on any nameable entity in any panel. The rename should: update the filesystem path, update any internal references (e.g., a renamed agent referenced in a workforce), and refresh the panel view.

Source: Raw instruction 21 (2026-04-24-20-16.md)

### OPT-015: Design the Publish page plan
Write a detailed plan for the Publish page and add it to PLAN.md as a new phase/section. The Publish page handles release packaging, distribution, and compliance. Include at minimum:
- Release packaging (build artifacts, installers, archives per platform)
- Version tagging and changelog generation (integrate with `/release`)
- Platform distribution (publish to crates.io, PyPI, npm, Docker Hub, GitHub Releases, AUR, Homebrew, etc.) -- reuse/invoke the OPT-008 porting workflow
- License compliance audit (invoke Licensing Auditor agent)
- Copyright verification (invoke Copyright Investigator)
- Release notes generation (auto-draft from conventional commits)
- Marketing material generation (invoke Market Researcher, UX Specialist)
- Pre-release checklist enforcement (all tests pass, no open blockers, user verification complete)
- Post-release monitoring (download counts, issue reports)
- Rollback capability

This is high priority -- add to PLAN.md for immediate implementation.

Source: Raw instruction 22 (2026-04-24-20-16.md)

### OPT-016: Expand the Analyze page with comprehensive metrics
The Analyze page currently only shows per-provider API usage (item 25). Expand it with richer metrics. Research (via Researcher agent) which metrics are most valuable, but include at minimum:
- Token usage per project, per session, per workflow step, per agent role
- Cost breakdown (dollars) by provider, project, time period
- Session duration and throughput (features completed per hour/day)
- Agent performance: success rate, retry count, average tokens per task
- Error frequency and resolution time (leverage orrch-retrospect data)
- Workflow efficiency: time per step, bottleneck identification
- Project velocity: features completed over time, burndown charts
- Resource utilization: concurrent sessions, provider saturation
- Historical trends (daily/weekly/monthly charts via sparklines or ASCII graphs)
- Comparison across tiers (enterprise vs. mid-tier vs. local cost-effectiveness)

Source: Raw instruction 23 (2026-04-24-20-16.md)

### OPT-017: Hypervise panel feature expansion
The Hypervise panel needs significant capability upgrades. Required features:

**Session list view:**
- Rename sessions inline
- Expand sessions to show the 2 most recent messages (preview mode)
- Display each session's host machine and working directory
- Clear and restart a session with an editable spawning prompt

**Session detail view (drill-in on Enter):**
- Live message stream from the session (real-time output display)
- Text input box for writing prompts to the session (open nvim, send on save)
- Dashboard showing: assigned workforce, active workflow, available skills, tools, MCP servers
- Ability to invoke workflows or skills from this dashboard directly into the session
- Interrupt/pause the session's current process

Source: Raw instruction 24 (2026-04-24-20-16.md)

### OPT-018: Truncate webportal URL display (source: plans/2026-05-06-03-57.md)
Webui panel displays full portal URL — replace with truncated form (host:port or icon + port only). Remove full-URL rendering from portal list/status widgets.

Source: "The webportals entire url shouldnt be displayed."

### OPT-019: Settings popup menu (source: plans/2026-05-06-03-57.md)
Add dedicated settings popup (modal overlay) accessible from anywhere in the TUI. Centralize all user-configurable options: display styles/themes, webui access toggles, and every other currently-scattered setting. Single entry point, categorized sections, persists to config.

Source: "We need to create a dedicated settings interface. Make a popup menu that controls all settings from display styles, to webui access and everything else too."

### OPT-020: TUI UX audit + enterprise refactor via frontend-design agent (source: plans/2026-05-06-03-57.md)
Perform thorough UX analysis of current TUI (layout, navigation, affordances, density, discoverability, consistency across panels). Document findings. Then invoke the frontend-design agent/skill to refactor the entire TUI into an enterprise-grade interface based on the audit. Scope covers every panel, not a subset.

Source: "Do thorough analysis on the UX options of TUI's and then call the frontend-design agent/skill to refactor EVERYTHING into a usable enterprise project"
