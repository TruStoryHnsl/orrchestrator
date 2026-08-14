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
                tags: if tags.is_empty() {
                    vec![]
                } else {
                    tags.split(',').map(|s| s.to_string()).collect()
                },
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
                tags: if tags.is_empty() {
                    vec![]
                } else {
                    tags.split(',').map(|s| s.to_string()).collect()
                },
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
    use crate::fold::fold_bugs;
    use crate::ingest::{insert_event, insert_library_item};
    use crate::model::{EntityType, EventKind, EventRecord, LibraryRow};
    use crate::schema::init_schema;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn search_finds_library_item() {
        let conn = db();
        insert_library_item(
            &conn,
            &LibraryRow {
                kind: "skill".into(),
                name: "release".into(),
                description: "create a SemVer release".into(),
                tags: vec!["versioning".into()],
                path: "/r.md".into(),
                body_hash: "h".into(),
            },
        )
        .unwrap();
        let hits = library_search(&conn, "SemVer").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "release");
    }

    #[test]
    fn open_bugs_excludes_resolved() {
        let conn = db();
        let mk = |id: &str, eid: &str, ts: &str, k: EventKind, p: serde_json::Value| EventRecord {
            id: id.into(),
            project: "p".into(),
            ts: ts.into(),
            kind: k,
            entity_type: EntityType::Bug,
            entity_id: eid.into(),
            session_id: None,
            payload: p,
        };
        insert_event(
            &conn,
            &mk(
                "1",
                "open1",
                "2026-01-01T00:00:00Z",
                EventKind::BugOpened,
                serde_json::json!({"title":"A","severity":"low"}),
            ),
        )
        .unwrap();
        insert_event(
            &conn,
            &mk(
                "2",
                "done1",
                "2026-01-01T00:00:00Z",
                EventKind::BugOpened,
                serde_json::json!({"title":"B","severity":"low"}),
            ),
        )
        .unwrap();
        insert_event(
            &conn,
            &mk(
                "3",
                "done1",
                "2026-01-02T00:00:00Z",
                EventKind::BugResolved,
                serde_json::json!({"resolution":"fixed"}),
            ),
        )
        .unwrap();
        fold_bugs(&conn).unwrap();

        let open = open_bugs(&conn, "p").unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].bug_id, "open1");
    }
}
