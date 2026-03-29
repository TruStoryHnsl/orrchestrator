# Orrchestrator — Master Development Plan

A visual CLI application that serves as an AI development pipeline hypervisor — managing multiple parallel Claude Code instances, tracking project progress, and providing a unified interface for the user's entire development workflow.

## Open Conflicts
None yet.

## Architecture

### Core Concept
A terminal-based application (fully usable over SSH) with two main views:
1. **Dashboard view** — Project tracker showing planned/in-progress/done tasks across all projects, with live progress from parallel Claude Code instances
2. **Editor view** — Vim-based file editor with custom filesystem browser for the projects folder

### Hard Requirements
- **SSH-first**: Must be fully usable over a remote SSH session — no GUI dependencies, no Wayland/X11 required
- **Terminal multiplexing**: Uses PTY allocation to embed real terminal sessions (not just output capture)
- **All managed Claude sessions run with `--dangerously-skip-permissions`** by default
- **Unmanaged session awareness**: Detects Claude Code processes already running on the system that were NOT spawned by orrchestrator. These appear in the dashboard as "external" sessions and can be focused on click/select.
- **Focus/maximize**: Any terminal pane (managed or external) can be maximized to full screen for direct user interaction (answering questions, providing input, reviewing output). Press a key to return to dashboard.
- **External session focus**: Clicking/selecting an unmanaged Claude session centers the user's focus on that desktop window (if running locally) or brings that PTY to foreground (if over SSH)

### Key Components

**Process Manager (Hypervisor)**
- Spawns Claude Code sessions in managed PTYs: `claude --dangerously-skip-permissions -p "<project-dir>"`
- Each project can have multiple parallel feature pipelines running simultaneously
- Monitors session output for progress signals (task completion, errors, questions)
- Detects when a session is **waiting for user input** and surfaces it in the dashboard
- Manages the feedback intake queue across projects
- Discovers unmanaged Claude Code processes via `pgrep -f claude` and maps them to their working directories
- Tracks all sessions (managed + external) in a unified process table

**Dashboard Display (Project-Centric)**
The primary view is a **project tracker**, not a session list. Sessions are subordinate to project goals.
- **Project list** — default view on launch. Shows all `~/projects/` directories with:
  - Goal count from PLAN.md (planned/in-progress/done)
  - Active session count and their states
  - Scope badge (private/public/commercial)
  - Queued fb2p.md prompt count
- **Project detail** — Enter on a project to expand: PLAN.md roadmap checklist, active sessions with assigned goals, queued prompts, retrospect stats
- **Session spawn flow** — press `n`: pick project → enter goal (free-form or pick from roadmap) → pick backend → session spawns with goal as initial prompt
- **Session-goal binding** — every session has an assigned goal string, displayed in table and info panel
- Live status of each session: `working`, `waiting` (yellow highlight), `idle`, `dead`, `external`

**Feedback Editor (External Vim)**
- Press `f` from any view to open real vim in a new terminal window
- If no display server available (SSH), vim runs in the same terminal (TUI suspends, restores on exit)
- Terminal detection: checks `$TERMINAL`, then alacritty → kitty → konsole → gnome-terminal ��� xterm
- On save: feedback is saved to `~/projects/.feedback/<timestamp>.md` as a **draft**
- Drafts are managed in the **Feedback tab** — user reviews and submits from there
- Direct project feedback (e/f in project detail) routes immediately to that project's fb2p.md

**Feedback Pipeline Tab**
- Third top-level tab (Ideas | Projects | Feedback)
- Shows all feedback items grouped by status: Drafts and Routed
- Drafts: saved but not submitted — can resume editing (r), submit (s/Enter), or delete (d)
- Routed: submitted and distributed — shows target projects
- Pipeline status tracked in `.feedback/.status.json`
- Submit action routes feedback to projects, shows routing summary, offers to spawn sessions

**Default Session = "continue development"**
- The default spawn (pressing `n` → Enter with empty goal) uses the prompt "continue development"
- This is deliberately the most powerful prompt in the workflow: each project's CLAUDE.md defines what "continue development" means (read PLAN.md, check fb2p.md queue, execute next task)
- The feedback pipeline keeps PLAN.md and fb2p.md current, so "continue development" always has fresh context
- Custom goals remain available via typing or Tab-to-pick-from-roadmap, but the empty-goal fast path is the primary workflow

**Feedback Pipeline Integration**
- Write feedback in editor → save → route to project(s) → append to fb2p.md → offer to spawn sessions
- Each project's fb2p.md is the intake queue for development prompts
- "continue development" sessions read the queue and execute pending prompts
- The pipeline is: user writes feedback → orrchestrator processes → Claude sessions consume

**Troubleshooting Protocol Engine (Retrospect)**
The system continuously learns from development failures to prevent recurrence:

- **Error capture**: Session output is parsed for errors, stack traces, and failure patterns. Each error is fingerprinted (normalized message + context) and stored in a per-project `.troubleshooting.md` protocol file.
- **Solution tracking**: When an error is encountered and subsequently resolved within the same session (or a follow-up session in the same project), the resolution is paired with the error fingerprint. Solutions include: the fix applied, files changed, and the root cause category.
- **Retrospective analysis**: A background process periodically analyzes dev logs (`fb2p.md`, session output history, git diffs) across all projects to extract patterns:
  - Recurring error classes (e.g., "gevent thread confusion", "CIFS blocking I/O", "Textual API drift")
  - Time-to-resolution trends — which error types take longest to fix?
  - Cross-project patterns — does the same class of bug appear in multiple projects?
- **Protocol generation**: Analysis results are distilled into actionable troubleshooting protocols written to `<project>/.troubleshooting.md`. These are structured as decision trees: symptom → likely causes → known fixes → escalation steps.
- **Active injection**: When a managed session encounters a known error pattern, orrchestrator can optionally inject the troubleshooting protocol into the session's context (via the Claude Code prompt) before the user even notices — enabling self-healing behavior.
- **Cross-project knowledge**: Protocols from one project can inform others. A shared `~/projects/.troubleshooting-global.md` captures patterns that transcend individual projects (e.g., "Python 3.14 compatibility issues", "Textual widget API changes").

This is NOT just a bug ledger (`.bugfix-ledger.md` already exists for that). This is an automated, continuously-running analytical process that turns raw failure data into structured institutional knowledge.

### Session Management Detail

**Managed sessions (spawned by orrchestrator):**
- Launched in a PTY with full terminal emulation
- Output is captured and parsed for status signals
- Input is forwarded from the user when the session is focused/maximized
- Can be paused, resumed, killed from the dashboard
- Always run with `--dangerously-skip-permissions`

**External sessions (discovered on system):**
- Found via process scan: `pgrep -af 'claude'` to find PIDs and working directories
- Displayed in the session list with an `[external]` badge
- Cannot be killed or controlled by orrchestrator
- Focus action: over SSH, displays a notice with the PTY path so user can attach; locally, uses `wmctrl` or equivalent to raise the window

### Technical Stack
- **Rust + Cargo** — native Linux binary, matching concord v2 toolchain
  - `ratatui` + `crossterm` for TUI (replaces Python/Textual)
  - `tokio` async runtime for PTY management and process spawning
  - `nix` crate for Unix process control (fork, ioctl, signals, PTY)
  - `serde` + `serde_json` for JSONL error store and config
  - `sha2` for error fingerprinting
  - `regex` for error pattern matching
- **Cargo workspace** — modular crate structure:
  - `crates/orrch-core` — process manager, session types, PTY handling
  - `crates/orrch-tui` — ratatui dashboard, focus screen, project picker
  - `crates/orrch-retrospect` — error parser, fingerprinting, store, solution tracker
  - Root binary crate ties them together
- **Development workflow**: `cargo watch -x run` for live-reload, `cargo test` for testing
- **Distribution**: `cargo build --release` → single static binary, no runtime deps
- Must run on: orrion (CachyOS/Alacritty), any SSH client (PuTTY, macOS Terminal, etc.)
- No GUI toolkit dependencies — pure terminal
- Python prototype (`orrchestrator/` package) preserved as design reference

### Multi-Backend AI Execution

Orrchestrator is not limited to Claude Code. It supports multiple AI CLI backends as **processing protocols**. Each managed session specifies which backend to use. The hypervisor treats them uniformly — same PTY management, same focus/maximize, same output parsing.

| Backend | Command | Default Flags | Use Case |
|---------|---------|---------------|----------|
| **Claude Code** | `claude` | `--dangerously-skip-permissions` | Primary dev agent — code generation, architecture, debugging |
| **Gemini CLI** | `gemini` | (configured per-node) | Alternative reasoning, second opinions, parallel exploration |

**How it works:**
- When spawning a session, orrchestrator checks which backends are available on the current node (`which claude`, `which gemini`)
- The user (or the task queue) specifies which backend to use: `orrch new --backend gemini concord`
- Default backend is Claude Code. Gemini is opt-in per task.
- Both backends get the same project context (PLAN.md, fb2p.md, CLAUDE.md)
- Output from either backend is parsed for the same signals (task completion, errors, waiting-for-input)
- The session list shows backend with a badge: `[claude]` or `[gemini]`

**Gemini CLI setup:**
- Must be installed and authenticated on each node that uses it
- Orrchestrator does NOT install Gemini — it discovers it if present
- Config at `~/.config/orrchestrator/backends.yaml`:
  ```yaml
  backends:
    claude:
      command: claude
      flags: ["--dangerously-skip-permissions"]
      available: true  # auto-detected
    gemini:
      command: gemini
      flags: []
      available: true  # auto-detected
  ```

**Future backends:** Any AI CLI that runs in a terminal and accepts text input can be added as a backend. The protocol is: spawn in PTY, send prompt text, parse output.

### Keybindings (default)
| Key | Action |
|-----|--------|
| `1` | Switch to Dashboard view |
| `2` | Switch to Editor view |
| `Enter` | Maximize/focus selected session |
| `Esc` | Return to dashboard from maximized view |
| `n` | Spawn new Claude Code session (prompts for project) |
| `k` | Kill selected managed session |
| `f` | Open vim for new feedback (new terminal window or same terminal over SSH) |
| `r` | Refresh process scan (discover external sessions) |
| `q` | Quit orrchestrator |

## Feature Roadmap

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
29. [x] **Project file browser** — split-pane browser (b key), preview pane for text/metadata, Enter to edit/navigate, Backspace to go up
30. [x] **Deprecated browser** — read-only file browser for archived projects (d key), same split-pane layout
31. [x] **PLAN.md detection fix** — case-insensitive scan, finds plan.md/PLAN.md/DEVELOPMENT_PLAN.md, falls back to CLAUDE.md for descriptions
32. [x] **Project metadata line** — shows CLAUDE.md | Cargo.toml | master plan | v2 | Docker etc. under each project
27. [x] **Smart default actions** — "create plan" / "run queued" / "continue dev" shown per project state
28. [x] **"Personal" scope** — new scope level below private for user-only projects
33. [x] **Feedback pipeline tab** — new "Feedback" tab showing drafts/routed items with status, submit/resume/delete actions, routing target display
34. [x] **External vim replacement** — replaced custom inline editor with real vim spawned in new terminal windows (auto-detects $TERMINAL/alacritty/kitty/konsole)
35. [x] **Feedback lifecycle** — drafts saved to .feedback/, status tracked in .status.json, submit routes to projects, delete cleans up

## Recent Changes
- 2026-03-29: ROADMAP COMPLETE. All 35 features implemented. Final push: (1) Retrospect engine: analyzer.rs mines error stores across all projects for recurring patterns, cross-project trends, resolution rates. protocol.rs generates per-project .troubleshooting.md decision trees and ~/projects/.troubleshooting-global.md cross-project KB. Runs every 10min in background. (2) Feedback routing made suggestive — Claude decides final destinations after analysis. (3) Feedback processor fix — runner script approach instead of $(cat) shell expansion that garbled prompts. (4) Tree browser replaces three-column layout. (5) Feedback metadata YAML frontmatter. 44/44 tests.
- 2026-03-29: Major UX overhaul: (1) CRITICAL FIX — removed all KWin/qdbus window management that crashed Plasma desktop. (2) Session management migrated from Alacritty windows to tmux — `spawn_tmux_session()` creates named windows in "orrch" tmux session. (3) Context action menu (`a` key) — popup with all actions for selected item, replaces 15+ scattered hotkeys. (4) New project wizard (`P` key) — 3-step: name → scope → confirm, auto-spawns plan session. (5) Scope editing (`S` key) — cycles personal/private/public/commercial. (6) Styled status bar — grouped hints with accent-colored keys and `│` separators. (7) Feedback confirmation overlay — shows auto-detected routing with toggleable project checkboxes, spawns Claude tmux session for real processing. (8) Smart routing — word-boundary matching with scoring (orr-prefix boost, ambiguity penalties, context signals). (9) Recovered misrouted feedback — deleted bad fb2p.md entries from notes/admin/claude, restored originals to draft. (10) Deprecated project deletion (`d` key in deprecated browser). (11) Remote sessions show `@hostname` badges. (12) All projects expanded by default. 44/44 tests.
- 2026-03-28: Parallel feature pipelines + remote session fix. (1) Parallel pipelines: `pipelines_for_project()` groups sessions by goal, duplicate-goal warnings in spawn overlay and session table, pipeline count badge (⊞) in project rows, Shift+N multi-spawn for all open roadmap items. (2) Remote session starter: orrch-agent.sh cross-platform agent (piped via SSH stdin, works on Linux+macOS), SSH target fixed for orrpheus (`coltonorr@orrpheus`), macOS discovery via `lsof`/`ps` instead of `/proc`, auto-detects tmux/screen/nohup for session management, initial capability probe on startup. 38/38 tests, 5.8MB binary.
- 2026-03-28: Bugfix session: (1) Panic hook added to main.rs — terminal now recovers on crash instead of locking up permanently. (2) Clipboard functions rewritten to shell out to wl-paste/wl-copy (Wayland) with xclip/arboard fallback — arboard's direct Wayland client was unreliable. (3) UTF-8 char boundary crash fixed in editor.rs — cursor_col used byte offsets but moved ±1 per keystroke, causing panics on multi-byte characters (em-dashes, smart quotes from Claude output). Added prev_boundary/next_boundary/snap_boundary helpers for all cursor movement.
- 2026-03-28: External vim replacement — custom inline editor replaced with real vim spawned in new terminal windows. Feedback pipeline tab added.
- 2026-03-26: Initial plan created from user feedback in general admin session
- 2026-03-26: Added SSH-first requirement, external session discovery, session maximize/focus, --dangerously-skip-permissions default, keybindings, Textual as primary tech stack candidate
- 2026-03-26: v0.1.0 — initial implementation: ProcessManager (PTY spawn/kill/monitor), TerminalView (pyte-based ANSI rendering), Dashboard (session table + info panel + project picker), FocusScreen (full-screen session with keyboard forwarding), external session discovery via pgrep
- 2026-03-26: Added Troubleshooting Protocol Engine ("Retrospect") to architecture — automated error capture, solution pairing, retrospective analysis of dev logs, protocol generation, active injection into sessions, cross-project knowledge base
- 2026-03-26: Implemented Retrospect capture layer — error parser (regex-based, 12 patterns), fingerprinting (SHA256 of normalized text), classification (13 categories), JSONL error store, solution tracker (cooldown-based resolution detection), SessionAnalyzer wired into app output pipeline. Dashboard shows Retrospect stats per session.
- 2026-03-26: **MAJOR**: Rust rewrite. Python/Textual prototype archived as design reference. New stack: Rust + ratatui + tokio + cargo workspace (matching concord v2 patterns). Native Linux binary distribution. `cargo watch -x run` for live dev, `cargo test` for testing.
- 2026-03-27: Rust build complete. 4 crates (orrch-core, orrch-retrospect, orrch-tui, orrchestrator binary). 17/17 tests pass. 4.7MB native binary. cargo-watch installed. Full feature parity with Python prototype.
- 2026-03-27: Feature push: TTY guard, multi-backend, project picker, waiting-for-input detection, live progress parsing, active injection. 26/26 tests, 4.8MB binary.
- 2026-03-27: Architecture shift: project-centric tracker replaces session-centric dashboard as primary workflow. Session spawn flow now: project → goal → backend. Sessions are subordinate to project goals.
- 2026-03-27: Implemented: Project data model + PLAN.md parser, session goals, project-centric dashboard, 3-step spawn wizard. 32/32 tests.
- 2026-03-27: Feedback editor + routing pipeline. Default spawn = "continue development". 41/41 tests.
- 2026-03-27: Major UX overhaul (12-item batch): inline editor, master plan append, ideas vault, color tags, expandable rows, smart defaults, personal scope. 41/41 tests.
- 2026-03-27: PLAN.md detection fix (case-insensitive, multiple filenames, CLAUDE.md fallback). Project metadata line. Next-task contrast bumped to white. 44/44 tests.
- 2026-03-27: File browser, deprecated browser, external session mapping. 44/44 tests.
- 2026-03-27: 5-panel system, symlink exclusion, color audit, file browser, production panel, vim panel.
- 2026-03-27: Tab nav fix (Up-at-top to focus tab bar). Two-column browser on all panels. Session panel display-only. Vim return-to panel. External discovery every 5s with better filtering.
- 2026-03-27: Projects panel restructured: Hot (active)/Cold (parked)/Facilities (admin hyperfolder + deprecated link). Mark complete (C → v1 packaging → Production). Deprecated panel removed, now opened via g key from Facilities. 4 panels: Ideas → Projects → Production → Vim. 44/44 tests.
