//! Audit trail for the instruction intake pipeline.
//!
//! Each instruction that flows through the COO intake pipeline is recorded as
//! an `AuditEntry`. Entries are appended as newline-delimited JSON (JSONL) to
//! `<project_dir>/.feedback/audit.jsonl`, enabling deduplication via the
//! source-chunk hash and a full audit trail of what was ingested and when.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::Path;

// ─── SHA-256 via sha2 ────────────────────────────────────────────────
use sha2::{Digest, Sha256};

// ─── Types ───────────────────────────────────────────────────────────

/// Source coordinates of the chunk inside the original feedback file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkCoordinate {
    /// 0-based line number of the first line of the chunk.
    pub line_start: usize,
    /// 0-based line number of the last line of the chunk (inclusive).
    pub line_end: usize,
    /// Byte offset of the first character of the chunk within the file.
    pub char_start: usize,
    /// Byte offset just past the last character of the chunk within the file.
    pub char_end: usize,
}

/// A single intake event: one source chunk → one optimized instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Instruction identifier, e.g. `"INS-004"`.
    pub instruction_id: String,
    /// Relative path to the feedback source file.
    pub source_file: String,
    /// SHA-256 hex digest of the trimmed source chunk text.
    /// Two instructions from the same source chunk produce the same hash —
    /// use this for deduplication and lookup.
    pub source_hash: String,
    /// Exact position of the chunk in the source file.
    pub coordinate: ChunkCoordinate,
    /// Verbatim source text (the raw chunk fed to the COO).
    pub raw_text: String,
    /// COO-generated optimized instruction.
    pub optimized_text: String,
    /// Unix timestamp (seconds since epoch) when this entry was created.
    pub created_at: u64,
}

// ─── Hash ────────────────────────────────────────────────────────────

/// Compute a SHA-256 digest of `text` (trimmed of leading/trailing whitespace).
///
/// Returns a lowercase hex string. Deterministic: same input always produces
/// the same output, enabling identity-based deduplication.
pub fn compute_source_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.trim().as_bytes());
    let result = hasher.finalize();
    // Format each byte as two lowercase hex digits
    result.iter().map(|b| format!("{b:02x}")).collect()
}

// ─── Persistence ─────────────────────────────────────────────────────

/// Path to the audit JSONL file for a project.
fn audit_path(project_dir: &Path) -> std::path::PathBuf {
    project_dir.join(".feedback").join("audit.jsonl")
}

/// Append a single `AuditEntry` to `<project_dir>/.feedback/audit.jsonl`.
///
/// Creates the `.feedback/` directory and the file if they do not yet exist.
/// Each call appends exactly one JSON line followed by a newline.
pub fn write_audit_entry(project_dir: &Path, entry: &AuditEntry) -> std::io::Result<()> {
    let path = audit_path(project_dir);

    // Ensure the parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string(entry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    file.write_all(json.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

/// Read all `AuditEntry` records from `<project_dir>/.feedback/audit.jsonl`.
///
/// - Returns an empty vec if the file does not exist.
/// - Silently skips lines that cannot be parsed (malformed JSON).
pub fn load_audit_entries(project_dir: &Path) -> Vec<AuditEntry> {
    let path = audit_path(project_dir);

    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(_) => return Vec::new(),
    };

    let reader = std::io::BufReader::new(file);
    reader
        .lines()
        .filter_map(|line| {
            let line = line.ok()?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            serde_json::from_str(trimmed).ok()
        })
        .collect()
}
