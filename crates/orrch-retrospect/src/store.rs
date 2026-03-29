use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::error_parser::ErrorCategory;

/// A single error occurrence record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecord {
    pub fingerprint: String,
    pub category: ErrorCategory,
    pub raw_context: String,
    pub session_id: String,
    pub project_dir: String,
    pub timestamp: f64,
    pub resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_timestamp: Option<f64>,
}

impl ErrorRecord {
    pub fn new(
        fingerprint: String,
        category: ErrorCategory,
        raw_context: String,
        session_id: String,
        project_dir: String,
    ) -> Self {
        Self {
            fingerprint,
            category,
            raw_context,
            session_id,
            project_dir,
            timestamp: now(),
            resolved: false,
            resolution: None,
            resolution_timestamp: None,
        }
    }
}

/// Append-only JSONL error store for a project.
pub struct ErrorStore {
    store_dir: PathBuf,
    store_path: PathBuf,
    /// In-memory index: fingerprint → records
    index: HashMap<String, Vec<ErrorRecord>>,
    loaded: bool,
}

impl ErrorStore {
    pub fn new(project_dir: &Path) -> Self {
        let store_dir = project_dir.join(".retrospect");
        let store_path = store_dir.join("errors.jsonl");
        Self {
            store_dir,
            store_path,
            index: HashMap::new(),
            loaded: false,
        }
    }

    fn ensure_dir(&self) {
        let _ = fs::create_dir_all(&self.store_dir);
    }

    fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;

        if !self.store_path.exists() {
            return;
        }

        if let Ok(file) = fs::File::open(&self.store_path) {
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let Ok(line) = line else { continue };
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(record) = serde_json::from_str::<ErrorRecord>(line) {
                    self.index
                        .entry(record.fingerprint.clone())
                        .or_default()
                        .push(record);
                }
            }
        }
    }

    /// Append an error record to the store.
    pub fn append(&mut self, record: ErrorRecord) {
        self.ensure_dir();
        self.load();

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.store_path)
        {
            if let Ok(json) = serde_json::to_string(&record) {
                let _ = writeln!(file, "{json}");
            }
        }

        self.index
            .entry(record.fingerprint.clone())
            .or_default()
            .push(record);
    }

    /// Check if we've seen this error fingerprint before.
    pub fn has_fingerprint(&mut self, fp: &str) -> bool {
        self.load();
        self.index.contains_key(fp)
    }

    /// Get the most recent resolution for a fingerprint.
    pub fn get_resolution(&mut self, fp: &str) -> Option<&str> {
        self.load();
        self.index.get(fp).and_then(|records| {
            records
                .iter()
                .rev()
                .find(|r| r.resolved && r.resolution.is_some())
                .and_then(|r| r.resolution.as_deref())
        })
    }

    /// Mark all unresolved records for a fingerprint as resolved.
    pub fn mark_resolved(&mut self, fp: &str, resolution: &str) {
        self.load();

        // Update in-memory
        if let Some(records) = self.index.get_mut(fp) {
            for record in records.iter_mut() {
                if !record.resolved {
                    record.resolved = true;
                    record.resolution = Some(resolution.to_string());
                    record.resolution_timestamp = Some(now());
                }
            }

            // Append resolution marker to store file
            if let Some(first) = records.first() {
                let marker = ErrorRecord {
                    fingerprint: fp.to_string(),
                    category: first.category,
                    raw_context: format!("[RESOLVED] {resolution}"),
                    session_id: "retrospect".to_string(),
                    project_dir: first.project_dir.clone(),
                    timestamp: now(),
                    resolved: true,
                    resolution: Some(resolution.to_string()),
                    resolution_timestamp: Some(now()),
                };
                self.ensure_dir();
                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.store_path)
                {
                    if let Ok(json) = serde_json::to_string(&marker) {
                        let _ = writeln!(file, "{json}");
                    }
                }
            }
        }
    }

    /// Summary statistics.
    pub fn stats(&mut self) -> StoreStats {
        self.load();
        let total: usize = self.index.values().map(|v| v.len()).sum();
        let unique = self.index.len();
        let resolved = self
            .index
            .values()
            .filter(|recs| recs.iter().any(|r| r.resolved))
            .count();
        StoreStats {
            total_occurrences: total,
            unique_errors: unique,
            resolved,
            unresolved: unique - resolved,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_occurrences: usize,
    pub unique_errors: usize,
    pub resolved: usize,
    pub unresolved: usize,
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_store_append_and_query() {
        let tmp = TempDir::new().unwrap();
        let mut store = ErrorStore::new(tmp.path());

        let record = ErrorRecord::new(
            "abc123".into(),
            ErrorCategory::Lookup,
            "KeyError: 'x'".into(),
            "s1".into(),
            tmp.path().to_string_lossy().into(),
        );
        store.append(record);

        assert!(store.has_fingerprint("abc123"));
        assert!(!store.has_fingerprint("xyz999"));

        let stats = store.stats();
        assert_eq!(stats.unique_errors, 1);
        assert_eq!(stats.unresolved, 1);
    }

    #[test]
    fn test_store_resolution() {
        let tmp = TempDir::new().unwrap();
        let mut store = ErrorStore::new(tmp.path());

        let record = ErrorRecord::new(
            "abc123".into(),
            ErrorCategory::Lookup,
            "KeyError: 'x'".into(),
            "s1".into(),
            tmp.path().to_string_lossy().into(),
        );
        store.append(record);
        store.mark_resolved("abc123", "Fixed by adding default value");

        assert_eq!(
            store.get_resolution("abc123"),
            Some("Fixed by adding default value")
        );

        let stats = store.stats();
        assert_eq!(stats.resolved, 1);
        assert_eq!(stats.unresolved, 0);
    }

    #[test]
    fn test_store_persistence() {
        let tmp = TempDir::new().unwrap();

        // Write
        {
            let mut store = ErrorStore::new(tmp.path());
            store.append(ErrorRecord::new(
                "fp1".into(),
                ErrorCategory::Import,
                "ImportError: no module".into(),
                "s1".into(),
                tmp.path().to_string_lossy().into(),
            ));
        }

        // Read back
        {
            let mut store = ErrorStore::new(tmp.path());
            assert!(store.has_fingerprint("fp1"));
        }
    }
}
