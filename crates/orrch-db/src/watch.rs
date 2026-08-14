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
        let n: i64 = conn
            .query_row("SELECT count(*) FROM bugs WHERE bug_id='nb'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
    }
}
