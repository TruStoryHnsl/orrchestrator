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
//! Loops are stored as JSON at the global app-data `loops.json`. The TUI's
//! Hypervise > Loops sub-tab and the Oversee project action menu both read
//! and mutate this file. A read fallback preserves legacy
//! `<projects_dir>/.orrch/loops.json` data until migration runs.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::context_location::{artifact_path, Artifact};

/// LOOP-011 — the destructiveness class of a loop. Two top-level kinds; Support
/// carries a sub-kind purely for UI/diagnostics (it does NOT change the policy —
/// every Support sub-kind is equally non-destructive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoopClass {
    /// Destructive / may-modify-code: commercial & general software development.
    /// Full tool surface; may write/edit code, run mutating bash, git commit.
    Dev,
    /// Non-destructive support loop. Reads/analyzes only; writes `.md` artifacts
    /// to support the dev agents. NEVER mutates code, NEVER commits. The sub-kind
    /// is informational (Planning/Analysis/Testing/Research/Critique).
    Support(SupportKind),
}

/// Informational sub-kind of a Support loop. All sub-kinds share the SAME
/// (non-destructive) ToolPolicy — this only labels the loop for the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SupportKind {
    #[default]
    Planning,
    Analysis,
    Testing,
    Research,
    Critique,
}

impl Default for LoopClass {
    /// Legacy loops (no `class` key) materialize as Dev — preserves prior
    /// behavior (every existing loop could modify code).
    fn default() -> Self {
        LoopClass::Dev
    }
}

impl LoopClass {
    /// True when this class may mutate code / commit (Dev only).
    pub fn is_destructive(self) -> bool {
        matches!(self, LoopClass::Dev)
    }
    /// True for any Support sub-kind (non-destructive).
    pub fn is_support(self) -> bool {
        matches!(self, LoopClass::Support(_))
    }
}

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
    /// LOOP-011 destructiveness class. serde-defaulted to Dev so every legacy
    /// loops.json (and the loops.rs round-trip tests) parse unchanged.
    #[serde(default)]
    pub class: LoopClass,
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
            class: LoopClass::default(),
        }
    }
}

/// Resolve the canonical global loops storage path.
pub fn loops_path(_projects_dir: &Path) -> PathBuf {
    artifact_path(Artifact::LoopRegistry, None)
        .expect("global loop registry path should not require a project")
}

/// Resolve the legacy loops storage path under the top-level projects directory.
pub fn legacy_loops_path(projects_dir: &Path) -> PathBuf {
    projects_dir.join(".orrch").join("loops.json")
}

/// Load every saved loop schedule. Returns an empty vec if the file is
/// missing or unreadable. Falls back to the legacy projects-dir location only
/// when the canonical global file does not exist.
pub fn load_loops(projects_dir: &Path) -> Vec<LoopSchedule> {
    let canonical = loops_path(projects_dir);
    let legacy = legacy_loops_path(projects_dir);
    let path = if canonical.exists() {
        canonical
    } else if legacy.exists() {
        legacy
    } else {
        canonical
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&content).unwrap_or_default()
}

/// Persist the supplied loop schedules to the canonical global path. Creates
/// the parent directory if missing. Overwrites the existing file atomically.
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
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
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

    struct HomeGuard {
        old_home: Option<String>,
    }

    impl HomeGuard {
        fn set(home: &Path) -> Self {
            let old_home = std::env::var("HOME").ok();
            unsafe { std::env::set_var("HOME", home) };
            Self { old_home }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.old_home {
                    Some(home) => std::env::set_var("HOME", home),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
    }

    fn fresh_dir() -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("orrch-loops-test-{nonce}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn isolated_home() -> (std::sync::MutexGuard<'static, ()>, HomeGuard, PathBuf) {
        let lock = crate::shadow::TEST_ENV_LOCK.lock().unwrap();
        let home = fresh_dir();
        let guard = HomeGuard::set(&home);
        (lock, guard, home)
    }

    #[test]
    fn save_and_load_round_trip() {
        let (_lock, _home_guard, _home) = isolated_home();
        let dir = fresh_dir();
        let s = LoopSchedule::new(
            "Continuous Dev",
            dir.join("orrchestrator"),
            vec![
                "general_software_development".into(),
                "commercial_software_development".into(),
            ],
        );
        save_loops(&dir, &[s.clone()]).unwrap();
        assert!(loops_path(&dir).exists());
        assert!(!legacy_loops_path(&dir).exists());
        let loaded = load_loops(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], s);
    }

    #[test]
    fn upsert_replaces_existing_id() {
        let (_lock, _home_guard, _home) = isolated_home();
        let dir = fresh_dir();
        let mut s1 = LoopSchedule::new("Same Name", dir.join("p1"), vec!["a".into()]);
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
        let (_lock, _home_guard, _home) = isolated_home();
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
        let (_lock, _home_guard, _home) = isolated_home();
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
        let (_lock, _home_guard, _home) = isolated_home();
        let dir = fresh_dir();
        // Don't save anything; load should yield empty vec.
        assert!(load_loops(&dir).is_empty());
    }

    #[test]
    fn load_falls_back_to_legacy_and_save_writes_canonical() {
        let (_lock, _home_guard, _home) = isolated_home();
        let dir = fresh_dir();
        let legacy_path = legacy_loops_path(&dir);
        std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        let s = LoopSchedule::new("legacy loop", dir.join("p"), vec!["legacy".into()]);
        std::fs::write(
            &legacy_path,
            serde_json::to_string_pretty(&vec![s.clone()]).unwrap(),
        )
        .unwrap();

        assert!(!loops_path(&dir).exists());
        let loaded = load_loops(&dir);
        assert_eq!(loaded, vec![s.clone()]);

        save_loops(&dir, &[s]).unwrap();

        assert!(loops_path(&dir).exists());
        assert!(legacy_path.exists());
        let canonical = load_loops(&dir);
        assert_eq!(canonical.len(), 1);
        assert_eq!(canonical[0].name, "legacy loop");
    }

    // ── LOOP-011: LoopClass ──────────────────────────────────────────────

    #[test]
    fn loop_class_default_is_dev() {
        assert_eq!(LoopClass::default(), LoopClass::Dev);
        assert!(LoopSchedule::new("x", PathBuf::from("/p"), vec![]).class == LoopClass::Dev);
    }

    #[test]
    fn loop_class_is_support_and_is_destructive() {
        assert!(LoopClass::Dev.is_destructive());
        assert!(!LoopClass::Dev.is_support());
        for k in [
            SupportKind::Planning,
            SupportKind::Analysis,
            SupportKind::Testing,
            SupportKind::Research,
            SupportKind::Critique,
        ] {
            let c = LoopClass::Support(k);
            assert!(c.is_support());
            assert!(!c.is_destructive());
        }
    }

    #[test]
    fn legacy_loop_json_without_class_deserializes_to_dev() {
        // JSON with NO `class` key (a pre-existing loops.json) → class=Dev.
        let json = r#"[{
            "id": "legacy",
            "name": "Legacy",
            "project_dir": "/p",
            "workflows": ["a"],
            "enabled": false,
            "created_at": "2026-01-01T00:00:00Z"
        }]"#;
        let loaded: Vec<LoopSchedule> = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].class, LoopClass::Dev);
    }

    #[test]
    fn loop_class_serde_tokens_are_kebab_case() {
        // Dev → "dev"
        let dev = serde_json::to_string(&LoopClass::Dev).unwrap();
        assert_eq!(dev, "\"dev\"");
        // Support(Planning) → externally-tagged map with kebab token
        let sup = serde_json::to_string(&LoopClass::Support(SupportKind::Planning)).unwrap();
        assert_eq!(sup, "{\"support\":\"planning\"}");
        // round-trip
        let back: LoopClass = serde_json::from_str(&sup).unwrap();
        assert_eq!(back, LoopClass::Support(SupportKind::Planning));
    }

    #[test]
    fn schedule_with_support_class_round_trips() {
        let (_lock, _home_guard, _home) = isolated_home();
        let dir = fresh_dir();
        let mut s = LoopSchedule::new("Planner", dir.join("p"), vec!["plan".into()]);
        s.class = LoopClass::Support(SupportKind::Planning);
        save_loops(&dir, &[s.clone()]).unwrap();
        let loaded = load_loops(&dir);
        assert_eq!(loaded[0], s);
        assert!(loaded[0].class.is_support());
    }
}
