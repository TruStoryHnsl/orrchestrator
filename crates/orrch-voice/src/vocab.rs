// Ported from mojovoice (MIT, Copyright (c) 2024-2026 itsdevcoffee).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabEntry {
    pub id: i64,
    pub term: String,
    pub added_at: i64,
    pub use_count: i64,
    pub source: String,
}

pub struct VocabStore {
    conn: Connection,
}

impl VocabStore {
    pub fn open() -> Result<Self> {
        Self::open_with_path(default_vocab_path()?)
    }

    pub fn open_with_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create vocab dir {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open vocab DB at {}", path.display()))?;
        let store = Self { conn };
        store.create_schema()?;
        Ok(store)
    }

    pub fn add_term(&self, term: &str) -> Result<()> {
        self.add_term_with_source(term, "manual")
    }

    pub fn add_term_with_source(&self, term: &str, source: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.conn
            .execute(
                "INSERT OR IGNORE INTO vocabulary (term, added_at, use_count, source)
                 VALUES (?1, ?2, 0, ?3)",
                params![term.trim(), now, source],
            )
            .context("failed to add vocab term")?;
        Ok(())
    }

    pub fn remove_term(&self, term: &str) -> Result<bool> {
        let rows = self
            .conn
            .execute("DELETE FROM vocabulary WHERE term = ?1", params![term])
            .context("failed to remove vocab term")?;
        Ok(rows > 0)
    }

    pub fn list_terms(&self) -> Result<Vec<VocabEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, term, added_at, use_count, source
             FROM vocabulary
             ORDER BY use_count DESC, term ASC",
        )?;
        let entries = stmt
            .query_map([], |row| {
                Ok(VocabEntry {
                    id: row.get(0)?,
                    term: row.get(1)?,
                    added_at: row.get(2)?,
                    use_count: row.get(3)?,
                    source: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to list vocab terms")?;
        Ok(entries)
    }

    pub fn get_prompt_string(&self, max_tokens: usize) -> Result<Option<String>> {
        let entries = self.list_terms()?;
        if entries.is_empty() {
            return Ok(None);
        }

        let char_budget = max_tokens * 4;
        let mut prompt = String::new();

        for entry in entries {
            let candidate = if prompt.is_empty() {
                entry.term
            } else {
                format!(", {}", entry.term)
            };

            if prompt.len() + candidate.len() > char_budget {
                break;
            }
            prompt.push_str(&candidate);
        }

        if prompt.is_empty() {
            Ok(None)
        } else {
            Ok(Some(prompt))
        }
    }

    pub fn increment_use_count(&self, term: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE vocabulary SET use_count = use_count + 1 WHERE term = ?1",
                params![term],
            )
            .context("failed to increment vocab use count")?;
        Ok(())
    }

    fn create_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS vocabulary (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                term      TEXT    NOT NULL UNIQUE,
                added_at  INTEGER NOT NULL,
                use_count INTEGER NOT NULL DEFAULT 0,
                source    TEXT    NOT NULL
            );",
        )?;
        Ok(())
    }
}

pub fn default_vocab_path() -> Result<PathBuf> {
    Ok(orrch_core::data_dir().join("voice").join("vocab.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_list_prompt_round_trip_uses_tmp_db() {
        let temp = tempfile::tempdir().unwrap();
        let store = VocabStore::open_with_path(temp.path().join("vocab.db")).unwrap();

        store.add_term("Orrchestrator").unwrap();
        store.add_term("Candle").unwrap();
        store.increment_use_count("Candle").unwrap();

        let terms = store.list_terms().unwrap();
        assert_eq!(terms.len(), 2);
        assert_eq!(terms[0].term, "Candle");
        assert_eq!(
            store.get_prompt_string(224).unwrap(),
            Some("Candle, Orrchestrator".to_string())
        );
    }

    #[test]
    fn remove_term_reports_whether_row_existed() {
        let temp = tempfile::tempdir().unwrap();
        let store = VocabStore::open_with_path(temp.path().join("vocab.db")).unwrap();
        store.add_term("Whisper").unwrap();

        assert!(store.remove_term("Whisper").unwrap());
        assert!(!store.remove_term("Whisper").unwrap());
    }
}
