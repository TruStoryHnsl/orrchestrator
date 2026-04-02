use std::path::{Path, PathBuf};
use crate::item::{ItemKind, LibraryItem};

/// Access layer for the library's git-backed storage.
pub struct LibraryStore {
    root: PathBuf,
}

impl LibraryStore {
    /// Open a library store at the given root directory.
    pub fn open(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }

    /// Default library path: ~/.config/orrchestrator/library/
    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
        PathBuf::from(home)
            .join(".config")
            .join("orrchestrator")
            .join("library")
    }

    /// List all items of a given kind.
    pub fn list(&self, kind: ItemKind) -> Vec<LibraryItem> {
        let dir = self.root.join(kind.directory());
        let mut items = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    if let Some(item) = self.load_item(&path, kind) {
                        items.push(item);
                    }
                }
            }
        }
        items.sort_by(|a, b| a.name.cmp(&b.name));
        items
    }

    /// List all items across all kinds.
    pub fn list_all(&self) -> Vec<LibraryItem> {
        let kinds = [
            ItemKind::Agent, ItemKind::Skill, ItemKind::Tool,
            ItemKind::McpServer, ItemKind::WorkforceTemplate, ItemKind::ApiKey,
        ];
        kinds.iter().flat_map(|k| self.list(*k)).collect()
    }

    /// Load a single library item from a .md file.
    fn load_item(&self, path: &Path, kind: ItemKind) -> Option<LibraryItem> {
        let content = std::fs::read_to_string(path).ok()?;
        let (frontmatter, body) = parse_frontmatter(&content)?;

        Some(LibraryItem {
            name: extract_field(&frontmatter, "name")?,
            kind,
            description: extract_field(&frontmatter, "description").unwrap_or_default(),
            tags: extract_list(&frontmatter, "tags"),
            content: body.trim().to_string(),
            path: path.to_path_buf(),
        })
    }
}

/// Parse YAML frontmatter delimited by `---` lines.
fn parse_frontmatter(content: &str) -> Option<(String, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..].trim_start_matches(['\r', '\n']);
    let end = after_first.find("\n---")?;
    let frontmatter = after_first[..end].to_string();
    let body = after_first[end + 4..].to_string();
    Some((frontmatter, body))
}

/// Extract a simple key: value field from YAML frontmatter.
fn extract_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let stripped = line.trim();
        if let Some(rest) = stripped.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix(':') {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Extract a YAML list (- item format) under a key.
fn extract_list(frontmatter: &str, key: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut in_list = false;
    for line in frontmatter.lines() {
        let stripped = line.trim();
        if stripped.starts_with(key) && stripped.contains(':') {
            in_list = true;
            continue;
        }
        if in_list {
            if let Some(item) = stripped.strip_prefix("- ") {
                items.push(item.trim().to_string());
            } else if !stripped.is_empty() && !stripped.starts_with('-') {
                break;
            }
        }
    }
    items
}

// ─── Public accessors for sibling modules ────────────────────────────

/// Public wrapper for frontmatter parsing (used by model.rs, harness.rs).
pub fn parse_frontmatter_pub(content: &str) -> Option<(String, String)> {
    parse_frontmatter(content)
}

/// Public wrapper for field extraction (used by model.rs, harness.rs).
pub fn extract_field_pub(frontmatter: &str, key: &str) -> Option<String> {
    extract_field(frontmatter, key)
}

/// Public wrapper for list extraction (used by model.rs, harness.rs).
pub fn extract_list_pub(frontmatter: &str, key: &str) -> Vec<String> {
    extract_list(frontmatter, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = "---\nname: Test Skill\ndescription: A test\n---\n\nBody content.";
        let (fm, body) = parse_frontmatter(content).unwrap();
        assert!(fm.contains("name: Test Skill"));
        assert!(body.contains("Body content"));
    }

    #[test]
    fn test_extract_list() {
        let fm = "tags:\n- rust\n- async\n- testing\nother: value";
        let tags = extract_list(fm, "tags");
        assert_eq!(tags, vec!["rust", "async", "testing"]);
    }

    #[test]
    fn test_default_path() {
        let path = LibraryStore::default_path();
        assert!(path.to_string_lossy().contains("orrchestrator"));
        assert!(path.to_string_lossy().contains("library"));
    }
}
