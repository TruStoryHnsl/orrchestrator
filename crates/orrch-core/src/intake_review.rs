use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::Project;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IntakeReviewFile {
    pub raw: String,
    pub optimized: String,
    pub status: String, // "pending" | "confirmed" | "rejected"
}

#[derive(Debug, Clone)]
pub struct IntakeReview {
    pub raw: String,
    pub optimized: String,
    pub source_project: PathBuf,
    pub source_path: PathBuf,
}

/// Scan all projects for a pending intake review.
pub fn load_intake_review(projects: &[Project]) -> Option<IntakeReview> {
    for proj in projects {
        let path = proj.path.join(".orrch").join("intake_review.json");
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(file) = serde_json::from_slice::<IntakeReviewFile>(&bytes) {
                if file.status == "pending" {
                    return Some(IntakeReview {
                        raw: file.raw,
                        optimized: file.optimized,
                        source_project: proj.path.clone(),
                        source_path: path,
                    });
                }
            }
        }
    }
    None
}

/// Write the user's decision back to the review file.
pub fn write_intake_decision(review: &IntakeReview, decision: &str, optimized: &str) -> anyhow::Result<()> {
    let file = IntakeReviewFile {
        raw: review.raw.clone(),
        optimized: optimized.to_string(),
        status: decision.to_string(),
    };
    let json = serde_json::to_string_pretty(&file)?;
    let tmp = review.source_path.with_extension("tmp");
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, &review.source_path)?;
    Ok(())
}
