//! Ideas vault — simple plaintext note storage for undeveloped ideas.
//!
//! Stored in ~/projects/orrchestrator/plans/*.md

use std::fs;
use std::path::{Path, PathBuf};

/// A single idea/note in the vault.
#[derive(Debug, Clone)]
pub struct Idea {
    pub filename: String,
    pub title: String,
    pub preview: String,
    pub path: PathBuf,
}

/// Load all ideas from the vault directory.
pub fn load_ideas(vault_dir: &Path) -> Vec<Idea> {
    let _ = fs::create_dir_all(vault_dir);
    let mut ideas = Vec::new();

    if let Ok(entries) = fs::read_dir(vault_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Ok(contents) = fs::read_to_string(&path) {
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();

                    // Title: first non-empty line (strip # prefix)
                    let title = contents
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or(&filename)
                        .trim_start_matches('#')
                        .trim()
                        .to_string();

                    // Preview: first 80 chars of second non-empty line
                    let preview = contents
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .nth(1)
                        .unwrap_or("")
                        .chars()
                        .take(80)
                        .collect();

                    ideas.push(Idea {
                        filename,
                        title,
                        preview,
                        path,
                    });
                }
            }
        }
    }

    ideas.sort_by(|a, b| b.filename.cmp(&a.filename)); // newest first
    ideas
}

/// Get the vault directory path.
pub fn vault_dir(projects_dir: &Path) -> PathBuf {
    projects_dir.join("orrchestrator").join("plans")
}

/// Save a new idea to the vault.
pub fn save_idea(vault_dir: &Path, text: &str) -> anyhow::Result<PathBuf> {
    let _ = fs::create_dir_all(vault_dir);

    let timestamp = crate::feedback::chrono_lite_timestamp();
    let filename = format!("{}.md", timestamp.replace([':', ' '], "-"));
    let path = vault_dir.join(&filename);
    fs::write(&path, text)?;
    Ok(path)
}
