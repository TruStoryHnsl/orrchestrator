# Robust Context Handling — Implementation Plan

**Goal:** (1) Fix orrchestrator's plan browser so it discovers projects whose plan lives only at `<project>/.orrch/PLAN.md`. (2) Formalize the project-local vs global split for all structured context orrchestrator creates, with a canonical location API, rerouted outliers, and a safe idempotent migration-on-launch.

**Scope decision (user):** Full migration on launch — mutates live data, therefore MUST be checkpointed, idempotent, and read-fallback-safe.

**Tech stack:** Rust, orrchestrator workspace (`crates/orrch-core`, binary at `src/`).

---

## Root cause (verified)

`crates/orrch-core/src/project.rs::scan_project_meta()` scans only the project ROOT and sets `meta.plan_file` from a root `PLAN.md`/`development_plan.md`. `resolve_plan_path()` (which prefers `.orrch/PLAN.md`) is only consulted when `meta.plan_file` is already `Some`. A project with ONLY `.orrch/PLAN.md` (e.g. concord) gets `plan_file=None` → `has_plan=false` → filtered out of the Plans panel (`app.rs` `plans_refresh_project_list` filters on `p.has_plan`). The one existing test (`prefers_dot_orrch_plan_over_root`) only exercises `resolve_plan_path()` in isolation, so it passed while discovery was broken.

## Local vs global split (governing rule, from user)

- **Project-local** → `<project>/.orrch/` (+ project dotfiles): anything specific to ONE project (plan, devlog, events, session briefs, backup config, file registry, scope/tag/temperature dotfiles).
- **Global** → `~/.config/orrchestrator/` (settings) and `~/.local/share/orrchestrator/` (data): anything spanning MANY projects or about the USER (config.json, valves.json, backends/agents/library, usage log, shadow repos, the multi-project loop registry).

Canonical helpers already exist: `config::config_dir()`, `config::data_dir()`. Outlier to fix: `loops.json` currently at `~/projects/.orrch/loops.json` (multi-project state in a pseudo-local path) → should be global `data_dir()/loops.json`.

---

## CH1 — Fix `.orrch/PLAN.md` discovery (+ rigorous regression test)

**Files:** `crates/orrch-core/src/project.rs`.

- In `scan_project_meta()` (or right after it in `Project::load()`), when no root plan file is found, check `<project>/.orrch/PLAN.md`; if it exists, treat the project as having a plan (set `meta.plan_file = Some(".orrch/PLAN.md".into())` or equivalent so `Project::load` resolves and parses it). Keep `resolve_plan_path()` preference intact.
- Ensure `Project::load` parses the `.orrch/PLAN.md` content and sets `has_plan=true` + `plan_phases`.

**Regression test (empirically-proven bug → allowed this session):** a tmp project dir with ONLY `.orrch/PLAN.md` (no root PLAN.md), run the FULL chain (`Project::load` and/or `load_projects` on a tmp projects dir), assert the project has `has_plan == true` and the resolved plan path ends with `.orrch/PLAN.md`, and (user-observable) that it would appear in the Plans list. Also keep a project with NEITHER → `has_plan == false`. Do NOT just test `resolve_plan_path`.

## CH2 — Canonical context-location API

**Files:** new `crates/orrch-core/src/context_location.rs`; re-export from `lib.rs`.

- Define `pub enum ContextScope { Project, Global }` and `pub enum Artifact { Plan, Devlog, Events, Architecture, Licensing, CodebaseBrief, InstructionsInbox, BackupConfig, SessionBriefs, FileRegistry, FileRegistryAudit, /* global */ Config, Valves, Usage, LoopRegistry, ShadowRepo, ... }` (cover the inventory).
- `impl Artifact { pub fn scope(&self) -> ContextScope; }` — the single source of truth for local vs global, with a doc comment per artifact stating WHY.
- `pub fn artifact_path(a: Artifact, project_dir: Option<&Path>) -> PathBuf` — returns the canonical path: project-local → `project_dir/.orrch/<name>`; global → `config_dir()` or `data_dir()/<name>`. Return an error/None if a Project-scoped artifact is requested without a `project_dir`.
- Unit tests: each artifact's scope is as documented; project artifacts land under `.orrch/`; global ones under config/data dir.

## CH3 — Reroute outliers through the API + read-fallback

**Files:** `crates/orrch-core/src/loops.rs` (and any other writer pointing at a non-canonical path).

- `loops_path()` returns `artifact_path(Artifact::LoopRegistry, None)` = `data_dir()/loops.json`.
- READ-FALLBACK: when loading, if the canonical path doesn't exist but the legacy `~/projects/.orrch/loops.json` does, read the legacy one (so nothing breaks pre-migration). Writes always go to canonical.
- Test: read-fallback returns legacy data when canonical absent; write goes to canonical.

## CH4 — Idempotent migration-on-launch (safe; mutates live data)

**Files:** new `crates/orrch-core/src/context_migrate.rs`; wired into startup in `src/src/main.rs` (or core init).

- `pub fn migrate_context() -> MigrationReport`: for each artifact with a known LEGACY location differing from its canonical location, if legacy exists and canonical does not, **copy** legacy→canonical, verify byte-equality, then record the move; only after a successful verified copy, remove (or rename to `.migrated.bak`) the legacy file. Never delete unverified.
- **Idempotent:** safe to run every launch. Write a marker (e.g. `data_dir()/.context_migrated_v1`) and/or detect that canonical already exists → skip. Running twice = second run is a no-op (assert in test).
- **Backup/safety:** before the first migration, write a manifest (`data_dir()/context_migration_<ts>.log`) listing every move (from→to). Do not move a file if the destination exists with different content (log a conflict, leave both, surface it). No data loss under any path.
- Wire `migrate_context()` once at startup, before the TUI/state loads, behind the same init the config load uses. Log a one-line summary.
- Tests (with tmp dirs, NEVER touching the real home): legacy-present+canonical-absent → moved + verified + legacy gone/renamed; run-twice → idempotent no-op; conflict (both exist, differ) → both preserved + reported.

## CH5 — Review, LOOK-verification, merge

- Independent review focused on MIGRATION SAFETY (no data-loss path, idempotency, verify-before-delete) and the CH1 regression-test rigor.
- LOOK (per testing rules): in the real workspace, confirm the Plans browser now discovers concord's `.orrch/PLAN.md` (run the discovery code against `~/projects` and assert concord present with has_plan). Confirm `migrate_context()` run twice is a no-op. Report OBSERVED.
- Merge to `main` via the tiered tool.

---

## Hard constraints
- Migration must NEVER lose data: copy+verify before removing legacy; preserve both on conflict.
- All tests use tmp dirs; never read/write the real `~/.config`/`~/.local/share`/`~/projects` in tests.
- Conventional commits. Whole workspace must build. Existing tests stay green.
