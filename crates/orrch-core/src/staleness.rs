//! Source-tree staleness detection.
//!
//! At build time the binary records its own build timestamp and the
//! absolute path to the workspace repo root (injected via `build.rs`).
//! At runtime, [`StalenessMonitor`] periodically walks the repo root
//! looking for source files (`.rs`, `Cargo.toml`, `build.rs`) whose
//! mtime is newer than the build timestamp.
//!
//! Hot-swappable paths — runtime-loaded markdown, config files,
//! per-user state — are excluded from the scan because changing them
//! does NOT require a rebuild.
//!
//! When stale files are present, the TUI and WebUI render a protest
//! banner so the operator never silently runs against a build that
//! doesn't match the source tree.
//!
//! See `src/build.rs` for the env-var injection, and `bin/orrchestrator`
//! / `bin/orrchestrator-dev` (in the `packaging/` tree) for the install
//! pattern.

use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Process-global staleness state, written by [`StalenessMonitor`] and
/// read by the TUI and WebUI banner renderers.
static STATE: OnceLock<RwLock<StalenessState>> = OnceLock::new();

fn state() -> &'static RwLock<StalenessState> {
    STATE.get_or_init(|| RwLock::new(StalenessState::default()))
}

/// Snapshot of staleness information used by UI renderers.
#[derive(Debug, Clone, Default)]
pub struct StalenessState {
    /// Whether the monitor has been initialized (build info available).
    pub initialized: bool,
    /// Files that have mtime > build timestamp.
    pub stale_files: Vec<PathBuf>,
    /// Last time the source tree was scanned.
    pub last_scan: Option<SystemTime>,
    /// Build timestamp the running binary was compiled at.
    pub build_timestamp: Option<SystemTime>,
    /// Repo root the binary was built from, if it still exists on disk.
    pub repo_root: Option<PathBuf>,
}

impl StalenessState {
    /// True when there is at least one source file newer than the binary.
    pub fn is_stale(&self) -> bool {
        !self.stale_files.is_empty()
    }
}

/// Read a snapshot of the current staleness state.
pub fn snapshot() -> StalenessState {
    state().read().map(|s| s.clone()).unwrap_or_default()
}

/// True iff the running binary is older than at least one rebuild-required source file.
pub fn is_stale() -> bool {
    state().read().map(|s| s.is_stale()).unwrap_or(false)
}

/// Build-time configuration passed to [`start_monitor`].
#[derive(Debug, Clone, Copy)]
pub struct BuildInfo {
    /// Nanoseconds since UNIX epoch at compile time.
    pub timestamp_ns: u128,
    /// Workspace root the binary was built from (absolute path).
    pub repo_root: &'static str,
}

/// Spawn a background thread that polls the source tree for staleness.
///
/// Safe to call multiple times — only the first call wins. If the repo
/// root no longer exists on disk (e.g. the binary was shipped to a
/// machine without the source tree), the monitor records that fact and
/// exits silently — the UI then never shows the banner.
pub fn start_monitor(info: BuildInfo) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }

    let build_ts = UNIX_EPOCH + Duration::from_nanos(info.timestamp_ns as u64);
    let repo_root = PathBuf::from(info.repo_root);

    // Always populate the build info, even if the repo root is gone.
    if let Ok(mut st) = state().write() {
        st.initialized = true;
        st.build_timestamp = Some(build_ts);
        if repo_root.exists() {
            st.repo_root = Some(repo_root.clone());
        }
    }

    if !repo_root.exists() {
        return;
    }

    let interval = if std::env::var("ORRCH_DEV_MODE").is_ok() {
        Duration::from_secs(2)
    } else {
        Duration::from_secs(15)
    };

    std::thread::Builder::new()
        .name("orrch-staleness".into())
        .spawn(move || loop {
            let stale = scan(&repo_root, build_ts);
            if let Ok(mut st) = state().write() {
                st.stale_files = stale;
                st.last_scan = Some(SystemTime::now());
            }
            std::thread::sleep(interval);
        })
        .ok();
}

/// Convenience wrapper that reads ORRCH_BUILD_TIMESTAMP_NS / ORRCH_BUILD_REPO_ROOT
/// embedded by `build.rs` and starts the monitor. Returns `false` if the env
/// vars were missing (shouldn't happen for a normal build, but defensive).
pub fn start_monitor_from_env() -> bool {
    let Some(ts) = option_env!("ORRCH_BUILD_TIMESTAMP_NS").and_then(|s| s.parse::<u128>().ok())
    else {
        return false;
    };
    let Some(root) = option_env!("ORRCH_BUILD_REPO_ROOT") else {
        return false;
    };
    start_monitor(BuildInfo {
        timestamp_ns: ts,
        repo_root: root,
    });
    true
}

// ─── Scanning ──────────────────────────────────────────────────────────

/// Source file patterns that require a rebuild when changed.
///
/// Hot-swappable paths (markdown libraries, agents, workforces, plans,
/// per-user config) are NOT in this list — those are read from disk at
/// runtime and changing them does not invalidate the binary.
fn is_rebuild_required(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        // Files with no extension that still trigger rebuilds.
        return matches!(name, "Cargo.lock" | "build.rs");
    };
    matches!(ext, "rs") || matches!(name, "Cargo.toml" | "Cargo.lock" | "build.rs")
}

/// Directories never worth descending into.
fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        "target"
            | ".git"
            | "node_modules"
            | ".idea"
            | ".vscode"
            | "dist"
            | "build"
            | "__pycache__"
    )
}

fn scan(repo_root: &Path, build_ts: SystemTime) -> Vec<PathBuf> {
    let mut stale: Vec<PathBuf> = Vec::new();
    walk(repo_root, build_ts, &mut stale, 0);
    stale.sort();
    stale
}

const MAX_DEPTH: usize = 8;

fn walk(dir: &Path, build_ts: SystemTime, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if is_ignored_dir(name) || name.starts_with('.') {
                continue;
            }
            walk(&path, build_ts, out, depth + 1);
        } else if ft.is_file() && is_rebuild_required(&path) {
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    if mtime > build_ts {
                        out.push(path);
                    }
                }
            }
        }
    }
}

// ─── Banner rendering helpers ──────────────────────────────────────────

/// Banner text rendered in TUI + WebUI when stale files are detected.
/// Single string for both surfaces so the operator sees identical wording.
pub fn banner_text(state: &StalenessState) -> String {
    let n = state.stale_files.len();
    if n == 0 {
        return String::new();
    }
    format!(
        "⚠  Changes have been made to files that can't be hot-swapped. \
         Rebuild before further testing.  ({} file{} changed since last build)",
        n,
        if n == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_required_classifier() {
        assert!(is_rebuild_required(Path::new("src/main.rs")));
        assert!(is_rebuild_required(Path::new("crates/x/src/lib.rs")));
        assert!(is_rebuild_required(Path::new("Cargo.toml")));
        assert!(is_rebuild_required(Path::new("build.rs")));
        assert!(is_rebuild_required(Path::new("Cargo.lock")));
        assert!(!is_rebuild_required(Path::new("README.md")));
        assert!(!is_rebuild_required(Path::new("library/skills/foo.md")));
        assert!(!is_rebuild_required(Path::new("agents/pm.md")));
        assert!(!is_rebuild_required(Path::new(".gitignore")));
    }

    #[test]
    fn ignored_dirs() {
        assert!(is_ignored_dir("target"));
        assert!(is_ignored_dir(".git"));
        assert!(is_ignored_dir("node_modules"));
        assert!(!is_ignored_dir("src"));
        assert!(!is_ignored_dir("crates"));
    }

    #[test]
    fn banner_text_format() {
        let mut st = StalenessState::default();
        assert_eq!(banner_text(&st), "");
        st.stale_files = vec![PathBuf::from("src/main.rs")];
        let s = banner_text(&st);
        assert!(s.contains("1 file changed"));
        assert!(s.contains("hot-swapped"));
        st.stale_files.push(PathBuf::from("Cargo.toml"));
        let s = banner_text(&st);
        assert!(s.contains("2 files changed"));
    }
}
