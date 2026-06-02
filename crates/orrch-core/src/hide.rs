// ============================================================================
// orrch-core/src/hide.rs  — CTX-003
// Move operational context into `.orrch/` and leave a 1-line @import stub at the
// root CLAUDE.md so it stays root-discoverable. Reversible. Idempotent.
// MUST NOT be auto-applied to orrchestrator's own tree (caller-enforced guard).
// ============================================================================
use std::io;
use std::path::{Path, PathBuf};

/// Files relocated wholesale by hide_context (CLAUDE.md is special — stubbed).
const FULLY_HIDDEN: &[&str] = &["PLAN.md", "DEVLOG.md", "instructions_inbox.md"];

/// The exact stub written to the root CLAUDE.md after its body is moved.
pub const CLAUDE_IMPORT_STUB: &str = "@.orrch/CLAUDE.md\n";

/// Result describing what changed.
#[derive(Debug, Clone, Default)]
pub struct HideReport {
    /// Paths now living under `.orrch/` (relative to project root).
    pub moved: Vec<PathBuf>,
    /// True if a root CLAUDE.md stub was written this run.
    pub stub_written: bool,
    /// True if nothing needed doing (already hidden).
    pub already_hidden: bool,
}

/// Move PLAN/DEVLOG/instructions_inbox + CLAUDE.md body into `.orrch/`, and
/// replace root `CLAUDE.md` with the one-line `@.orrch/CLAUDE.md` import stub.
pub fn hide_context(project_dir: &Path) -> io::Result<HideReport> {
    let orrch = project_dir.join(".orrch");
    let mut report = HideReport::default();

    let claude_root = project_dir.join("CLAUDE.md");
    let claude_hidden = orrch.join("CLAUDE.md");

    // Pre-flight: detect a pre-existing differing `.orrch/CLAUDE.md` BEFORE any
    // mutation so we error cleanly with no partial state.
    let claude_needs_move = claude_root.exists() && {
        let body = std::fs::read_to_string(&claude_root)?;
        body != CLAUDE_IMPORT_STUB
    };
    if claude_needs_move && claude_hidden.exists() {
        let existing = std::fs::read_to_string(&claude_hidden)?;
        let body = std::fs::read_to_string(&claude_root)?;
        if existing != body {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "refusing to clobber differing {}",
                    claude_hidden.display()
                ),
            ));
        }
    }

    std::fs::create_dir_all(&orrch)?;

    // Move FULLY_HIDDEN files.
    let mut did_something = false;
    for name in FULLY_HIDDEN {
        let root = project_dir.join(name);
        let hidden = orrch.join(name);
        if root.exists() {
            if hidden.exists() {
                // Both exist — idempotent skip is only valid when root absent;
                // here root still present so remove the root copy (hidden wins).
                // To avoid silent data loss we only drop root if contents match.
                let r = std::fs::read(&root)?;
                let h = std::fs::read(&hidden)?;
                if r == h {
                    std::fs::remove_file(&root)?;
                    report.moved.push(PathBuf::from(format!(".orrch/{name}")));
                    did_something = true;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("refusing to clobber differing {}", hidden.display()),
                    ));
                }
            } else {
                std::fs::rename(&root, &hidden)?;
                report.moved.push(PathBuf::from(format!(".orrch/{name}")));
                did_something = true;
            }
        }
    }

    // CLAUDE.md handling.
    if claude_needs_move {
        let body = std::fs::read_to_string(&claude_root)?;
        std::fs::write(&claude_hidden, body)?;
        std::fs::write(&claude_root, CLAUDE_IMPORT_STUB)?;
        report.moved.push(PathBuf::from(".orrch/CLAUDE.md"));
        report.stub_written = true;
        did_something = true;
    }

    if !did_something {
        report.already_hidden = true;
    }
    Ok(report)
}

/// Inverse of hide_context.
pub fn reveal_context(project_dir: &Path) -> io::Result<HideReport> {
    let orrch = project_dir.join(".orrch");
    let mut report = HideReport::default();
    let mut did_something = false;

    for name in FULLY_HIDDEN {
        let root = project_dir.join(name);
        let hidden = orrch.join(name);
        if hidden.exists() && !root.exists() {
            std::fs::rename(&hidden, &root)?;
            report.moved.push(PathBuf::from(name));
            did_something = true;
        }
    }

    // CLAUDE.md: if root is the stub and hidden exists, restore.
    let claude_root = project_dir.join("CLAUDE.md");
    let claude_hidden = orrch.join("CLAUDE.md");
    if claude_hidden.exists() {
        let root_is_stub = match std::fs::read_to_string(&claude_root) {
            Ok(s) => s == CLAUDE_IMPORT_STUB,
            Err(_) => false,
        };
        if root_is_stub {
            let body = std::fs::read_to_string(&claude_hidden)?;
            std::fs::write(&claude_root, body)?;
            std::fs::remove_file(&claude_hidden)?;
            report.moved.push(PathBuf::from("CLAUDE.md"));
            did_something = true;
        }
    }

    if !did_something {
        report.already_hidden = true;
    }
    Ok(report)
}

/// Returns true if `project_dir` is orrchestrator's own source tree.
pub fn is_orrchestrator_self(project_dir: &Path) -> bool {
    let basename_ok = project_dir
        .file_name()
        .map(|n| n == "orrchestrator")
        .unwrap_or(false);
    basename_ok && project_dir.join("crates/orrch-core/Cargo.toml").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, s: &str) {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, s).unwrap();
    }

    #[test]
    fn hide_and_reveal_roundtrip() {
        let proj = tempfile::tempdir().unwrap();
        write(&proj.path().join("CLAUDE.md"), "FULL\n");
        write(&proj.path().join("PLAN.md"), "plan\n");
        write(&proj.path().join("DEVLOG.md"), "log\n");

        let rep = hide_context(proj.path()).unwrap();
        assert!(rep.stub_written);
        assert!(!rep.already_hidden);

        assert_eq!(
            std::fs::read_to_string(proj.path().join("CLAUDE.md")).unwrap(),
            "@.orrch/CLAUDE.md\n"
        );
        assert_eq!(
            std::fs::read_to_string(proj.path().join(".orrch/CLAUDE.md")).unwrap(),
            "FULL\n"
        );
        assert!(!proj.path().join("PLAN.md").exists());
        assert!(proj.path().join(".orrch/PLAN.md").exists());
        assert!(rep.moved.iter().any(|p| p == &PathBuf::from(".orrch/PLAN.md")));
        assert!(rep.moved.iter().any(|p| p == &PathBuf::from(".orrch/DEVLOG.md")));

        let rev = reveal_context(proj.path()).unwrap();
        assert!(!rev.already_hidden);
        assert_eq!(
            std::fs::read_to_string(proj.path().join("CLAUDE.md")).unwrap(),
            "FULL\n"
        );
        assert!(proj.path().join("PLAN.md").exists());
        assert!(proj.path().join("DEVLOG.md").exists());
        assert!(!proj.path().join(".orrch/CLAUDE.md").exists());
    }

    #[test]
    fn hide_twice_is_idempotent() {
        let proj = tempfile::tempdir().unwrap();
        write(&proj.path().join("CLAUDE.md"), "FULL\n");
        write(&proj.path().join("PLAN.md"), "plan\n");

        hide_context(proj.path()).unwrap();
        let again = hide_context(proj.path()).unwrap();
        assert!(again.already_hidden);
        // no data loss
        assert_eq!(
            std::fs::read_to_string(proj.path().join(".orrch/PLAN.md")).unwrap(),
            "plan\n"
        );
        assert_eq!(
            std::fs::read_to_string(proj.path().join(".orrch/CLAUDE.md")).unwrap(),
            "FULL\n"
        );
    }

    #[test]
    fn differing_hidden_claude_errors_no_partial_state() {
        let proj = tempfile::tempdir().unwrap();
        write(&proj.path().join("CLAUDE.md"), "ROOT BODY\n");
        write(&proj.path().join(".orrch/CLAUDE.md"), "DIFFERENT HIDDEN\n");

        let err = hide_context(proj.path()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        // root untouched
        assert_eq!(
            std::fs::read_to_string(proj.path().join("CLAUDE.md")).unwrap(),
            "ROOT BODY\n"
        );
    }

    #[test]
    fn reveal_on_revealed_is_noop() {
        let proj = tempfile::tempdir().unwrap();
        write(&proj.path().join("CLAUDE.md"), "FULL\n");
        write(&proj.path().join("PLAN.md"), "plan\n");
        let rep = reveal_context(proj.path()).unwrap();
        assert!(rep.already_hidden);
    }

    #[test]
    fn detects_orrchestrator_self() {
        let proj = tempfile::tempdir().unwrap();
        // random tempdir → false
        assert!(!is_orrchestrator_self(proj.path()));

        // build a dir named orrchestrator with the marker
        let base = tempfile::tempdir().unwrap();
        let orr = base.path().join("orrchestrator");
        write(&orr.join("crates/orrch-core/Cargo.toml"), "[package]\n");
        assert!(is_orrchestrator_self(&orr));
    }
}
