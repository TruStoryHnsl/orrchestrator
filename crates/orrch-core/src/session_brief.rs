//! OPT-010(d,e): Session close protocol — write per-session brief markdown
//! to `<project>/.orrch/session_briefs/<timestamp>-<sid>.md` capturing the
//! session's goal, duration, files changed (git diff --stat) and commits
//! since the session started. Plus helpers to enumerate existing briefs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Directory name (relative to project root) where session briefs are persisted.
pub const SESSION_BRIEFS_SUBDIR: &str = ".orrch/session_briefs";

/// One brief on disk. Filename format: `<epoch>-<sid>.md`.
#[derive(Debug, Clone)]
pub struct SessionBrief {
    pub path: PathBuf,
    pub filename: String,
    /// Unix epoch seconds parsed from filename. 0 if unparseable.
    pub epoch: u64,
    /// Session id parsed from filename (suffix after the epoch).
    pub sid: String,
}

/// Input parameters for `write_session_brief`.
pub struct SessionBriefInput<'a> {
    pub project_dir: &'a Path,
    pub sid: &'a str,
    pub goal: Option<&'a str>,
    pub duration_secs: u64,
    /// Optional git revision the session began at. When provided, the brief
    /// includes a `Commits since start` section with `git log <start>..HEAD`.
    pub start_commit: Option<&'a str>,
}

/// Write a session brief markdown file for `sid` under the project's
/// `.orrch/session_briefs/` directory. Returns the path on success.
///
/// The file has four sections: Goal, Duration, Files changed (git diff --stat),
/// Commits since start (git log). If git isn't available or the project isn't
/// a git repo, the git-derived sections gracefully degrade to "(no git data)".
pub fn write_session_brief(input: SessionBriefInput<'_>) -> std::io::Result<PathBuf> {
    let dir = input.project_dir.join(SESSION_BRIEFS_SUBDIR);
    std::fs::create_dir_all(&dir)?;

    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let safe_sid = sanitize_sid(input.sid);
    let filename = format!("{epoch}-{safe_sid}.md");
    let path = dir.join(&filename);

    let goal = input.goal.unwrap_or("(no goal)");
    let duration = format_duration(input.duration_secs);
    let files_changed = git_diff_stat(input.project_dir);
    let commits = match input.start_commit {
        Some(rev) => git_log_since(input.project_dir, rev),
        None => String::from("(no start revision recorded)"),
    };

    let body = format!(
        "# Session brief: {sid}\n\n\
         _Written at epoch {epoch} ({date})._\n\n\
         ## Goal\n{goal}\n\n\
         ## Duration\n{duration}\n\n\
         ## Files changed\n```\n{files_changed}\n```\n\n\
         ## Commits since start\n```\n{commits}\n```\n",
        sid = input.sid,
        epoch = epoch,
        date = rfc3339_from_epoch(epoch),
        goal = goal,
        duration = duration,
        files_changed = files_changed,
        commits = commits,
    );

    std::fs::write(&path, body)?;
    Ok(path)
}

/// Enumerate existing session briefs for a project. Most-recent first.
pub fn list_session_briefs(project_dir: &Path) -> Vec<SessionBrief> {
    let dir = project_dir.join(SESSION_BRIEFS_SUBDIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut briefs: Vec<SessionBrief> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let filename = e.file_name().to_string_lossy().into_owned();
            if !filename.ends_with(".md") {
                return None;
            }
            let (epoch, sid) = parse_brief_filename(&filename);
            Some(SessionBrief { path, filename, epoch, sid })
        })
        .collect();
    briefs.sort_by(|a, b| b.epoch.cmp(&a.epoch));
    briefs
}

/// Parse `<epoch>-<sid>.md` → (epoch, sid). Degrades gracefully on malformed names.
fn parse_brief_filename(filename: &str) -> (u64, String) {
    let stem = filename.strip_suffix(".md").unwrap_or(filename);
    match stem.split_once('-') {
        Some((ep, sid)) => (ep.parse().unwrap_or(0), sid.to_string()),
        None => (0, stem.to_string()),
    }
}

fn sanitize_sid(sid: &str) -> String {
    sid.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn rfc3339_from_epoch(_epoch: u64) -> String {
    // Minimal placeholder — orrch-core deliberately avoids pulling chrono for
    // one-line display strings. The epoch is authoritative; this is cosmetic.
    String::from("utc")
}

fn git_diff_stat(project_dir: &Path) -> String {
    let output = Command::new("git")
        .arg("-C").arg(project_dir)
        .arg("diff").arg("--stat").arg("HEAD")
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { "(no changes)".into() } else { s }
        }
        _ => "(no git data)".into(),
    }
}

fn git_log_since(project_dir: &Path, start: &str) -> String {
    let range = format!("{start}..HEAD");
    let output = Command::new("git")
        .arg("-C").arg(project_dir)
        .arg("log").arg("--oneline").arg(&range)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { "(no new commits)".into() } else { s }
        }
        _ => "(no git data)".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_project() -> PathBuf {
        let name = format!(
            "orrch-brief-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn writes_brief_with_all_sections() {
        let proj = tmp_project();
        let res = write_session_brief(SessionBriefInput {
            project_dir: &proj,
            sid: "test-sid-001",
            goal: Some("do the thing"),
            duration_secs: 185,
            start_commit: None,
        });
        let path = res.expect("brief write");
        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("# Session brief: test-sid-001"));
        assert!(contents.contains("## Goal"));
        assert!(contents.contains("do the thing"));
        assert!(contents.contains("## Duration"));
        assert!(contents.contains("3m05s"));
        assert!(contents.contains("## Files changed"));
        assert!(contents.contains("## Commits since start"));
        std::fs::remove_dir_all(&proj).ok();
    }

    #[test]
    fn lists_briefs_most_recent_first() {
        let proj = tmp_project();
        let dir = proj.join(SESSION_BRIEFS_SUBDIR);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("100-aaa.md"), "a").unwrap();
        std::fs::write(dir.join("200-bbb.md"), "b").unwrap();
        std::fs::write(dir.join("150-ccc.md"), "c").unwrap();
        std::fs::write(dir.join("not-a-brief.txt"), "skip").unwrap();

        let briefs = list_session_briefs(&proj);
        assert_eq!(briefs.len(), 3);
        assert_eq!(briefs[0].sid, "bbb");
        assert_eq!(briefs[1].sid, "ccc");
        assert_eq!(briefs[2].sid, "aaa");
        std::fs::remove_dir_all(&proj).ok();
    }

    #[test]
    fn sanitizes_sid_in_filename() {
        let proj = tmp_project();
        let res = write_session_brief(SessionBriefInput {
            project_dir: &proj,
            sid: "weird/sid with spaces",
            goal: None,
            duration_secs: 0,
            start_commit: None,
        }).unwrap();
        let filename = res.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!filename.contains('/'));
        assert!(!filename.contains(' '));
        assert!(filename.ends_with(".md"));
        std::fs::remove_dir_all(&proj).ok();
    }
}
