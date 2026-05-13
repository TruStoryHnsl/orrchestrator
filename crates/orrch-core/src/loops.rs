//! Loop schedule storage for autonomous workflow loops.
//!
//! A loop is a series of workforces stacked sequentially that orrchestrator
//! manages programmatically — when one workforce completes (the cleanup
//! team's `cleanup_summary.md` is written), the loop closes that workforce's
//! sessions and starts the next workforce in the list, repeating when the
//! last workforce in the list finishes.
//!
//! Examples of useful loops:
//! - Continuous system optimization: monitoring → debugging → improving.
//! - Continuous autonomous software development.
//! - "Until done" sessions that iterate until a checker agent confirms the
//!   user's task is complete.
//!
//! Loops are stored as JSON at `<projects_dir>/.orrch/loops.json`. The TUI's
//! Hypervise > Loops sub-tab and the Oversee project action menu both read
//! and mutate this file.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One scheduled loop — a sequence of workflows that runs sequentially and
/// repeats when the final workflow finishes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopSchedule {
    /// Stable identifier (slug). Generated from `name` if unspecified.
    pub id: String,
    /// User-facing display name.
    pub name: String,
    /// Absolute path to the project this loop targets.
    pub project_dir: PathBuf,
    /// Ordered list of workflow names (workforce filename stems).
    /// Each workflow is invoked via the `workflow_call` MCP tool.
    pub workflows: Vec<String>,
    /// Whether the loop is currently running. The Hypervise > Loops sub-tab
    /// toggles this; the loop runner only acts on enabled schedules.
    pub enabled: bool,
    /// ISO 8601 timestamp of creation.
    pub created_at: String,
    /// Optional kind classifier — e.g. "continuous_dev", "until_done",
    /// "system_optimization". Free-form for now; reserved for future
    /// runner-side dispatch logic.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
}

impl LoopSchedule {
    /// Construct a fresh loop schedule with sensible defaults.
    pub fn new(name: impl Into<String>, project_dir: PathBuf, workflows: Vec<String>) -> Self {
        let name = name.into();
        let id = slugify(&name);
        Self {
            id,
            name,
            project_dir,
            workflows,
            enabled: false,
            created_at: iso_now(),
            kind: String::new(),
        }
    }
}

/// Resolve the loops storage path for a given top-level projects directory.
pub fn loops_path(projects_dir: &Path) -> PathBuf {
    projects_dir.join(".orrch").join("loops.json")
}

/// Load every saved loop schedule. Returns an empty vec if the file is
/// missing or unreadable.
pub fn load_loops(projects_dir: &Path) -> Vec<LoopSchedule> {
    let path = loops_path(projects_dir);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&content).unwrap_or_default()
}

/// Persist the supplied loop schedules to disk. Creates the parent `.orrch/`
/// directory if missing. Overwrites the existing file atomically.
pub fn save_loops(projects_dir: &Path, loops: &[LoopSchedule]) -> std::io::Result<()> {
    let path = loops_path(projects_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(loops).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Add a new loop schedule. If a loop with the same id already exists,
/// it's replaced. Returns the resulting full list.
pub fn upsert_loop(
    projects_dir: &Path,
    schedule: LoopSchedule,
) -> std::io::Result<Vec<LoopSchedule>> {
    let mut loops = load_loops(projects_dir);
    loops.retain(|l| l.id != schedule.id);
    loops.push(schedule);
    save_loops(projects_dir, &loops)?;
    Ok(loops)
}

/// Toggle the enabled flag for the loop with the given id. Returns true
/// if a loop matched and was toggled.
pub fn toggle_loop(projects_dir: &Path, id: &str) -> std::io::Result<bool> {
    let mut loops = load_loops(projects_dir);
    let Some(schedule) = loops.iter_mut().find(|l| l.id == id) else {
        return Ok(false);
    };
    schedule.enabled = !schedule.enabled;
    save_loops(projects_dir, &loops)?;
    Ok(true)
}

/// Remove the loop with the given id. Returns true if a loop was removed.
pub fn delete_loop(projects_dir: &Path, id: &str) -> std::io::Result<bool> {
    let mut loops = load_loops(projects_dir);
    let before = loops.len();
    loops.retain(|l| l.id != id);
    if loops.len() != before {
        save_loops(projects_dir, &loops)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn slugify(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn iso_now() -> String {
    crate::usage::iso_now()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_dir() -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("orrch-loops-test-{nonce}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = fresh_dir();
        let s = LoopSchedule::new(
            "Continuous Dev",
            dir.join("orrchestrator"),
            vec!["general_software_development".into(), "commercial_software_development".into()],
        );
        save_loops(&dir, &[s.clone()]).unwrap();
        let loaded = load_loops(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], s);
    }

    #[test]
    fn upsert_replaces_existing_id() {
        let dir = fresh_dir();
        let mut s1 = LoopSchedule::new(
            "Same Name",
            dir.join("p1"),
            vec!["a".into()],
        );
        s1.enabled = false;
        save_loops(&dir, &[s1.clone()]).unwrap();

        let mut s2 = s1.clone();
        s2.enabled = true;
        s2.workflows = vec!["b".into(), "c".into()];
        let after = upsert_loop(&dir, s2.clone()).unwrap();
        assert_eq!(after.len(), 1);
        assert!(after[0].enabled);
        assert_eq!(after[0].workflows, vec!["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn toggle_flips_enabled() {
        let dir = fresh_dir();
        let s = LoopSchedule::new("toggle me", dir.join("p"), vec!["x".into()]);
        let id = s.id.clone();
        save_loops(&dir, &[s]).unwrap();

        assert!(toggle_loop(&dir, &id).unwrap());
        assert!(load_loops(&dir)[0].enabled);
        assert!(toggle_loop(&dir, &id).unwrap());
        assert!(!load_loops(&dir)[0].enabled);
    }

    #[test]
    fn delete_removes_matching_loop() {
        let dir = fresh_dir();
        let s = LoopSchedule::new("kill me", dir.join("p"), vec!["x".into()]);
        let id = s.id.clone();
        save_loops(&dir, &[s]).unwrap();
        assert!(delete_loop(&dir, &id).unwrap());
        assert!(load_loops(&dir).is_empty());
        // Second delete returns false (no-op).
        assert!(!delete_loop(&dir, &id).unwrap());
    }

    #[test]
    fn empty_load_when_no_file() {
        let dir = fresh_dir();
        // Don't save anything; load should yield empty vec.
        assert!(load_loops(&dir).is_empty());
    }
}
