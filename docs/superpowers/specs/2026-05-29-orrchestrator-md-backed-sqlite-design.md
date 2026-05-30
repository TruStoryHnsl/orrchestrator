# Design: `.orrchestrator/` md-backed, rebuildable SQLite memory backend

**Date:** 2026-05-29
**Status:** Approved design — pending spec review → implementation plan
**Scope target:** orrchestrator (`private`)

## Problem

orrchestrator's project memory is spread across three uncoordinated mechanisms:

- **Markdown** — `PLAN.md` (parsed for roadmap via `plan_parser.rs`), `DEVLOG.md`, `HANDOFF.md`, `instructions_inbox.md`.
- **Scattered JSON/JSONL** — `.retrospect/errors.jsonl` (append-only bug/error store with an in-memory `HashMap` index, `orrch-retrospect/store.rs`), `plans/.diff_log.json`, `.orrch/workflow.json`, `.orrch/loops.json`, feedback `.status.json`.
- **mempalace** (ChromaDB) — cross-session *semantic* memory, a separate MCP server.

The library of reusable components orrchestrator manages via its MCP server (skills, workflows, agents, tools, MCP servers, workforce templates) lives as **markdown-with-frontmatter** files under `~/.config/orrchestrator/library/<kind>/*.md` (`orrch-library/store.rs`). Every `list_skills` / `library_search` MCP call **re-reads and re-parses every `.md` file on disk**, loading full bodies into memory to filter — discovery cost scales linearly with library size and search is not indexed.

Consequences:

- **No single queryable source of truth.** Answering "open bugs + in-progress features for project X" means loading and parsing whole `PLAN.md` + `errors.jsonl` into context.
- **Token-expensive.** Whole-file loads + markdown parsing into the model's context, where a targeted row query would do.
- **Non-deterministic.** Linear file scans, ad-hoc parsing, in-memory indices rebuilt per process.

## Goal

A **deterministic, token-efficient, fast** query layer for everything orrchestrator canonically tracks — without introducing a server, a daemon, or an operational dependency on another machine, and without degrading the human-readability or git/Syncthing-sharing of the underlying data.

## Decisions (locked during brainstorming)

1. **Embedded SQLite, not Postgres.** Postgres is client-server — always a separate daemon/port/install, even "locally." The requirement ("the program builds and hosts the DB itself; it's just a file; back it up or share it if you want") is the definition of an *embedded* database. SQLite compiles into orrchestrator (Rust-native via `rusqlite`/`sqlx`, both already license-cleared in `compliance.rs`). No daemon, no port, no extra machine. ~95% of the SQL surface we need (joins, indexes, JSON1, FTS5).

2. **The DB holds zero unique durable state.** It is a **purely ephemeral local query accelerator**, rebuilt from markdown on every launch. `rm orrch.db` costs nothing. There are therefore **no schema migrations, ever** — the running binary's DDL *is* the schema, recreated each rebuild.

3. **Source of truth = structured, human-readable markdown in a hidden `.orrchestrator/` folder inside each project.** Everything orrchestrator canonically tracks lives there. The user can navigate the folder and read the plan exactly as before.

4. **Sharing = sharing the project folder.** Project folders already sync across the Tailscale network via Syncthing/git. Any machine with the folder rebuilds an identical DB. No DB replication, no binary-file conflicts, no daemon. This honors the **Local-First** rule (no operational dependency on a remote machine).

5. **Human-readable stays human-readable.** Files that are human-readable today (`PLAN.md`, `DEVLOG.md`) remain so after moving into `.orrchestrator/`. orrchestrator MAY create additional **compressed / non-human-readable derived artifacts** (the SQLite DB itself is exactly that) — but it MUST NOT degrade the readability of the canonical markdown to suit the machine. Canonical md is for humans; the derived DB is for the machine.

6. **Everything migrates into `.orrchestrator/`.** `PLAN.md` and `DEVLOG.md` move there intact (single readable files, not exploded). Code that currently reads root `PLAN.md` repoints to `.orrchestrator/PLAN.md` (with a fallback to root for un-migrated projects during transition).

## Architecture

### `.orrchestrator/` folder layout (per project)

```
.orrchestrator/
  PLAN.md            # human-readable roadmap (moved from project root, intact) — parsed -> features
  DEVLOG.md          # human-readable dev log (moved from project root, intact)
  architecture.md    # SNAPSHOT doc: current architecture facts (frontmatter + sections)
  licensing.md       # SNAPSHOT doc: dependencies, licenses, audit state
  events/            # APPEND-ONLY, one immutable file per record:
                     #   <ISO-ts>-<shortid>.md  e.g. 20260529T142233-a1b2c3.md
                     #   bug reports, status changes, audit entries, known-issue records
```

Genuinely-transient runtime state (active-loop status, in-flight workflow status — today's `.orrch/loops.json`, `.orrch/workflow.json`) is **out of scope**: it is process-runtime state, not durable project knowledge, and need not be markdown-backed or rebuildable. It may stay as-is.

### Two format classes, by access pattern

**1. Snapshot docs** (`PLAN.md`, `DEVLOG.md`, `architecture.md`, `licensing.md`)
Edited in place; frontmatter + markdown sections. Rarely edited concurrently; the existing tiered-merge tool's union-merge handles additive edits. Parsed into current-state tables.

**2. Event log** (`events/`)
**One immutable file per record**, timestamp-sortable filename + short id. **Nothing is ever rewritten** — a bug moving `open -> fixed` is a *new* event file, not an edit to an existing one.

This is the key design choice. Append-only + one-file-per-record means:
- **Zero merge conflicts ever** — two machines/sessions never touch the same file.
- **Trivial deterministic parse** — reuse the existing library frontmatter parser.
- **Fully git/Syncthing-safe** — every write is a new file; union-merge is a no-op concatenation of distinct files.

A record's *current state* (e.g. a bug's status) is **not stored as truth anywhere** — it is **computed by folding the event chain** at rebuild time. This is event-sourcing: the markdown is the log, the SQLite DB is the materialized view.

### SQLite schema (illustrative — ephemeral, no migrations)

Snapshot-derived tables:
- `projects(slug, path, name, scope)` — from the `~/projects` scan
- `architecture_facts(project, section, key, value)` — from `architecture.md`
- `licensing(project, dependency, license, audit_status, notes)` — from `licensing.md`
- `features(project, slug, title, status, description, source)` — from `.orrchestrator/PLAN.md`

Event-log tables:
- `events(id, project, ts, kind, entity_type, entity_id, payload_json, session_id)` — raw immutable log, one row per `events/*.md`
- `bugs(project, bug_id, title, severity, status, first_seen, last_ts, resolution)` — **folded** from `entity_type='bug'` events

Library tables (same mechanism, root = `~/.config/orrchestrator/library/<kind>/*.md`):
- `library_items(kind, name, description, tags, path, body_hash)` — metadata index; full body fetched on demand, never loaded during search
- `source_files(path, mtime, hash)` — drives incremental re-ingest

Search:
- FTS5 virtual tables over descriptions/bodies, so `library_search` and bug/feature search become indexed queries, not file scans.

### Rebuild pipeline (`orrch-db` crate)

1. **Launch — full rebuild.** Open `orrch.db` (WAL mode). Scan: global library dir + every project's `.orrchestrator/` + (transitional) any un-migrated root `PLAN.md`/`DEVLOG.md`. Parse → insert. Snapshots → state tables; `events/*.md` → `events`; then **fold events** into `bugs` / feature-status overrides. Build FTS indexes. Expected sub-second at current project/library counts.
2. **Runtime — incremental.** A `notify` file-watcher on those directories. On change, re-ingest only files whose `mtime`/`hash` differ from `source_files`, and re-fold affected entities.
3. **Write path — file-first, always.** orrchestrator never writes the DB as the primary action:
   - new bug / status change → write a **new** `events/<ts>-<id>.md` (never edit) → incremental ingest updates the DB.
   - architecture/licensing/plan edit → rewrite that snapshot doc → re-ingest.
   - The DB can never diverge from the files because the file write *is* the commit; the DB update is strictly downstream.

### Crate + integration

- New **`orrch-db`** crate owns: schema DDL, parsers (reusing the `orrch-library` frontmatter parser), event-fold logic, the file-watcher, and a typed query API.
- MCP tools (`project_state`, `list_skills`, `library_search`, `codebase_brief`, and the retrospect/bug surface) call the `orrch-db` query API instead of re-parsing files. **This is where the token/latency win lands.**
- `orrch-retrospect`'s `errors.jsonl` store is superseded by `events/` + the `bugs` fold; provide a one-time migration that converts existing `errors.jsonl` records into `events/*.md` files.
- Concurrency: WAL + short write transactions. Parallel sessions/agents serialize writes through the single embedded engine — more deterministic than today's scattered-file races. Each orrchestrator process opens its own on-disk `orrch.db` (rebuildable, cheap); the markdown files are the cross-process/cross-machine sync point.

## Migration

1. Introduce `.orrchestrator/` and move `PLAN.md` + `DEVLOG.md` into it per project (keep a root-fallback read path during transition).
2. Convert `.retrospect/errors.jsonl` → `.orrchestrator/events/*.md` (one file per record).
3. Repoint `plan_parser`/`project.rs` reads to `.orrchestrator/PLAN.md`.
4. Seed `architecture.md` / `licensing.md` (licensing can bootstrap from `compliance.rs`'s current hardcoded list).

## Non-Goals (YAGNI)

- No DB replication / distributed SQLite / libSQL sync (sharing is via existing folder sync). Revisit only if folder-level sync proves insufficient.
- No DB-canonical durable data of any kind.
- No schema migration framework (DB is rebuilt, never upgraded in place).
- No change to mempalace — semantic memory stays its own layer; this is the *structured/deterministic* layer alongside it.
- Transient runtime status (`.orrch/loops.json`, `workflow.json`) not migrated.

## Open questions for spec review

- Exact frontmatter schema per event `kind` (bug, status_change, audit, known_issue) — define the minimal required fields.
- Whether `features` status can also be event-sourced (status-change events) or stays snapshot-only in `PLAN.md`. Default: snapshot-only initially; events can override later if needed.
- Naming: `.orrchestrator/` vs reusing/renaming the existing `.orrch/` directory.
