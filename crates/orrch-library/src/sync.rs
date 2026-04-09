//! Git-backed synchronization for the library directory.
//!
//! The library is a git repo containing agent profiles, workforces, models, etc.
//! These functions provide a thin wrapper around `git` for clone/pull/push.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

/// Clone the library repo into `library_dir` if it doesn't already exist as a git repo.
/// No-op if `library_dir/.git` exists. Creates parent dirs as needed.
pub fn clone_if_missing(library_dir: &Path, repo_url: &str) -> Result<()> {
    if library_dir.join(".git").exists() {
        return Ok(());
    }

    if let Some(parent) = library_dir.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent dir {}", parent.display()))?;
    }

    let output = Command::new("git")
        .arg("clone")
        .arg(repo_url)
        .arg(library_dir)
        .output()
        .context("failed to invoke git clone")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git clone failed: {}", stderr);
    }

    Ok(())
}

/// Run `git pull --ff-only` in `library_dir`.
/// Errors if `library_dir` is not a git repo or pull fails.
pub fn sync_pull(library_dir: &Path) -> Result<()> {
    if !library_dir.join(".git").exists() {
        bail!(
            "not a git repo: {} (missing .git directory)",
            library_dir.display()
        );
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(library_dir)
        .arg("pull")
        .arg("--ff-only")
        .output()
        .context("failed to invoke git pull")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git pull failed: {}", stderr);
    }

    Ok(())
}

/// Stage all changes, commit with `message`, and push.
/// No-op (returns Ok) if there's nothing to commit.
/// Errors if `library_dir` is not a git repo or push fails.
pub fn sync_push(library_dir: &Path, message: &str) -> Result<()> {
    if !library_dir.join(".git").exists() {
        bail!(
            "not a git repo: {} (missing .git directory)",
            library_dir.display()
        );
    }

    // Stage all changes.
    let add_output = Command::new("git")
        .arg("-C")
        .arg(library_dir)
        .arg("add")
        .arg("-A")
        .output()
        .context("failed to invoke git add")?;

    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        bail!("git add failed: {}", stderr);
    }

    // Detect "nothing to commit" via `git diff --cached --quiet`.
    // Exit code 0 = no staged changes, exit code 1 = changes present.
    let diff_output = Command::new("git")
        .arg("-C")
        .arg(library_dir)
        .arg("diff")
        .arg("--cached")
        .arg("--quiet")
        .output()
        .context("failed to invoke git diff --cached")?;

    if diff_output.status.success() {
        // Nothing staged — no-op.
        return Ok(());
    }

    // Commit.
    let commit_output = Command::new("git")
        .arg("-C")
        .arg(library_dir)
        .arg("commit")
        .arg("-m")
        .arg(message)
        .output()
        .context("failed to invoke git commit")?;

    if !commit_output.status.success() {
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        bail!("git commit failed: {}", stderr);
    }

    // Push.
    let push_output = Command::new("git")
        .arg("-C")
        .arg(library_dir)
        .arg("push")
        .output()
        .context("failed to invoke git push")?;

    if !push_output.status.success() {
        let stderr = String::from_utf8_lossy(&push_output.stderr);
        bail!("git push failed: {}", stderr);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Create a unique temp dir under the system temp root.
    fn unique_tmp(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "orrch-sync-test-{}-{}-{}-{}",
            label, pid, nanos, seq
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn git_init(dir: &Path) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["init", "-q"])
            .status()
            .expect("git init");
        assert!(status.success(), "git init failed");

        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", "--local", "user.email", "test@test"])
            .status()
            .expect("git config email");
        assert!(status.success());

        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", "--local", "user.name", "Test"])
            .status()
            .expect("git config name");
        assert!(status.success());

        // Disable gpg signing for tests.
        let _ = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", "--local", "commit.gpgsign", "false"])
            .status();
    }

    #[test]
    fn test_clone_if_missing_skips_existing_repo() {
        let tmp = unique_tmp("clone-skip");
        git_init(&tmp);

        // Record the .git inode / mtime baseline via directory listing.
        let git_dir = tmp.join(".git");
        assert!(git_dir.exists());
        let before = std::fs::metadata(&git_dir).unwrap().modified().ok();

        // Call with a bogus URL — should be a no-op because .git already exists.
        let res = clone_if_missing(&tmp, "file:///nonexistent/bogus-url");
        assert!(res.is_ok(), "expected no-op Ok, got {:?}", res);
        assert!(git_dir.exists());

        let after = std::fs::metadata(&git_dir).unwrap().modified().ok();
        assert_eq!(before, after, ".git mtime should be unchanged");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_sync_pull_errors_on_non_repo() {
        let tmp = unique_tmp("pull-nonrepo");
        // No git init — just an empty dir.

        let res = sync_pull(&tmp);
        assert!(res.is_err(), "expected Err for non-repo, got {:?}", res);
        let err = format!("{}", res.unwrap_err());
        assert!(
            err.contains("not a git repo"),
            "error should mention 'not a git repo': {}",
            err
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_sync_push_no_changes_is_ok() {
        let tmp = unique_tmp("push-noop");
        git_init(&tmp);

        // No files, no staged changes — sync_push should return Ok without touching remote.
        let res = sync_push(&tmp, "test message");
        assert!(
            res.is_ok(),
            "expected Ok (no changes to commit), got {:?}",
            res
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
