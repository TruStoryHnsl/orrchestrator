use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// A registered AI coding harness in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessEntry {
    pub name: String,
    pub command: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub limitations: Vec<String>,
    pub supported_models: Vec<String>,
    pub flags: Vec<String>,
    pub available: bool,
    pub notes: String,
    /// ISO 8601 date/time of the last Mentor-triggered refresh of this entry.
    /// Populated from the YAML frontmatter field `last_checked`; `None` means
    /// the entry has never been verified (PLAN item 58).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked: Option<String>,
    #[serde(skip)]
    pub path: PathBuf,
}

impl HarnessEntry {
    pub fn summary_line(&self) -> String {
        let status = if self.available { "●" } else { "○" };
        format!("{} {} — {}", status, self.name, self.description)
    }
}

/// Load harness entries from .md files in a directory.
pub fn load_harnesses(dir: &Path) -> Vec<HarnessEntry> {
    let mut harnesses = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Some(mut h) = parse_harness_file(&path) {
                    // Auto-detect availability
                    h.available = which_exists(&h.command);
                    harnesses.push(h);
                }
            }
        }
    }
    harnesses.sort_by(|a, b| b.available.cmp(&a.available).then(a.name.cmp(&b.name)));
    harnesses
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn parse_harness_file(path: &Path) -> Option<HarnessEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let (fm, body) = crate::store::parse_frontmatter_pub(&content)?;

    Some(HarnessEntry {
        name: extract(&fm, "name")?,
        command: extract(&fm, "command").unwrap_or_default(),
        description: extract(&fm, "description").unwrap_or_default(),
        capabilities: extract_list(&fm, "capabilities"),
        limitations: extract_list(&fm, "limitations"),
        supported_models: extract_list(&fm, "supported_models"),
        flags: extract_list(&fm, "flags"),
        available: false, // set by load_harnesses
        notes: body.trim().to_string(),
        last_checked: extract(&fm, "last_checked"),
        path: path.to_path_buf(),
    })
}

fn extract(fm: &str, key: &str) -> Option<String> {
    crate::store::extract_field_pub(fm, key)
}

fn extract_list(fm: &str, key: &str) -> Vec<String> {
    crate::store::extract_list_pub(fm, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_summary() {
        let h = HarnessEntry {
            name: "Claude Code".into(),
            command: "claude".into(),
            description: "Full agentic coding".into(),
            capabilities: vec!["tool_use".into(), "subagents".into()],
            limitations: vec![],
            supported_models: vec!["claude-opus-4-6".into()],
            flags: vec!["--dangerously-skip-permissions".into()],
            available: true,
            notes: String::new(),
            last_checked: None,
            path: PathBuf::new(),
        };
        assert!(h.summary_line().contains("●"));
        assert!(h.summary_line().contains("Claude Code"));
    }

    #[test]
    fn test_harness_last_checked_defaults_none() {
        // parse_harness_file should return None for last_checked when the
        // frontmatter has no such field (the real claude_code.md doesn't).
        let tmp = std::env::temp_dir().join("orrch_last_checked_test.md");
        std::fs::write(
            &tmp,
            "---\nname: LCtest\ncommand: lctest\n---\n\nBody.\n",
        ).unwrap();
        let parsed = parse_harness_file(&tmp).unwrap();
        assert_eq!(parsed.last_checked, None);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_harness_last_checked_parses_when_present() {
        let tmp = std::env::temp_dir().join("orrch_last_checked_test2.md");
        std::fs::write(
            &tmp,
            "---\nname: LCtest\ncommand: lctest\nlast_checked: 2026-04-08\n---\n\nBody.\n",
        ).unwrap();
        let parsed = parse_harness_file(&tmp).unwrap();
        assert_eq!(parsed.last_checked.as_deref(), Some("2026-04-08"));
        let _ = std::fs::remove_file(&tmp);
    }
}
