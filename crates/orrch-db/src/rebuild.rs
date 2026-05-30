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
