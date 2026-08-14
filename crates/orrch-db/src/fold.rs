use rusqlite::{Connection, params};

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
    let mut acc: BTreeMap<
        (String, String),
        (String, String, String, String, String, Option<String>),
    > = BTreeMap::new();
    // tuple = (title, severity, status, first_seen, last_ts, resolution)

    for row in rows {
        let (project, bug_id, ts, kind, payload_json) = row?;
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
        let entry = acc.entry((project, bug_id)).or_insert_with(|| {
            (
                String::new(),
                "unknown".into(),
                "open".into(),
                ts.clone(),
                ts.clone(),
                None,
            )
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
                entry.5 = payload
                    .get("resolution")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
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
        insert_event(
            &conn,
            &ev(
                "e1",
                "2026-05-29T01:00:00Z",
                EventKind::BugOpened,
                serde_json::json!({"title":"Crash","severity":"high"}),
            ),
        )
        .unwrap();
        insert_event(
            &conn,
            &ev(
                "e2",
                "2026-05-29T02:00:00Z",
                EventKind::BugResolved,
                serde_json::json!({"resolution":"clamp slice on char boundary"}),
            ),
        )
        .unwrap();

        fold_bugs(&conn).unwrap();

        let (status, title, sev, res): (String, String, String, Option<String>) = conn
            .query_row(
                "SELECT status, title, severity, resolution FROM bugs WHERE bug_id='b1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
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
        insert_event(
            &conn,
            &ev(
                "e1",
                "2026-05-29T01:00:00Z",
                EventKind::BugOpened,
                serde_json::json!({"title":"X","severity":"low"}),
            ),
        )
        .unwrap();
        fold_bugs(&conn).unwrap();
        fold_bugs(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM bugs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
