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

    // Append to each project's instructions_inbox.md
    for (_, project_path) in &routes {
        append_to_inbox(feedback_text, project_path, &timestamp)?;
    }

    // If no projects identified, append to workspace-level instructions_inbox.md
    if routes.is_empty() {
        append_to_inbox(feedback_text, projects_dir, &timestamp)?;
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

/// Append a feedback entry to a project's instructions_inbox.md.
pub fn append_to_inbox(
    feedback_text: &str,
    project_dir: &Path,
    timestamp: &str,
) -> anyhow::Result<()> {
    let inbox_path = project_dir.join("instructions_inbox.md");

    // Create if doesn't exist
    if !inbox_path.exists() {
        let project_name = project_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        fs::write(
            &inbox_path,
            format!("# {project_name} — Instructions Inbox\n"),
        )?;
    }

    let mut file = OpenOptions::new().append(true).open(&inbox_path)?;

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
    // Flush the append before the truncation check so the byte size reflects
    // the new entry.
    drop(file);

    // Task 27b (Layer a): COO intake compression hook. Every inbox write —
    // including instructions distributed from intake review confirmation —
    // triggers a one-time truncation check. `truncate_inbox_if_large` is a
    // no-op when the file is under the threshold, so this is cheap.
    if let Err(err) = truncate_inbox_if_large(project_dir, INBOX_WRITE_SOFT_CAP) {
        tracing::warn!(
            "append_to_inbox: post-write truncate failed for {}: {err}",
            project_dir.display()
        );
    }

    Ok(())
}

/// Soft cap (bytes) used by `append_to_inbox` to trigger post-write
/// compression. Keep this aligned with the walker tick cap in the TUI main
/// loop so both entry points converge on the same steady-state size.
const INBOX_WRITE_SOFT_CAP: usize = 65_536;

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

/// Append feedback directly to a specific project's instructions_inbox.md (no routing needed).
pub fn append_to_inbox_direct(
    feedback_text: &str,
    project_dir: &Path,
    timestamp: &str,
) -> anyhow::Result<()> {
    append_to_inbox(feedback_text, project_dir, timestamp)
}

/// Deprecated alias kept so other crates continue to compile.
#[deprecated(note = "use append_to_inbox")]
pub fn append_to_fb2p(
    feedback_text: &str,
    project_dir: &Path,
    timestamp: &str,
) -> anyhow::Result<()> {
    append_to_inbox(feedback_text, project_dir, timestamp)
}

/// Deprecated alias kept so other crates continue to compile.
#[deprecated(note = "use append_to_inbox_direct")]
pub fn append_to_fb2p_direct(
    feedback_text: &str,
    project_dir: &Path,
    timestamp: &str,
) -> anyhow::Result<()> {
    append_to_inbox_direct(feedback_text, project_dir, timestamp)
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
    /// tmux session name (when Processing).
    pub tmux_session: Option<String>,
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

/// Public access to status map for the deny_commit workflow.
pub fn load_status_map_pub(feedback_dir: &Path) -> HashMap<String, FeedbackMeta> {
    load_status_map(feedback_dir)
}

/// Public access to save status map for the deny_commit workflow.
pub fn save_status_map_pub(feedback_dir: &Path, map: &HashMap<String, FeedbackMeta>) {
    save_status_map(feedback_dir, map);
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
                // Skip temp processing files — they belong to their parent feedback item
                if filename.starts_with('.') || filename.starts_with("append-") {
                    continue;
                }
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

                let tmux_session = meta.and_then(|m| m.tmux_session.clone());

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
                    tmux_session,
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
        append_to_inbox(&text, project_path, &timestamp)?;
    }
    if routes.is_empty() {
        append_to_inbox(&text, projects_dir, &timestamp)?;
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

/// Get the last non-empty line from a tmux session's visible pane.
/// Returns a short status string showing what Claude is currently doing.
pub fn tmux_session_status(session_name: &str) -> Option<String> {
    let output = std::process::Command::new("tmux")
        .args(["capture-pane", "-t", session_name, "-p"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let text = String::from_utf8_lossy(&output.stdout);
    // Find the last non-empty, non-decoration line
    text.lines()
        .rev()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("───") && !t.starts_with("⏵")
            && !t.starts_with("Esc to") && !t.starts_with("ctrl+")
        })
        .next()
        .map(|l| l.trim().chars().take(60).collect())
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

/// Truncates the project's instructions_inbox.md if it exceeds max_bytes.
/// Keeps only the most recent `## Entry:` blocks that fit under the limit,
/// prepends a header line and a `<!-- truncated YYYY-MM-DD HH:MM, kept N of M entries -->` marker.
/// Returns Ok(true) if truncation occurred, Ok(false) if file missing or already small enough.
pub fn truncate_inbox_if_large(project_dir: &Path, max_bytes: usize) -> anyhow::Result<bool> {
    let inbox_path = project_dir.join("instructions_inbox.md");
    if !inbox_path.exists() {
        return Ok(false);
    }

    let contents = fs::read_to_string(&inbox_path)?;
    if contents.len() <= max_bytes {
        return Ok(false);
    }

    let entries = split_entries(&contents);
    let total = entries.len();
    if total == 0 {
        return Ok(false);
    }

    let project_name = project_dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let timestamp = chrono_lite_timestamp();
    let header = format!("# {project_name} — Instructions Inbox\n");
    let marker_template = |kept: usize| {
        format!(
            "\n<!-- truncated {timestamp}, kept {kept} of {total} entries -->\n"
        )
    };

    // Walk newest (last) to oldest (first), accumulating bytes until the next would overflow.
    let mut kept_rev: Vec<&String> = Vec::new();
    let mut used_bytes: usize = header.len() + marker_template(total).len();
    for entry in entries.iter().rev() {
        let cost = entry.len();
        if used_bytes + cost > max_bytes {
            break;
        }
        used_bytes += cost;
        kept_rev.push(entry);
    }

    // Always keep at least one (the most recent) even if it alone exceeds budget.
    if kept_rev.is_empty() {
        if let Some(last) = entries.last() {
            kept_rev.push(last);
        }
    }

    let kept_count = kept_rev.len();
    let mut out = String::new();
    out.push_str(&header);
    out.push_str(&marker_template(kept_count));
    for entry in kept_rev.iter().rev() {
        out.push_str(entry);
    }

    fs::write(&inbox_path, out)?;
    Ok(true)
}

/// Removes any `## Entry:` block from instructions_inbox.md whose `### Status` section
/// contains `Executed: complete` or `Executed: done`. Returns the number of entries removed.
/// Ok(0) if file missing.
pub fn trim_completed_entries(project_dir: &Path) -> anyhow::Result<usize> {
    let inbox_path = project_dir.join("instructions_inbox.md");
    if !inbox_path.exists() {
        return Ok(0);
    }

    let contents = fs::read_to_string(&inbox_path)?;
    let preamble = extract_preamble(&contents);
    let entries = split_entries(&contents);

    let mut removed = 0usize;
    let mut kept: Vec<&String> = Vec::new();
    for entry in &entries {
        if entry_is_completed(entry) {
            removed += 1;
        } else {
            kept.push(entry);
        }
    }

    if removed == 0 {
        return Ok(0);
    }

    let mut out = String::new();
    out.push_str(&preamble);
    for entry in kept {
        out.push_str(entry);
    }

    fs::write(&inbox_path, out)?;
    Ok(removed)
}

/// Report from [`maintain_all_project_inboxes`].
#[derive(Debug, Default)]
pub struct InboxMaintenanceReport {
    /// Project dirs whose inbox was truncated.
    pub truncated: Vec<PathBuf>,
    /// (Project dir, count of entries removed) pairs from [`trim_completed_entries`].
    pub trimmed: Vec<(PathBuf, usize)>,
}

/// Walk every direct subdirectory of `projects_dir` containing
/// `instructions_inbox.md`, then run [`truncate_inbox_if_large`] and
/// [`trim_completed_entries`] on each. Returns a report of what changed.
///
/// Errors on individual projects are logged via `tracing::warn!` and the walk
/// continues. Dotfile dirs and non-directory entries are skipped. Returns an
/// error only if `projects_dir` cannot be read at all.
pub fn maintain_all_project_inboxes(
    projects_dir: &Path,
    max_bytes: usize,
) -> anyhow::Result<InboxMaintenanceReport> {
    let mut report = InboxMaintenanceReport::default();

    let read_dir = fs::read_dir(projects_dir)?;
    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(
                    "maintain_all_project_inboxes: failed to read entry in {}: {err}",
                    projects_dir.display()
                );
                continue;
            }
        };

        // Skip dotfile entries (by name).
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }

        // Skip non-directories.
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(err) => {
                tracing::warn!(
                    "maintain_all_project_inboxes: failed file_type for {}: {err}",
                    entry.path().display()
                );
                continue;
            }
        };
        if !file_type.is_dir() {
            continue;
        }

        let project_path = entry.path();
        if !project_path.join("instructions_inbox.md").exists() {
            continue;
        }

        match truncate_inbox_if_large(&project_path, max_bytes) {
            Ok(true) => report.truncated.push(project_path.clone()),
            Ok(false) => {}
            Err(err) => {
                tracing::warn!(
                    "maintain_all_project_inboxes: truncate failed for {}: {err}",
                    project_path.display()
                );
            }
        }

        match trim_completed_entries(&project_path) {
            Ok(0) => {}
            Ok(n) => report.trimmed.push((project_path.clone(), n)),
            Err(err) => {
                tracing::warn!(
                    "maintain_all_project_inboxes: trim failed for {}: {err}",
                    project_path.display()
                );
            }
        }
    }

    Ok(report)
}

/// Split an inbox file into `## Entry:` blocks. Each returned entry starts with
/// `## Entry:` and ends just before the next one (or EOF).
fn split_entries(contents: &str) -> Vec<String> {
    let marker = "## Entry:";
    let mut entries: Vec<String> = Vec::new();
    let mut search_from = 0usize;
    let mut current_start: Option<usize> = None;
    while let Some(rel) = contents[search_from..].find(marker) {
        let abs = search_from + rel;
        if let Some(start) = current_start {
            entries.push(contents[start..abs].to_string());
        }
        current_start = Some(abs);
        search_from = abs + marker.len();
    }
    if let Some(start) = current_start {
        entries.push(contents[start..].to_string());
    }
    entries
}

/// Return everything in the file before the first `## Entry:` marker (i.e., the preamble/header).
fn extract_preamble(contents: &str) -> String {
    match contents.find("## Entry:") {
        Some(idx) => contents[..idx].to_string(),
        None => contents.to_string(),
    }
}

/// Check whether an entry block's Status section marks it as finished.
fn entry_is_completed(entry: &str) -> bool {
    entry.contains("Executed: complete") || entry.contains("Executed: done")
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

        let inbox = fs::read_to_string(tmp.path().join("testproj").join("instructions_inbox.md")).unwrap();
        assert!(inbox.contains("Fix testproj auth"));
        assert!(inbox.contains("Executed: pending"));
    }

    #[test]
    fn test_word_boundary_match() {
        assert!(word_boundary_match("fix the concord deployment", "concord"));
        assert!(word_boundary_match("concord is broken", "concord"));
        assert!(word_boundary_match("update concord", "concord"));
        assert!(!word_boundary_match("disconcordant behavior", "concord")); // substring
        assert!(word_boundary_match("fix concord/v2 websocket", "concord")); // slash boundary
    }

    fn write_entry(buf: &mut String, ts: &str, body: &str, executed: &str) {
        buf.push_str(&format!(
            "\n---\n\n\
             ## Entry: {ts} — test\n\n\
             ### Raw Input\n\
             {body}\n\n\
             ### Status\n\
             Generated: {ts}\n\
             Executed: {executed}\n"
        ));
    }

    #[test]
    fn test_truncate_inbox_keeps_most_recent() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path();
        let inbox = project.join("instructions_inbox.md");

        let mut contents = String::from("# test — Instructions Inbox\n");
        // 5 entries of varying sizes (each ~500 bytes of body)
        for i in 1..=5 {
            let body = format!("entry-{i}-").repeat(50);
            write_entry(&mut contents, &format!("2026-04-08 10:0{i}"), &body, "pending");
        }
        fs::write(&inbox, &contents).unwrap();
        let original_len = contents.len();

        // Pick a limit that fits roughly 2 entries + header/marker
        let limit = 2000;
        assert!(original_len > limit, "setup: original should exceed limit");

        let changed = truncate_inbox_if_large(project, limit).unwrap();
        assert!(changed, "expected truncation to occur");

        let new_contents = fs::read_to_string(&inbox).unwrap();
        assert!(new_contents.len() <= limit + 200, "truncated output should be near limit");
        assert!(new_contents.contains("<!-- truncated "), "marker missing");
        assert!(new_contents.contains("kept "), "marker missing kept count");
        assert!(new_contents.contains("of 5 entries"), "marker should show original total");
        // Most recent entry (entry-5) must survive; oldest (entry-1) should be gone.
        assert!(new_contents.contains("entry-5-"), "newest entry should be kept");
        assert!(!new_contents.contains("entry-1-"), "oldest entry should be dropped");
        assert!(new_contents.starts_with("# "), "header missing");
        assert!(new_contents.contains("— Instructions Inbox"), "header missing inbox label");
    }

    #[test]
    fn test_trim_completed_removes_finished() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path();
        let inbox = project.join("instructions_inbox.md");

        let mut contents = String::from("# test — Instructions Inbox\n");
        write_entry(&mut contents, "2026-04-08 10:01", "pending-work body", "pending");
        write_entry(&mut contents, "2026-04-08 10:02", "complete-work body", "complete");
        write_entry(&mut contents, "2026-04-08 10:03", "done-work body", "done");
        fs::write(&inbox, &contents).unwrap();

        let removed = trim_completed_entries(project).unwrap();
        assert_eq!(removed, 2, "should remove both complete and done entries");

        let new_contents = fs::read_to_string(&inbox).unwrap();
        assert!(new_contents.contains("pending-work body"), "pending entry should remain");
        assert!(!new_contents.contains("complete-work body"), "complete entry should be gone");
        assert!(!new_contents.contains("done-work body"), "done entry should be gone");
        assert!(new_contents.contains("# test — Instructions Inbox"), "preamble should remain");
    }

    #[test]
    fn test_inbox_lifecycle_handles_missing_file() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path();
        assert!(!project.join("instructions_inbox.md").exists());

        let truncated = truncate_inbox_if_large(project, 1024).unwrap();
        assert!(!truncated, "missing file should yield Ok(false)");

        let trimmed = trim_completed_entries(project).unwrap();
        assert_eq!(trimmed, 0, "missing file should yield Ok(0)");
    }

    #[test]
    fn test_maintain_all_project_inboxes() {
        let tmp = TempDir::new().unwrap();
        let projects_dir = tmp.path();

        // projA: 5 entries, oversized (each body ~1KB, total well over 5KB)
        let proj_a = projects_dir.join("projA");
        fs::create_dir(&proj_a).unwrap();
        let mut a_contents = String::from("# projA — Instructions Inbox\n");
        for i in 1..=5 {
            let body = format!("a-entry-{i}-").repeat(100);
            write_entry(&mut a_contents, &format!("2026-04-08 11:0{i}"), &body, "pending");
        }
        fs::write(proj_a.join("instructions_inbox.md"), &a_contents).unwrap();
        assert!(a_contents.len() > 4096, "setup: projA must exceed limit");

        // projB: 3 entries, one marked Executed: complete
        let proj_b = projects_dir.join("projB");
        fs::create_dir(&proj_b).unwrap();
        let mut b_contents = String::from("# projB — Instructions Inbox\n");
        write_entry(&mut b_contents, "2026-04-08 12:01", "b-pending body", "pending");
        write_entry(&mut b_contents, "2026-04-08 12:02", "b-done body", "complete");
        write_entry(&mut b_contents, "2026-04-08 12:03", "b-more body", "pending");
        fs::write(proj_b.join("instructions_inbox.md"), &b_contents).unwrap();

        // projC: no inbox file at all
        let proj_c = projects_dir.join("projC");
        fs::create_dir(&proj_c).unwrap();

        // Dotfile dir with an inbox — should be skipped entirely
        let dot_dir = projects_dir.join(".feedback");
        fs::create_dir(&dot_dir).unwrap();
        fs::write(dot_dir.join("instructions_inbox.md"), "# should be ignored\n").unwrap();

        let report = maintain_all_project_inboxes(projects_dir, 4096).unwrap();

        assert!(
            report.truncated.iter().any(|p| p == &proj_a),
            "projA should have been truncated, got: {:?}",
            report.truncated
        );
        assert!(
            !report.truncated.iter().any(|p| p == &proj_b),
            "projB was under the limit, should not be truncated"
        );
        assert!(
            !report.truncated.iter().any(|p| p == &proj_c),
            "projC has no inbox, should not appear"
        );

        let b_trim = report.trimmed.iter().find(|(p, _)| p == &proj_b);
        assert!(b_trim.is_some(), "projB should appear in trimmed: {:?}", report.trimmed);
        assert_eq!(b_trim.unwrap().1, 1, "projB should have 1 completed entry removed");

        assert!(
            !report.trimmed.iter().any(|(p, _)| p == &proj_c),
            "projC has no inbox, should not appear in trimmed"
        );
        assert!(
            !report.truncated.iter().any(|p| p == &dot_dir),
            "dotfile dirs must be skipped"
        );
        assert!(
            !report.trimmed.iter().any(|(p, _)| p == &dot_dir),
            "dotfile dirs must be skipped"
        );
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
