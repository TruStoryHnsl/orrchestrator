//! Intentions vault — ideas with instruction intake pipeline tracking.
//!
//! Stored in ~/projects/orrchestrator/plans/*.md
//! Pipeline state stored in ~/projects/orrchestrator/plans/.pipeline/<filename>.json

use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Pipeline state for an idea going through instruction intake.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineState {
    /// 0-100 progress. 0-50 = intake processing, 51-100 = implementation.
    pub progress: u8,
    /// Target projects this idea's instructions were routed to.
    pub targets: Vec<PipelineTarget>,
    /// AI-generated name for the feature package (set at progress=50).
    pub package_name: Option<String>,
    /// Timestamp when submission started.
    pub submitted_at: Option<u64>,
}

/// Instruction distribution to a target project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTarget {
    pub project: String,
    pub instruction_count: u32,
    pub implemented_count: u32,
}

impl PipelineState {
    /// Whether this idea has been submitted to the pipeline.
    pub fn is_submitted(&self) -> bool {
        self.progress > 0
    }

    /// Whether all instructions have been confirmed implemented.
    pub fn is_complete(&self) -> bool {
        self.progress >= 100
    }

    /// Total instructions across all targets.
    pub fn total_instructions(&self) -> u32 {
        self.targets.iter().map(|t| t.instruction_count).sum()
    }

    /// Total confirmed implementations.
    pub fn total_implemented(&self) -> u32 {
        self.targets.iter().map(|t| t.implemented_count).sum()
    }

    /// Implementation percentage (0.0 - 1.0) based on instruction completion.
    pub fn implementation_ratio(&self) -> f64 {
        let total = self.total_instructions();
        if total == 0 { return 0.0; }
        self.total_implemented() as f64 / total as f64
    }

    /// Compute the display color as (r, g, b) based on the 100-step gradient.
    ///
    /// 0-4: default text color (no change)
    /// 5: harsh transition to maximum yellow
    /// 5-50: gradient from yellow back to default
    /// 50: default (instructions in inboxes)
    /// 50-100: gradient from default to green
    /// 100: maximum green
    pub fn gradient_color(&self, default: (u8, u8, u8), yellow: (u8, u8, u8), green: (u8, u8, u8)) -> (u8, u8, u8) {
        let p = self.progress;
        if p < 5 {
            default
        } else if p == 5 {
            yellow
        } else if p <= 50 {
            // Gradient from yellow (5) to default (50)
            let t = (p - 5) as f64 / 45.0; // 0.0 at p=5, 1.0 at p=50
            lerp_color(yellow, default, t)
        } else if p < 100 {
            // Gradient from default (50) to green (100)
            let t = (p - 50) as f64 / 50.0; // 0.0 at p=50, 1.0 at p=100
            lerp_color(default, green, t)
        } else {
            green
        }
    }
}

fn lerp_color(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t) as u8,
        (a.1 as f64 + (b.1 as f64 - a.1 as f64) * t) as u8,
        (a.2 as f64 + (b.2 as f64 - a.2 as f64) * t) as u8,
    )
}

/// A single idea/note in the vault with pipeline tracking.
#[derive(Debug, Clone)]
pub struct Idea {
    pub filename: String,
    pub title: String,
    pub preview: String,
    pub path: PathBuf,
    pub modified: u64, // unix timestamp for sorting
    pub pipeline: PipelineState,
}

/// Load all ideas from the vault directory, sorted by modification time (newest first).
pub fn load_ideas(vault_dir: &Path) -> Vec<Idea> {
    let _ = fs::create_dir_all(vault_dir);
    let pipeline_dir = vault_dir.join(".pipeline");
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

                    // Modification time
                    let modified = entry.metadata()
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
                        .map(|d: std::time::Duration| d.as_secs())
                        .unwrap_or(0);

                    // Load pipeline state
                    let pipeline = load_pipeline_state(&pipeline_dir, &filename);

                    ideas.push(Idea {
                        filename,
                        title,
                        preview,
                        path,
                        modified,
                        pipeline,
                    });
                }
            }
        }
    }

    // Sort by modification time, newest first
    ideas.sort_by(|a, b| b.modified.cmp(&a.modified));
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

/// Load pipeline state for an idea file.
fn load_pipeline_state(pipeline_dir: &Path, filename: &str) -> PipelineState {
    let state_file = pipeline_dir.join(format!("{}.json", filename));
    if state_file.exists() {
        if let Ok(content) = fs::read_to_string(&state_file) {
            if let Ok(state) = serde_json::from_str(&content) {
                return state;
            }
        }
    }
    PipelineState::default()
}

/// Save pipeline state for an idea file.
pub fn save_pipeline_state(vault_dir: &Path, filename: &str, state: &PipelineState) -> std::io::Result<()> {
    let pipeline_dir = vault_dir.join(".pipeline");
    fs::create_dir_all(&pipeline_dir)?;
    let state_file = pipeline_dir.join(format!("{}.json", filename));
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(state_file, json)
}

/// Submit an idea to the instruction intake pipeline.
/// Sets progress to 1 (processing started) and records the timestamp.
pub fn submit_to_pipeline(vault_dir: &Path, idea: &Idea) -> std::io::Result<PipelineState> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut state = idea.pipeline.clone();
    state.progress = 1;
    state.submitted_at = Some(now);
    save_pipeline_state(vault_dir, &idea.filename, &state)?;
    Ok(state)
}

/// Update pipeline progress for an idea.
pub fn update_pipeline_progress(vault_dir: &Path, filename: &str, progress: u8) -> std::io::Result<()> {
    let mut state = load_pipeline_state(&vault_dir.join(".pipeline"), filename);
    state.progress = progress.min(100);
    save_pipeline_state(vault_dir, filename, &state)
}

/// Set pipeline targets (called when instructions are distributed to projects).
pub fn set_pipeline_targets(vault_dir: &Path, filename: &str, package_name: &str, targets: Vec<PipelineTarget>) -> std::io::Result<()> {
    let pipeline_dir = vault_dir.join(".pipeline");
    let mut state = load_pipeline_state(&pipeline_dir, filename);
    state.package_name = Some(package_name.to_string());
    state.targets = targets;
    if state.progress < 50 {
        state.progress = 50;
    }
    save_pipeline_state(vault_dir, filename, &state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gradient_default_at_zero() {
        let state = PipelineState { progress: 0, ..Default::default() };
        let color = state.gradient_color((200, 200, 220), (255, 200, 50), (80, 200, 120));
        assert_eq!(color, (200, 200, 220));
    }

    #[test]
    fn test_gradient_yellow_at_5() {
        let state = PipelineState { progress: 5, ..Default::default() };
        let color = state.gradient_color((200, 200, 220), (255, 200, 50), (80, 200, 120));
        assert_eq!(color, (255, 200, 50));
    }

    #[test]
    fn test_gradient_back_to_default_at_50() {
        let state = PipelineState { progress: 50, ..Default::default() };
        let color = state.gradient_color((200, 200, 220), (255, 200, 50), (80, 200, 120));
        assert_eq!(color, (200, 200, 220));
    }

    #[test]
    fn test_gradient_green_at_100() {
        let state = PipelineState { progress: 100, ..Default::default() };
        let color = state.gradient_color((200, 200, 220), (255, 200, 50), (80, 200, 120));
        assert_eq!(color, (80, 200, 120));
    }

    #[test]
    fn test_gradient_midpoint_yellow_to_default() {
        let state = PipelineState { progress: 27, ..Default::default() }; // ~halfway 5-50
        let color = state.gradient_color((200, 200, 220), (255, 200, 50), (80, 200, 120));
        // Should be roughly between yellow and default
        assert!(color.0 > 200 && color.0 < 255);
    }

    #[test]
    fn test_implementation_ratio() {
        let state = PipelineState {
            progress: 75,
            targets: vec![
                PipelineTarget { project: "foo".into(), instruction_count: 10, implemented_count: 5 },
                PipelineTarget { project: "bar".into(), instruction_count: 10, implemented_count: 10 },
            ],
            ..Default::default()
        };
        assert_eq!(state.total_instructions(), 20);
        assert_eq!(state.total_implemented(), 15);
        assert!((state.implementation_ratio() - 0.75).abs() < 0.01);
    }
}
