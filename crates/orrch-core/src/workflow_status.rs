use std::path::{Path, PathBuf};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowAgentStatus {
    pub role: String,
    pub status: String, // "running" | "waiting" | "complete" | "failed"
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowStatus {
    pub workflow: String,
    pub step: u32,
    pub total_steps: u32,
    pub status: String, // "running" | "complete" | "failed" | "paused"
    #[serde(default)]
    pub agents: Vec<WorkflowAgentStatus>,
    #[serde(skip)]
    pub source_path: PathBuf,
}

pub fn load_workflow_status(project_dir: &Path) -> Option<WorkflowStatus> {
    let path = project_dir.join(".orrch").join("workflow.json");
    let bytes = std::fs::read(&path).ok()?;
    let mut status: WorkflowStatus = serde_json::from_slice(&bytes).ok()?;
    status.source_path = path;
    Some(status)
}
