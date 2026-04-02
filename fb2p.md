# Orrchestrator — Feedback to Prompt Log

---

## Entry: 2026-03-26 — chat feedback (self-healing + retrospect engine)

### Raw Input
This needs to be a flexible environment that is responsive to common errors and remembers solutions to problems. It should update itself with troubleshooting protocols created by retrospect analysis of dev logs. Actually, claude as a whole should implement an automatically running process that analyzes dev logs and creates troubleshooting protocols specific to the project being developed.

### Optimized Prompt

## Objective
Build the Retrospect engine — an automated error-learning pipeline within orrchestrator that captures errors from managed Claude sessions, pairs them with resolutions, runs retrospective analysis across dev logs, and generates per-project troubleshooting protocols that can be actively injected into future sessions.

## Requirements
1. **Error parser** — watch session output for stack traces, error patterns (`Error:`, `Exception:`, `FAILED`, `Traceback`), and Claude Code error formatting. Normalize into fingerprints (strip variable values, line numbers) for deduplication.
2. **Error store** — per-project JSON store (`<project>/.retrospect/errors.jsonl`) holding fingerprinted errors with timestamps, session IDs, raw context.
3. **Solution tracker** — when a session that previously errored reaches a successful state (no more errors, task completion signal), capture the diff (files changed, commands run) as the resolution. Pair with the error fingerprint.
4. **Retrospective analyzer** — background async task that periodically scans `~/projects/*/fb2p.md`, `~/projects/*/.bugfix-ledger.md`, `~/projects/*/.retrospect/errors.jsonl`, and session output history. Extracts: recurring error classes, cross-project patterns, resolution effectiveness.
5. **Protocol generator** — distill analysis into `<project>/.troubleshooting.md` as structured decision trees: symptom → likely causes → known fixes → escalation.
6. **Active injection** — when a managed session hits a known error fingerprint, write the troubleshooting protocol snippet to the session's stdin (or queue it for the next prompt) so Claude gets the solution context automatically.
7. **Global knowledge base** — `~/projects/.troubleshooting-global.md` for patterns that span projects (Python version issues, Textual API drift, gevent gotchas).

## Constraints
- Scope is `private` — no over-engineering. Start with error capture + store, layer analysis on top.
- Must not block the event loop — all file I/O and analysis in background tasks.
- Error store should be append-only JSONL for simplicity and crash safety.
- Don't duplicate the existing `.bugfix-ledger.md` system — read from it as an input source, don't replace it.

## Technical Decisions
- JSONL for error store (one JSON object per line, easy append, easy grep)
- Fingerprinting: strip numbers, normalize whitespace, hash the result → stable ID for dedup
- Protocol files are Markdown (human-readable, Claude-readable)
- Background analysis runs on a timer (e.g., every 10 minutes while orrchestrator is running)

## Open Questions
- None — user intent is clear. Build it incrementally starting from capture layer.

## Acceptance Criteria
- Errors from managed sessions are automatically captured and fingerprinted
- When the same error class recurs, the system recognizes it and can surface the prior resolution
- `.troubleshooting.md` files are generated per-project with actionable decision trees
- Active injection works: Claude sessions receive known-fix context when hitting recognized errors

### Status
Generated: 2026-03-26
Executed: 2026-03-26 (capture layer: error parser, store, solution tracker, app wiring — items 11-12 complete)
Remaining: items 13-16 (retrospective analyzer, protocol generation, active injection, cross-project KB)

---

## Entry: 2026-03-26 — chat feedback (Rust rewrite + cargo build pipeline)

### Raw Input
Create a test build pathway that allows me to test changes to the code in real time as the changes are being made. This is from here on intended to be distributed as a native linux application. I am using cargo to test concord builds to great effect. Consider using cargo here as well as the build test facilitator.

### Optimized Prompt

## Objective
Rewrite orrchestrator in Rust with a cargo workspace, matching the concord v2 project structure. Set up `cargo watch` for live-reload development and `cargo test` as the test pathway. The Python prototype served as design validation — the Rust version is the real build.

## Requirements
1. **Cargo workspace** — root `Cargo.toml` with workspace members mirroring the component architecture (process manager, TUI, retrospect engine)
2. **Live-reload dev loop** — `cargo watch -x run` for instant feedback on code changes during development
3. **Test pathway** — `cargo test` runs unit + integration tests, `cargo watch -x test` for continuous testing
4. **Native Linux binary** — single statically-linked binary, no runtime dependencies (no Python, no venv)
5. **Ratatui TUI** — replace Python/Textual with ratatui + crossterm for the terminal UI
6. **Tokio async** — match concord v2's async runtime for PTY management and process spawning
7. **Port all existing functionality** — process manager, session discovery, dashboard, focus screen, retrospect error capture

## Constraints
- Maintain the same architecture and keybindings from the Python prototype
- Must still be SSH-first (pure terminal, no GUI deps)
- The Python prototype (`orrchestrator/`) stays as reference until Rust port is feature-complete, then gets archived
- Follow concord v2 patterns: workspace layout, edition 2024, shared workspace dependencies

## Technical Decisions
- **Rust + cargo** — the user's chosen toolchain, proven in concord v2
- **ratatui** — dominant Rust TUI framework, active development, crossterm backend
- **tokio** — async runtime (already used in concord v2)
- **portable-pty** or `nix` crate — PTY management
- **serde + serde_json** — for JSONL error store (matching concord v2 deps)
- Workspace structure: `crates/orrch-core`, `crates/orrch-tui`, `crates/orrch-retrospect`

## Open Questions
- None — intent is clear: Rust rewrite with cargo, matching concord v2 patterns.

## Acceptance Criteria
- `cargo watch -x run` launches the TUI and reflects code changes in real time
- `cargo test` passes with tests for process manager, error parsing, fingerprinting
- `cargo build --release` produces a single native Linux binary
- Feature parity with Python prototype: spawn/kill sessions, dashboard, focus screen, retrospect capture

### Status
Generated: 2026-03-26
Executed: 2026-03-27 — All acceptance criteria met. 17/17 tests pass, 4.7MB release binary, cargo-watch installed.

---

## Entry: 2026-03-27 — chat feedback (project tracker as primary workflow)

### Raw Input
Continue development. Prioritize building a robust project tracker. This project tracker, not just a terminal window list, needs to be the primary workflow. When I open a new session in orrchestrator, it should prompt me for the intended project, and ask me what goal I would like to assign for the session.

### Optimized Prompt

## Objective
Replace the session-centric dashboard with a **project-centric tracker** as the primary workflow. The project tracker reads each project's PLAN.md and fb2p.md, displays goals/tasks per project, and when spawning a session, prompts for a specific goal to assign. Sessions become subordinate to project goals, not the other way around.

## Requirements
1. **Project tracker data model** — a `Project` struct that loads from `~/projects/<name>/`, reads PLAN.md (roadmap items + status), fb2p.md (queued prompts), and .scope. Tracks assigned sessions.
2. **Project list view** — the primary dashboard view. Shows all projects with their goal count (planned/in-progress/done), active session count, and scope badge. Sorted by activity.
3. **Goal assignment on session spawn** — when user presses `n`, the flow is: pick project → enter goal text (free-form or pick from PLAN.md roadmap) → pick backend → session spawns with that goal as context. The goal is passed to Claude/Gemini as the initial prompt.
4. **Session-goal binding** — each session has an assigned goal string. Displayed in the session table and info panel. Progress against the goal is tracked via output parsing.
5. **Project detail view** — pressing Enter on a project expands to show: its PLAN.md roadmap, active sessions with their goals, queued fb2p.md prompts, and retrospect stats.
6. **PLAN.md parser** — extract `## Feature Roadmap` items with `[x]`/`[ ]` status. Display as a checklist in the project detail view.

## Constraints
- Keep session focus/maximize working — this is still how the user interacts with individual sessions
- The project list is the default view on launch, replacing the bare session table
- Must not modify project PLAN.md files — read-only from orrchestrator's perspective
- Free-form goal input needs a text input widget in the TUI

## Technical Decisions
- `Project` struct lives in `orrch-core` (it's data, not UI)
- PLAN.md parsing is simple regex: lines matching `N. [x] **text**` or `N. [ ] **text**`
- Goal text is stored on the `Session` struct and passed as `-p "<goal>"` to the AI backend
- Project list view replaces the current session-only dashboard as the default

## Open Questions
- None.

## Acceptance Criteria
- On launch, the dashboard shows a project list with goal/session counts
- Pressing `n` walks through: project → goal → backend → spawn
- Each session displays its assigned goal in the table and info panel
- Project detail view shows PLAN.md roadmap items and active sessions

### Status
Generated: 2026-03-27
Executed: 2026-03-27 — All acceptance criteria met. Project list as default view, 3-step spawn wizard with goal input + roadmap picker, session-goal binding, project detail view. 32/32 tests.

---

## Entry: 2026-03-27 — chat feedback (editor + continue-development default)

### Raw Input
1. I want a robust vim based text editor that is always available for me to create new feedback for the dev team. Orrchestrator will then take the feedback, process it into optimized prompts, distribute them to the appropriate projects and then offer to initiate a general dev session to continue the work.

2. the default "new session" should be a general dev session with a simple "continue development" prompt. The feedback pipeline should make that the most effective prompt we have in our workflow.

### Optimized Prompt

## Objective
Build a vim-based text editor accessible from any view for writing feedback, and change the default session spawn to a "continue development" prompt that relies on the feedback pipeline (PLAN.md + fb2p.md) to provide all context.

## Requirements
1. **Embedded editor** — pressing `f` from any view opens a vim-like text editor in orrchestrator. The user writes free-form feedback. On save+quit (`:wq`), orrchestrator processes it through the feedback pipeline: parse → route to project(s) → generate optimized prompts → update fb2p.md + PLAN.md.
2. **Feedback routing** — after processing, show a summary: "Routed N items to project X, M items to project Y." Then offer: "Spawn continue-dev sessions for affected projects? (Enter/Esc)"
3. **Default spawn is "continue development"** — pressing `n` on a project no longer requires a custom goal. The default (just press Enter with empty goal) spawns `claude --dangerously-skip-permissions -p "continue development"`. The feedback pipeline ensures PLAN.md and fb2p.md are always current, making "continue development" the most context-rich prompt possible.
4. **Goal input is optional** — the spawn wizard still allows a custom goal (type text or Tab for roadmap), but the empty-goal path is the fast default and produces a "continue development" session.
5. **Editor saves to temp file** — editor content is saved to `~/projects/.feedback/<timestamp>.md` before processing, so nothing is lost if processing fails.

## Constraints
- The editor does NOT need to be a full vim clone. A basic text editor with insert mode, basic cursor movement, and :wq/:q! is sufficient. Use crossterm raw input handling.
- Keep it simple — the editor is for writing feedback, not editing code. No syntax highlighting, no line numbers required (though nice to have).
- The "continue development" prompt text is literally "continue development" — the AI's own CLAUDE.md instructions define what that trigger means.

## Technical Decisions
- Editor implemented as a new View variant in the TUI, using crossterm key events
- Temp feedback files in `~/projects/.feedback/` directory
- Processing pipeline: save file → call feedback parsing logic (port of /interpret-user-instructions concepts) → route to projects → update fb2p.md
- For now, the "processing" step writes the raw text to the target project's fb2p.md as a new entry with status pending. Full optimization (structured prompt generation) can come later.

## Open Questions
- None.

## Acceptance Criteria
- `f` opens a text editor from any view
- User can write multi-line text, save with Esc then confirm, or :wq
- On save, feedback is appended to target project's fb2p.md
- User is offered to spawn continue-dev sessions
- Default `n` → Enter (empty goal) spawns "continue development" session
- The spawn wizard still supports custom goals via typing or Tab

### Status
Generated: 2026-03-27
Executed: 2026-03-27 — Editor, routing, and continue-dev default all implemented. 41/41 tests.

---

## Entry: 2026-03-27 — chat feedback (comprehensive UX overhaul — 12 items)

### Raw Input
1. Existing plans aren't imported/displayed properly. External sessions should be incorporated. 2. Inline vim editor in project detail view. 3. Session-goal tracking with dual-session warning. 4. Append-only master plan management with v2 versioning option. 5. Project descriptions from PLAN.md displayed under titles. 6. "Plans" vault for undeveloped ideas, accessible from anywhere. 7. Manual green/yellow/red project tagging. 8. Sort by color tags. 9. Expandable project listings showing sessions+goals. 10. "Next priority" display per project. 11. Smart default actions based on project state. 12. New "personal" scope, use GitHub privacy as scope hint.

### Optimized Prompt

## Objective
Major UX overhaul making orrchestrator a complete project management environment. Improve PLAN.md parsing, add inline editing, master plan management, ideas vault, project tagging, smart defaults, and a new "personal" scope.

## Requirements (prioritized by user emphasis on editor/feedback workflow)

### Tier 1 — Editor & Feedback (build first)
1. **Inline project editor** — project detail view has a vim text box for writing feedback directly targeting that project
2. **Master plan append mode** — view/append-only editor for the user's original project vision document. Appends trigger feedback intake. Option to flag append as "v2 structural change"
3. **Ideas vault** ("Plans" menu) — top-level menu for storing undeveloped ideas as plaintext notes. Accessible from anywhere via keybind. Stored in `~/projects/orrchestrator/plans/`
4. **Better PLAN.md parsing** — extract description line, detect more roadmap formats, parse open items as "next priority"

### Tier 2 — Project Display
5. **Project descriptions** — first non-heading paragraph from PLAN.md shown under project name
6. **Next priority indicator** — show next open roadmap item text next to each project
7. **Color tags** — manual green/yellow/red tag per project (stored in `.orrtag` file). Default sort by tag color
8. **Expandable project rows** — toggle expand to show active sessions with goal tags inline
9. **External session incorporation** — map discovered external Claude sessions to projects, display them as managed

### Tier 3 — Session Intelligence
10. **Session-goal inheritance** — sessions working on a goal auto-inherit it. Yellow warning when 2+ sessions share a goal
11. **Smart default actions** — if project has no PLAN.md, default action is "create master plan". If has plan but no sessions, default is "continue development"
12. **"Personal" scope** — new scope level below private. For full-size projects only the user intends to use. GitHub privacy as scope hint for detection.

## Constraints
- Master plan is append-only — no editing previous content (protects pipeline integrity)
- All orrchestrator data files in plaintext within ~/projects/orrchestrator/
- Ideas vault is simple markdown files, not a database

## Technical Decisions
- Project color tags stored in `<project>/.orrtag` (one line: green/yellow/red)
- Master plan file: `<project>/MASTER_PLAN.md` (user's original vision, append-only)
- Ideas stored in `~/projects/orrchestrator/plans/*.md`
- "Personal" scope added to Scope enum and .scope file format
- Project description: first paragraph after `#` heading in PLAN.md

## Open Questions
- None.

## Acceptance Criteria
- Inline editor works in project detail view for direct feedback
- Master plan append mode with v2 versioning flag
- Ideas vault accessible from any view
- Projects show descriptions, next priority, color tags
- Sort by color tag works
- External sessions mapped to projects
- "Personal" scope recognized

### Status
Generated: 2026-03-27
Executed: 2026-03-27 — 10 of 12 items implemented. Remaining: external session mapping (#25), session-goal inheritance (#26).

---

## Entry: 2026-03-27 — chat feedback (contrast fix + cbsr routing)

### Raw Input
1. CBSR Live displays are muted by default. This confused me. They should be unmuted by default, and the "mute" button should be a highlighted toggle button rather than changing between "mute and unmute"
2. the font for the "next step" display and the "simple description" display is too low contrast. With the translucent background its barely visible

### Routing
- Item 1 → **orrapus** (cbsr/cbweb live display mute behavior)
- Item 2 → **orrchestrator** (TUI text contrast for description + next priority lines)

### Status
Generated: 2026-03-27
Executed: 2026-03-27 — Item 1 routed to orrapus/fb2p.md. Item 2 fixed: description and next-priority text bumped from DarkGray/DIM to DIM_BRIGHT (200,200,220) for readability on translucent terminals.

---

## Entry: 2026-03-28 — chat feedback (external vim + feedback pipeline tab)

### Raw Input
1. I want the feedback editor text input display to be optionally opened in a new window so I can continue to monitor the other projects while I write feedback
2. feedback files should be managed carefully as well. Unfinished feedback files should be saved to be resumed later. make the "submit" action something the user does in some kind of feedback menu. Actually, this is what it is: add a new tab called "feedback" that will manage the feedback pipeline. It keeps track of in-progress feedback files. the user can submit feedback files from thsi menu and it will report on the various stages of the feedback processing pipeline.

Clarification: "get rid of your custom text editor and literally replace it by simply opening a fresh vim terminal whenever needed."

### Optimized Prompt
Replace the custom inline vim-clone editor with real vim spawned in a new terminal window. Add a "Feedback" tab as the third top-level panel that manages the full feedback lifecycle: drafts, submission, routing, and pipeline status tracking.

### Status
Generated: 2026-03-28
Executed: 2026-03-28 — Both items implemented. Custom Editor removed, replaced with external vim (new terminal window with display server fallback). Feedback tab added with draft/routed lifecycle, submit/resume/delete actions, .status.json tracking. 38/38 tests pass.

---

## Entry: 2026-03-28 — bugfix session (terminal lockup + clipboard + UTF-8 crash)

### Raw Input
User reported: Ctrl+Shift+V or Ctrl+Shift+C crashes orrchestrator and permanently locks up the terminal. Pasting Claude output (containing multi-byte UTF-8 characters) also crashes the app.

### Root Cause Analysis
Three layered issues:

1. **No panic hook** — `main.rs` entered raw mode + alternate screen but had no `std::panic::set_hook` to restore terminal state on crash. Any panic left the terminal in raw mode with no cursor — permanently locked.

2. **Clipboard via arboard unreliable** — `arboard::Clipboard::new()` creates a direct Wayland client connection. This fails silently in many contexts (inside tmux, when WAYLAND_DISPLAY isn't forwarded, on some compositor states). The app reported "clipboard empty or unavailable" even though system clipboard had content.

3. **UTF-8 char boundary panic** (the actual crash) — `editor.rs` tracked cursor position as a byte offset (`cursor_col`) but moved it by `+1`/`-1` per keystroke. ASCII chars are 1 byte so this worked for English text. But pasted content from Claude often contains multi-byte UTF-8 characters (em-dashes `—` = 3 bytes, smart quotes = 3 bytes, emoji = 4 bytes). After one such char, `cursor_col` drifted off a char boundary and `String::insert(cursor_col, ch)` panicked: `assertion failed: self.is_char_boundary(idx)`.

### Fixes Applied

**1. Panic hook (main.rs)**
```rust
let default_hook = std::panic::take_hook();
std::panic::set_hook(Box::new(move |info| {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    default_hook(info);
}));
```

**2. Clipboard rewrite (editor.rs)**
Replaced `arboard::Clipboard::new()` calls with subprocess-based clipboard:
- `wl-paste --no-newline` (Wayland) → `xclip -selection clipboard -o` (X11) → arboard (fallback)
- `wl-copy` (Wayland) → `xclip -selection clipboard` (X11) → arboard (fallback)

**3. UTF-8 cursor movement (editor.rs)**
Added char-boundary-aware navigation helpers:
- `prev_boundary(s, pos)` — finds previous char boundary before byte position
- `next_boundary(s, pos)` — finds next char boundary after byte position
- `snap_boundary(s, pos)` — snaps any byte offset to nearest valid char boundary (rounding down)

Updated all cursor operations: char insert (+len_utf8), backspace (prev_boundary), left/right arrow (prev/next_boundary), up/down line change (snap_boundary).

### Also fixed (tmux environment)
- Added `WAYLAND_DISPLAY` to tmux's `update-environment` list in `~/.tmux.conf` — was missing, causing `wl-paste`/`wl-copy` to fail in tmux panes.

### Status
Generated: 2026-03-28
Executed: 2026-03-28 — All three fixes applied. 8/8 editor tests pass. Panic hook confirmed working (terminal recovers on crash). Clipboard confirmed working via wl-paste. UTF-8 handling untested with live multi-byte content (pending user verification).

---

## Entry: 2026-03-28 — parallel pipelines + remote session fix

### Raw Input
Continue dev. Take care of the parallel feature pipelines. Also fix the remote session starter — it failed to start a session on orrpheus. If we need to build something to run on orrpheus to maintain the orrchestration connection we can do that.

### Root Cause Analysis (remote session failure)
Three layered issues prevented orrchestrator from managing sessions on orrpheus:

1. **Wrong SSH target** — `known_hosts()` used `orrpheus` but orrpheus requires `coltonorr@orrpheus` (different username). SSH failed with `Permission denied`.
2. **No `/proc` on macOS** — `discover_remote_sessions()` used `readlink /proc/{pid}/cwd` which doesn't exist on Darwin. macOS needs `lsof -p PID -a -d cwd`.
3. **No tmux on orrpheus** — `spawn_remote_session()` relied on tmux which isn't installed on the Mac. orrpheus has `screen` available.
4. **`pgrep -af` incompatible** — macOS pgrep doesn't output command lines with `-a` flag like Linux does.

### Solution: orrch-agent.sh

Built a cross-platform POSIX shell agent (`agent/orrch-agent.sh`) that handles all platform differences:
- Piped to remote nodes via SSH stdin (`bash -s -- <cmd>`) — no deployment needed
- Auto-detects OS (Linux/macOS), session multiplexer (tmux/screen/nohup), and available backends
- Commands: `discover` (JSON lines), `spawn`, `kill`, `list`, `check` (capabilities JSON)
- Discovery: `ps -eo pid,command` (POSIX) + platform-specific CWD lookup
- Spawning: tries tmux → screen → nohup, returns `OK:<name>:<mux>`

Rewrote `remote.rs` to use the agent:
- `run_agent()` / `run_agent_with_args()` — pipe script, read stdout
- `include_str!("../../../agent/orrch-agent.sh")` — embedded in binary
- Startup capability probe via background task → feeds reachability + capabilities to UI
- Host selector shows OS/mux info: "orrpheus (macos/screen)"

### Parallel Feature Pipelines (roadmap #8 + #26)

Added pipeline-aware session management:

1. **`pipelines_for_project()`** — groups active sessions by goal, returns (goal, count, worst_state) tuples
2. **`duplicate_goal_count()`** — checks how many sessions share a goal for a project
3. **Duplicate goal warning** — spawn overlay shows yellow "⚠ N session(s) already working on this goal"
4. **Roadmap item badges** — each open roadmap item in spawn picker shows existing session count
5. **Pipeline-grouped expanded rows** — expanded projects show goals as pipelines with ×count and ⚠ badges
6. **Pipeline count badge** — project rows show `N⊞` when multiple distinct goals are active (vs `N▶` for single-goal)
7. **Shift+N multi-spawn** — spawns parallel sessions for ALL open roadmap items at once
8. **Session table badges** — project detail session table shows "⚠ ×N" when multiple sessions share a goal

### Status
Generated: 2026-03-28
Executed: 2026-03-28 — Both features complete. 38/38 tests pass. 5.8MB release binary. Remote spawn tested on orrpheus (screen session created successfully).

---

## Entry: 2026-03-29 — inline feedback (projects panel directory browser)

### Raw Input
I think you missed this in one of my feedback docs but the main projects display is supposed to have collapsable browser contents that are navigable with the left and right arrow keys.

### Optimized Prompt

## Objective
Add inline collapsible directory browsing to the main Projects panel. When a project row is expanded, it should show both active sessions AND a navigable directory tree below them. Left/right arrow keys control tree depth (right enters a directory, left goes up).

## Requirements
1. **Expanded project rows show directory contents** — below the existing session lines, show the project's top-level files and directories with tree indicators (▸/▾)
2. **Left/right arrow navigation** — right arrow expands/enters a directory (shows its children indented), left arrow collapses or goes up a level
3. **Preview on file selection** — when a file is highlighted, show a preview in a right-side pane or in the existing info area
4. **Tree state per project** — each project remembers which directories are expanded
5. **Coexists with sessions** — sessions appear first in the expanded section, then the directory tree below
6. **Performance** — only load directory contents when expanded, not on every render tick

## Constraints
- Must not break the existing Tab toggle for expand/collapse
- Left/right currently switch panels (tab bar). Need to only capture left/right when inside an expanded project's tree, not when at the top project level
- The project list can get very long — need to handle scroll correctly with variable-height items

## Technical Decisions
- Tree state stored as `HashMap<usize, HashSet<PathBuf>>` — project index → set of expanded directory paths
- Directory entries loaded lazily on first expand
- Indentation: 2 spaces per depth level
- Files show icon + name, directories show ▸/▾ + icon + name

## Open Questions
- None — user intent is clear from original feedback and this clarification.

## Acceptance Criteria
- Expanding a project on the Projects panel shows its directory tree
- Right arrow on a directory expands it inline
- Left arrow collapses it or goes up
- Files are selectable and show a preview
- Performance acceptable with large project directories

### Status
Generated: 2026-03-29
Executed: 2026-03-29 — Inline tree browser added to Projects panel. Expanded projects show directory tree with ▸/▾ indicators. Right arrow enters tree browsing mode, left collapses dirs or exits. Up/down navigate nodes. Enter opens files in vim. Preview pane appears alongside when tree browsing. Tree state is per-project (remembers which dirs are expanded). 44/44 tests.

---

## Entry: 2026-03-29 — inline feedback (planning document feedback mode)

### Raw Input
When a new project is created it is expected that the user will provide some kind of guidance documentation. Or the user will give broad directions and claude will write the plan. Either way we need to have a feedback intake mode that is used when a new project needs a user-written design document of some kind to get started with. When a new piece of feedback is being created add a parameter that will mark the resultant feedback document as a plan document for a discrete build of a project. This means that if I create a new planning document and send it to an existing project, claude will determine what to do based on the content of both. It will process and optimize the content of the plan and then it will distribute the instructions where they belong. The planning document tag permits this feedback to be sent to a project that doesn't exist yet if that is what claude determines should happen. Claude should also recognize feedback with this planning document flag as a potential trigger for a versioning repackage of the project and the creation of a new version folder. The new version will be treated like a new project and given its own PLAN.md.

### Optimized Prompt

## Objective
Add a "planning document" mode to the feedback pipeline. When creating feedback, the user can flag it as a planning document. This flag changes how Claude processes it: the feedback is treated as a project design document that can create new projects, trigger version repackaging, or restructure existing project plans.

## Requirements
1. **Planning document flag on feedback creation** — when pressing `f` (new feedback), offer a toggle: `[p] plan mode` that marks the resulting feedback file with a `type: plan` metadata tag
2. **Plan-mode processing rules** (sent to Claude in the processing prompt):
   - Read the planning document as a comprehensive design/build specification
   - If the document describes a new project that doesn't exist: Claude MAY create the project directory, write CLAUDE.md and PLAN.md, set .scope based on the document's language
   - If the document targets an existing project: Claude reads the current PLAN.md and determines whether to merge, replace, or version the plan
   - If the content implies a major rewrite/restructure: Claude should trigger versioning — archive current build as vN, scaffold vN+1 with new PLAN.md derived from the planning document
   - The optimized instructions are still distributed to fb2p.md for each affected project
3. **Planning document metadata** — feedback files flagged as plans get frontmatter: `type: plan` (vs default `type: feedback`). This metadata travels through the pipeline
4. **New project creation permission** — ONLY plan-mode documents can create new project directories. Regular feedback must NEVER create projects (existing constraint)
5. **Version trigger detection** — when Claude sees a plan document targeting an existing project, it evaluates: "Does this describe a fundamentally new version, or incremental changes?" If new version, run the versioning pipeline (archive → scaffold → new PLAN.md)
6. **Confirmation overlay update** — the feedback confirmation overlay shows the plan flag status and warns about destructive actions (new project creation, versioning repackage)

## Constraints
- Regular feedback (non-plan) retains all existing restrictions: no project creation, no versioning triggers
- Plan documents still go through the confirmation overlay — user sees what Claude intends to do before execution
- The flag is a property of the feedback file, not the processing session — it persists in the .status.json metadata
- Version archiving must use the same logic as /versioning-init (exclude .git, node_modules, venv, etc.)

## Technical Decisions
- Metadata field in feedback frontmatter: `type: plan` or `type: feedback` (default)
- The FeedbackStatus enum does NOT change — plan is orthogonal to draft/routed status
- Plan-mode prompt template is a separate string constant from the regular processing prompt
- The `spawn_feedback_processor` function checks the plan flag and uses the appropriate prompt

## Open Questions
- None — user intent is clear. Plan documents are the mechanism for creating and versioning projects through the feedback pipeline.

## Acceptance Criteria
- User can flag feedback as a planning document during creation
- Plan-mode feedback can create new project directories (regular feedback cannot)
- Plan-mode feedback can trigger version repackaging when targeting existing projects
- Confirmation overlay clearly shows plan-mode status and potential destructive actions
- The metadata tag persists through the pipeline and is visible in the Feedback tab

### Status
Generated: 2026-03-29
Executed: 2026-03-29 — Plan mode implemented. p key toggles in Feedback tab and confirmation overlay. Plan-mode Claude prompt allows project creation + versioning triggers. Regular feedback prompt unchanged. 📋 badge on plan items. 44/44 tests.

---

## Entry: 2026-03-31 — plan document (beta redesign → agent orchestration platform)

### Raw Input
Full redesign_plan document: "ORRCHESTRATOR BETA REDESIGN — A full service AI powered software development hypervisor that unifies AI workflow and enables node-based corporate emulation models for AI agents."

Key concepts from the redesign:
- 4 departments of AI agents (Admin, Development, Marketing, Legal) with 19 defined roles
- Workforce templates as node graphs: agents + connections + operation step sequences
- Operation modules with trigger/blocker/interrupt/step pipelines
- Multi-provider AI (all major distributors + Ollama)
- Token optimization via layered abstraction/recompilation
- Node-based visual workforce designer (may need native window)
- 4 panels: Design (Project Design + Workforce Design), Oversee, Hypervise, Library
- Input type classification: instructions, plans, ideas, knowledge — each routed to a type-specific workforce
- Dynamic API usage throttling via Intelligence Resources Manager

### Optimized Prompt

## Objective
Transform orrchestrator from a session manager (1.0.0) into an agent orchestration platform (2.0.0). The redesign adds agent profiles, workforce templates, operation module pipelines, multi-provider AI, a component library, and a visual workflow designer — all while preserving the existing Rust/ratatui codebase and building incrementally.

## Deliverable
A fresh PLAN.md with:
- 8 open questions that gate specific phases
- 8 development phases from 1.1.0 → 2.0.0
- 43 roadmap items + 2 carried forward
- Architecture docs: crate structure, department hierarchy, operation module schema, backend config, key workflows
- SemVer versioning throughout (no v1/v2 labels)

## Phase Summary
- Phase 0 (1.1.0): Foundation prep — panel restructuring, 3 new crates, config migration
- Phase 1 (1.2.0): Agent framework — profiles, library, execution binding, COO optimizer
- Phase 2 (1.3.0): Workforce templates — data model, built-ins, selector, grouped sessions
- Phase 3 (1.4.0): Operation modules — step engine, INSTRUCTION INTAKE, DEVELOP FEATURE
- Phase 4 (1.5.0): Multi-provider — Ollama, raw APIs, usage monitoring, throttling, token optimization
- Phase 5 (1.6.0): Library — storage, panel UI, MCP server, AI-assisted creation, Mentor auto-assignment
- Phase 6 (1.7.0): Node-based designer — workforce format, TUI node list, nesting, import/export
- Phase 7 (1.8.0): Native window mode — window spawning, visual node editor, non-TUI mode
- Phase 8 (2.0.0-rc): Intake workforces — instruction/plan/idea/knowledge processing pipelines

## Open Questions (must resolve before gated phases)
Q1: Versioning strategy — tag 1.0.0 and build incrementally, or archive and scaffold fresh?
Q2: Agent execution model — PTY per agent? shared context? inter-agent communication?
Q3: Workforce step execution — parallel branches? blocker polling? data handoff format?
Q4: Native window mode — TUI-only? egui? web-based? defer?
Q5: Ollama integration — PTY interactive mode? HTTP API wrapper? API-only?
Q6: Token optimization specifics — which layers get compressed, with what strategies?
Q7: Library storage — flat files? SQLite? git-backed? auto vs manual tool assignment?
Q8: Workforce definition format — YAML/TOML? structured markdown? TUI wizard?

### Status
Generated: 2026-03-31
Executed: 2026-03-31 — PLAN.md rewritten. 8-phase roadmap with SemVer milestones. 8 open questions flagged. Awaiting user resolution of open questions before Phase 0 begins.

---

## Entry: 2026-03-31 — feedback (Interactive Dev Map + AI Tooling Management)

### Raw Input
Two major features:
1. Interactive dev map in Oversee panel — Plan.md syntax parser that tracks features through states (planned → implementing → tested → verified → user-confirmed). Reorder features, add via popup, quick-spawn sessions, diff log persistence, user verification tracking, direct PM interaction, git commit grouping display.
2. AI tooling management in Library — model registry with pricing/capabilities/tiers, three-tier workforce profiles (enterprise/mid-tier/local), Resource Optimizer agent (assesses task complexity, annotates plan with model+harness suggestions), harness registry (Claude Code/OpenCode/Crush/Codex/Gemini), mixed-model workflows, Mentor periodic resource updates.

Also: new Resource Optimizer agent injected into DEVELOP FEATURE module after instruction ingest (step 2), before PM delegates. Repository Manager now reviews PM's commit packaging (step 7).

### Optimized Prompt

## Objective
Two cross-cutting feature sets that span multiple existing phases.

**Feature 1: Interactive Dev Map** — transform the Oversee project detail view into a live development tracking interface. Plan.md is parsed into a structured feature tree with visual state tracking, reordering, quick-spawn, diff persistence, and user verification. 9 roadmap items (44-52).

**Feature 2: AI Tooling Management** — add model/harness registries to the Library, three-tier workforce micromanagement profiles, a Resource Optimizer agent that annotates tasks with cost-optimal model assignments, and Mentor-driven resource updates. 6 roadmap items (53-58).

## Changes Made
- Resource Optimizer agent profile created (`agents/resource_optimizer.md`)
- AgentRole::ResourceOptimizer added to department hierarchy (21 roles total)
- DEVELOP FEATURE operation updated: Resource Optimizer at step 2, Repository Manager commit review at step 7
- 15 new roadmap items added to PLAN.md (items 44-58)
- PLAN.md department hierarchy updated

### Status
Generated: 2026-03-31
Executed: 2026-03-31 — Agent profile, department hierarchy, operation module, and PLAN.md all updated. Feature implementation is queued in roadmap.

---

## Entry: 2026-03-31 — feedback (Library priority: model + harness management + MCP)

### Raw Input
1. Library panel is top priority. Model and harness management via APIs/logins.
2. MCP server management page in Library. Question: one monolithic server vs specialized array?

### Optimized Prompt
Build out the Library panel as the immediate priority. Four sub-panels: Agents, Models, Harnesses, MCP Servers. Models include tier (enterprise/mid-tier/local), pricing, capabilities, API keys. Harnesses include availability detection, supported models, flags. MCP architecture: specialized servers (orrch-tools, orrch-research, orrch-library, orrch-project).

### Changes Made
- Library panel restructured with 4 sub-panels (LibrarySub enum, Shift+Tab navigation)
- Model registry: ModelEntry struct with tier, pricing model, capabilities, limitations, max context, API key env
- Harness registry: HarnessEntry struct with auto-detection (which), supported models, flags
- 8 model seed files (Claude Opus/Sonnet, Gemini Pro/Flash, Mistral Large/Codestral/Local, GPT-4o)
- 5 harness seed files (Claude Code, Gemini CLI, Crush CLI, OpenCode, Codex CLI)
- Preview pane shows full details per item type
- MCP sub-panel is placeholder awaiting user confirmation on server architecture

### Status
Generated: 2026-03-31
Executed: 2026-03-31 — Library panel fully functional with 4 sub-panels. 13 seed files. 81 tests pass. MCP architecture question pending user answer.
