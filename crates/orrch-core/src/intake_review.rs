//! Pending instruction-intake reviews — pulled from per-idea workspaces.
//!
//! The intake skill produces a `review.json` file in each idea's per-idea
//! workspace at `<vault>/.pipeline/<idea_stem>/review.json`. This module
//! finds the oldest pending review and exposes it to the TUI for the user
//! to confirm/edit/reject.
//!
//! For backward compatibility, this module also scans legacy
//! `<project>/.orrch/intake_review.json` paths so reviews written before
//! the workspace migration are still surfaced.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::Project;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct IntakeReviewFile {
    pub raw: String,
    pub optimized: String,
    /// "pending" | "confirmed" | "rejected".
    /// For legacy compat, "pending_review" is also accepted on read.
    pub status: String,
    /// Idea filename in the vault (e.g. "2026-04-21-00-14.md") that this
    /// review originated from. Optional only for legacy review files
    /// written before the per-idea workspace migration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_idea: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IntakeReview {
    pub raw: String,
    pub optimized: String,
    /// Vault filename of the source idea, if known. Used to advance the
    /// idea's pipeline progress when the user confirms/rejects.
    pub source_idea: Option<String>,
    /// Directory containing the review.json file. The TUI writes scratch
    /// files (e.g. the nvim edit buffer) here, never into the project tree.
    pub workspace: PathBuf,
    /// Path to the review.json file itself.
    pub source_path: PathBuf,
}

fn is_pending_status(status: &str) -> bool {
    status == "pending" || status == "pending_review"
}

/// Find a review file inside a workspace, accepting either the canonical
/// `review.json` name or the legacy `intake_review.json` name. Agents have
/// been observed writing the latter despite the skill instructions.
fn find_review_file(workspace: &Path) -> Option<PathBuf> {
    let canonical = workspace.join("review.json");
    if canonical.exists() {
        return Some(canonical);
    }
    let legacy = workspace.join("intake_review.json");
    if legacy.exists() {
        return Some(legacy);
    }
    None
}

/// Find the oldest pending intake review.
///
/// Scans, in order:
///   1. `<vault>/.pipeline/*/review.json` — the canonical per-idea workspaces
///   2. `<project>/.orrch/intake_review.json` — legacy fallback for reviews
///      written by the pre-migration skill
///
/// Within each set, the file with the oldest mtime wins so that submissions
/// are surfaced in FIFO order.
pub fn load_intake_review(vault_dir: &Path, projects: &[Project]) -> Option<IntakeReview> {
    let mut candidates: Vec<(SystemTime, IntakeReview)> = Vec::new();

    // 1. Scan per-idea workspaces under .pipeline/
    let pipeline_dir = vault_dir.join(".pipeline");
    if let Ok(entries) = std::fs::read_dir(&pipeline_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let review_file = match find_review_file(&path) {
                Some(p) => p,
                None => continue,
            };
            let bytes = match std::fs::read(&review_file) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let parsed: IntakeReviewFile = match serde_json::from_slice(&bytes) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !is_pending_status(&parsed.status) {
                continue;
            }
            let mtime = std::fs::metadata(&review_file)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            // If the review file lacks an explicit source_idea (legacy file
            // that landed in a workspace dir somehow), recover it from the
            // workspace directory name.
            let source_idea = parsed.source_idea.clone().or_else(|| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|stem| format!("{stem}.md"))
            });
            candidates.push((
                mtime,
                IntakeReview {
                    raw: parsed.raw,
                    optimized: parsed.optimized,
                    source_idea,
                    workspace: path,
                    source_path: review_file,
                },
            ));
        }
    }

    // 2. Legacy: scan project .orrch/intake_review.json files
    for proj in projects {
        let path = proj.path.join(".orrch").join("intake_review.json");
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let parsed: IntakeReviewFile = match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !is_pending_status(&parsed.status) {
            continue;
        }
        let mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        candidates.push((
            mtime,
            IntakeReview {
                raw: parsed.raw,
                optimized: parsed.optimized,
                source_idea: parsed.source_idea,
                workspace: proj.path.join(".orrch"),
                source_path: path,
            },
        ));
    }

    candidates.into_iter().min_by_key(|(t, _)| *t).map(|(_, r)| r)
}

/// Load a single review file from a specific workspace directory, even if
/// its status is not "pending". Used by the TUI's `r` (review) handler to
/// pull up a review for an idea regardless of state.
pub fn load_review_at(workspace: &Path) -> Option<IntakeReview> {
    let review_file = find_review_file(workspace)?;
    let bytes = std::fs::read(&review_file).ok()?;
    let parsed: IntakeReviewFile = serde_json::from_slice(&bytes).ok()?;
    let source_idea = parsed.source_idea.clone().or_else(|| {
        workspace
            .file_name()
            .and_then(|n| n.to_str())
            .map(|stem| format!("{stem}.md"))
    });
    Some(IntakeReview {
        raw: parsed.raw,
        optimized: parsed.optimized,
        source_idea,
        workspace: workspace.to_path_buf(),
        source_path: review_file,
    })
}

/// Task 27b: distribute a COO-optimized instruction block from an intake
/// review into a target project's inbox, then run `truncate_inbox_if_large`
/// on that project's inbox so the one-time compression sweep fires at write
/// time rather than waiting for the periodic walker tick.
///
/// `append_to_inbox` already invokes the truncation hook internally, but this
/// helper lets the intake_review distribution path call a single function and
/// makes the compression contract explicit at the call site.
pub fn distribute_to_inbox_from_intake(
    optimized_text: &str,
    project_dir: &Path,
    timestamp: &str,
    max_bytes: usize,
) -> anyhow::Result<()> {
    crate::feedback::append_to_inbox(optimized_text, project_dir, timestamp)?;
    // Redundant with the hook inside append_to_inbox but explicit per the
    // Task 27b acceptance criterion. `truncate_inbox_if_large` is idempotent
    // and a no-op when the file is already under the cap, so running it
    // twice is harmless and keeps the contract visible in both entry points.
    let _ = crate::feedback::truncate_inbox_if_large(project_dir, max_bytes);
    Ok(())
}

/// Write the user's decision back to the review file. Preserves source_idea
/// so subsequent loads can still trace the file back to its origin idea.
pub fn write_intake_decision(
    review: &IntakeReview,
    decision: &str,
    optimized: &str,
) -> anyhow::Result<()> {
    let file = IntakeReviewFile {
        raw: review.raw.clone(),
        optimized: optimized.to_string(),
        status: decision.to_string(),
        source_idea: review.source_idea.clone(),
    };
    let json = serde_json::to_string_pretty(&file)?;
    let tmp = review.source_path.with_extension("tmp");
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, &review.source_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_pending_accepts_legacy_status() {
        assert!(is_pending_status("pending"));
        assert!(is_pending_status("pending_review"));
        assert!(!is_pending_status("confirmed"));
        assert!(!is_pending_status("rejected"));
    }

    #[test]
    fn test_load_review_from_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        let ws = vault.join(".pipeline").join("2026-04-21-00-14");
        std::fs::create_dir_all(&ws).unwrap();
        let review = serde_json::json!({
            "raw": "raw text",
            "optimized": "OPT-001 do thing",
            "status": "pending",
            "source_idea": "2026-04-21-00-14.md",
        });
        std::fs::write(
            ws.join("review.json"),
            serde_json::to_string_pretty(&review).unwrap(),
        )
        .unwrap();

        let result = load_intake_review(vault, &[]);
        let result = result.expect("review should be found");
        assert_eq!(result.source_idea.as_deref(), Some("2026-04-21-00-14.md"));
        assert_eq!(result.optimized, "OPT-001 do thing");
        assert_eq!(result.workspace, ws);
    }

    #[test]
    fn test_load_review_recovers_source_idea_from_dir_name() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        let ws = vault.join(".pipeline").join("test-idea");
        std::fs::create_dir_all(&ws).unwrap();
        // Note: no source_idea field
        let review = serde_json::json!({
            "raw": "r",
            "optimized": "o",
            "status": "pending",
        });
        std::fs::write(
            ws.join("review.json"),
            serde_json::to_string_pretty(&review).unwrap(),
        )
        .unwrap();

        let result = load_intake_review(vault, &[]).unwrap();
        assert_eq!(result.source_idea.as_deref(), Some("test-idea.md"));
    }

    #[test]
    fn test_load_skips_non_pending() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        let ws = vault.join(".pipeline").join("done");
        std::fs::create_dir_all(&ws).unwrap();
        let review = serde_json::json!({
            "raw": "r",
            "optimized": "o",
            "status": "confirmed",
            "source_idea": "done.md",
        });
        std::fs::write(
            ws.join("review.json"),
            serde_json::to_string_pretty(&review).unwrap(),
        )
        .unwrap();

        assert!(load_intake_review(vault, &[]).is_none());
    }

    #[test]
    fn test_load_review_falls_back_to_legacy_filename() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        let ws = vault.join(".pipeline").join("legacy-name");
        std::fs::create_dir_all(&ws).unwrap();
        // Note: agent wrote intake_review.json instead of review.json
        let review = serde_json::json!({
            "raw": "raw text",
            "optimized": "OPT-001 do thing",
            "status": "pending_review",
            "source_idea": "legacy-name.md",
        });
        std::fs::write(
            ws.join("intake_review.json"),
            serde_json::to_string_pretty(&review).unwrap(),
        )
        .unwrap();

        let result = load_intake_review(vault, &[]).expect("review should be found via fallback");
        assert_eq!(result.source_idea.as_deref(), Some("legacy-name.md"));
        assert_eq!(result.optimized, "OPT-001 do thing");
        assert!(result.source_path.ends_with("intake_review.json"));
    }

    #[test]
    fn test_load_review_prefers_canonical_over_legacy() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        let ws = vault.join(".pipeline").join("both");
        std::fs::create_dir_all(&ws).unwrap();
        let canonical = serde_json::json!({
            "raw": "canonical raw",
            "optimized": "canonical opt",
            "status": "pending",
            "source_idea": "both.md",
        });
        let legacy = serde_json::json!({
            "raw": "legacy raw",
            "optimized": "legacy opt",
            "status": "pending",
            "source_idea": "both.md",
        });
        std::fs::write(ws.join("review.json"), serde_json::to_string(&canonical).unwrap()).unwrap();
        std::fs::write(ws.join("intake_review.json"), serde_json::to_string(&legacy).unwrap()).unwrap();

        let result = load_intake_review(vault, &[]).unwrap();
        assert_eq!(result.optimized, "canonical opt");
    }

    #[test]
    fn test_write_decision_preserves_source_idea() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let path = ws.join("review.json");
        std::fs::write(&path, "{}").unwrap();

        let review = IntakeReview {
            raw: "r".into(),
            optimized: "o".into(),
            source_idea: Some("test.md".into()),
            workspace: ws.clone(),
            source_path: path.clone(),
        };
        write_intake_decision(&review, "confirmed", "o-edited").unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let reloaded: IntakeReviewFile = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reloaded.status, "confirmed");
        assert_eq!(reloaded.optimized, "o-edited");
        assert_eq!(reloaded.source_idea.as_deref(), Some("test.md"));
    }
}
