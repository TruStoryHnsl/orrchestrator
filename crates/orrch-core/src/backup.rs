// ============================================================================
// orrch-core/src/backup.rs  — CTX-002
// Per-project backup config (stored in the project's `.orrch/`) + push of the
// ShadowRepo to the configured remote. Independent of the project's code repo.
// ============================================================================
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::shadow::ShadowRepo;

/// Where shadow commits get mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupTier {
    /// Bare mirror reachable only over the tailscale subnet (e.g. orrigins).
    TailnetMirror,
    /// Dedicated PRIVATE GitHub repo, separate from the project's code repo.
    GithubPrivate,
    /// Local shadow only — never pushes.
    None,
}

impl Default for BackupTier {
    fn default() -> Self {
        BackupTier::None
    }
}

/// Persisted at `<project>/.orrch/backup.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    #[serde(default)]
    pub tier: BackupTier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default = "default_remote_name")]
    pub remote_name: String,
    #[serde(default = "default_branch")]
    pub branch: String,
}
fn default_remote_name() -> String {
    "backup".into()
}
fn default_branch() -> String {
    "master".into()
}

impl Default for BackupConfig {
    fn default() -> Self {
        // Match serde field defaults so `Default::default()` and a deserialized
        // empty `{}` produce identical configs (remote_name/branch non-empty).
        BackupConfig {
            tier: BackupTier::default(),
            remote_url: None,
            remote_name: default_remote_name(),
            branch: default_branch(),
        }
    }
}

impl BackupConfig {
    /// Path to the config file for a project dir.
    pub fn path_for(project_dir: &Path) -> PathBuf {
        project_dir.join(".orrch").join("backup.json")
    }

    /// Load from `.orrch/backup.json`; returns default (tier None) when absent
    /// or unparseable (legacy-safe).
    pub fn load(project_dir: &Path) -> Self {
        let p = Self::path_for(project_dir);
        match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist to `.orrch/backup.json`, creating `.orrch/` if needed.
    pub fn save(&self, project_dir: &Path) -> std::io::Result<()> {
        let p = Self::path_for(project_dir);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&p, json)
    }
}

/// Outcome of a push attempt, for the CTX-006 coverage table.
#[derive(Debug, Clone)]
pub enum PushOutcome {
    /// tier == None → intentionally did nothing.
    Skipped,
    /// Pushed; carries the resolved remote and branch.
    Pushed { remote: String, branch: String },
    /// Remote unreachable / non-FF / auth failure. Carries stderr; NON-fatal.
    Failed { reason: String },
}

/// Configure (create-or-update) the shadow repo's backup remote per `cfg`.
/// Idempotent. For tier None, REMOVES the remote if present.
pub fn configure_remote(shadow: &ShadowRepo, cfg: &BackupConfig) -> std::io::Result<()> {
    if cfg.tier == BackupTier::None {
        // Remove remote if present (ignore "no such remote").
        let _ = git(shadow)
            .args(["remote", "remove", &cfg.remote_name])
            .output()?;
        return Ok(());
    }

    let url = match &cfg.remote_url {
        Some(u) => u,
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "backup tier requires remote_url",
            ))
        }
    };

    // Does the remote already exist?
    let exists = git(shadow)
        .args(["remote", "get-url", &cfg.remote_name])
        .output()?
        .status
        .success();

    let out = if exists {
        git(shadow)
            .args(["remote", "set-url", &cfg.remote_name, url])
            .output()?
    } else {
        git(shadow)
            .args(["remote", "add", &cfg.remote_name, url])
            .output()?
    };
    if !out.status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "configure_remote failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ),
        ));
    }
    Ok(())
}

/// Fast-forward push the shadow repo to the configured remote.
pub fn push_backup(shadow: &ShadowRepo, cfg: &BackupConfig) -> std::io::Result<PushOutcome> {
    if cfg.tier == BackupTier::None {
        return Ok(PushOutcome::Skipped);
    }
    configure_remote(shadow, cfg)?;

    let out = git(shadow)
        .args(["push", &cfg.remote_name, &cfg.branch])
        .output()?;
    if out.status.success() {
        Ok(PushOutcome::Pushed {
            remote: cfg.remote_name.clone(),
            branch: cfg.branch.clone(),
        })
    } else {
        Ok(PushOutcome::Failed {
            reason: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProvisionPlan {
    pub remote_url: String,
    /// Shell hint the caller may run to create the bare/private remote.
    pub create_hint: String,
}

/// Pure string/struct builder — no network. orrch-core stays gh-free.
pub fn provision_plan(project_id: &str, cfg: &BackupConfig) -> ProvisionPlan {
    match cfg.tier {
        BackupTier::GithubPrivate => {
            let remote_url = cfg.remote_url.clone().unwrap_or_else(|| {
                format!("git@github.com:TruStoryHnsl/{project_id}-context.git")
            });
            let create_hint = format!(
                "gh repo create TruStoryHnsl/{project_id}-context --private --confirm"
            );
            ProvisionPlan {
                remote_url,
                create_hint,
            }
        }
        BackupTier::TailnetMirror => {
            let remote_url = cfg
                .remote_url
                .clone()
                .unwrap_or_else(|| format!("git@orrigins:/srv/orrch-shadow/{project_id}.git"));
            // Derive the on-host bare path from the URL if it's an SSH path.
            let bare_path = remote_url
                .rsplit_once(':')
                .map(|(_, p)| p.to_string())
                .unwrap_or_else(|| format!("/srv/orrch-shadow/{project_id}.git"));
            let create_hint = format!("ssh orrigins 'git init --bare {bare_path}'");
            ProvisionPlan {
                remote_url,
                create_hint,
            }
        }
        BackupTier::None => ProvisionPlan {
            remote_url: String::new(),
            create_hint: String::new(),
        },
    }
}

/// Build a git Command scoped to the shadow repo (GIT_DIR/GIT_WORK_TREE),
/// clearing inherited GIT_* env.
fn git(shadow: &ShadowRepo) -> Command {
    let mut c = Command::new("git");
    c.env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_COMMON_DIR")
        .env("GIT_DIR", shadow.git_dir())
        .env("GIT_WORK_TREE", shadow.work_tree())
        .current_dir(shadow.work_tree());
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shadow::ShadowRepo;
    // Share the ONE process-wide HOME lock with shadow.rs so $HOME mutations in
    // shadow and backup tests serialize against each other (not just within a module).
    use crate::shadow::TEST_ENV_LOCK as ENV_LOCK;

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

    #[test]
    fn config_roundtrips_and_legacy_defaults() {
        let proj = tempfile::tempdir().unwrap();
        let cfg = BackupConfig {
            tier: BackupTier::TailnetMirror,
            remote_url: Some("git@orrigins:/srv/orrch-shadow/x.git".into()),
            remote_name: "backup".into(),
            branch: "master".into(),
        };
        cfg.save(proj.path()).unwrap();
        let loaded = BackupConfig::load(proj.path());
        assert_eq!(loaded.tier, BackupTier::TailnetMirror);
        assert_eq!(loaded.remote_url.as_deref(), Some("git@orrigins:/srv/orrch-shadow/x.git"));

        // legacy/empty json → defaults
        write(&BackupConfig::path_for(proj.path()), "{}");
        let legacy = BackupConfig::load(proj.path());
        assert_eq!(legacy.tier, BackupTier::None);
        assert_eq!(legacy.remote_name, "backup");
        assert_eq!(legacy.branch, "master");

        // snake_case on disk
        let raw = std::fs::read_to_string(BackupConfig::path_for(proj.path())).unwrap();
        cfg.save(proj.path()).unwrap();
        let raw = std::fs::read_to_string(BackupConfig::path_for(proj.path())).unwrap_or(raw);
        assert!(raw.contains("tailnet_mirror"), "raw: {raw}");
    }

    #[test]
    fn push_to_bare_remote() {
        let _l = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let _h = HomeGuard::set(home.path());
        let proj = tempfile::tempdir().unwrap();

        let shadow = ShadowRepo::init(proj.path(), "push-test").unwrap();
        write(&proj.path().join(".orrch/PLAN.md"), "# plan\n");
        shadow.commit_all("plan").unwrap();

        // stand-in tailnet bare remote
        let bare = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .arg(bare.path())
            .output()
            .unwrap();

        let cfg = BackupConfig {
            tier: BackupTier::TailnetMirror,
            remote_url: Some(bare.path().to_string_lossy().to_string()),
            remote_name: "backup".into(),
            branch: "master".into(),
        };

        let outcome = push_backup(&shadow, &cfg).unwrap();
        match outcome {
            PushOutcome::Pushed { remote, branch } => {
                assert_eq!(remote, "backup");
                assert_eq!(branch, "master");
            }
            other => panic!("expected Pushed, got {other:?}"),
        }

        // bare remote now has the commit
        let log = Command::new("git")
            .arg("--git-dir")
            .arg(bare.path())
            .args(["log", "--oneline"])
            .output()
            .unwrap();
        let log_s = String::from_utf8_lossy(&log.stdout);
        assert!(log_s.contains("plan"), "bare log: {log_s}");
    }

    #[test]
    fn none_tier_never_pushes() {
        let _l = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let _h = HomeGuard::set(home.path());
        let proj = tempfile::tempdir().unwrap();

        let shadow = ShadowRepo::init(proj.path(), "none-test").unwrap();
        write(&proj.path().join("PLAN.md"), "x\n");
        shadow.commit_all("x").unwrap();

        let bare = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .arg(bare.path())
            .output()
            .unwrap();

        let cfg = BackupConfig {
            tier: BackupTier::None,
            remote_url: Some(bare.path().to_string_lossy().to_string()),
            ..Default::default()
        };
        assert!(matches!(push_backup(&shadow, &cfg).unwrap(), PushOutcome::Skipped));

        // bare stays empty
        let log = Command::new("git")
            .arg("--git-dir")
            .arg(bare.path())
            .args(["log", "--oneline"])
            .output()
            .unwrap();
        assert!(!log.status.success() || log.stdout.is_empty());
    }

    #[test]
    fn configure_remote_switch_and_remove() {
        let _l = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let _h = HomeGuard::set(home.path());
        let proj = tempfile::tempdir().unwrap();
        let shadow = ShadowRepo::init(proj.path(), "switch-test").unwrap();

        let cfg = BackupConfig {
            tier: BackupTier::TailnetMirror,
            remote_url: Some("ssh://example/x.git".into()),
            remote_name: "backup".into(),
            branch: "master".into(),
        };
        configure_remote(&shadow, &cfg).unwrap();

        let rv = Command::new("git")
            .arg("--git-dir")
            .arg(shadow.git_dir())
            .args(["remote", "-v"])
            .output()
            .unwrap();
        let rv_s = String::from_utf8_lossy(&rv.stdout);
        assert!(rv_s.contains("ssh://example/x.git"), "remote -v: {rv_s}");

        // switch back to None removes it
        let none = BackupConfig {
            tier: BackupTier::None,
            ..Default::default()
        };
        configure_remote(&shadow, &none).unwrap();
        let rv2 = Command::new("git")
            .arg("--git-dir")
            .arg(shadow.git_dir())
            .args(["remote", "-v"])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&rv2.stdout).trim().is_empty());
    }

    #[test]
    fn provision_plan_builds_urls() {
        let gh = BackupConfig {
            tier: BackupTier::GithubPrivate,
            ..Default::default()
        };
        let plan = provision_plan("widget", &gh);
        assert!(plan.remote_url.contains("widget-context.git"));
        assert!(plan.create_hint.contains("gh repo create"));

        let tn = BackupConfig {
            tier: BackupTier::TailnetMirror,
            ..Default::default()
        };
        let plan = provision_plan("widget", &tn);
        assert!(plan.remote_url.contains("orch-shadow") || plan.remote_url.contains("orrch-shadow"));
        assert!(plan.create_hint.contains("git init --bare"));
    }
}
