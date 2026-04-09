# Orrchestrator Development Log

## Dev Session: 2026-04-08 — Item 36 (Workforce import/export)

### Completed
- **36. Workforce import/export** — Added the serializer side of the workforce markdown format so workforce designs can round-trip to disk and back. Three-cluster sequential implementation:
  - **Cluster 1** (`crates/orrch-workforce/src/parser.rs`, `lib.rs`): `pub fn serialize_workforce_markdown(&Workforce) -> String` emits frontmatter (name/description/operations) + `## Agents` and `## Connections` pipe tables structurally identical to what `parse_workforce_markdown` consumes. Helper `data_flow_token(&DataFlow) -> &'static str` mirrors `parse_data_flow` across all 5 variants (Instructions/Deliverable/Report/Research/Message). Empty `operations` vec emits `operations: []` to avoid an unterminated list in frontmatter. Uses `std::fmt::Write`. Re-exported from `lib.rs`.
  - **Cluster 2** (`crates/orrch-workforce/src/engine.rs`): `pub fn export_workforce_to_path(&Workforce, &Path) -> io::Result<()>` and `pub fn import_workforce_from_path(&Path) -> io::Result<Workforce>`. Signatures use `std::io::Result` (no `anyhow` dep in this crate). Export overwrites via `fs::write`. Import maps parser `None` → `io::Error::new(InvalidData, ...)` with path context; file-not-found and permission errors also wrapped with path.
  - **Cluster 3** (`crates/orrch-tui/src/app.rs`, `ui.rs`): `x` key in Design > Workforce > Workflows tab exports the selected workforce to `~/Downloads/<sanitized_name>.md` (sanitizer: spaces → `_`, filter to ASCII alnum/`-`/`_`, fallback "workforce"). Uses existing `self.notify()` toast for success/failure. Creates `~/Downloads` if missing via `create_dir_all`. `i` key reads `~/Downloads/import.md` (Option B — hardcoded path, no file picker, private-scope-appropriate), parses, and pushes into both `workforce_files` (sorted) and `loaded_workforces` so it renders immediately. Full `orrch_workforce::engine::` path used (functions not re-exported from workforce `lib.rs`). Help line in `ui.rs` updated with `x=export i=import` on the Workflows tab title. HOME via `std::env::var("HOME")` with `/home/corr` fallback, no new deps.

### Verification
- **Tester A** (workforce crate, independent): PASS. `cargo build -p orrch-workforce` zero warnings. `cargo test -p orrch-workforce` 17/17 (3 new round-trip tests + 2 error-path tests). Inspected parser.rs:186-194 (all 5 DataFlow variants covered), parser.rs:201-244 (serializer emits correct structure), engine.rs:178-189 (export), engine.rs:197-215 (import). Tests use `env::temp_dir + pid + nanos` → parallel-safe. Spot-checked `workforces/personal_tech_support.md`: layout matches serializer output.
- **Tester B** (TUI integration, independent): PASS. `cargo build` (full workspace, debug) exits 0. 11 pre-existing warnings, zero new. Key handlers gated on `workforce_tab == WorkforceTab::Workflows` at app.rs:1787 and 1818. No `x`/`i` conflicts — other bindings are in `key_ideas`/`key_projects`/`key_inside_project`/`key_detail_sessions`/`key_sessions_tab`, all separate panels. Help line at ui.rs:398 conditional on Workflows tab.
- **PM verdict** (hypervisor inline): PASS. Both testers unanimous. Issues flagged are minor, non-blocking under private scope: (a) serializer doesn't escape `|`/newlines inside fields (not exercised by current workforce files), (b) pre-existing parser fragility at parser.rs:130 (substring match on "ID"/"From" — not introduced by this diff), (c) import path hardcoded (documented as Option B), (d) `wf_selected` may drift after import+sort (non-panic), (e) `create_dir_all` errors silently ignored but subsequent `export_workforce_to_path` surfaces the real failure. None block ship.

### Known follow-ups (non-blocking)
- Import path is a hardcoded `~/Downloads/import.md` (no file picker). Upgrade to a text-input SubView later when a richer flow is needed.
- `.md` extension is not forced by engine-layer export; the TUI sanitizer appends `.md`, so in practice every file lands correctly.
- Serializer field escaping (`|`, newlines) unneeded today but worth adding when a workforce name/description legitimately contains those characters.
- Minor cursor drift on `wf_selected` after import+sort — cosmetic only.

### Files Changed
- `crates/orrch-workforce/src/parser.rs` — `serialize_workforce_markdown`, `data_flow_token`, 2 round-trip tests
- `crates/orrch-workforce/src/lib.rs` — re-export of serializer
- `crates/orrch-workforce/src/engine.rs` — `export_workforce_to_path`, `import_workforce_from_path`, 3 tests (round-trip + 2 error paths)
- `crates/orrch-tui/src/app.rs` — `x`/`i` key handlers at key_design_workforce:1787-1833, sanitizer, create_dir_all, notify integration
- `crates/orrch-tui/src/ui.rs` — Workflows tab help line at 398
- `PLAN.md` — item #36 flipped to `[x]` with implementation note

### Workflow
- Executed `develop_feature` MCP dispatch loop end-to-end
- 1 PM planning agent (chose item #36 from 21 unchecked) → 3 clusters in sequential waves (T1 serializer → T2 file I/O → T3 TUI keybinds, dependency chain) → 2 independent verifier subagents in parallel (both PASS) → inline PM evaluation → SHIP
- Zero rework cycles (cap is 3)
- Token efficiency: each cluster was a self-contained subagent spawn, main context never loaded the full 4921-line `app.rs` or the workforce crate source

---

## Dev Session: 2026-04-09 — Instruction Inbox Migration (fb2p.md → instructions_inbox.md)

### Completed
- **Instruction inbox migration** (Carried Forward item) — migrated all live filesystem references from `fb2p.md` to `instructions_inbox.md` across the codebase. `crates/orrch-core/src/feedback.rs`: renamed `append_to_fb2p` → `append_to_inbox` and `append_to_fb2p_direct` → `append_to_inbox_direct`, retained the old names as `#[deprecated]` wrappers forwarding to the new functions, switched the filesystem path and the initial-file header text to "Instructions Inbox", updated `save_and_route_feedback`/`submit_feedback` call sites, and fixed the `test_save_and_route` test. `crates/orrch-tui/src/app.rs`: 5 `.join("fb2p.md")` literals + 2 `append_to_fb2p_direct` call sites + local variable renames + 6 stale comments updated. `crates/orrch-tui/src/ui.rs`: user-visible message "saved to workspace fb2p.md" → "saved to workspace instructions_inbox.md". Rework pass caught a reader regression in `crates/orrch-core/src/project.rs` `count_queued_prompts()` (still pointed at `fb2p.md`, would report 0 queued prompts for migrated projects) — fixed. Non-blocking: `library/skills/agent-coo.md` ignore list updated so COO does not re-ingest its own inbox.

### Verification
- Tester A (independent): PASS across 7 checks — build clean, 90 orrch-core tests pass, no live `fb2p.md` literals in orrch-tui, deprecated wrappers in place with correct attributes, PLAN.md checkbox + changelog entry present, inbox header text correct.
- Tester B (independent): FAIL on first pass — found the `project.rs:832-841` reader regression. After rework: `grep fb2p crates/orrch-core/src/project.rs` returns 0 matches, `cargo test -p orrch-core` 90/90, `cargo build` clean.
- PM verdict: REWORK → SHIP. One rework cycle (within the 3-cycle cap).

### Known follow-ups (non-blocking)
- `library/skills/interpret-user-instructions.md` is an already-deprecated skill that still contains 9 active `fb2p.md` references inside its instructions. The whole skill is marked deprecated in CLAUDE.md; leaving as-is (do not edit deprecated skill bodies).
- No new unit test explicitly asserting `append_to_inbox` writes to `instructions_inbox.md` under a tempdir. `test_save_and_route` exercises the full `save_and_route_feedback` → inbox write path, so the behavior is covered indirectly.

### Files Changed
- `crates/orrch-core/src/feedback.rs` — function renames, deprecated wrappers, filesystem path, header text, test update
- `crates/orrch-core/src/project.rs` — `count_queued_prompts` now reads `instructions_inbox.md`, local `fb2p_path` → `inbox_path`
- `crates/orrch-tui/src/app.rs` — 5 path literals, 2 function calls, 6 comments, 2 local vars
- `crates/orrch-tui/src/ui.rs` — 1 user-visible message string
- `library/skills/agent-coo.md` — ignore list updated
- `PLAN.md` — `Instruction inbox migration` checkbox flipped to `[x]` with status note, Recent Changes entry added

### Workflow
- Executed `develop_feature` MCP dispatch loop end-to-end
- 1 PM planning agent → 3 tasks in 3 sequential waves (T1 feedback.rs → T2 tui → T3 PLAN.md done inline) → 2 independent verifier subagents in parallel (Tester A PASS, Tester B FAIL with regression) → 1 Developer rework → SHIP
- 1 rework cycle total (cap is 3)

---

## Dev Session: 2026-04-08 — Items 23 + 24 (Crush/OpenCode Backends + Raw API Backends)

### Completed
- **23. Ollama backend via Crush/OpenCode** — `BackendKind::Crush` and `BackendKind::OpenCode` were already wired into `BackendsConfig::default()` (commands `crush` / `opencode`), `detect_availability()` (via `which`), and `process_manager::spawn()` (same `to_provider()` → `cli_args()` PTY path as Claude/Gemini). This session verified the wiring end-to-end and added unit coverage: `test_crush_default_command`, `test_crush_availability_detection`, `test_crush_to_provider_routes_cli_pty`, plus extended `test_default_config` and `test_backend_labels` to assert Crush + OpenCode entries.
- **24. Raw API backends** — `AnthropicApi` and `OpenAiApi` now dispatch real HTTP requests instead of `bail!`-ing. Added in `crates/orrch-core/src/backend.rs`: `send_api_message(backend, model_id, prompt)` dispatcher, `http_client()` (reqwest blocking, 120s timeout), `send_anthropic()` (POST `https://api.anthropic.com/v1/messages` with `x-api-key` + `anthropic-version: 2023-06-01`, parses `content[].text`), `send_openai()` (POST `https://api.openai.com/v1/chat/completions` with bearer auth, parses `choices[0].message.content`). `is_provider_available()` now reports API backends as available iff their env var (`ANTHROPIC_API_KEY` / `OPENAI_API_KEY`) is set. `process_manager::spawn_api_oneshot()` runs the HTTP call on `tokio::task::spawn_blocking` and emits the response as `SessionEvent::Output` followed by `SessionEvent::Died`. Added `reqwest = { version = "0.12", features = ["json", "rustls-tls", "blocking"] }` to workspace Cargo.toml. Coverage: `test_all_backends_route_to_correct_provider_kind` (all 6 variants), `test_valve_blocked_overrides_all_backends`, `test_api_backend_env_var_availability` (single test using save/restore to avoid parallel races), `test_session_new_preserves_backend_for_all_variants` (in `session.rs`).

### Verification
- 1 build verifier (run inline due to API overload): `cargo check --workspace` clean, `cargo test -p orrch-core` 90 passed / 0 failed (up from 86 baseline — 4 new tests), only pre-existing dead-code warnings
- 1 contract verifier subagent: PASS on contract integrity, dependencies, and B1 backend coverage. Flagged minor follow-ups (logged below as known gaps) but explicitly marked severity as **minor — not blocking**

### Known follow-ups (logged in PLAN.md)
- **Hardcoded model_id**: `BackendKind::to_provider()` hardcodes `"claude-sonnet-4-20250514"` and `"gpt-4o"` for the API providers. The Library > Models system (`library/models/*.md`) is not yet wired through to the spawn path. Future work: source `model_id` from the active model selection in `ProviderConfig`.
- **Session-tracking asymmetry**: `spawn_api_oneshot` returns a sid but does not insert into `ProcessManager.sessions`. `get_session(sid)` will return `None` for API sessions; `write_to_session` / `kill_session` silently no-op. Currently fine because the API path is one-shot, but multi-turn API conversations will require a parallel session model.
- **Error surfacing**: HTTP errors (401 / 429 / network) are stringified into `"[error] {e}"` and emitted on the same `Output` channel as success responses. `SessionEvent::Died` has no exit-code field. Pre-spawn env-var check guards the happy path. Acceptable for `private` scope.

### Files Changed
- `Cargo.toml` (workspace) — added `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "blocking"] }`
- `crates/orrch-core/Cargo.toml` — `reqwest.workspace = true`
- `crates/orrch-core/src/backend.rs` — ~110 lines added (HTTP send functions, env-var availability checks, 8 new tests)
- `crates/orrch-core/src/process_manager.rs` — ~40 lines net (`spawn_api_oneshot` helper, removed `bail!` for API backends)
- `crates/orrch-core/src/session.rs` — new test module with `test_session_new_preserves_backend_for_all_variants`
- `PLAN.md` — items 23 and 24 marked `[x]` with status notes

### Workflow
- Executed `develop_feature` MCP dispatch loop end-to-end
- 1 PM planning agent → 4 tasks → 2 file-clustered Wave 1 agents (Developer for B1, Software Engineer for B2/B3, run sequentially due to shared central files `backend.rs` + `process_manager.rs`) → 1 Wave 2 agent (Feature Tester for B4) → 2 verifiers (1 inline build verifier, 1 isolated contract reviewer subagent) → SHIP_WITH_ISSUES (minor follow-ups documented, not blocking)
- Note: 2 verifier subagent attempts hit Anthropic API 529 (overloaded); fallback to inline `cargo` invocation succeeded

---

## Dev Session: 2026-04-07 — Items 40 + 52 (instruction-intake Operation + Commit Grouping Display)

### Completed
- **40. workforce:instruction-intake** — `operations/instruction_intake.md` expanded from a stub into a 7-step pipe-delimited table mirroring the canonical pipeline in `library/skills/instruction-intake.md`: (1) EA Triage → (2) COO Optimize → (3) COO Write Review → (4) Hypervisor BLOCKING user confirm (`tool=*`) → (5) COO Parse/Route → (6) COO Append via `tool:copy-file` → (7) PM Incorporate via `skill:synthesize_instructions`. Parses cleanly via `parse_operation_markdown`; `cargo test -p orrch-workforce` reports 12 passed / 0 failed, including `test_parse_instruction_intake` and `test_instruction_intake_module`.
- **52. Git commit grouping display (core)** — `crates/orrch-tui/src/ui.rs` dev map feature rendering now shows up to 3 child lines beneath each feature: `  <short-sha> <subject>` in `TEXT_MUTED` / `TEXT_DIM`. Data sourced from the existing `orrch_core::git::commits_for_feature()` — no new git plumbing required. Implemented as additional `Line`s inside the **same** `ListItem` so `App::devmap_flat_count` / `devmap_item_at` selection math in `app.rs` stays correct without modification. Advanced Repository-Manager advisory layer is a future enhancement.

### Verification
- 2 isolated verifier subagents reviewed both changes in parallel (Tester A + Tester B)
- Both returned PASS / VERIFIED for both changes with cross-checked file:line evidence
- `cargo build --release`: clean, only pre-existing dead-code warnings
- Non-blocker notes logged: `commits_for_feature` shells out to `git log` per render frame (candidate for `Project` struct caching if devmap gets sluggish); subject truncation is char-count based (wide CJK could slightly over-fill); `operations/instruction_intake.md` omits an optional `Blocker:` line (parser defaults to `None`, harmless)

### Files Changed
- `operations/instruction_intake.md` — stub → full 7-step table
- `crates/orrch-tui/src/ui.rs` — dev map feature row render (lines ~1632-1672)
- `PLAN.md` — items 40 and 52 marked `[x]` with status notes

### Workflow
- Executed `develop_feature` MCP dispatch loop end-to-end
- 1 PM planning agent → 2 tasks → 2 file-clustered Developer agents (parallel, Wave 1) → 2 isolated Feature Tester agents (parallel) → consensus PASS → ship

---

## Dev Session: 2026-04-07 — Items 41 + 42 + 43 (Intake Operation Modules)

### Completed
- **41. workforce:plan-intake** — New `operations/plan_intake.md` operation module modeled on the canonical `instruction_intake.md`. 5-step sequential pipeline: EA separates plan/design input → COO clarifies scope → COO parses for routing → COO routes to target project workspace → PM incorporates into existing PLAN.md or scaffolds a new project plan. Trigger: UserSubmit. Parses cleanly via `parse_operation_markdown`.
- **42. workforce:idea-intake** — New `operations/idea_intake.md`. 4-step sequential pipeline: EA classifies input as idea → COO stores via `tool:save-idea` (backed by `save_idea()` in `orrch-core::vault`) → Mentor reviews for feasibility → idea remains dormant in `plans/` vault, unattached, no PM dispatch. Matches acceptance criterion that ideas not be forced into immediate execution.
- **43. workforce:knowledge-intake** — New `operations/knowledge_intake.md`. 4-step sequential pipeline: EA classifies input as a custom library item → COO determines target subdirectory from `library/{skills, tools, mcp_servers, models, harnesses}` → Mentor scans for duplicates/conflicts → Repository Manager commits the new `.md` to the chosen subdir.

### Verification
- 2 isolated verifier subagents reviewed all 3 files in parallel
- Structural parse verified against `crates/orrch-workforce/src/parser.rs::parse_operation_markdown` — all 3 files yield operation name, `TriggerCondition::UserSubmit`, and complete step list
- Defect found + fixed in rework cycle: `knowledge_intake.md` step 2 originally listed a nonexistent `library/agents` subdir; corrected to the real set (`library/skills, library/tools, library/mcp_servers, library/models, library/harnesses`)
- Known issues (pre-existing, not this cluster's responsibility):
  - Parser silently drops `Blocker:` and `Interrupts:` content — affects `instruction_intake.md` identically
  - Step rows reference `skill:` and `tool:` names that don't exist as library items — matches canonical convention, ecosystem catch-up is a separate task

### Files Changed
- `operations/plan_intake.md` — **new**: 5-step EA → COO → PM intake pipeline
- `operations/idea_intake.md` — **new**: 4-step EA → COO → Mentor dormant-vault pipeline
- `operations/knowledge_intake.md` — **new**: 4-step EA → COO → Mentor → Repository Manager library-insert pipeline
- `PLAN.md` — items 41, 42, 43 marked `[x]`

### Workflow
- Executed `develop_feature` MCP dispatch loop end-to-end
- 1 PM planning agent → 3 tasks → 3 file-clustered impl agents (parallel) → 2 isolated verifier agents (parallel) → 1 PM evaluator → 1 REWORK cycle (single-file fix) → 1 re-verifier → PASS

---

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
