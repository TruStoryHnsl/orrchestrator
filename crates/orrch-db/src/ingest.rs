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
    // Keep the content-table FTS index in sync. We insert using the rowid of
    // the library_items row so FTS5 can resolve content back from the table.
    conn.execute(
        "INSERT INTO library_fts (rowid, name, description, tags) \
         SELECT rowid, name, description, tags FROM library_items WHERE name=?1",
        params![row.name],
    )?;
    Ok(())
}

/// Record a source file's mtime + content hash so incremental rebuilds can
/// skip unchanged files.
pub fn record_source_file(conn: &Connection, path: &str, mtime: i64, content: &str) -> rusqlite::Result<()> {
    let hash: String = Sha256::digest(content.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
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
