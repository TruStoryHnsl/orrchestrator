//! Release tooling: release notes generation and pre-release checklist.

use std::path::Path;
use std::process::Command;
use std::collections::BTreeMap;

// ─── Release Notes ────────────────────────────────────────────────────

/// Generate grouped release notes from conventional commits since the last tag.
/// Returns a formatted markdown string. Falls back to full log if no tag exists.
pub fn generate_release_notes(project_dir: &Path) -> String {
    let last_tag = get_last_tag(project_dir);
    let range = match &last_tag {
        Some(tag) => format!("{tag}..HEAD"),
        None => String::new(),
    };

    let mut args = vec!["log", "--oneline", "--no-merges"];
    if !range.is_empty() {
        args.push(&range);
    }

    let output = Command::new("git")
        .args(&args)
        .current_dir(project_dir)
        .output();

    let log = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return "No git history available.".to_string(),
    };

    if log.trim().is_empty() {
        return match &last_tag {
            Some(t) => format!("No commits since `{t}`."),
            None => "No commits found.".to_string(),
        };
    }

    // Group commits by conventional prefix.
    let order: &[(&str, &str)] = &[
        ("feat", "Features"),
        ("fix", "Bug Fixes"),
        ("perf", "Performance"),
        ("refactor", "Refactoring"),
        ("docs", "Documentation"),
        ("test", "Tests"),
        ("chore", "Chores"),
        ("ci", "CI"),
        ("build", "Build"),
    ];

    let mut groups: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut other: Vec<String> = Vec::new();

    for line in log.lines() {
        // format: "<hash> <message>"
        let msg = line.splitn(2, ' ').nth(1).unwrap_or(line).trim();
        let prefix = msg.split(&[':', '('][..]).next().unwrap_or("").trim();
        let matched = order.iter().find(|(k, _)| prefix == *k || prefix.starts_with(k));
        if let Some((key, _)) = matched {
            groups.entry(key).or_default().push(msg.to_string());
        } else {
            other.push(msg.to_string());
        }
    }

    let since = match &last_tag {
        Some(t) => format!("since `{t}`"),
        None => "full history".to_string(),
    };

    let mut out = format!("## Release Notes ({since})\n\n");
    for (key, label) in order {
        if let Some(items) = groups.get(key) {
            out.push_str(&format!("### {label}\n"));
            for item in items {
                out.push_str(&format!("- {item}\n"));
            }
            out.push('\n');
        }
    }
    if !other.is_empty() {
        out.push_str("### Other\n");
        for item in &other {
            out.push_str(&format!("- {item}\n"));
        }
        out.push('\n');
    }
    out
}

fn get_last_tag(project_dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .current_dir(project_dir)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

// ─── Pre-release Checklist ────────────────────────────────────────────

/// A pre-release gate check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreReleaseCheck {
    ChangelogExists,
    NoEnvFiles,
    CargoVersionPresent,
    GitWorkingTreeClean,
}

impl PreReleaseCheck {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ChangelogExists => "CHANGELOG.md exists",
            Self::NoEnvFiles => "No .env files in project root",
            Self::CargoVersionPresent => "Cargo.toml has version field",
            Self::GitWorkingTreeClean => "Git working tree is clean",
        }
    }
}

/// Run all pre-release checks against `project_dir`.
/// Returns a vec of (check, passed) pairs.
pub fn run_checklist(project_dir: &Path) -> Vec<(PreReleaseCheck, bool)> {
    vec![
        (PreReleaseCheck::ChangelogExists, check_changelog(project_dir)),
        (PreReleaseCheck::NoEnvFiles, check_no_env_files(project_dir)),
        (PreReleaseCheck::CargoVersionPresent, check_cargo_version(project_dir)),
        (PreReleaseCheck::GitWorkingTreeClean, check_git_clean(project_dir)),
    ]
}

fn check_changelog(project_dir: &Path) -> bool {
    project_dir.join("CHANGELOG.md").exists()
}

fn check_no_env_files(project_dir: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if s == ".env" || s.starts_with(".env.") {
                return false;
            }
        }
    }
    true
}

fn check_cargo_version(project_dir: &Path) -> bool {
    let path = project_dir.join("Cargo.toml");
    if let Ok(contents) = std::fs::read_to_string(path) {
        return contents.contains("version =") || contents.contains("version=");
    }
    false
}

fn check_git_clean(project_dir: &Path) -> bool {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(project_dir)
        .output();
    match out {
        Ok(o) if o.status.success() => o.stdout.is_empty(),
        _ => false,
    }
}
