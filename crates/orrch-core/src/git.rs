//! Git operations — check status and spawn Claude-powered commit sessions.

use std::path::Path;
use std::process::Command;

/// Summary of a project's git state.
#[derive(Debug, Clone)]
pub struct GitStatus {
    pub has_repo: bool,
    pub has_remote: bool,
    pub remote_url: String,
    pub dirty_count: usize,
    pub branch: String,
    pub unpushed: usize,
}

/// Check the git status of a project directory.
pub fn check_status(project_dir: &Path) -> GitStatus {
    let git_dir = project_dir.join(".git");
    if !git_dir.exists() {
        return GitStatus {
            has_repo: false, has_remote: false, remote_url: String::new(),
            dirty_count: 0, branch: String::new(), unpushed: 0,
        };
    }

    let remote_url = git_output(project_dir, &["remote", "get-url", "origin"]);
    let branch = git_output(project_dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let dirty = git_output(project_dir, &["status", "--porcelain"]);
    let dirty_count = dirty.lines().filter(|l| !l.trim().is_empty()).count();

    let unpushed = git_output(project_dir, &["rev-list", "--count", "@{u}..HEAD"])
        .trim()
        .parse::<usize>()
        .unwrap_or(0);

    GitStatus {
        has_repo: true,
        has_remote: !remote_url.is_empty(),
        remote_url,
        dirty_count,
        branch,
        unpushed,
    }
}

/// Spawn a Claude tmux session to commit and push a project.
///
/// Claude analyzes the changes, writes a proper commit message,
/// stages appropriate files, commits, and pushes.
pub fn spawn_commit_session(
    project_dir: &Path,
    project_name: &str,
) -> anyhow::Result<String> {
    let session_name = format!("orrch-git-{}", project_name);
    let projects_dir = project_dir.parent().unwrap_or(project_dir);
    let feedback_dir = projects_dir.join(".feedback");
    std::fs::create_dir_all(&feedback_dir)?;

    let prompt = format!(
        "You are committing changes for the {} project.\n\n\
Review the current git status and diff, then:\n\
1. Stage all appropriate changes (skip .env files, credentials, large binaries)\n\
2. Write a commit message using Conventional Commits format: <type>[scope]: <description>\n\
   Types: feat, fix, docs, refactor, perf, test, chore, ci, build\n\
   Example: feat(scraper): add retry logic\n\
3. If there are multiple unrelated changes, create multiple commits — one per logical change\n\
4. Commit the changes\n\
5. Push to the remote if one is configured\n\
6. Report what was committed and pushed\n\n\
Use `git add -A` for most cases unless there are files that should be excluded.\n\
If there are no changes to commit, just say so and exit.",
        project_name,
    );

    let prompt_path = feedback_dir.join(format!(".git-commit-{}.md", project_name));
    std::fs::write(&prompt_path, &prompt)?;

    let runner_path = feedback_dir.join(format!(".git-commit-{}.sh", project_name));
    let runner = format!(
        "#!/bin/bash\ncd {dir}\nprompt=$(cat {prompt})\nclaude --dangerously-skip-permissions \"$prompt\"\nrm -f {prompt} {runner}\n",
        dir = project_dir.display(),
        prompt = prompt_path.display(),
        runner = runner_path.display(),
    );
    std::fs::write(&runner_path, &runner)?;

    // Kill any existing session with this name to prevent "duplicate session" crashes
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", &session_name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", &session_name])
        .arg("bash")
        .arg(runner_path.to_string_lossy().as_ref())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(session_name),
        Ok(s) => anyhow::bail!("tmux exited with {}", s),
        Err(e) => anyhow::bail!("Failed to run tmux: {e}"),
    }
}

/// Spawn commit sessions for ALL projects that have dirty git repos with remotes.
/// Returns the list of (project_name, session_name) pairs spawned.
pub fn spawn_commit_all(projects_dir: &Path) -> Vec<(String, String)> {
    let mut spawned = Vec::new();
    let Ok(entries) = std::fs::read_dir(projects_dir) else { return spawned };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if name.starts_with('.') || name == "deprecated" { continue; }

        let status = check_status(&path);
        if status.has_repo && status.has_remote && status.dirty_count > 0 {
            match spawn_commit_session(&path, &name) {
                Ok(session) => spawned.push((name, session)),
                Err(_) => {} // skip failures silently
            }
        }
    }
    spawned
}

fn git_output(dir: &Path, args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}
