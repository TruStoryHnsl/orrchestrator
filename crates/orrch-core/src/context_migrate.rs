use std::path::{Path, PathBuf};

use crate::context_location::{artifact_path, Artifact};

#[derive(Debug, Default, Clone)]
pub struct MigrationReport {
    pub moved: Vec<(PathBuf, PathBuf)>,
    pub conflicts: Vec<(PathBuf, PathBuf)>,
    pub skipped: usize,
}

pub fn migrate_context() -> MigrationReport {
    let mut report = MigrationReport::default();

    migrate_one(
        Artifact::LoopRegistry,
        legacy_loop_registry_path(),
        &mut report,
    );

    report
}

fn migrate_one(artifact: Artifact, legacy: PathBuf, report: &mut MigrationReport) {
    let canonical = match artifact_path(artifact, None) {
        Ok(path) => path,
        Err(err) => {
            tracing::warn!("context migration: could not resolve {artifact:?}: {err}");
            report.skipped += 1;
            return;
        }
    };

    if canonical.exists() {
        if legacy.exists() && files_differ(&legacy, &canonical) {
            tracing::warn!(
                "context migration conflict for {artifact:?}: legacy={} canonical={}",
                legacy.display(),
                canonical.display()
            );
            report.conflicts.push((legacy, canonical));
        } else {
            report.skipped += 1;
        }
        return;
    }

    if !legacy.exists() {
        return;
    }

    if let Some(parent) = canonical.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                "context migration: could not create canonical parent {}: {err}",
                parent.display()
            );
            report.skipped += 1;
            return;
        }
    }

    if let Err(err) = std::fs::copy(&legacy, &canonical) {
        tracing::warn!(
            "context migration: could not copy {} to {}: {err}",
            legacy.display(),
            canonical.display()
        );
        report.skipped += 1;
        return;
    }

    if files_differ(&legacy, &canonical) {
        tracing::warn!(
            "context migration: copy verification failed for {} to {}",
            legacy.display(),
            canonical.display()
        );
        report.conflicts.push((legacy, canonical));
        return;
    }

    let backup = migrated_backup_path(&legacy);
    if backup.exists() {
        tracing::warn!(
            "context migration: backup already exists, leaving legacy in place: {}",
            backup.display()
        );
        report.skipped += 1;
        return;
    }

    if let Err(err) = std::fs::rename(&legacy, &backup) {
        tracing::warn!(
            "context migration: copied but could not rename legacy {} to {}: {err}",
            legacy.display(),
            backup.display()
        );
        report.skipped += 1;
        return;
    }

    append_manifest_line(artifact, &legacy, &canonical, &backup);
    report.moved.push((legacy, canonical));
}

fn files_differ(left: &Path, right: &Path) -> bool {
    match (std::fs::read(left), std::fs::read(right)) {
        (Ok(left), Ok(right)) => left != right,
        _ => true,
    }
}

fn migrated_backup_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    name.push_str(".migrated.bak");
    path.with_file_name(name)
}

fn append_manifest_line(artifact: Artifact, legacy: &Path, canonical: &Path, backup: &Path) {
    let log_path = crate::config::data_dir().join("context_migration.log");
    if let Some(parent) = log_path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                "context migration: could not create log parent {}: {err}",
                parent.display()
            );
            return;
        }
    }

    let line = format!(
        "{}\t{:?}\t{}\t{}\t{}\n",
        crate::usage::iso_now(),
        artifact,
        legacy.display(),
        canonical.display(),
        backup.display()
    );

    if let Err(err) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()))
    {
        tracing::warn!(
            "context migration: could not append migration log {}: {err}",
            log_path.display()
        );
    }
}

fn legacy_loop_registry_path() -> PathBuf {
    crate::loops::legacy_loops_path(&crate::config::Config::default().projects_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct HomeGuard {
        old_home: Option<String>,
    }

    impl HomeGuard {
        fn set(home: &Path) -> Self {
            let old_home = std::env::var("HOME").ok();
            unsafe { std::env::set_var("HOME", home) };
            Self { old_home }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.old_home {
                    Some(home) => std::env::set_var("HOME", home),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
    }

    fn fresh_home() -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "orrch-context-migrate-home-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    fn isolated_home() -> (std::sync::MutexGuard<'static, ()>, HomeGuard, PathBuf) {
        let lock = crate::shadow::TEST_ENV_LOCK.lock().unwrap();
        let home = fresh_home();
        let guard = HomeGuard::set(&home);
        (lock, guard, home)
    }

    fn write_legacy(bytes: &[u8]) -> PathBuf {
        let legacy = legacy_loop_registry_path();
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, bytes).unwrap();
        legacy
    }

    #[test]
    fn legacy_present_canonical_absent_copies_verifies_and_renames_legacy() {
        let (_lock, _home_guard, _home) = isolated_home();
        let legacy = write_legacy(br#"[{"id":"one"}]"#);
        let canonical = artifact_path(Artifact::LoopRegistry, None).unwrap();

        let report = migrate_context();

        assert_eq!(report.moved, vec![(legacy.clone(), canonical.clone())]);
        assert!(canonical.exists());
        assert_eq!(std::fs::read(&canonical).unwrap(), br#"[{"id":"one"}]"#);
        assert!(!legacy.exists());
        assert_eq!(
            std::fs::read(migrated_backup_path(&legacy)).unwrap(),
            br#"[{"id":"one"}]"#
        );
        assert!(crate::config::data_dir()
            .join("context_migration.log")
            .exists());
    }

    #[test]
    fn migrate_twice_is_idempotent() {
        let (_lock, _home_guard, _home) = isolated_home();
        let legacy = write_legacy(b"same bytes");
        let canonical = artifact_path(Artifact::LoopRegistry, None).unwrap();

        let first = migrate_context();
        let before = std::fs::read(&canonical).unwrap();
        let second = migrate_context();

        assert_eq!(first.moved, vec![(legacy.clone(), canonical.clone())]);
        assert!(second.moved.is_empty());
        assert!(second.skipped >= 1);
        assert_eq!(std::fs::read(&canonical).unwrap(), before);
        assert!(migrated_backup_path(&legacy).exists());
    }

    #[test]
    fn conflict_preserves_legacy_and_canonical() {
        let (_lock, _home_guard, _home) = isolated_home();
        let legacy = write_legacy(b"legacy");
        let canonical = artifact_path(Artifact::LoopRegistry, None).unwrap();
        std::fs::create_dir_all(canonical.parent().unwrap()).unwrap();
        std::fs::write(&canonical, b"canonical").unwrap();

        let report = migrate_context();

        assert!(report.moved.is_empty());
        assert_eq!(report.conflicts, vec![(legacy.clone(), canonical.clone())]);
        assert_eq!(std::fs::read(&legacy).unwrap(), b"legacy");
        assert_eq!(std::fs::read(&canonical).unwrap(), b"canonical");
        assert!(!migrated_backup_path(&legacy).exists());
    }
}
