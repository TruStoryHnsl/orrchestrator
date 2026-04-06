# Orrchestrator Development Log

## Dev Session: 2026-04-06 — Items 49 + 50 (Diff Log + User Verification)

### Completed
- **49. Diff log persistence** — New `orrch-core::diff_log` module exposing `DiffEntry`, `append_diff`, `load_diffs`, `load_all_diffs`, and `diff_log_path`. Append-only JSON storage at `<project>/plans/.diff_log.json` keyed by `feature_id → Vec<DiffEntry>`. Timestamps via existing `chrono_lite_timestamp()`. Standalone API — auto-hook into plan flips deferred (needs state diffing infra). 2 unit tests cover round-trip + multi-feature load.
- **50. User verification tracking** — `PlanFeature` gains `user_verified: bool` field, set to `true` when the parser encounters a `[v]` marker. New free function `mark_verified_in_plan(plan_path, feature_title) -> io::Result<bool>` performs a byte-precise rewrite of a single `[x]` to `[v]` matching the trimmed title. 2 new tests verify parse + idempotent rewrite.
- **TUI rendering** — `draw_dev_map` in orrch-tui appends three conditional badges per feature row: `✓` (GREEN) when verified, `+N` (CYAN) when diff entries exist, `●N` (TEXT_MUTED) when matching git commits exist. `lookup_id` falls back from `feat.id` to `feat.title`.
- **TUI keybinding** — `key_detail_devmap` binds `V` (uppercase) to call `mark_verified_in_plan` then reload the project plan. Silently no-ops on phase headers, missing plan file, or write errors.
- **52 partial: git commit display** — `orrch-core::git` exposes `FeatureCommit { sha, subject }` and `commits_for_feature(project_dir, feature_id)`. Spawns `git -C <dir> log --oneline -n 50 --fixed-strings --grep <id>`, returns up to 3 entries, empty on any failure. Smoke test asserts no panic on nonexistent dir. **The Repository Manager advisory loop on PM grouping is NOT implemented — only the display side. Item 52 stays unchecked.**

### Verification
- 2 isolated tester subagents (orrch-core and orrch-tui scopes) independently verified PASS with no issues
- `cargo build` workspace: PASS
- `cargo test -p orrch-core`: 68 passed (+4 new), 0 failed
- `cargo clippy`: 0 errors (pre-existing warnings only)
- API contract verified for all 4 tasks

### Files Changed
- `crates/orrch-core/src/diff_log.rs` — **new**: ~144 lines, DiffEntry + append/load API + 2 tests
- `crates/orrch-core/src/lib.rs` — registered `diff_log` module + re-exports
- `crates/orrch-core/src/plan_parser.rs` — `user_verified` field, `mark_verified_in_plan`, 2 tests
- `crates/orrch-core/src/git.rs` — +64 lines: `FeatureCommit`, `commits_for_feature`, smoke test
- `crates/orrch-tui/src/ui.rs` — `draw_dev_map` badge rendering (lines ~1600-1647)
- `crates/orrch-tui/src/app.rs` — `key_detail_devmap` V keybinding (lines ~2805-2823)

### Workflow
- Executed `develop_feature` MCP dispatch loop end-to-end
- 1 PM planning agent → 4 tasks → 2 file-clustered impl agents (parallel) → 2 isolated verifier agents (parallel) → 1 PM evaluator → PASS

---

## Dev Session: 2026-04-03 — INS-004 through INS-009 (UI Polish & Infrastructure)

### Completed
- **INS-004: Harnesses tab** — Added `WorkforceTab::Harnesses` as first tab in Design > Workforce. Lists harness entries with availability indicators (●/○). Preview shows name, command, repo URL, notes. Edit keys show "coming soon" notification.
- **INS-005: Rich markdown preview** — New `markdown.rs` module using `pulldown-cmark` parser. Renders headers (H1=accent+bold, H2=cyan+bold, H3=bold), bold, italic, code blocks (indented+muted), bullets, links, horizontal rules. Wired into Workforce editor, Library agents, and Library generic preview panes. 64KB input size cap with truncation notice.
- **INS-006: tmux session cleanup** — `kill_all_managed_tmux_sessions()` kills all orrch-* sessions on exit. Session state file (`managed-sessions.json`) tracks spawned windows. Startup orphan detection with notification. Wired to exit path in main.rs.
- **INS-007: Custom tmux status bar** — `orrch-tmux-status.sh` script queries window states and outputs W/I/A counts. Applied via `apply_custom_status_bar()` on session creation. F9 keybind for jump-to-window (currently first window, not most-urgent).
- **INS-008: Hub vim window** — `hub_vim_open()` checks for existing `hub-edit` window and opens files as vsplits. Sends Escape before `:vsp` to handle insert-mode case. `detect_split_off_editors()` tracks windows split off from hub. Split-off count shown in Intentions panel.
- **INS-009: Audit trail** — `AuditEntry` struct with `ChunkCoordinate` and SHA-256 hashing in new `audit.rs` module. JSONL persistence at `.feedback/audit.jsonl`. Intentions panel `i` key expands inline audit view with source, range, raw/optimized preview, hash prefix.

### Verification Findings Fixed
- UTF-8 byte boundary panic in audit display (char-based truncation)
- `vim_ex_escape` now escapes `|`, `#`, `%`, `\n`, `\r` (command injection prevention)
- `hub_vim_open` sends Escape before `:vsp` (insert-mode safety)
- `detect_split_off_editors` excludes "hub" placeholder window
- `orrch-tmux-status.sh` fixed `-l 5` flag incompatibility with tmux 3.6a
- Markdown renderer capped at 64KB input (OOM prevention)

### Known Issues (deferred)
- Esc key in Intentions panel routes to AppMenu instead of collapsing audit trail (workaround: press `i` to toggle)
- F9 hotkey jumps to first window by index, not most urgent (Rust `jump_to_most_urgent()` exists but isn't wired to F9)
- Markdown preview and audit trail read from disk on every render frame (acceptable for current file sizes)
- `managed-sessions.json` has no file locking (single-user tool, TOCTOU race unlikely)
- Pre-existing: shell injection in `spawn_feedback_processor` and SSH remote command construction

### Files Changed
- `crates/orrch-tui/src/markdown.rs` — **new**: pulldown-cmark → ratatui Line renderer
- `crates/orrch-core/src/audit.rs` — **new**: AuditEntry, ChunkCoordinate, JSONL persistence, SHA-256 hashing
- `library/tools/orrch-tmux-status.sh` — **new**: tmux window status query script
- `crates/orrch-core/src/windows.rs` — hub vim, session cleanup, status bar, split-off detection, orphan detection
- `crates/orrch-tui/src/app.rs` — WorkforceTab::Harnesses, split_off_editors field, audit expansion state, orphan notification
- `crates/orrch-tui/src/ui.rs` — Harnesses tab rendering, markdown preview wiring, editor section in Intentions, audit trail expansion
- `crates/orrch-tui/src/lib.rs` — markdown module registration
- `crates/orrch-core/src/lib.rs` — audit module registration
- `crates/orrch-core/Cargo.toml` — sha2 dependency
- `crates/orrch-tui/Cargo.toml` — pulldown-cmark dependency
- `Cargo.toml` — pulldown-cmark workspace dependency
- `src/src/main.rs` — tmux cleanup on exit, split-off editor refresh

### Next
- Wire F9 to Rust's `jump_to_most_urgent()` instead of simple `head -1`
- Fix Esc routing to allow audit collapse without opening AppMenu
- Add render-frame caching for markdown preview and audit trail I/O
- Address pre-existing shell injection findings (spawn_feedback_processor, remote.rs)

---

## Dev Session: 2026-04-03 — Critical Path CP-2/4/5/6 + Security Fixes

### Completed
- **CP-2 (Agent role skills)**: Created 7 new agent skill files for workflow-referenced agents (executive-assistant, resource-optimizer, feature-tester, penetration-tester, beta-tester, ui-designer, researcher). Total: 13 agent skills.
- **CP-4 (Skill invocation on spawn)**: Spawn flow now uses `/develop-feature <goal>` slash command instead of text hint to MCP tool. Solo-developer fallback preserved.
- **CP-5 (Hypervise live agent tree)**: Wired filesystem polling — TUI reads `.orrch/workflow.json` from active sessions' working directories and renders agent tree with step progress. Fixed visibility to show all workflow states (running, paused, failed, complete), not just running. Fixed color mapping: running=green, waiting=yellow.
- **CP-6 (Intake review wiring)**: Wired intake review polling — TUI reads `.orrch/intake_review.json` from project directories when viewing Intentions panel. Auto-populates review overlay when status is "pending".
- **Security fix**: Shell-escaped `flags_str` in remote spawn (remote.rs) — prevented potential command injection on remote hosts.
- **Tester isolation**: Removed Write/Edit from feature tester and tester skill files to enforce verification agent isolation.

### Failed / Deferred
- BUG-16: Session list polling performance with many sessions (needs throttle/cache — architectural change)
- BUG-17: Only first running workflow visible (needs Vec<WorkflowStatus> — structural change)
- BUG-23: 8 non-workflow agents still lack skill files (hypervisor, IRM, mentor, talent_scout, specialist, ux_specialist, market_researcher, licensing_auditor, copyright_investigator)
- Finding-2: Shell quoting in orrch-agent.sh (bash script outside Rust codebase)

### Known Issues
- Intake review and workflow status polling happens on every render frame (synchronous I/O in render path). Acceptable for <20 projects but will need throttling for larger workspaces.
- Duplicate agent skill files: `agent-tester.md` and `agent-feature-tester.md` both reference the same profile. The older `agent-tester.md` is kept for backward compatibility.

### Files Changed
- `crates/orrch-tui/src/app.rs` — spawn flow changed to `/develop-feature` slash command
- `crates/orrch-tui/src/ui.rs` — workflow status polling, intake review polling, visibility fix, color fix
- `crates/orrch-core/src/remote.rs` — shell-escaped flags in remote spawn
- `~/.claude/commands/agent-{executive-assistant,resource-optimizer,feature-tester,penetration-tester,beta-tester,ui-designer,researcher}.md` — new files
- `~/.claude/commands/agent-{tester,feature-tester}.md` — removed Write/Edit tools

### Next
- CP-3: More tool scripts (git commit packaging, version tagging)
- CP-7 polish: Add `session_list` and `operation_status` MCP tools
- CP-1 validation: Live end-to-end test of develop-feature and instruction-intake workflows
- INS-004 through INS-009: UI polish items from instructions inbox
