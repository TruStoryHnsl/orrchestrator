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

    /// Recompute progress from instruction counts.
    /// 0 = not submitted, 1-49 = intake processing, 50 = instructions distributed,
    /// 51-99 = partially implemented, 100 = all implemented.
    pub fn recompute_progress(&mut self) {
        if self.progress == 0 { return; } // not submitted, don't touch
        let total = self.total_instructions();
        if total == 0 {
            // Submitted but no instructions distributed yet — intake phase
            if self.progress < 50 { return; }
        }
        let implemented = self.total_implemented();
        if implemented >= total && total > 0 {
            self.progress = 100;
        } else if total > 0 {
            // 50 = all distributed, 100 = all implemented
            self.progress = 50 + ((implemented as f64 / total as f64) * 50.0) as u8;
        }
    }

    /// Compute the display color as (r, g, b) based on the 100-step gradient.
    ///
    /// Semantics:
    /// - `0-4`: default text color — intention exists but not submitted, so
    ///   it blends in with regular list items.
    /// - `5`: harsh transition to maximum yellow when the user submits.
    /// - `5-100`: linear gradient from yellow to green across the entire
    ///   post-submission lifecycle. At progress 5-49 the COO is processing
    ///   intake; at progress 50 intake is complete with 0% implemented; at
    ///   progress 51-99 implementation is partial; at 100 it's done.
    /// - `100`: maximum green.
    ///
    /// The `default` colour parameter is accepted for callers that still
    /// want to pass it, but only the `0-4` pre-submission band actually uses
    /// it. A just-taken-in intention (progress 50, 0% implemented) is
    /// rendered at full yellow so it's maximally visible during the window
    /// when the user most needs to act on it.
    pub fn gradient_color(&self, default: (u8, u8, u8), yellow: (u8, u8, u8), green: (u8, u8, u8)) -> (u8, u8, u8) {
        let p = self.progress;
        if p < 5 {
            default
        } else if p >= 100 {
            green
        } else {
            // Linear gradient from yellow (p=5) to green (p=100).
            let t = (p - 5) as f64 / 95.0; // 0.0 at p=5, 1.0 at p=100
            lerp_color(yellow, green, t)
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
    if progress == 0 {
        // Reset: clear submission marker so the idea looks "not submitted" again.
        state.submitted_at = None;
    }
    save_pipeline_state(vault_dir, filename, &state)
}

/// Per-idea intake workspace directory: `<vault>/.pipeline/<idea_stem>/`
/// where idea_stem is the filename with the `.md` extension stripped.
///
/// This is the canonical location for the instruction-intake skill's working
/// files (workflow.json, review.json, log files). Each idea gets its own
/// isolated directory so concurrent submissions cannot clobber each other.
pub fn intake_workspace(vault_dir: &Path, idea_filename: &str) -> PathBuf {
    let stem = idea_filename.trim_end_matches(".md");
    vault_dir.join(".pipeline").join(stem)
}

/// Map a skill workflow step (0-7) to a vault progress percentage in the
/// intake range (5-49). Used by `sync_intake_progress` to translate the
/// skill's discrete step counter into the vault's continuous gradient.
///
/// The intake range stops at 49 — progress only advances to 50 once the
/// user has *explicitly confirmed* the optimized review in the TUI.
fn intake_step_to_progress(step: u32) -> u8 {
    match step {
        0 => 5,   // initialized
        1 => 10,  // executive assistant triaging
        2 => 20,  // COO optimizing
        3 => 30,  // pending review (waiting on user)
        4 => 32,  // user reviewing
        5 => 38,  // COO routing
        6 => 44,  // appending to inboxes
        7 => 49,  // PM incorporating
        _ => 49,
    }
}

/// Read the per-idea intake workspace's `workflow.json` and translate the
/// current step into a vault progress percentage. Only updates the vault
/// state if the new value is strictly greater than the current value AND
/// the idea is still in the intake phase (progress < 50).
///
/// Returns true if the vault state changed.
pub fn sync_intake_progress(vault_dir: &Path, idea_filename: &str) -> bool {
    let workspace = intake_workspace(vault_dir, idea_filename);
    let workflow_path = workspace.join("workflow.json");
    let bytes = match std::fs::read(&workflow_path) {
        Ok(b) => b,
        Err(_) => return false,
    };
    // Parse just the fields we care about.
    #[derive(Deserialize)]
    struct WorkflowFile {
        #[serde(default)]
        step: u32,
        #[serde(default)]
        status: String,
    }
    let parsed: WorkflowFile = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let pipeline_dir = vault_dir.join(".pipeline");
    let mut state = load_pipeline_state(&pipeline_dir, idea_filename);
    // Don't touch ideas that have already advanced past intake.
    if state.progress >= 50 { return false; }
    // Don't touch ideas that were never submitted.
    if state.progress == 0 { return false; }

    let new_progress: u8 = match parsed.status.as_str() {
        // Skill marked itself failed/rejected — surface as 1 (stuck) so the
        // user sees the problem but the submission marker is preserved.
        "failed" | "rejected" => 1,
        // Skill ran all 7 steps without pausing at user-review (rare — the
        // gate at step 4 normally pauses execution). Advance to 50.
        "complete" => 50,
        // Normal case: map the discrete step counter into 5-49.
        _ => intake_step_to_progress(parsed.step),
    };

    if new_progress != state.progress {
        state.progress = new_progress;
        let _ = save_pipeline_state(vault_dir, idea_filename, &state);
        return true;
    }
    false
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

/// Scan a project's instructions_inbox.md and PLAN.md to count implemented instructions,
/// then update the pipeline state for the source idea. Returns true if progress changed.
pub fn sync_pipeline_progress(vault_dir: &Path, projects_dir: &Path, idea: &Idea) -> bool {
    let pipeline_dir = vault_dir.join(".pipeline");
    let mut state = load_pipeline_state(&pipeline_dir, &idea.filename);
    if state.targets.is_empty() || state.progress < 50 { return false; }

    let source_marker = format!("plans/{}", idea.filename);
    let mut changed = false;

    for target in &mut state.targets {
        let project_dir = projects_dir.join(&target.project);

        // Count implemented instructions by scanning instructions_inbox.md for ✓ IMPLEMENTED markers
        let inbox_path = project_dir.join("instructions_inbox.md");
        let mut implemented = 0u32;
        let mut in_source_block = false;
        if let Ok(content) = fs::read_to_string(&inbox_path) {
            for line in content.lines() {
                if line.contains(&source_marker) { in_source_block = true; }
                if in_source_block && (line.contains("✓ IMPLEMENTED") || line.contains("[x]") || line.contains("COMPLETED")) {
                    implemented += 1;
                }
            }
        }

        // Also check PLAN.md for [x] items matching INS- numbers from this package
        let plan_path = project_dir.join("PLAN.md");
        if let Ok(plan) = fs::read_to_string(&plan_path) {
            for line in plan.lines() {
                if line.contains("[x]") && line.contains("INS-") {
                    implemented += 1;
                }
            }
        }

        // Deduplicate: cap at instruction_count
        implemented = implemented.min(target.instruction_count);

        if target.implemented_count != implemented {
            target.implemented_count = implemented;
            changed = true;
        }
    }

    if changed {
        state.recompute_progress();
        let _ = save_pipeline_state(vault_dir, &idea.filename, &state);
    }
    changed
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
    fn test_gradient_at_50_is_mid_yellow_to_green() {
        // At progress 50 the pipeline has finished intake but nothing has
        // been implemented yet. Under the new gradient, this sits roughly
        // halfway between yellow and green — close to the yellow end,
        // because the 5..100 range is 95 units wide and 50 is at t ≈ 0.47.
        let state = PipelineState { progress: 50, ..Default::default() };
        let color = state.gradient_color((200, 200, 220), (255, 200, 50), (80, 200, 120));
        // Must NOT be default (200, 200, 220).
        assert_ne!(color, (200, 200, 220));
        // Red channel should have dropped from 255 but still be above green's 80.
        assert!(color.0 < 255 && color.0 > 80);
        // Blue channel should be rising from 50 toward 120 but still nearer 50.
        assert!(color.2 > 50 && color.2 < 120);
    }

    #[test]
    fn test_gradient_green_at_100() {
        let state = PipelineState { progress: 100, ..Default::default() };
        let color = state.gradient_color((200, 200, 220), (255, 200, 50), (80, 200, 120));
        assert_eq!(color, (80, 200, 120));
    }

    #[test]
    fn test_gradient_pure_yellow_immediately_after_submit() {
        // Progress 5 fires the moment the user presses `s`. This is the
        // "harsh transition to maximum yellow" state.
        let state = PipelineState { progress: 5, ..Default::default() };
        let color = state.gradient_color((200, 200, 220), (255, 200, 50), (80, 200, 120));
        assert_eq!(color, (255, 200, 50));
    }

    #[test]
    fn test_gradient_monotonic_from_yellow_to_green() {
        // Walk the gradient and confirm the red channel drops monotonically
        // (yellow → green means red decreases) and the blue channel rises
        // monotonically (50 → 120). Any dip into the "default" blue/gray
        // band would show up as non-monotonic movement.
        let mut prev_r = 255i16;
        let mut prev_b = 50i16;
        for p in 5..=100u8 {
            let state = PipelineState { progress: p, ..Default::default() };
            let (r, _g, b) = state.gradient_color((200, 200, 220), (255, 200, 50), (80, 200, 120));
            assert!(
                r as i16 <= prev_r,
                "red channel must be monotonically non-increasing; p={p} r={r} prev={prev_r}"
            );
            assert!(
                b as i16 >= prev_b,
                "blue channel must be monotonically non-decreasing; p={p} b={b} prev={prev_b}"
            );
            prev_r = r as i16;
            prev_b = b as i16;
        }
    }

    #[test]
    fn test_intake_workspace_strips_md_extension() {
        let vault = std::path::Path::new("/tmp/vault");
        let ws = intake_workspace(vault, "2026-04-21-00-14.md");
        assert_eq!(ws, std::path::PathBuf::from("/tmp/vault/.pipeline/2026-04-21-00-14"));
    }

    #[test]
    fn test_intake_workspace_handles_no_extension() {
        let vault = std::path::Path::new("/tmp/vault");
        let ws = intake_workspace(vault, "raw_idea");
        assert_eq!(ws, std::path::PathBuf::from("/tmp/vault/.pipeline/raw_idea"));
    }

    #[test]
    fn test_intake_step_progress_mapping_monotonic() {
        let mut prev = 0u8;
        for step in 0..=7u32 {
            let p = intake_step_to_progress(step);
            assert!(p >= prev, "step {step} -> {p} regressed from {prev}");
            assert!(p < 50, "step {step} -> {p} exceeded intake ceiling 49");
            prev = p;
        }
    }

    #[test]
    fn test_sync_intake_progress_advances_on_step() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        std::fs::create_dir_all(vault.join(".pipeline")).unwrap();
        // Pre-existing vault state at progress=1 (just submitted).
        let mut state = PipelineState::default();
        state.progress = 1;
        state.submitted_at = Some(123);
        save_pipeline_state(vault, "test.md", &state).unwrap();

        // Skill writes step 3 to per-idea workspace.
        let ws = intake_workspace(vault, "test.md");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("workflow.json"),
            r#"{"workflow":"instruction-intake","step":3,"total_steps":7,"status":"running","agents":[]}"#,
        ).unwrap();

        let changed = sync_intake_progress(vault, "test.md");
        assert!(changed, "expected progress to advance");
        let reloaded = load_pipeline_state(&vault.join(".pipeline"), "test.md");
        assert_eq!(reloaded.progress, 30);
    }

    #[test]
    fn test_sync_intake_progress_skips_when_past_intake() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        std::fs::create_dir_all(vault.join(".pipeline")).unwrap();
        // Idea has already been confirmed (progress=50).
        let mut state = PipelineState::default();
        state.progress = 50;
        save_pipeline_state(vault, "test.md", &state).unwrap();

        let ws = intake_workspace(vault, "test.md");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("workflow.json"),
            r#"{"workflow":"instruction-intake","step":1,"total_steps":7,"status":"running","agents":[]}"#,
        ).unwrap();

        let changed = sync_intake_progress(vault, "test.md");
        assert!(!changed, "should not regress past-intake state");
        let reloaded = load_pipeline_state(&vault.join(".pipeline"), "test.md");
        assert_eq!(reloaded.progress, 50);
    }

    #[test]
    fn test_sync_intake_progress_skips_unsubmitted() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        std::fs::create_dir_all(vault.join(".pipeline")).unwrap();
        // Never submitted (progress=0).
        let state = PipelineState::default();
        save_pipeline_state(vault, "test.md", &state).unwrap();

        let ws = intake_workspace(vault, "test.md");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("workflow.json"),
            r#"{"workflow":"instruction-intake","step":3,"total_steps":7,"status":"running","agents":[]}"#,
        ).unwrap();

        let changed = sync_intake_progress(vault, "test.md");
        assert!(!changed, "should not advance unsubmitted ideas");
    }

    #[test]
    fn test_sync_intake_progress_failed_status() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        std::fs::create_dir_all(vault.join(".pipeline")).unwrap();
        let mut state = PipelineState::default();
        state.progress = 30;
        save_pipeline_state(vault, "test.md", &state).unwrap();

        let ws = intake_workspace(vault, "test.md");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("workflow.json"),
            r#"{"workflow":"instruction-intake","step":3,"total_steps":7,"status":"failed","agents":[]}"#,
        ).unwrap();

        let changed = sync_intake_progress(vault, "test.md");
        assert!(changed);
        let reloaded = load_pipeline_state(&vault.join(".pipeline"), "test.md");
        assert_eq!(reloaded.progress, 1, "failed status should drop to 1");
    }

    #[test]
    fn test_update_pipeline_progress_to_zero_clears_submission() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        std::fs::create_dir_all(vault.join(".pipeline")).unwrap();
        let mut state = PipelineState::default();
        state.progress = 30;
        state.submitted_at = Some(999);
        save_pipeline_state(vault, "test.md", &state).unwrap();

        update_pipeline_progress(vault, "test.md", 0).unwrap();
        let reloaded = load_pipeline_state(&vault.join(".pipeline"), "test.md");
        assert_eq!(reloaded.progress, 0);
        assert_eq!(reloaded.submitted_at, None, "rejecting should clear submission");
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
