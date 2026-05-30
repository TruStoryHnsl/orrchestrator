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
