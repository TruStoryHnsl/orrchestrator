// ============================================================================
// orrch-core/src/shadow.rs  — CTX-001
// ShadowRepo: git via a SEPARATE --git-dir under app-data + --work-tree = the
// project dir. Tracks ONLY operational/context paths. The project's MAIN repo
// must show NOTHING and must contain no second `.git`.
// ============================================================================
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// App-data root: `$HOME/.local/share/orrchestrator/`.
pub fn data_dir() -> PathBuf {
    crate::config::data_dir()
}

/// Per-project shadow GIT_DIR: `<data_dir>/shadow/<project-id>/`.
pub fn shadow_git_dir(project_id: &str) -> PathBuf {
    data_dir().join("shadow").join(project_id)
}

/// The default gitignore-style set of operational paths the shadow repo tracks.
pub const DEFAULT_TRACKED_PATHS: &[&str] = &[
    ".orrch/**",
    "PLAN.md",
    "DEVLOG.md",
    "instructions_inbox.md",
];

const TRACKED_FILE_NAME: &str = "orrch_tracked_paths";

/// A git repository whose state lives entirely OUTSIDE the project tree.
#[derive(Debug, Clone)]
pub struct ShadowRepo {
    git_dir: PathBuf,
    work_tree: PathBuf,
    tracked: Vec<String>,
}

impl ShadowRepo {
    /// Open an existing shadow repo or create it. Idempotent.
    pub fn init(project_dir: &Path, project_id: &str) -> io::Result<Self> {
        let git_dir = shadow_git_dir(project_id);
        let work_tree = project_dir.to_path_buf();
        std::fs::create_dir_all(&git_dir)?;

        let already_inited = git_dir.join("HEAD").exists();

        let mut repo = ShadowRepo {
            git_dir: git_dir.clone(),
            work_tree: work_tree.clone(),
            tracked: load_tracked(&git_dir),
        };

        if !already_inited {
            // `git init` against the external git_dir. We use --bare-style by
            // pointing GIT_DIR at it directly; the work-tree is supplied per
            // command. Run `init` with explicit dir so refs/objects land there.
            run_git_init(&git_dir)?;

            // info/exclude = `*` so untracked project files are never listed.
            let info_dir = git_dir.join("info");
            std::fs::create_dir_all(&info_dir)?;
            std::fs::write(info_dir.join("exclude"), "*\n")?;

            // Configure identity locally so commit always succeeds.
            repo.git_config("user.name", "orrchestrator-shadow")?;
            repo.git_config("user.email", "shadow@orrchestrator.local")?;

            // Persist default tracked set if none recorded yet.
            if repo.tracked.is_empty() {
                repo.tracked = DEFAULT_TRACKED_PATHS.iter().map(|s| s.to_string()).collect();
                write_tracked(&git_dir, &repo.tracked)?;
            }

            // Empty root commit so `log` always succeeds. master branch.
            let out = repo
                .git()
                .args(["commit", "--allow-empty", "-m", "shadow: init"])
                .output()?;
            if !out.status.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "shadow init commit failed: {}",
                        String::from_utf8_lossy(&out.stderr)
                    ),
                ));
            }
        } else if repo.tracked.is_empty() {
            repo.tracked = DEFAULT_TRACKED_PATHS.iter().map(|s| s.to_string()).collect();
        }

        Ok(repo)
    }

    /// Open without creating. Errs if the git_dir doesn't exist yet.
    pub fn open(project_dir: &Path, project_id: &str) -> io::Result<Self> {
        let git_dir = shadow_git_dir(project_id);
        if !git_dir.join("HEAD").exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("shadow repo not initialized at {}", git_dir.display()),
            ));
        }
        let mut tracked = load_tracked(&git_dir);
        if tracked.is_empty() {
            tracked = DEFAULT_TRACKED_PATHS.iter().map(|s| s.to_string()).collect();
        }
        Ok(ShadowRepo {
            git_dir,
            work_tree: project_dir.to_path_buf(),
            tracked,
        })
    }

    /// Override the tracked path set. Persisted to `<git_dir>/orrch_tracked_paths`.
    pub fn track_paths<I, S>(&mut self, globs: I) -> io::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tracked = globs.into_iter().map(Into::into).collect();
        write_tracked(&self.git_dir, &self.tracked)
    }

    /// Stage every changed/new/deleted file under the tracked globs and commit
    /// iff the index is dirty. Returns the new commit's short hash, or None.
    pub fn commit_all(&self, message: &str) -> io::Result<Option<String>> {
        // Translate tracked globs to git pathspecs. git treats a directory
        // pathspec as recursive, so `.orrch/**` → `.orrch`. Only include
        // pathspecs whose target currently exists in the work-tree, otherwise
        // `git add` errors fatally on an unmatched pathspec.
        let pathspecs = self.existing_pathspecs();
        if pathspecs.is_empty() {
            // Nothing tracked present on disk. But a tracked file may have been
            // DELETED (present in index, gone from disk) — still need to detect
            // that via status over the full tracked set. Fall through to status.
        } else {
            // git add --all -- <pathspecs> : stages adds, mods, and deletions
            // within those pathspecs.
            // -f (force): the shadow's info/exclude is `*`, which would
            // otherwise refuse every path as ignored. We explicitly opt in the
            // tracked pathspecs only.
            let mut add = self.git();
            add.args(["add", "--all", "-f", "--"]);
            for g in &pathspecs {
                add.arg(g);
            }
            let out = add.output()?;
            if !out.status.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("shadow add failed: {}", String::from_utf8_lossy(&out.stderr)),
                ));
            }
        }

        // Dirty check: compare index to HEAD over tracked pathspecs. Use
        // `status --porcelain` with pathspecs that exist OR are known to HEAD.
        let status_specs = self.status_pathspecs();
        let mut status = self.git();
        status.args(["status", "--porcelain", "--"]);
        for g in &status_specs {
            status.arg(g);
        }
        let st = status.output()?;
        if !st.status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("shadow status failed: {}", String::from_utf8_lossy(&st.stderr)),
            ));
        }
        if st.stdout.is_empty() {
            return Ok(None);
        }

        let cm = self.git().args(["commit", "-m", message]).output()?;
        if !cm.status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("shadow commit failed: {}", String::from_utf8_lossy(&cm.stderr)),
            ));
        }

        let rev = self.git().args(["rev-parse", "--short", "HEAD"]).output()?;
        if !rev.status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("shadow rev-parse failed: {}", String::from_utf8_lossy(&rev.stderr)),
            ));
        }
        Ok(Some(String::from_utf8_lossy(&rev.stdout).trim().to_string()))
    }

    /// HARD INVARIANT CHECK. Returns true iff the project's MAIN repo shows
    /// nothing attributable to orrchestrator's operational files AND there is no
    /// stray `.git` for the shadow inside the project.
    pub fn is_project_tree_clean(&self) -> io::Result<bool> {
        // (a) shadow git_dir must never be inside work_tree.
        if let (Ok(gd), Ok(wt)) = (self.git_dir.canonicalize(), self.work_tree.canonicalize()) {
            if gd.starts_with(&wt) {
                return Ok(false);
            }
        }

        // (b) project's own git (if any): status --porcelain must not list any
        // tracked operational path.
        let project_git = self.work_tree.join(".git");
        if !project_git.exists() {
            // No project repo → nothing to leak into. Clean by definition.
            return Ok(true);
        }

        let out = Command::new("git")
            .arg("-C")
            .arg(&self.work_tree)
            .args(["status", "--porcelain"])
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()?;
        if !out.status.success() {
            // Not a usable git repo; treat as clean (nothing to leak).
            return Ok(true);
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            // porcelain: "XY <path>"; path starts at col 3.
            let path = line.get(3..).unwrap_or("").trim();
            if path.is_empty() {
                continue;
            }
            if self.path_matches_tracked(path) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Convert a tracked glob into a git pathspec base (strip trailing `/**`
    /// and `/*` — git recurses dirs by default).
    fn glob_to_pathspec(g: &str) -> String {
        if let Some(p) = g.strip_suffix("/**") {
            return p.to_string();
        }
        if let Some(p) = g.strip_suffix("/*") {
            return p.to_string();
        }
        g.to_string()
    }

    /// Pathspecs whose target currently exists in the work-tree (safe to feed
    /// `git add` without an unmatched-pathspec fatal error).
    fn existing_pathspecs(&self) -> Vec<String> {
        self.tracked
            .iter()
            .map(|g| Self::glob_to_pathspec(g))
            .filter(|spec| !spec.is_empty() && self.work_tree.join(spec).exists())
            .collect()
    }

    /// Pathspecs for the dirty-check. Includes existing ones plus any that git
    /// already knows in HEAD (so deletions are detected). We approximate by
    /// using all tracked pathspecs that are tracked-in-index.
    fn status_pathspecs(&self) -> Vec<String> {
        let mut specs: Vec<String> = self.existing_pathspecs();
        // Add pathspecs known to the index (covers deletions of tracked files).
        let ls = self
            .git()
            .args(["ls-files"])
            .output()
            .ok();
        if let Some(ls) = ls {
            if ls.status.success() {
                let listed = String::from_utf8_lossy(&ls.stdout);
                for g in &self.tracked {
                    let base = Self::glob_to_pathspec(g);
                    if base.is_empty() {
                        continue;
                    }
                    let in_index = listed.lines().any(|l| {
                        l == base || l.starts_with(&format!("{base}/"))
                    });
                    if in_index && !specs.contains(&base) {
                        specs.push(base);
                    }
                }
            }
        }
        if specs.is_empty() {
            // Nothing to scope to; return a no-match spec so status is empty.
            // (Empty pathspec list would make status scan the whole tree.)
            specs.push(".orrch-nonexistent-sentinel".to_string());
        }
        specs
    }

    fn path_matches_tracked(&self, path: &str) -> bool {
        for g in &self.tracked {
            if glob_matches(g, path) {
                return true;
            }
        }
        false
    }

    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }
    pub fn work_tree(&self) -> &Path {
        &self.work_tree
    }
    pub fn tracked_paths(&self) -> &[String] {
        &self.tracked
    }

    /// Build a `Command` with GIT_DIR/GIT_WORK_TREE set, inherited GIT_* cleared.
    fn git(&self) -> Command {
        let mut c = Command::new("git");
        c.env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_COMMON_DIR")
            .env("GIT_DIR", &self.git_dir)
            .env("GIT_WORK_TREE", &self.work_tree)
            .current_dir(&self.work_tree);
        c
    }

    fn git_config(&self, key: &str, val: &str) -> io::Result<()> {
        let out = self.git().args(["config", key, val]).output()?;
        if !out.status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("git config {key} failed: {}", String::from_utf8_lossy(&out.stderr)),
            ));
        }
        Ok(())
    }
}

/// `git init` directly against an external dir (no work-tree needed for init).
fn run_git_init(git_dir: &Path) -> io::Result<()> {
    let out = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .args(["init", "--bare", "--initial-branch=master"])
        .arg(git_dir)
        .output()?;
    if out.status.success() {
        return Ok(());
    }
    // Older git without --initial-branch: retry plain, then set HEAD.
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if stderr.contains("initial-branch") || stderr.contains("unknown option") {
        let out2 = Command::new("git")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .args(["init", "--bare"])
            .arg(git_dir)
            .output()?;
        if !out2.status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("git init failed: {}", String::from_utf8_lossy(&out2.stderr)),
            ));
        }
        // Point HEAD at master.
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/master\n")?;
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::Other,
        format!("git init failed: {stderr}"),
    ))
}

fn load_tracked(git_dir: &Path) -> Vec<String> {
    let p = git_dir.join(TRACKED_FILE_NAME);
    match std::fs::read_to_string(&p) {
        Ok(s) => s
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn write_tracked(git_dir: &Path, tracked: &[String]) -> io::Result<()> {
    let p = git_dir.join(TRACKED_FILE_NAME);
    let mut f = std::fs::File::create(&p)?;
    for t in tracked {
        writeln!(f, "{t}")?;
    }
    Ok(())
}

/// Minimal gitignore-ish glob matcher for the porcelain-path check. Supports a
/// trailing `/**` (recursive dir prefix) and exact matches. Sufficient for the
/// DEFAULT_TRACKED_PATHS shapes.
fn glob_matches(pattern: &str, path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        if let Some(rest) = path.strip_prefix(&format!("{prefix}/")) {
            return !rest.contains('/');
        }
        return false;
    }
    if pattern.contains('*') {
        // single-segment wildcard fallback
        if let Some((a, b)) = pattern.split_once('*') {
            return path.starts_with(a) && path.ends_with(b);
        }
    }
    pattern == path
}

/// One process-wide lock shared by every test (in this module AND backup.rs)
/// that overrides $HOME — distinct per-module locks let HOME mutations race.
#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    // data_dir() reads $HOME; ALL tests that override $HOME (here AND in
    // backup.rs) must share ONE process-wide lock, else HOME mutations race
    // across modules under default parallelism. See TEST_ENV_LOCK below.
    use super::TEST_ENV_LOCK as ENV_LOCK;

    struct HomeGuard {
        old: Option<String>,
    }
    impl HomeGuard {
        fn set(p: &Path) -> Self {
            let old = std::env::var("HOME").ok();
            unsafe { std::env::set_var("HOME", p) };
            HomeGuard { old }
        }
    }
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.old {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
    }

    fn write(p: &Path, s: &str) {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, s).unwrap();
    }

    fn git_in(dir: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap()
    }

    #[test]
    fn init_commit_log_and_no_stray_dotgit() {
        let _l = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let _h = HomeGuard::set(home.path());

        // project that is its OWN git repo
        let proj = tempfile::tempdir().unwrap();
        git_in(proj.path(), &["init", "-q"]);
        // main repo gitignores operational paths
        write(
            &proj.path().join(".gitignore"),
            ".orrch/\nPLAN.md\nDEVLOG.md\ninstructions_inbox.md\n",
        );
        git_in(proj.path(), &["add", "-A"]);
        git_in(proj.path(), &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "base"]);

        let shadow = ShadowRepo::init(proj.path(), "proj-test").unwrap();

        // edit operational file
        write(&proj.path().join(".orrch/PLAN.md"), "# plan\n");

        let hash = shadow.commit_all("shadow: plan").unwrap();
        assert!(hash.is_some(), "expected a commit hash");

        // shadow log shows it
        let gd = shadow_git_dir("proj-test");
        let log = Command::new("git")
            .arg("--git-dir")
            .arg(&gd)
            .args(["log", "--oneline"])
            .output()
            .unwrap();
        let log_s = String::from_utf8_lossy(&log.stdout);
        assert!(log_s.contains("shadow: plan"), "log was: {log_s}");

        // no stray shadow .git inside project; shadow_git_dir NOT under project
        assert!(!gd.starts_with(proj.path()));
        // project's only .git is its own repo
        assert!(proj.path().join(".git").exists());

        // project main status clean (operational paths gitignored)
        let st = git_in(proj.path(), &["status", "--porcelain"]);
        assert!(
            st.stdout.is_empty(),
            "project status not clean: {}",
            String::from_utf8_lossy(&st.stdout)
        );

        assert!(shadow.is_project_tree_clean().unwrap());
    }

    #[test]
    fn second_commit_noop_when_unchanged() {
        let _l = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let _h = HomeGuard::set(home.path());
        let proj = tempfile::tempdir().unwrap();

        let shadow = ShadowRepo::init(proj.path(), "noop-test").unwrap();
        write(&proj.path().join(".orrch/DEVLOG.md"), "log\n");
        assert!(shadow.commit_all("first").unwrap().is_some());
        assert!(shadow.commit_all("second").unwrap().is_none());
    }

    #[test]
    fn untracked_file_never_staged() {
        let _l = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let _h = HomeGuard::set(home.path());
        let proj = tempfile::tempdir().unwrap();

        let shadow = ShadowRepo::init(proj.path(), "stage-test").unwrap();
        // a file OUTSIDE tracked globs
        write(&proj.path().join("src/main.rs"), "fn main(){}\n");
        // nothing tracked changed → no commit
        assert!(shadow.commit_all("nope").unwrap().is_none());

        // even after a tracked change, main.rs is not in the tree
        write(&proj.path().join("PLAN.md"), "plan\n");
        shadow.commit_all("plan").unwrap();
        let gd = shadow_git_dir("stage-test");
        let ls = Command::new("git")
            .arg("--git-dir")
            .arg(&gd)
            .args(["ls-files"])
            .output()
            .unwrap();
        let files = String::from_utf8_lossy(&ls.stdout);
        assert!(files.contains("PLAN.md"), "files: {files}");
        assert!(!files.contains("src/main.rs"), "main.rs leaked: {files}");
    }

    #[test]
    fn project_repo_with_unignored_op_path_is_not_clean() {
        let _l = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let _h = HomeGuard::set(home.path());
        let proj = tempfile::tempdir().unwrap();
        git_in(proj.path(), &["init", "-q"]);

        let shadow = ShadowRepo::init(proj.path(), "dirty-test").unwrap();
        // op path NOT gitignored → project status will list it → not clean
        write(&proj.path().join("PLAN.md"), "leak\n");
        assert!(!shadow.is_project_tree_clean().unwrap());
    }

    #[test]
    fn open_errs_before_init() {
        let _l = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let _h = HomeGuard::set(home.path());
        let proj = tempfile::tempdir().unwrap();
        assert!(ShadowRepo::open(proj.path(), "never-inited").is_err());
        ShadowRepo::init(proj.path(), "open-test").unwrap();
        assert!(ShadowRepo::open(proj.path(), "open-test").is_ok());
    }

    #[test]
    fn track_paths_persists() {
        let _l = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let _h = HomeGuard::set(home.path());
        let proj = tempfile::tempdir().unwrap();
        let mut shadow = ShadowRepo::init(proj.path(), "track-test").unwrap();
        shadow.track_paths(["docs/**", "NOTES.md"]).unwrap();
        let reopened = ShadowRepo::open(proj.path(), "track-test").unwrap();
        assert_eq!(reopened.tracked_paths(), &["docs/**", "NOTES.md"]);
    }
}
