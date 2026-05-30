# `orrch-db` md-backed rebuildable SQLite backend — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give orrchestrator a deterministic, token-efficient query layer (`orrch-db`) backed by an ephemeral SQLite database that is fully rebuilt on every launch from human-readable markdown stored in each project's `.orrch/` folder.

**Architecture:** Source of truth is markdown in `.orrch/` per project (snapshot docs + an append-only `events/` log of one immutable file per record) plus the global library at `~/.config/orrchestrator/library/`. A new `orrch-db` crate parses those files into a local SQLite file (rusqlite, bundled, WAL), folds the event log into current-state tables, and exposes a typed query API. The DB holds zero unique durable state — `rm orrch.db` is always safe; there are no migrations. A `notify` watcher does incremental re-ingest at runtime; all writes go file-first. MCP tools query `orrch-db` instead of re-parsing files.

**Tech Stack:** Rust (edition 2024), `rusqlite` (bundled SQLite + FTS5), `notify` (file watcher), `serde`/`serde_json`, `sha2`, reuse of `orrch_library::store` frontmatter parser.

**Spec:** `docs/superpowers/specs/2026-05-29-orrchestrator-md-backed-sqlite-design.md`

---

## File Structure

New crate `crates/orrch-db/`:

- `Cargo.toml` — crate manifest, deps.
- `src/lib.rs` — module wiring + public re-exports.
- `src/model.rs` — record types (`EventRecord`, `BugRow`, `FeatureRow`, `LibraryRow`, `ArchFact`, `LicenseRow`, `EventKind`, `EntityType`).
- `src/schema.rs` — SQLite DDL (`CREATE TABLE`/`CREATE VIRTUAL TABLE` statements) + `init_schema`.
- `src/parse.rs` — markdown → record parsers (events, snapshot docs), reusing the library frontmatter parser.
- `src/ingest.rs` — insert parsed records into tables + `source_files` bookkeeping.
- `src/fold.rs` — fold `events` into the `bugs` current-state table.
- `src/rebuild.rs` — orchestration: open DB, scan all sources, full rebuild.
- `src/query.rs` — typed read API used by MCP/TUI.
- `src/watch.rs` — `notify`-based incremental re-ingest.
- `src/migrate.rs` — one-time migrations: `errors.jsonl` → `events/`, relocate root `PLAN.md`/`DEVLOG.md` into `.orrch/`.

Modified:

- `Cargo.toml` (workspace) — add member + workspace deps.
- `crates/orrch-core/src/project.rs:309` — repoint `PLAN.md` read path to `.orrch/PLAN.md` with root fallback.
- `crates/orrch-mcp/Cargo.toml` + `crates/orrch-mcp/src/tools.rs` — `library_search`/`list_skills`/`list_agents`/`project_state` query `orrch-db`.

---

## Task 1: Scaffold the `orrch-db` crate

**Files:**
- Create: `crates/orrch-db/Cargo.toml`
- Create: `crates/orrch-db/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add workspace deps + member**

In root `Cargo.toml`, under `[workspace.dependencies]` add these two lines after the `sha2 = "0.10"` line:

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
notify = "6"
```

In root `Cargo.toml`, under `[workspace] members = [...]`, add this line after `"crates/orrch-core",`:

```toml
    "crates/orrch-db",
```

- [ ] **Step 2: Create the crate manifest**

Create `crates/orrch-db/Cargo.toml`:

```toml
[package]
name = "orrch-db"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rusqlite = { workspace = true }
notify = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
sha2 = { workspace = true }
thiserror = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
orrch-library = { path = "../orrch-library" }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Create minimal `lib.rs`**

Create `crates/orrch-db/src/lib.rs`:

```rust
//! `orrch-db`: an ephemeral, rebuildable SQLite query layer for orrchestrator.
//!
//! Source of truth is markdown in each project's `.orrch/` folder plus the
//! global library. The database holds zero unique durable state and is fully
//! reconstructable from those files. There are no schema migrations.

pub mod model;
pub mod schema;
pub mod parse;
pub mod ingest;
pub mod fold;
pub mod rebuild;
pub mod query;
pub mod watch;
pub mod migrate;

pub use model::{
    ArchFact, BugRow, EntityType, EventKind, EventRecord, FeatureRow, LibraryRow, LicenseRow,
};
pub use rebuild::{rebuild_all, RebuildSources};
```

> The modules referenced above are created in later tasks. To keep the crate compiling between tasks, create each module file as an empty file (`touch`) the first time `lib.rs` references it, then fill it in its own task. Step 4 does this.

- [ ] **Step 4: Create empty module files so the crate compiles**

Run:

```bash
cd crates/orrch-db/src
for m in model schema parse ingest fold rebuild query watch migrate; do touch "$m.rs"; done
```

Then put a temporary stub in each so `lib.rs`'s `pub use` lines resolve. In `model.rs`:

```rust
// Filled in Task 2.
```

In `rebuild.rs`:

```rust
// Filled in Task 7.
pub struct RebuildSources;
pub fn rebuild_all() {}
```

Comment out the `pub use` lines in `lib.rs` for now (re-enable in their tasks):

```rust
// pub use model::{...};   // Task 2
// pub use rebuild::{...}; // Task 7
```

- [ ] **Step 5: Verify the crate compiles**

Run: `cargo build -p orrch-db`
Expected: compiles with warnings about unused empty modules; no errors.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/orrch-db
git commit -m "feat(orrch-db): scaffold ephemeral md-backed sqlite crate"
```

---

## Task 2: Record types (`model.rs`)

**Files:**
- Modify: `crates/orrch-db/src/model.rs`
- Modify: `crates/orrch-db/src/lib.rs` (re-enable `pub use model::...`)

- [ ] **Step 1: Write the failing test**

Append to `crates/orrch-db/src/model.rs`:

```rust
use serde::{Deserialize, Serialize};

/// The entity an event is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Bug,
    Feature,
    Audit,
}

/// The kind of mutation an event records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    BugOpened,
    BugStatusChanged,
    BugResolved,
    KnownIssue,
    Audit,
}

impl EventKind {
    /// Parse from the `kind:` frontmatter value. Returns None for unknown kinds.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim() {
            "bug_opened" => Some(Self::BugOpened),
            "bug_status_changed" => Some(Self::BugStatusChanged),
            "bug_resolved" => Some(Self::BugResolved),
            "known_issue" => Some(Self::KnownIssue),
            "audit" => Some(Self::Audit),
            _ => None,
        }
    }
}

/// One immutable record parsed from an `events/<ts>-<id>.md` file.
#[derive(Debug, Clone)]
pub struct EventRecord {
    pub id: String,
    pub project: String,
    /// RFC3339 timestamp string, lexically sortable.
    pub ts: String,
    pub kind: EventKind,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub session_id: Option<String>,
    /// All remaining frontmatter fields + the markdown body, as JSON.
    pub payload: serde_json::Value,
}

/// Current-state bug row, folded from the event log.
#[derive(Debug, Clone, PartialEq)]
pub struct BugRow {
    pub project: String,
    pub bug_id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub first_seen: String,
    pub last_ts: String,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureRow {
    pub project: String,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub description: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryRow {
    pub kind: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub path: String,
    pub body_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArchFact {
    pub project: String,
    pub section: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LicenseRow {
    pub project: String,
    pub dependency: String,
    pub license: String,
    pub audit_status: String,
    pub notes: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_kind_parses_known_and_rejects_unknown() {
        assert_eq!(EventKind::from_str("bug_opened"), Some(EventKind::BugOpened));
        assert_eq!(EventKind::from_str("bug_resolved"), Some(EventKind::BugResolved));
        assert_eq!(EventKind::from_str(" audit "), Some(EventKind::Audit));
        assert_eq!(EventKind::from_str("nonsense"), None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p orrch-db model::tests::event_kind_parses_known_and_rejects_unknown`
Expected: FAIL — module currently only has the placeholder comment, so the test won't exist until the code above is present; if you pasted it, it should now compile and the test should be discovered.

(If it already passes after pasting, that is acceptable — this task is type definitions; the test guards `from_str`.)

- [ ] **Step 3: Re-enable the re-export**

In `crates/orrch-db/src/lib.rs`, uncomment:

```rust
pub use model::{
    ArchFact, BugRow, EntityType, EventKind, EventRecord, FeatureRow, LibraryRow, LicenseRow,
};
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p orrch-db`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add crates/orrch-db/src/model.rs crates/orrch-db/src/lib.rs
git commit -m "feat(orrch-db): record types for events, bugs, features, library"
```

---

## Task 3: Schema DDL (`schema.rs`)

**Files:**
- Modify: `crates/orrch-db/src/schema.rs`

- [ ] **Step 1: Write the failing test**

Replace `crates/orrch-db/src/schema.rs` with:

```rust
use rusqlite::Connection;

/// All DDL for the ephemeral database. Recreated on every rebuild — there are
/// no migrations; this string *is* the schema.
pub const SCHEMA_SQL: &str = r#"
CREATE TABLE projects (
    slug  TEXT PRIMARY KEY,
    path  TEXT NOT NULL,
    name  TEXT NOT NULL,
    scope TEXT NOT NULL
);

CREATE TABLE architecture_facts (
    project TEXT NOT NULL,
    section TEXT NOT NULL,
    key     TEXT NOT NULL,
    value   TEXT NOT NULL
);
CREATE INDEX idx_arch_project ON architecture_facts(project);

CREATE TABLE licensing (
    project      TEXT NOT NULL,
    dependency   TEXT NOT NULL,
    license      TEXT NOT NULL,
    audit_status TEXT NOT NULL,
    notes        TEXT NOT NULL
);
CREATE INDEX idx_lic_project ON licensing(project);

CREATE TABLE features (
    project     TEXT NOT NULL,
    slug        TEXT NOT NULL,
    title       TEXT NOT NULL,
    status      TEXT NOT NULL,
    description TEXT NOT NULL,
    source      TEXT NOT NULL,
    PRIMARY KEY (project, slug)
);

CREATE TABLE events (
    id           TEXT PRIMARY KEY,
    project      TEXT NOT NULL,
    ts           TEXT NOT NULL,
    kind         TEXT NOT NULL,
    entity_type  TEXT NOT NULL,
    entity_id    TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    session_id   TEXT
);
CREATE INDEX idx_events_entity ON events(project, entity_type, entity_id, ts);

CREATE TABLE bugs (
    project    TEXT NOT NULL,
    bug_id     TEXT NOT NULL,
    title      TEXT NOT NULL,
    severity   TEXT NOT NULL,
    status     TEXT NOT NULL,
    first_seen TEXT NOT NULL,
    last_ts    TEXT NOT NULL,
    resolution TEXT,
    PRIMARY KEY (project, bug_id)
);

CREATE TABLE library_items (
    kind        TEXT NOT NULL,
    name        TEXT NOT NULL,
    description TEXT NOT NULL,
    tags        TEXT NOT NULL,
    path        TEXT NOT NULL,
    body_hash   TEXT NOT NULL,
    PRIMARY KEY (kind, name)
);

CREATE TABLE source_files (
    path  TEXT PRIMARY KEY,
    mtime INTEGER NOT NULL,
    hash  TEXT NOT NULL
);

CREATE VIRTUAL TABLE library_fts USING fts5(
    name, description, tags, content=''
);
"#;

/// Create all tables on a fresh connection and set WAL mode.
pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN \
                 ('projects','architecture_facts','licensing','features','events','bugs','library_items','source_files')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 8);
    }
}
```

- [ ] **Step 2: Run test to verify it fails, then passes**

Run: `cargo test -p orrch-db schema::tests::schema_creates_all_tables`
Expected: PASS (the code and test are added together; this guards the DDL parses and all 8 tables exist).

- [ ] **Step 3: Commit**

```bash
git add crates/orrch-db/src/schema.rs
git commit -m "feat(orrch-db): sqlite schema DDL + WAL init"
```

---

## Task 4: Event-file parser (`parse.rs`)

**Files:**
- Modify: `crates/orrch-db/src/parse.rs`

- [ ] **Step 1: Write the failing test**

Replace `crates/orrch-db/src/parse.rs` with:

```rust
use crate::model::{EntityType, EventKind, EventRecord};
use orrch_library::store::{extract_field_pub, parse_frontmatter_pub};

/// Parse one `events/<ts>-<id>.md` file body into an `EventRecord`.
///
/// `project` is the owning project slug (the file lives under that project's
/// `.orrch/events/`). Returns `None` if required fields are missing or the
/// `kind` is unknown — a malformed event file is skipped, never fatal.
pub fn parse_event(content: &str, project: &str) -> Option<EventRecord> {
    let (frontmatter, body) = parse_frontmatter_pub(content)?;

    let id = extract_field_pub(&frontmatter, "id")?;
    let ts = extract_field_pub(&frontmatter, "ts")?;
    let kind = EventKind::from_str(&extract_field_pub(&frontmatter, "kind")?)?;
    let entity_id = extract_field_pub(&frontmatter, "entity_id")?;
    let entity_type = match extract_field_pub(&frontmatter, "entity_type")?.as_str() {
        "bug" => EntityType::Bug,
        "feature" => EntityType::Feature,
        "audit" => EntityType::Audit,
        _ => return None,
    };
    let session_id = extract_field_pub(&frontmatter, "session_id");

    // Payload = selected scalar frontmatter fields + the human-readable body note.
    let mut payload = serde_json::Map::new();
    for key in ["title", "severity", "status", "resolution"] {
        if let Some(v) = extract_field_pub(&frontmatter, key) {
            payload.insert(key.to_string(), serde_json::Value::String(v));
        }
    }
    let note = body.trim();
    if !note.is_empty() {
        payload.insert("note".to_string(), serde_json::Value::String(note.to_string()));
    }

    Some(EventRecord {
        id,
        project: project.to_string(),
        ts,
        kind,
        entity_type,
        entity_id,
        session_id,
        payload: serde_json::Value::Object(payload),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\n\
id: a1b2c3\n\
ts: 2026-05-29T14:22:33Z\n\
kind: bug_opened\n\
entity_type: bug\n\
entity_id: parser-emoji-crash\n\
session_id: T8\n\
severity: high\n\
title: Parser crashes on 4-byte emoji\n\
---\n\
PLAN.md lines starting with a green-circle emoji panic the slicer.";

    #[test]
    fn parses_a_well_formed_bug_event() {
        let ev = parse_event(SAMPLE, "orrchestrator").unwrap();
        assert_eq!(ev.id, "a1b2c3");
        assert_eq!(ev.kind, EventKind::BugOpened);
        assert_eq!(ev.entity_type, EntityType::Bug);
        assert_eq!(ev.entity_id, "parser-emoji-crash");
        assert_eq!(ev.session_id.as_deref(), Some("T8"));
        assert_eq!(ev.payload["severity"], "high");
        assert_eq!(ev.payload["title"], "Parser crashes on 4-byte emoji");
        assert!(ev.payload["note"].as_str().unwrap().contains("green-circle"));
    }

    #[test]
    fn rejects_unknown_kind() {
        let bad = SAMPLE.replace("kind: bug_opened", "kind: teleport");
        assert!(parse_event(&bad, "orrchestrator").is_none());
    }

    #[test]
    fn rejects_missing_required_field() {
        let bad = SAMPLE.replace("entity_id: parser-emoji-crash\n", "");
        assert!(parse_event(&bad, "orrchestrator").is_none());
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p orrch-db parse::`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/orrch-db/src/parse.rs
git commit -m "feat(orrch-db): event-file frontmatter parser"
```

---

## Task 5: Event ingest + source-file bookkeeping (`ingest.rs`)

**Files:**
- Modify: `crates/orrch-db/src/ingest.rs`

- [ ] **Step 1: Write the failing test**

Replace `crates/orrch-db/src/ingest.rs` with:

```rust
use crate::model::{EventRecord, LibraryRow};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

/// Insert (or replace) one event row.
pub fn insert_event(conn: &Connection, ev: &EventRecord) -> rusqlite::Result<()> {
    let kind = serde_json::to_value(ev.kind).unwrap();
    let etype = serde_json::to_value(ev.entity_type).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO events \
         (id, project, ts, kind, entity_type, entity_id, payload_json, session_id) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            ev.id,
            ev.project,
            ev.ts,
            kind.as_str().unwrap(),
            etype.as_str().unwrap(),
            ev.entity_id,
            ev.payload.to_string(),
            ev.session_id,
        ],
    )?;
    Ok(())
}

/// Insert (or replace) one library metadata row + its FTS entry.
pub fn insert_library_item(conn: &Connection, row: &LibraryRow) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO library_items \
         (kind, name, description, tags, path, body_hash) VALUES (?1,?2,?3,?4,?5,?6)",
        params![row.kind, row.name, row.description, row.tags.join(","), row.path, row.body_hash],
    )?;
    conn.execute(
        "INSERT INTO library_fts (name, description, tags) VALUES (?1,?2,?3)",
        params![row.name, row.description, row.tags.join(" ")],
    )?;
    Ok(())
}

/// Record a source file's mtime + content hash so incremental rebuilds can
/// skip unchanged files.
pub fn record_source_file(conn: &Connection, path: &str, mtime: i64, content: &str) -> rusqlite::Result<()> {
    let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    conn.execute(
        "INSERT OR REPLACE INTO source_files (path, mtime, hash) VALUES (?1,?2,?3)",
        params![path, mtime, hash],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EntityType, EventKind};
    use crate::schema::init_schema;

    fn sample_event() -> EventRecord {
        EventRecord {
            id: "e1".into(),
            project: "p".into(),
            ts: "2026-05-29T00:00:00Z".into(),
            kind: EventKind::BugOpened,
            entity_type: EntityType::Bug,
            entity_id: "b1".into(),
            session_id: Some("S".into()),
            payload: serde_json::json!({"title": "T", "severity": "low"}),
        }
    }

    #[test]
    fn inserts_event_and_reads_back() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        insert_event(&conn, &sample_event()).unwrap();
        let (kind, eid): (String, String) = conn
            .query_row("SELECT kind, entity_id FROM events WHERE id='e1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(kind, "bug_opened");
        assert_eq!(eid, "b1");
    }

    #[test]
    fn library_row_is_searchable_via_fts() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        insert_library_item(
            &conn,
            &LibraryRow {
                kind: "skill".into(),
                name: "pm-loop-review".into(),
                description: "review PM loop output".into(),
                tags: vec!["pm".into(), "review".into()],
                path: "/x.md".into(),
                body_hash: "h".into(),
            },
        )
        .unwrap();
        let hits: i64 = conn
            .query_row("SELECT count(*) FROM library_fts WHERE library_fts MATCH 'review'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hits, 1);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p orrch-db ingest::`
Expected: PASS (2 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/orrch-db/src/ingest.rs
git commit -m "feat(orrch-db): event + library ingest with FTS and source-file hashing"
```

---

## Task 6: Fold events into current-state bugs (`fold.rs`)

**Files:**
- Modify: `crates/orrch-db/src/fold.rs`

- [ ] **Step 1: Write the failing test**

Replace `crates/orrch-db/src/fold.rs` with:

```rust
use rusqlite::{params, Connection};

/// Fold the `events` log into the `bugs` current-state table.
///
/// For each `(project, entity_id)` with `entity_type='bug'`, events are
/// replayed in `ts` order: `bug_opened` seeds title/severity/status=open;
/// `bug_status_changed` updates status; `bug_resolved` sets status=resolved
/// and records the resolution. The current row is the result of the fold.
pub fn fold_bugs(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM bugs", [])?;

    let mut stmt = conn.prepare(
        "SELECT project, entity_id, ts, kind, payload_json \
         FROM events WHERE entity_type='bug' ORDER BY project, entity_id, ts",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;

    // (project, bug_id) -> folded fields
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<(String, String), (String, String, String, String, String, Option<String>)> =
        BTreeMap::new();
    // tuple = (title, severity, status, first_seen, last_ts, resolution)

    for row in rows {
        let (project, bug_id, ts, kind, payload_json) = row?;
        let payload: serde_json::Value = serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
        let entry = acc.entry((project, bug_id)).or_insert_with(|| {
            (String::new(), "unknown".into(), "open".into(), ts.clone(), ts.clone(), None)
        });
        entry.4 = ts.clone(); // last_ts
        match kind.as_str() {
            "bug_opened" => {
                if let Some(t) = payload.get("title").and_then(|v| v.as_str()) {
                    entry.0 = t.to_string();
                }
                if let Some(s) = payload.get("severity").and_then(|v| v.as_str()) {
                    entry.1 = s.to_string();
                }
                entry.2 = "open".into();
            }
            "bug_status_changed" => {
                if let Some(s) = payload.get("status").and_then(|v| v.as_str()) {
                    entry.2 = s.to_string();
                }
            }
            "bug_resolved" => {
                entry.2 = "resolved".into();
                entry.5 = payload.get("resolution").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            _ => {}
        }
    }

    for ((project, bug_id), (title, severity, status, first_seen, last_ts, resolution)) in acc {
        conn.execute(
            "INSERT INTO bugs (project, bug_id, title, severity, status, first_seen, last_ts, resolution) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![project, bug_id, title, severity, status, first_seen, last_ts, resolution],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::insert_event;
    use crate::model::{EntityType, EventKind, EventRecord};
    use crate::schema::init_schema;

    fn ev(id: &str, ts: &str, kind: EventKind, payload: serde_json::Value) -> EventRecord {
        EventRecord {
            id: id.into(),
            project: "p".into(),
            ts: ts.into(),
            kind,
            entity_type: EntityType::Bug,
            entity_id: "b1".into(),
            session_id: None,
            payload,
        }
    }

    #[test]
    fn open_then_resolve_yields_resolved_status() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        insert_event(&conn, &ev("e1", "2026-05-29T01:00:00Z", EventKind::BugOpened,
            serde_json::json!({"title":"Crash","severity":"high"}))).unwrap();
        insert_event(&conn, &ev("e2", "2026-05-29T02:00:00Z", EventKind::BugResolved,
            serde_json::json!({"resolution":"clamp slice on char boundary"}))).unwrap();

        fold_bugs(&conn).unwrap();

        let (status, title, sev, res): (String, String, String, Option<String>) = conn
            .query_row("SELECT status, title, severity, resolution FROM bugs WHERE bug_id='b1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap();
        assert_eq!(status, "resolved");
        assert_eq!(title, "Crash");
        assert_eq!(sev, "high");
        assert_eq!(res.as_deref(), Some("clamp slice on char boundary"));
    }

    #[test]
    fn fold_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        insert_event(&conn, &ev("e1", "2026-05-29T01:00:00Z", EventKind::BugOpened,
            serde_json::json!({"title":"X","severity":"low"}))).unwrap();
        fold_bugs(&conn).unwrap();
        fold_bugs(&conn).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM bugs", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p orrch-db fold::`
Expected: PASS (2 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/orrch-db/src/fold.rs
git commit -m "feat(orrch-db): fold event log into current-state bugs table"
```

---

## Task 7: Full rebuild orchestration (`rebuild.rs`)

**Files:**
- Modify: `crates/orrch-db/src/rebuild.rs`
- Modify: `crates/orrch-db/src/lib.rs` (re-enable `pub use rebuild::...`)

- [ ] **Step 1: Write the failing test**

Replace `crates/orrch-db/src/rebuild.rs` with:

```rust
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::fold::fold_bugs;
use crate::ingest::{insert_event, insert_library_item, record_source_file};
use crate::model::LibraryRow;
use crate::parse::parse_event;
use crate::schema::init_schema;
use orrch_library::store::{extract_field_pub, extract_list_pub, parse_frontmatter_pub};
use sha2::{Digest, Sha256};

/// Where to scan when rebuilding.
pub struct RebuildSources {
    /// Project directories, each expected to (optionally) contain `.orrch/`.
    pub project_dirs: Vec<PathBuf>,
    /// Global library root, e.g. `~/.config/orrchestrator/library`.
    pub library_root: PathBuf,
}

/// The library item kinds and their subdirectories (mirrors `orrch_library`).
const LIBRARY_KINDS: &[(&str, &str)] = &[
    ("agent", "agents"),
    ("skill", "skills"),
    ("tool", "tools"),
    ("mcp_server", "mcp_servers"),
    ("workforce_template", "workforce_templates"),
];

fn mtime_secs(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn project_slug(dir: &Path) -> String {
    dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Ingest one project's `.orrch/events/*.md` into the connection.
fn ingest_project_events(conn: &Connection, dir: &Path) -> rusqlite::Result<()> {
    let slug = project_slug(dir);
    let events_dir = dir.join(".orrch").join("events");
    let Ok(entries) = fs::read_dir(&events_dir) else { return Ok(()); };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else { continue };
        if let Some(ev) = parse_event(&content, &slug) {
            insert_event(conn, &ev)?;
        }
        record_source_file(conn, &path.to_string_lossy(), mtime_secs(&path), &content)?;
    }
    Ok(())
}

/// Ingest the global library into `library_items` + FTS.
fn ingest_library(conn: &Connection, root: &Path) -> rusqlite::Result<()> {
    for (kind, subdir) in LIBRARY_KINDS {
        let dir = root.join(subdir);
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else { continue };
            let Some((fm, body)) = parse_frontmatter_pub(&content) else { continue };
            let name = match extract_field_pub(&fm, "name") {
                Some(n) => n,
                None => continue,
            };
            let row = LibraryRow {
                kind: (*kind).to_string(),
                name,
                description: extract_field_pub(&fm, "description").unwrap_or_default(),
                tags: extract_list_pub(&fm, "tags"),
                path: path.to_string_lossy().to_string(),
                body_hash: format!("{:x}", Sha256::digest(body.as_bytes())),
            };
            insert_library_item(conn, &row)?;
            record_source_file(conn, &path.to_string_lossy(), mtime_secs(&path), &content)?;
        }
    }
    Ok(())
}

/// Open `db_path` fresh and fully rebuild it from the sources. Any existing DB
/// file is deleted first — the DB holds no durable state.
pub fn rebuild_all(db_path: &Path, sources: &RebuildSources) -> anyhow::Result<Connection> {
    let _ = fs::remove_file(db_path);
    let _ = fs::remove_file(db_path.with_extension("db-wal"));
    let _ = fs::remove_file(db_path.with_extension("db-shm"));
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    init_schema(&conn)?;

    for dir in &sources.project_dirs {
        ingest_project_events(&conn, dir)?;
    }
    ingest_library(&conn, &sources.library_root)?;
    fold_bugs(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rebuild_ingests_events_and_folds_bugs() {
        let tmp = tempdir().unwrap();
        // Fake project with one bug_opened event.
        let proj = tmp.path().join("myproj");
        let ev_dir = proj.join(".orrch").join("events");
        fs::create_dir_all(&ev_dir).unwrap();
        fs::write(
            ev_dir.join("20260529T010000-aaa.md"),
            "---\nid: aaa\nts: 2026-05-29T01:00:00Z\nkind: bug_opened\n\
             entity_type: bug\nentity_id: b1\nseverity: high\ntitle: Boom\n---\nboom happened",
        )
        .unwrap();

        let lib = tmp.path().join("library");
        fs::create_dir_all(lib.join("skills")).unwrap();

        let db = tmp.path().join("orrch.db");
        let conn = rebuild_all(
            &db,
            &RebuildSources { project_dirs: vec![proj], library_root: lib },
        )
        .unwrap();

        let (status, title): (String, String) = conn
            .query_row("SELECT status, title FROM bugs WHERE bug_id='b1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(status, "open");
        assert_eq!(title, "Boom");
    }

    #[test]
    fn rebuild_is_safe_on_empty_sources() {
        let tmp = tempdir().unwrap();
        let db = tmp.path().join("orrch.db");
        let conn = rebuild_all(
            &db,
            &RebuildSources { project_dirs: vec![], library_root: tmp.path().join("nope") },
        )
        .unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
}
```

- [ ] **Step 2: Re-enable the re-export**

In `crates/orrch-db/src/lib.rs`, replace the stub-era comment with:

```rust
pub use rebuild::{rebuild_all, RebuildSources};
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p orrch-db rebuild::`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/orrch-db/src/rebuild.rs crates/orrch-db/src/lib.rs
git commit -m "feat(orrch-db): full rebuild from .orrch/ events + global library"
```

---

## Task 8: Typed query API (`query.rs`)

**Files:**
- Modify: `crates/orrch-db/src/query.rs`

- [ ] **Step 1: Write the failing test**

Replace `crates/orrch-db/src/query.rs` with:

```rust
use rusqlite::Connection;

use crate::model::{BugRow, LibraryRow};

/// Library items of a given kind (metadata only — bodies are read from disk on
/// demand via `path`). This replaces the per-call file re-scan in the MCP layer.
pub fn library_items_by_kind(conn: &Connection, kind: &str) -> rusqlite::Result<Vec<LibraryRow>> {
    let mut stmt = conn.prepare(
        "SELECT kind, name, description, tags, path, body_hash \
         FROM library_items WHERE kind=?1 ORDER BY name",
    )?;
    let rows = stmt
        .query_map([kind], |r| {
            let tags: String = r.get(3)?;
            Ok(LibraryRow {
                kind: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                tags: if tags.is_empty() { vec![] } else { tags.split(',').map(|s| s.to_string()).collect() },
                path: r.get(4)?,
                body_hash: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Full-text search over library item name/description/tags. Returns matching
/// `LibraryRow`s (joined back from the FTS index by name).
pub fn library_search(conn: &Connection, query: &str) -> rusqlite::Result<Vec<LibraryRow>> {
    let mut stmt = conn.prepare(
        "SELECT li.kind, li.name, li.description, li.tags, li.path, li.body_hash \
         FROM library_fts f JOIN library_items li ON li.name = f.name \
         WHERE f.library_fts MATCH ?1 ORDER BY li.name",
    )?;
    let rows = stmt
        .query_map([query], |r| {
            let tags: String = r.get(3)?;
            Ok(LibraryRow {
                kind: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                tags: if tags.is_empty() { vec![] } else { tags.split(',').map(|s| s.to_string()).collect() },
                path: r.get(4)?,
                body_hash: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Open bugs for a project (status != 'resolved'), newest activity first.
pub fn open_bugs(conn: &Connection, project: &str) -> rusqlite::Result<Vec<BugRow>> {
    let mut stmt = conn.prepare(
        "SELECT project, bug_id, title, severity, status, first_seen, last_ts, resolution \
         FROM bugs WHERE project=?1 AND status != 'resolved' ORDER BY last_ts DESC",
    )?;
    let rows = stmt
        .query_map([project], |r| {
            Ok(BugRow {
                project: r.get(0)?,
                bug_id: r.get(1)?,
                title: r.get(2)?,
                severity: r.get(3)?,
                status: r.get(4)?,
                first_seen: r.get(5)?,
                last_ts: r.get(6)?,
                resolution: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{insert_event, insert_library_item};
    use crate::model::{EntityType, EventKind, EventRecord, LibraryRow};
    use crate::fold::fold_bugs;
    use crate::schema::init_schema;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn search_finds_library_item() {
        let conn = db();
        insert_library_item(&conn, &LibraryRow {
            kind: "skill".into(), name: "release".into(),
            description: "create a SemVer release".into(),
            tags: vec!["versioning".into()], path: "/r.md".into(), body_hash: "h".into(),
        }).unwrap();
        let hits = library_search(&conn, "SemVer").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "release");
    }

    #[test]
    fn open_bugs_excludes_resolved() {
        let conn = db();
        let mk = |id: &str, eid: &str, ts: &str, k: EventKind, p: serde_json::Value| EventRecord {
            id: id.into(), project: "p".into(), ts: ts.into(), kind: k,
            entity_type: EntityType::Bug, entity_id: eid.into(), session_id: None, payload: p,
        };
        insert_event(&conn, &mk("1", "open1", "2026-01-01T00:00:00Z", EventKind::BugOpened,
            serde_json::json!({"title":"A","severity":"low"}))).unwrap();
        insert_event(&conn, &mk("2", "done1", "2026-01-01T00:00:00Z", EventKind::BugOpened,
            serde_json::json!({"title":"B","severity":"low"}))).unwrap();
        insert_event(&conn, &mk("3", "done1", "2026-01-02T00:00:00Z", EventKind::BugResolved,
            serde_json::json!({"resolution":"fixed"}))).unwrap();
        fold_bugs(&conn).unwrap();

        let open = open_bugs(&conn, "p").unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].bug_id, "open1");
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p orrch-db query::`
Expected: PASS (2 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/orrch-db/src/query.rs
git commit -m "feat(orrch-db): typed query API (library list/search, open bugs)"
```

---

## Task 9: Incremental file watcher (`watch.rs`)

**Files:**
- Modify: `crates/orrch-db/src/watch.rs`

- [ ] **Step 1: Write the failing test**

Replace `crates/orrch-db/src/watch.rs` with:

```rust
use std::path::Path;

use rusqlite::Connection;

use crate::fold::fold_bugs;
use crate::ingest::{insert_event, record_source_file};
use crate::parse::parse_event;

/// Re-ingest a single changed event file and re-fold bugs. Used by the runtime
/// watcher so a new `events/*.md` write is reflected without a full rebuild.
///
/// Returns `true` if the file produced a usable event. `project` is the owning
/// project slug. The DB stays consistent because the file write is the commit;
/// this call is strictly downstream.
pub fn reingest_event_file(conn: &Connection, project: &str, path: &Path) -> anyhow::Result<bool> {
    let content = std::fs::read_to_string(path)?;
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    record_source_file(conn, &path.to_string_lossy(), mtime, &content)?;
    let parsed = parse_event(&content, project);
    let had = parsed.is_some();
    if let Some(ev) = parsed {
        insert_event(conn, &ev)?;
        fold_bugs(conn)?;
    }
    Ok(had)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::init_schema;
    use tempfile::tempdir;

    #[test]
    fn reingest_picks_up_a_new_event_file() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("20260529T010000-z.md");
        std::fs::write(
            &p,
            "---\nid: z\nts: 2026-05-29T01:00:00Z\nkind: bug_opened\n\
             entity_type: bug\nentity_id: nb\nseverity: low\ntitle: New\n---\nbody",
        )
        .unwrap();

        let ok = reingest_event_file(&conn, "p", &p).unwrap();
        assert!(ok);
        let n: i64 = conn.query_row("SELECT count(*) FROM bugs WHERE bug_id='nb'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}
```

> **Note on the live watcher:** wiring an actual `notify::RecommendedWatcher` to a long-running thread is integration glue, tested manually in Task 12, not unit-tested here. `reingest_event_file` is the unit the watcher callback invokes; it carries the logic and is fully tested. Keep the `notify` watcher thin: on a create/modify event under a `.orrch/events/` dir, resolve the owning project slug from the path and call `reingest_event_file`.

- [ ] **Step 2: Run the test**

Run: `cargo test -p orrch-db watch::`
Expected: PASS (1 test).

- [ ] **Step 3: Commit**

```bash
git add crates/orrch-db/src/watch.rs
git commit -m "feat(orrch-db): incremental re-ingest for changed event files"
```

---

## Task 10: One-time migrations (`migrate.rs`)

**Files:**
- Modify: `crates/orrch-db/src/migrate.rs`

- [ ] **Step 1: Write the failing test**

Replace `crates/orrch-db/src/migrate.rs` with:

```rust
use std::fs;
use std::path::Path;

/// Relocate a root `PLAN.md` / `DEVLOG.md` into `.orrch/` if present and not
/// already moved. Returns the list of relocated filenames. Idempotent: if the
/// `.orrch/` copy already exists, the root file is left alone (caller decides).
pub fn relocate_root_docs(project_dir: &Path) -> std::io::Result<Vec<String>> {
    let orrch = project_dir.join(".orrch");
    fs::create_dir_all(&orrch)?;
    let mut moved = Vec::new();
    for name in ["PLAN.md", "DEVLOG.md"] {
        let src = project_dir.join(name);
        let dst = orrch.join(name);
        if src.exists() && !dst.exists() {
            fs::rename(&src, &dst)?;
            moved.push(name.to_string());
        }
    }
    Ok(moved)
}

/// Convert a legacy `.retrospect/errors.jsonl` into one `events/*.md` file per
/// record. Each line is a JSON object with at least `fingerprint`, `category`,
/// `raw_context`, `timestamp`, `resolved`, and optionally `resolution`.
/// Returns the number of event files written. Idempotent by filename (derived
/// from fingerprint+timestamp); existing files are skipped.
pub fn migrate_errors_jsonl(project_dir: &Path) -> anyhow::Result<usize> {
    let src = project_dir.join(".retrospect").join("errors.jsonl");
    if !src.exists() {
        return Ok(0);
    }
    let events_dir = project_dir.join(".orrch").join("events");
    fs::create_dir_all(&events_dir)?;
    let content = fs::read_to_string(&src)?;
    let mut written = 0;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let fp = v.get("fingerprint").and_then(|x| x.as_str()).unwrap_or("nofp");
        let ts_secs = v.get("timestamp").and_then(|x| x.as_f64()).unwrap_or(0.0) as i64;
        let short = &fp[..fp.len().min(6)];
        let fname = format!("legacy-{ts_secs}-{short}.md");
        let dst = events_dir.join(&fname);
        if dst.exists() {
            continue;
        }
        let resolved = v.get("resolved").and_then(|x| x.as_bool()).unwrap_or(false);
        let kind = if resolved { "bug_resolved" } else { "bug_opened" };
        let category = v.get("category").and_then(|x| x.as_str()).unwrap_or("Unknown");
        let raw = v.get("raw_context").and_then(|x| x.as_str()).unwrap_or("");
        let resolution = v.get("resolution").and_then(|x| x.as_str()).unwrap_or("");
        // Lexical RFC3339-ish ts is not reconstructable from epoch secs without a
        // date lib; store the epoch in a sortable zero-padded form so ordering holds.
        let ts_field = format!("1970-epoch-{ts_secs:020}");
        let mut md = String::new();
        md.push_str("---\n");
        md.push_str(&format!("id: {short}{ts_secs}\n"));
        md.push_str(&format!("ts: {ts_field}\n"));
        md.push_str(&format!("kind: {kind}\n"));
        md.push_str("entity_type: bug\n");
        md.push_str(&format!("entity_id: {fp}\n"));
        md.push_str(&format!("title: {category} error\n"));
        md.push_str("severity: unknown\n");
        if resolved && !resolution.is_empty() {
            md.push_str(&format!("resolution: {resolution}\n"));
        }
        md.push_str("---\n");
        md.push_str(raw);
        md.push('\n');
        fs::write(&dst, md)?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn relocates_plan_and_devlog() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("PLAN.md"), "# Plan").unwrap();
        fs::write(tmp.path().join("DEVLOG.md"), "# Log").unwrap();
        let moved = relocate_root_docs(tmp.path()).unwrap();
        assert!(moved.contains(&"PLAN.md".to_string()));
        assert!(tmp.path().join(".orrch/PLAN.md").exists());
        assert!(!tmp.path().join("PLAN.md").exists());
        // Idempotent second run moves nothing.
        let again = relocate_root_docs(tmp.path()).unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn converts_errors_jsonl_to_event_files() {
        let tmp = tempdir().unwrap();
        let retro = tmp.path().join(".retrospect");
        fs::create_dir_all(&retro).unwrap();
        fs::write(
            retro.join("errors.jsonl"),
            "{\"fingerprint\":\"abcdef123\",\"category\":\"Type\",\"raw_context\":\"mismatched types\",\"timestamp\":1700000000.0,\"resolved\":false}\n",
        )
        .unwrap();
        let n = migrate_errors_jsonl(tmp.path()).unwrap();
        assert_eq!(n, 1);
        let dir = tmp.path().join(".orrch/events");
        let count = fs::read_dir(&dir).unwrap().count();
        assert_eq!(count, 1);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p orrch-db migrate::`
Expected: PASS (2 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/orrch-db/src/migrate.rs
git commit -m "feat(orrch-db): migrations — relocate root docs, errors.jsonl to events"
```

---

## Task 11: Repoint orrch-core's `PLAN.md` read path to `.orrch/`

**Files:**
- Modify: `crates/orrch-core/src/project.rs:305-312`

- [ ] **Step 1: Read the current code**

Run: `sed -n '300,315p' crates/orrch-core/src/project.rs`
Expected: shows a loop that builds `plan_path = path.join(plan_file)` and calls `parse_plan_file(&plan_path)`.

- [ ] **Step 2: Write the failing test**

Add this test to the `#[cfg(test)] mod tests` block at the bottom of `crates/orrch-core/src/project.rs` (create the block if absent):

```rust
#[test]
fn prefers_dot_orrch_plan_over_root() {
    let tmp = std::env::temp_dir().join(format!("orrch_plan_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join(".orrch")).unwrap();
    std::fs::write(tmp.join("PLAN.md"), "- [ ] root feature\n").unwrap();
    std::fs::write(tmp.join(".orrch/PLAN.md"), "- [ ] orrch feature\n").unwrap();

    let resolved = crate::project::resolve_plan_path(&tmp);
    assert!(resolved.ends_with(".orrch/PLAN.md"), "got {resolved:?}");
    let _ = std::fs::remove_dir_all(&tmp);
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p orrch-core prefers_dot_orrch_plan_over_root`
Expected: FAIL — `resolve_plan_path` does not exist.

- [ ] **Step 4: Implement `resolve_plan_path` and use it**

Add this function near the top of `crates/orrch-core/src/project.rs` (after the imports):

```rust
/// Resolve the canonical roadmap file for a project: prefer `.orrch/PLAN.md`,
/// fall back to a root `PLAN.md` for not-yet-migrated projects.
pub fn resolve_plan_path(project_dir: &std::path::Path) -> std::path::PathBuf {
    let dot = project_dir.join(".orrch").join("PLAN.md");
    if dot.exists() {
        return dot;
    }
    project_dir.join("PLAN.md")
}
```

Then change the read site. At `crates/orrch-core/src/project.rs:309`, replace:

```rust
            let plan_path = path.join(plan_file);
```

with:

```rust
            // Prefer .orrch/PLAN.md; fall back to root for un-migrated projects.
            let plan_path = if plan_file == "PLAN.md" {
                resolve_plan_path(path)
            } else {
                path.join(plan_file)
            };
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p orrch-core prefers_dot_orrch_plan_over_root`
Expected: PASS.

Run: `cargo test -p orrch-core`
Expected: PASS (no regressions in existing plan-parsing tests).

- [ ] **Step 6: Commit**

```bash
git add crates/orrch-core/src/project.rs
git commit -m "feat(orrch-core): prefer .orrch/PLAN.md with root fallback"
```

---

## Task 12: Wire `orrch-db` into the MCP library tools

**Files:**
- Modify: `crates/orrch-mcp/Cargo.toml`
- Modify: `crates/orrch-mcp/src/tools.rs` (the `library_search` and `list_skills`/`list_agents` arms of `dispatch`)

- [ ] **Step 1: Add the dependency**

In `crates/orrch-mcp/Cargo.toml`, under `[dependencies]`, add:

```toml
orrch-db = { path = "../orrch-db" }
```

- [ ] **Step 2: Locate the dispatch arms**

Run: `grep -n "\"library_search\"\|\"list_skills\"\|\"list_agents\"\|fn dispatch" crates/orrch-mcp/src/tools.rs`
Expected: shows the `dispatch` fn and the string arms that currently call into `orrch_library`.

- [ ] **Step 3: Add a helper that builds a rebuilt connection**

At the top of `crates/orrch-mcp/src/tools.rs` (after imports), add:

```rust
/// Build an ephemeral orrch-db connection by rebuilding from the standard
/// sources. Cheap (sub-second); the DB is purely a query accelerator.
fn orrch_db_conn() -> anyhow::Result<rusqlite::Connection> {
    use orrch_db::{rebuild_all, RebuildSources};
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".into());
    let library_root = std::path::PathBuf::from(&home)
        .join(".config").join("orrchestrator").join("library");
    let projects_root = std::path::PathBuf::from(&home).join("projects");
    let project_dirs = std::fs::read_dir(&projects_root)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let db_path = std::path::PathBuf::from(&home)
        .join(".cache").join("orrchestrator").join("orrch.db");
    Ok(rebuild_all(&db_path, &RebuildSources { project_dirs, library_root })?)
}
```

> Add `rusqlite = { workspace = true }` to `crates/orrch-mcp/Cargo.toml` `[dependencies]` as well, since the helper's return type names it.

- [ ] **Step 4: Replace the `library_search` arm**

In `dispatch`, find the `"library_search" =>` arm and replace its body so it queries the DB:

```rust
        "library_search" => {
            let q = arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
            match orrch_db_conn().and_then(|c| Ok(orrch_db::query::library_search(&c, q)?)) {
                Ok(hits) => {
                    if hits.is_empty() {
                        format!("No library items match '{q}'.")
                    } else {
                        hits.iter()
                            .map(|h| format!("- [{}] {} — {}", h.kind, h.name, h.description))
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                }
                Err(e) => format!("library_search error: {e}"),
            }
        }
```

- [ ] **Step 5: Replace the `list_skills` / `list_agents` arms**

Replace the `"list_skills" =>` arm body:

```rust
        "list_skills" => {
            match orrch_db_conn().and_then(|c| Ok(orrch_db::query::library_items_by_kind(&c, "skill")?)) {
                Ok(items) => items.iter()
                    .map(|i| format!("- {} — {}", i.name, i.description))
                    .collect::<Vec<_>>()
                    .join("\n"),
                Err(e) => format!("list_skills error: {e}"),
            }
        }
```

Replace the `"list_agents" =>` arm body the same way, with `"agent"` instead of `"skill"`.

- [ ] **Step 6: Update the tool-count test if present**

Run: `cargo test -p orrch-mcp 2>&1 | grep -i "tools.len\|assert_eq" | head`
If `protocol.rs`'s `test_tools_list_response` asserts a specific count (`33`), the count is unchanged (no tools added/removed), so it should still pass. If it fails, the number of tools changed unexpectedly — investigate before editing the assertion.

- [ ] **Step 7: Build + test the MCP crate**

Run: `cargo build -p orrch-mcp`
Expected: compiles.

Run: `cargo test -p orrch-mcp`
Expected: PASS.

- [ ] **Step 8: Manual end-to-end observation (REQUIRED — do not skip)**

This is the user-oriented verification the project's testing rules demand. Seed a real event and confirm a query sees it:

```bash
# Pick a scratch project under ~/projects, e.g. create one:
mkdir -p ~/projects/_orrchdb_smoke/.orrch/events
cat > ~/projects/_orrchdb_smoke/.orrch/events/20260529T120000-smoke.md <<'EOF'
---
id: smoke
ts: 2026-05-29T12:00:00Z
kind: bug_opened
entity_type: bug
entity_id: smoke-bug
severity: high
title: Smoke test bug
---
seeded by the implementation plan smoke test
EOF
```

Then add a tiny example binary or use a unit-style check. Create `crates/orrch-db/examples/smoke.rs`:

```rust
fn main() -> anyhow::Result<()> {
    let home = std::env::var("HOME")?;
    let conn = orrch_db::rebuild_all(
        &std::path::PathBuf::from(&home).join(".cache/orrchestrator/orrch.db"),
        &orrch_db::RebuildSources {
            project_dirs: vec![std::path::PathBuf::from(&home).join("projects/_orrchdb_smoke")],
            library_root: std::path::PathBuf::from(&home).join(".config/orrchestrator/library"),
        },
    )?;
    let bugs = orrch_db::query::open_bugs(&conn, "_orrchdb_smoke")?;
    println!("open bugs: {:#?}", bugs);
    assert_eq!(bugs.len(), 1, "expected the seeded smoke bug");
    println!("OK: query observed the seeded event");
    Ok(())
}
```

Run: `cargo run -p orrch-db --example smoke`
Expected output ends with: `OK: query observed the seeded event` and prints one `BugRow` with `title: "Smoke test bug"`, `status: "open"`.

Clean up: `rm -rf ~/projects/_orrchdb_smoke`

State the result as observed: "Ran the smoke example; the query returned the seeded open bug — confirmed the md→SQLite→query path works end to end."

- [ ] **Step 9: Commit**

```bash
git add crates/orrch-mcp/Cargo.toml crates/orrch-mcp/src/tools.rs crates/orrch-db/examples/smoke.rs
git commit -m "feat(orrch-mcp): library_search/list_skills/list_agents query orrch-db"
```

---

## Self-Review (completed during planning)

**Spec coverage:**
- Embedded SQLite, no daemon → Task 1 (rusqlite bundled). ✅
- Ephemeral, rebuilt-on-launch, zero durable state, no migrations → Task 7 (`rebuild_all` deletes + recreates). ✅
- `.orrch/` markdown source of truth → Tasks 4/7/10/11. ✅
- Snapshot docs vs append-only one-file-per-record events → Task 4 format + Task 7 scan. ✅
- Event-sourced fold → current state → Task 6. ✅
- Library metadata index + FTS (replace per-call file rescans) → Tasks 5/8/12. ✅
- Incremental re-ingest / write-file-first → Task 9. ✅
- Sharing via folder sync (no DB sync code) → satisfied by construction (no replication tasks). ✅
- Human-readable files preserved; DB is the only compressed artifact → no task degrades md; DB is the derived store. ✅
- Migrations: errors.jsonl → events, relocate root docs → Task 10. ✅
- PLAN.md relocation honored by readers → Task 11. ✅
- MCP tools query DB → Task 12. ✅
- Reuse existing library frontmatter parser → Tasks 4/7 use `parse_frontmatter_pub`/`extract_field_pub`/`extract_list_pub`. ✅

**Open spec items deferred (documented, not gaps):**
- Feature status event-sourcing — spec defaulted to snapshot-only initially; `features` table is populated from PLAN.md parsing (existing `plan_parser`), not events. A follow-up plan can add feature events if needed.
- `architecture.md` / `licensing.md` ingest — schema tables exist (Task 3) and `LibraryRow`-style ingest is straightforward; deferred to a follow-up since no canonical `architecture.md`/`licensing.md` files exist yet to parse. **Flagged: these two tables stay empty until a seeding task lands.**

**Placeholder scan:** none — every code step has complete code.

**Type consistency:** `EventRecord`/`BugRow`/`LibraryRow` field names are consistent across model/ingest/fold/query/rebuild; `library_items_by_kind` and `library_search` names match between query.rs (Task 8) and tools.rs (Task 12); `rebuild_all`/`RebuildSources` signature consistent between Task 7 and Task 12.
