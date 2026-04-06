//! Persistent per-feature diff log.
//!
//! Records a timestamped list of change summaries keyed by feature id, stored
//! as a single JSON file at `<project_dir>/plans/.diff_log.json`.
//!
//! This is intentionally a standalone append-only API — there is no automatic
//! hook into the plan parser or status flips. Callers invoke `append_diff`
//! directly when they have a change to record.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    pub timestamp: String,
    pub summary: String,
    pub verified: bool,
}

/// On-disk shape: `{ feature_id: [entry, ...] }`.
type DiffMap = HashMap<String, Vec<DiffEntry>>;

/// Path to the diff log file for a project.
pub fn diff_log_path(project_dir: &Path) -> PathBuf {
    project_dir.join("plans").join(".diff_log.json")
}

/// Load the entire diff map from disk. Returns an empty map if the file
/// does not exist or fails to parse (resilient by design — this log is
/// advisory, not load-bearing).
fn load_map(project_dir: &Path) -> DiffMap {
    let path = diff_log_path(project_dir);
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => DiffMap::new(),
    }
}

/// Append a diff entry for `feature_id`. Creates the `plans/` directory and
/// the log file if they do not yet exist. Timestamps use the crate-local
/// ISO-ish formatter (`feedback::chrono_lite_timestamp`).
pub fn append_diff(project_dir: &Path, feature_id: &str, summary: &str) -> std::io::Result<()> {
    let plans_dir = project_dir.join("plans");
    if !plans_dir.exists() {
        std::fs::create_dir_all(&plans_dir)?;
    }

    let mut map = load_map(project_dir);
    let entry = DiffEntry {
        timestamp: crate::feedback::chrono_lite_timestamp(),
        summary: summary.to_string(),
        verified: false,
    };
    map.entry(feature_id.to_string()).or_default().push(entry);

    let path = diff_log_path(project_dir);
    let json = serde_json::to_string_pretty(&map)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// Load all diffs for a specific feature id. Empty vec if none.
pub fn load_diffs(project_dir: &Path, feature_id: &str) -> Vec<DiffEntry> {
    load_map(project_dir)
        .get(feature_id)
        .cloned()
        .unwrap_or_default()
}

/// Load all diffs for all features.
pub fn load_all_diffs(project_dir: &Path) -> HashMap<String, Vec<DiffEntry>> {
    load_map(project_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal stand-in for a scoped temp dir (tempfile is only a dev-dep in
    /// some workflows — the task spec asks us to avoid adding a runtime dep).
    struct ScopedDir(PathBuf);
    impl ScopedDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "orrch_diff_test_{}_{}_{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create scoped dir");
            Self(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for ScopedDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn append_and_load_diffs() {
        let dir = ScopedDir::new("append_and_load");

        append_diff(dir.path(), "F1", "added foo").unwrap();
        append_diff(dir.path(), "F1", "fixed bar").unwrap();

        let f1 = load_diffs(dir.path(), "F1");
        assert_eq!(f1.len(), 2, "F1 should have 2 entries");
        assert_eq!(f1[0].summary, "added foo");
        assert_eq!(f1[1].summary, "fixed bar");
        assert!(!f1[0].timestamp.is_empty());
        assert!(!f1[1].timestamp.is_empty());

        let f2 = load_diffs(dir.path(), "F2");
        assert!(f2.is_empty(), "F2 should have no entries");

        // plans/ directory should have been created
        assert!(dir.path().join("plans").exists());
        assert!(diff_log_path(dir.path()).exists());
    }

    #[test]
    fn load_all_diffs_returns_all_features() {
        let dir = ScopedDir::new("load_all");

        append_diff(dir.path(), "F1", "one").unwrap();
        append_diff(dir.path(), "F2", "two").unwrap();
        append_diff(dir.path(), "F1", "three").unwrap();

        let all = load_all_diffs(dir.path());
        assert_eq!(all.len(), 2);
        assert_eq!(all.get("F1").map(|v| v.len()).unwrap_or(0), 2);
        assert_eq!(all.get("F2").map(|v| v.len()).unwrap_or(0), 1);
    }
}
