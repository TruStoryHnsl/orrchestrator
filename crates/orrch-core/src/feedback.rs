use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Result of routing feedback to projects.
#[derive(Debug)]
pub struct RoutingResult {
    pub routes: Vec<(String, PathBuf)>, // (project_name, project_path)
    pub saved_path: PathBuf,
}

/// Save feedback text to the .feedback directory and route to target projects.
///
/// Returns the routing result showing which projects were targeted.
pub fn save_and_route_feedback(
    feedback_text: &str,
    projects_dir: &Path,
) -> anyhow::Result<RoutingResult> {
    // Save to .feedback dir
    let feedback_dir = projects_dir.join(".feedback");
    fs::create_dir_all(&feedback_dir)?;

    let timestamp = chrono_lite_timestamp();
    let filename = format!("{timestamp}.md");
    let saved_path = feedback_dir.join(&filename);
    fs::write(&saved_path, feedback_text)?;

    // Route: scan for project name mentions in the text
    let routes = identify_target_projects(feedback_text, projects_dir);

    // Append to each project's fb2p.md
    for (_, project_path) in &routes {
        append_to_fb2p(feedback_text, project_path, &timestamp)?;
    }

    // If no projects identified, append to workspace-level fb2p.md
    if routes.is_empty() {
        append_to_fb2p(feedback_text, projects_dir, &timestamp)?;
    }

    Ok(RoutingResult {
        routes,
        saved_path,
    })
}

/// Common English words that happen to be project directory names.
/// These require stronger context signals to count as a match.
const AMBIGUOUS_NAMES: &[&str] = &[
    "notes", "admin", "claude", "oracle", "concord", "scratchpad",
];

/// Identify which projects the feedback is about using word-boundary matching
/// and context scoring. Returns matches sorted by confidence (highest first).
fn identify_target_projects(text: &str, projects_dir: &Path) -> Vec<(String, PathBuf)> {
    let text_lower = text.to_lowercase();
    let mut scored: Vec<(String, PathBuf, i32)> = Vec::new();

    if let Ok(entries) = fs::read_dir(projects_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.starts_with('.') || name == "deprecated" {
                continue;
            }

            let name_lower = name.to_lowercase();
            let score = score_project_mention(&text_lower, &name_lower);
            if score > 0 {
                scored.push((name, path, score));
            }
        }
    }

    scored.sort_by(|a, b| b.2.cmp(&a.2));
    scored.into_iter().map(|(name, path, _)| (name, path)).collect()
}

/// Score how likely a project name mention is intentional (not a false positive).
/// Returns 0 if no match, higher = more confident.
fn score_project_mention(text: &str, name: &str) -> i32 {
    // Must appear as a whole word (not substring of a longer word)
    if !word_boundary_match(text, name) {
        return 0;
    }

    let mut score: i32 = 10; // base score for word-boundary match

    // Explicit project reference patterns boost confidence
    let explicit_patterns = [
        format!("the {} project", name),
        format!("in {}", name),
        format!("for {}", name),
        format!("{} project", name),
        format!("{}'s", name),
        format!("{}:", name),
        format!("{}/", name),
    ];
    for pat in &explicit_patterns {
        if text.contains(pat.as_str()) {
            score += 15;
            break;
        }
    }

    // orr-prefixed names are unique to this ecosystem — strong signal
    if name.starts_with("orr") || name.starts_with("cb") || name.starts_with("nf") {
        score += 20;
    }

    // Ambiguous common English words need higher threshold
    if AMBIGUOUS_NAMES.iter().any(|&a| a == name) {
        score -= 15; // penalty: needs explicit context to survive
    }

    // Very short names (<=3 chars) are too likely to be false positives
    if name.len() <= 3 {
        score -= 10;
    }

    // Multiple word-boundary mentions boost confidence
    let mention_count = count_word_boundary_matches(text, name);
    if mention_count >= 3 {
        score += 10;
    } else if mention_count >= 2 {
        score += 5;
    }

    // Return 0 if score didn't survive penalties
    if score <= 0 { 0 } else { score }
}

/// Count how many times `name` appears in `text` at word boundaries.
fn count_word_boundary_matches(text: &str, name: &str) -> usize {
    let text_bytes = text.as_bytes();
    let mut count = 0;
    let mut start = 0;
    while let Some(pos) = text[start..].find(name) {
        let abs_pos = start + pos;
        let end_pos = abs_pos + name.len();
        let left_ok = abs_pos == 0 || !text_bytes[abs_pos - 1].is_ascii_alphanumeric();
        let right_ok = end_pos >= text.len() || !text_bytes[end_pos].is_ascii_alphanumeric();
        if left_ok && right_ok {
            count += 1;
        }
        start = abs_pos + 1;
        if start >= text.len() {
            break;
        }
    }
    count
}

/// Check if `name` appears in `text` at a word boundary.
/// A word boundary is: start/end of string, whitespace, punctuation, or `/`.
fn word_boundary_match(text: &str, name: &str) -> bool {
    let name_bytes = name.as_bytes();
    let text_bytes = text.as_bytes();
    if name_bytes.len() > text_bytes.len() {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = text[start..].find(name) {
        let abs_pos = start + pos;
        let end_pos = abs_pos + name.len();

        let left_ok = abs_pos == 0 || !text_bytes[abs_pos - 1].is_ascii_alphanumeric();
        let right_ok = end_pos >= text.len() || !text_bytes[end_pos].is_ascii_alphanumeric();

        if left_ok && right_ok {
            return true;
        }
        start = abs_pos + 1;
        if start >= text.len() {
            break;
        }
    }
    false
}

/// Public access to project identification for the confirmation overlay.
pub fn identify_target_projects_pub(text: &str, projects_dir: &Path) -> Vec<(String, PathBuf)> {
    identify_target_projects(text, projects_dir)
}

/// Append a feedback entry to a project's fb2p.md.
pub fn append_to_fb2p(
    feedback_text: &str,
    project_dir: &Path,
    timestamp: &str,
) -> anyhow::Result<()> {
    let fb2p_path = project_dir.join("fb2p.md");

    // Create if doesn't exist
    if !fb2p_path.exists() {
        let project_name = project_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        fs::write(
            &fb2p_path,
            format!("# {project_name} — Feedback to Prompt Log\n"),
        )?;
    }

    let mut file = OpenOptions::new().append(true).open(&fb2p_path)?;

    // Truncate for display if very long
    let display_text = if feedback_text.len() > 2000 {
        format!("{}...\n(truncated, full text in .feedback/)", &feedback_text[..2000])
    } else {
        feedback_text.to_string()
    };

    write!(
        file,
        "\n---\n\n\
         ## Entry: {timestamp} — orrchestrator feedback editor\n\n\
         ### Raw Input\n\
         {display_text}\n\n\
         ### Status\n\
         Generated: {timestamp}\n\
         Executed: pending\n"
    )?;

    Ok(())
}

pub fn chrono_lite_timestamp() -> String {
    // Simple timestamp without chrono dependency
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Approximate date — good enough for filenames
    let days = secs / 86400;
    let years = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    let hour = (secs % 86400) / 3600;
    let min = (secs % 3600) / 60;
    format!("{years:04}-{month:02}-{day:02} {hour:02}:{min:02}")
}

/// Format a SystemTime as a human-readable timestamp.
fn format_system_time(time: std::time::SystemTime) -> String {
    let secs = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = secs / 86400;
    let years = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    let hour = (secs % 86400) / 3600;
    let min = (secs % 3600) / 60;
    format!("{years:04}-{month:02}-{day:02} {hour:02}:{min:02}")
}

/// The default prompt for "continue development" sessions.
pub const CONTINUE_DEV_PROMPT: &str = "continue development";

/// Append feedback directly to a specific project's fb2p.md (no routing needed).
pub fn append_to_fb2p_direct(
    feedback_text: &str,
    project_dir: &Path,
    timestamp: &str,
) -> anyhow::Result<()> {
    append_to_fb2p(feedback_text, project_dir, timestamp)
}

// ─── Feedback Pipeline ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedbackStatus {
    /// Vim is still open (or was never submitted).
    Draft,
    /// Sent to Claude for analysis — waiting for results.
    Processing,
    /// Claude finished — user needs to review and commit results.
    Processed,
    /// User committed the results to project pipelines.
    Routed,
}

/// Whether feedback is regular or a planning document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedbackType {
    Feedback,
    Plan,
}

impl Default for FeedbackType {
    fn default() -> Self { Self::Feedback }
}

impl FeedbackType {
    pub fn label(&self) -> &'static str {
        match self { Self::Feedback => "feedback", Self::Plan => "plan" }
    }
}

/// Persistent metadata for a single feedback file, stored in .status.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackMeta {
    pub status: FeedbackStatus,
    #[serde(default)]
    pub routes: Vec<String>,
    #[serde(default)]
    pub submitted_at: Option<String>,
    #[serde(default)]
    pub feedback_type: FeedbackType,
    /// tmux session name when Processing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux_session: Option<String>,
}

/// A feedback item loaded for display in the Feedback tab.
#[derive(Debug, Clone)]
pub struct FeedbackItem {
    pub filename: String,
    pub path: PathBuf,
    pub status: FeedbackStatus,
    pub feedback_type: FeedbackType,
    pub preview: String,
    pub created: String,
    /// Last modification time as a displayable string.
    pub modified: String,
    pub routes: Vec<String>,
    /// True if the file is empty or whitespace-only.
    pub is_empty: bool,
}

/// Load the status map from .feedback/.status.json.
fn load_status_map(feedback_dir: &Path) -> HashMap<String, FeedbackMeta> {
    let path = feedback_dir.join(".status.json");
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Save the status map to .feedback/.status.json.
fn save_status_map(feedback_dir: &Path, map: &HashMap<String, FeedbackMeta>) {
    let path = feedback_dir.join(".status.json");
    if let Ok(json) = serde_json::to_string_pretty(map) {
        let _ = fs::write(path, json);
    }
}

/// Load all feedback items from the .feedback directory.
pub fn load_feedback_items(projects_dir: &Path) -> Vec<FeedbackItem> {
    let feedback_dir = projects_dir.join(".feedback");
    let _ = fs::create_dir_all(&feedback_dir);

    let status_map = load_status_map(&feedback_dir);
    let mut items = Vec::new();

    if let Ok(entries) = fs::read_dir(&feedback_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let contents = fs::read_to_string(&path).unwrap_or_default();
                let is_empty = contents.trim().is_empty();
                let preview: String = contents
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect();

                // Derive created timestamp from filename (e.g., "2026-03-28 14:30.md")
                let created = filename.trim_end_matches(".md").to_string();

                // Get actual modification time from filesystem
                let modified = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .map(|t| format_system_time(t))
                    .unwrap_or_else(|| created.clone());

                let meta = status_map.get(&filename);
                let status = meta.map(|m| m.status).unwrap_or(FeedbackStatus::Draft);
                let routes = meta.map(|m| m.routes.clone()).unwrap_or_default();
                let feedback_type = meta.map(|m| m.feedback_type).unwrap_or_default();

                items.push(FeedbackItem {
                    filename,
                    path,
                    status,
                    feedback_type,
                    preview,
                    created,
                    modified,
                    routes,
                    is_empty,
                });
            }
        }
    }

    // Sort newest first
    items.sort_by(|a, b| b.created.cmp(&a.created));
    items
}

/// Submit a draft feedback file: route it to projects and update status.
pub fn submit_feedback(
    filename: &str,
    projects_dir: &Path,
) -> anyhow::Result<RoutingResult> {
    let feedback_dir = projects_dir.join(".feedback");
    let file_path = feedback_dir.join(filename);
    let text = fs::read_to_string(&file_path)?;

    // Route to projects
    let routes = identify_target_projects(&text, projects_dir);
    let timestamp = chrono_lite_timestamp();

    for (_, project_path) in &routes {
        append_to_fb2p(&text, project_path, &timestamp)?;
    }
    if routes.is_empty() {
        append_to_fb2p(&text, projects_dir, &timestamp)?;
    }

    // Update status
    let mut status_map = load_status_map(&feedback_dir);
    status_map.insert(filename.to_string(), FeedbackMeta {
        status: FeedbackStatus::Routed,
        routes: routes.iter().map(|(name, _)| name.clone()).collect(),
        submitted_at: Some(timestamp),
        feedback_type: FeedbackType::Feedback,
        tmux_session: None,
    });
    save_status_map(&feedback_dir, &status_map);

    Ok(RoutingResult {
        routes,
        saved_path: file_path,
    })
}

/// Write a YAML metadata header to a feedback file before processing.
/// This tells the Claude processor where to route and what to expect.
pub fn write_feedback_metadata(
    file_path: &Path,
    route_names: &[String],
    intended_output: &str,
) -> anyhow::Result<()> {
    let existing = fs::read_to_string(file_path)?;

    // Skip if already has frontmatter
    if existing.starts_with("---\n") {
        return Ok(());
    }

    let routes_yaml = if route_names.is_empty() {
        "  - (workspace level)".to_string()
    } else {
        route_names.iter().map(|r| format!("  - {r}")).collect::<Vec<_>>().join("\n")
    };
    let output_desc = if intended_output.is_empty() { "development feedback" } else { intended_output };

    let header = format!(
        "---\ntargets:\n{routes_yaml}\noutput: {output_desc}\nsubmitted: {ts}\n---\n\n",
        ts = chrono_lite_timestamp(),
    );

    fs::write(file_path, format!("{header}{existing}"))?;
    Ok(())
}

/// Set the feedback type (plan vs regular) for a draft.
pub fn set_feedback_type(filename: &str, projects_dir: &Path, fb_type: FeedbackType) {
    let feedback_dir = projects_dir.join(".feedback");
    let mut status_map = load_status_map(&feedback_dir);
    let entry = status_map.entry(filename.to_string()).or_insert(FeedbackMeta {
        status: FeedbackStatus::Draft,
        routes: Vec::new(),
        submitted_at: None,
        feedback_type: FeedbackType::Feedback,
        tmux_session: None,
    });
    entry.feedback_type = fb_type;
    save_status_map(&feedback_dir, &status_map);
}

/// Mark a feedback file as being processed by Claude (Processing state).
pub fn mark_as_processing(filename: &str, projects_dir: &Path, route_names: &[String], fb_type: FeedbackType, tmux_session: Option<&str>) {
    let feedback_dir = projects_dir.join(".feedback");
    let mut status_map = load_status_map(&feedback_dir);
    status_map.insert(filename.to_string(), FeedbackMeta {
        status: FeedbackStatus::Processing,
        routes: route_names.to_vec(),
        submitted_at: Some(chrono_lite_timestamp()),
        feedback_type: fb_type,
        tmux_session: tmux_session.map(|s| s.to_string()),
    });
    save_status_map(&feedback_dir, &status_map);
}

/// Mark a feedback file as processed (Claude done, user hasn't committed yet).
pub fn mark_as_processed(filename: &str, projects_dir: &Path) {
    let feedback_dir = projects_dir.join(".feedback");
    let mut status_map = load_status_map(&feedback_dir);
    if let Some(meta) = status_map.get_mut(filename) {
        meta.status = FeedbackStatus::Processed;
        meta.tmux_session = None;
    }
    save_status_map(&feedback_dir, &status_map);
}

/// Mark a feedback file as committed (user approved the results).
pub fn mark_as_routed(filename: &str, projects_dir: &Path) {
    let feedback_dir = projects_dir.join(".feedback");
    let mut status_map = load_status_map(&feedback_dir);
    if let Some(meta) = status_map.get_mut(filename) {
        meta.status = FeedbackStatus::Routed;
        meta.tmux_session = None;
    }
    save_status_map(&feedback_dir, &status_map);
}

/// Check if a feedback file's tmux processing session has finished.
pub fn check_processing_complete(filename: &str, projects_dir: &Path) -> bool {
    let feedback_dir = projects_dir.join(".feedback");
    let status_map = load_status_map(&feedback_dir);
    if let Some(meta) = status_map.get(filename) {
        if meta.status == FeedbackStatus::Processing {
            if let Some(ref session) = meta.tmux_session {
                // Check if tmux session still exists
                let exists = std::process::Command::new("tmux")
                    .args(["has-session", "-t", session])
                    .output()
                    .is_ok_and(|o| o.status.success());
                return !exists; // complete if session is gone
            }
            return true; // no session recorded = assume complete
        }
    }
    false
}

/// Delete a feedback file and its status entry.
pub fn delete_feedback(filename: &str, projects_dir: &Path) {
    let feedback_dir = projects_dir.join(".feedback");
    let _ = fs::remove_file(feedback_dir.join(filename));
    let mut status_map = load_status_map(&feedback_dir);
    status_map.remove(filename);
    save_status_map(&feedback_dir, &status_map);
}

/// Create a new draft feedback file and return its path.
pub fn create_draft(projects_dir: &Path) -> anyhow::Result<PathBuf> {
    let feedback_dir = projects_dir.join(".feedback");
    fs::create_dir_all(&feedback_dir)?;
    let timestamp = chrono_lite_timestamp();
    let filename = format!("{timestamp}.md");
    let path = feedback_dir.join(&filename);
    fs::write(&path, "")?;
    Ok(path)
}

/// Create a temp file for master plan append and return its path.
pub fn create_append_draft(projects_dir: &Path) -> anyhow::Result<PathBuf> {
    let feedback_dir = projects_dir.join(".feedback");
    fs::create_dir_all(&feedback_dir)?;
    let timestamp = chrono_lite_timestamp();
    let filename = format!("append-{timestamp}.md");
    let path = feedback_dir.join(&filename);
    fs::write(&path, "")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_route_with_project_mention() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("concord")).unwrap();
        fs::create_dir(tmp.path().join("orrapus")).unwrap();

        // "concord" is ambiguous but appears with explicit context "in concord"
        let text = "We need to fix the WebSocket handling in concord and also update orrapus deployment.";
        let routes = identify_target_projects(text, tmp.path());
        let names: Vec<&str> = routes.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"orrapus"), "orr-prefixed names should match");
        assert!(names.contains(&"concord"), "'in concord' provides enough context");
    }

    #[test]
    fn test_route_no_mention() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("myproject")).unwrap();

        let text = "General infrastructure thoughts about the system.";
        let routes = identify_target_projects(text, tmp.path());
        assert!(routes.is_empty());
    }

    #[test]
    fn test_false_positive_common_words() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("notes")).unwrap();
        fs::create_dir(tmp.path().join("admin")).unwrap();
        fs::create_dir(tmp.path().join("claude")).unwrap();

        // These words appear in text but NOT as project references
        let text = "Nodes trade notes on this map data. Cluster admins can restrict users. Claude Code is an AI tool.";
        let routes = identify_target_projects(text, tmp.path());
        let names: Vec<&str> = routes.iter().map(|(n, _)| n.as_str()).collect();
        // All three are ambiguous common words used in non-project context
        assert!(!names.contains(&"notes"), "false positive: 'notes' as English word");
        assert!(!names.contains(&"admin"), "false positive: 'admin' as English word");
        assert!(!names.contains(&"claude"), "false positive: 'claude' as English word");
    }

    #[test]
    fn test_orr_prefix_boost() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("orrchestrator")).unwrap();
        fs::create_dir(tmp.path().join("orrapus")).unwrap();
        fs::create_dir(tmp.path().join("orradash")).unwrap();

        let text = "Update orrapus deployment config.";
        let routes = identify_target_projects(text, tmp.path());
        let names: Vec<&str> = routes.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"orrapus"));
        assert!(!names.contains(&"orrchestrator"), "not mentioned");
        assert!(!names.contains(&"orradash"), "not mentioned");
    }

    #[test]
    fn test_word_boundary_prevents_substring() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("or")).unwrap();
        fs::create_dir(tmp.path().join("con")).unwrap();

        let text = "We need to fix the concord or orrapus deployment.";
        let routes = identify_target_projects(text, tmp.path());
        let names: Vec<&str> = routes.iter().map(|(n, _)| n.as_str()).collect();
        // "or" appears as English word but is too short (2 chars) → penalty kills it
        assert!(!names.contains(&"or"), "'or' too short to be a project reference");
        // "con" is inside "concord" (no word boundary) → rejected by word_boundary_match
        assert!(!names.contains(&"con"), "'con' is substring of 'concord', not standalone");
    }

    #[test]
    fn test_explicit_project_reference() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("notes")).unwrap();

        // Explicit "the notes project" should match even though "notes" is ambiguous
        let text = "We should restructure the notes project to support search.";
        let routes = identify_target_projects(text, tmp.path());
        let names: Vec<&str> = routes.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"notes"), "explicit 'the notes project' should match");
    }

    #[test]
    fn test_save_and_route() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("testproj")).unwrap();

        let result = save_and_route_feedback("Fix testproj auth", tmp.path()).unwrap();
        assert_eq!(result.routes.len(), 1);
        assert_eq!(result.routes[0].0, "testproj");
        assert!(result.saved_path.exists());

        let fb2p = fs::read_to_string(tmp.path().join("testproj").join("fb2p.md")).unwrap();
        assert!(fb2p.contains("Fix testproj auth"));
        assert!(fb2p.contains("Executed: pending"));
    }

    #[test]
    fn test_word_boundary_match() {
        assert!(word_boundary_match("fix the concord deployment", "concord"));
        assert!(word_boundary_match("concord is broken", "concord"));
        assert!(word_boundary_match("update concord", "concord"));
        assert!(!word_boundary_match("disconcordant behavior", "concord")); // substring
        assert!(word_boundary_match("fix concord/v2 websocket", "concord")); // slash boundary
    }

    #[test]
    fn test_scoring() {
        // orr-prefix gets a boost
        assert!(score_project_mention("update orrapus", "orrapus") > score_project_mention("update concord", "concord"));
        // explicit context beats bare mention
        assert!(score_project_mention("the notes project", "notes") > score_project_mention("trade notes on data", "notes"));
        // ambiguous word with no context scores 0 or below threshold
        assert_eq!(score_project_mention("cluster admins can restrict", "admin"), 0);
    }
}
