//! Release tooling: release notes generation, version tagging, and pre-release checklist.

use std::path::Path;
use std::process::Command;
use std::collections::BTreeMap;

// ─── Version Tagging + Changelog ──────────────────────────────

/// SemVer bump kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpKind {
    Major,
    Minor,
    Patch,
}

impl BumpKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Patch => "patch",
        }
    }
}

/// Parse a semver string "vX.Y.Z" or "X.Y.Z" into (major, minor, patch).
fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim_start_matches('v');
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() < 3 { return None; }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    // Strip pre-release suffix from patch
    let patch_str = parts[2].split(&['-', '+'][..]).next().unwrap_or(parts[2]);
    let patch = patch_str.parse().ok()?;
    Some((major, minor, patch))
}

/// Read the current version from the last git tag. Returns "0.0.0" if none.
pub fn current_version(project_dir: &Path) -> (u64, u64, u64) {
    get_last_tag(project_dir)
        .and_then(|t| parse_semver(&t))
        .unwrap_or((0, 0, 0))
}

/// Compute the next version string from the last tag + bump kind.
pub fn next_version_string(project_dir: &Path, bump: BumpKind) -> String {
    let (maj, min, pat) = current_version(project_dir);
    match bump {
        BumpKind::Major => format!("v{}.0.0", maj + 1),
        BumpKind::Minor => format!("v{}.{}.0", maj, min + 1),
        BumpKind::Patch => format!("v{}.{}.{}", maj, min, pat + 1),
    }
}

/// Generate a CHANGELOG.md section for `version` from conventional commits since last tag.
pub fn generate_changelog_entry(project_dir: &Path, version: &str) -> String {
    let notes = generate_release_notes(project_dir);
    // Replace the "## Release Notes (since ...)" header with a versioned one
    let today = chrono_today();
    let header = format!("## [{version}] - {today}");
    // Strip the first "## Release Notes" line and replace
    let mut lines = notes.lines();
    let first = lines.next().unwrap_or("");
    let rest: String = lines.collect::<Vec<_>>().join("\n");
    let _ = first; // discard old header
    format!("{header}\n{rest}\n")
}

/// Bump version: compute next version, create annotated git tag, return new version string.
/// Returns `Err` if git tagging fails.
pub fn bump_version(project_dir: &Path, bump: BumpKind) -> anyhow::Result<String> {
    let version = next_version_string(project_dir, bump);
    let changelog = generate_changelog_entry(project_dir, &version);
    let message = format!("Release {version}\n\n{changelog}");

    let out = Command::new("git")
        .args(["tag", "-a", &version, "-m", &message])
        .current_dir(project_dir)
        .output()?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        anyhow::bail!("git tag failed: {stderr}");
    }

    Ok(version)
}

fn chrono_today() -> String {
    // Use std time to avoid adding chrono dep just for date formatting
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // seconds since epoch → date string (simple calculation, UTC)
    let days = secs / 86400;
    // Jan 1 1970 = day 0. Calculate year/month/day.
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Gregorian calendar approximation
    let mut year = 1970u64;
    loop {
        let leap = is_leap(year);
        let days_in_year = if leap { 366 } else { 365 };
        if days < days_in_year { break; }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let months = [31u64, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u64;
    for &m in &months {
        if days < m { break; }
        days -= m;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ─── Build Artifacts ──────────────────────────────────────────

/// A build target detected from the project structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTarget {
    pub kind: BuildTargetKind,
    pub label: String,
    pub command: Vec<String>,
}

/// The kind of build target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildTargetKind {
    CargoRelease,
    Python,
    Node,
    Docker,
}

/// Build result for a single target.
#[derive(Debug, Clone)]
pub struct BuildResult {
    pub target: BuildTarget,
    pub status: BuildStatus,
    pub output: String,
}

/// Status of a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildStatus {
    Pending,
    Running,
    Success,
    Failed,
}

impl BuildStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Pending => "·",
            Self::Running => "⏳",
            Self::Success => "✓",
            Self::Failed => "✗",
        }
    }
}

/// Detect build targets by inspecting project files.
pub fn detect_build_targets(project_dir: &Path) -> Vec<BuildTarget> {
    let mut targets = Vec::new();

    if project_dir.join("Cargo.toml").exists() {
        targets.push(BuildTarget {
            kind: BuildTargetKind::CargoRelease,
            label: "cargo build --release".to_string(),
            command: vec!["cargo".into(), "build".into(), "--release".into()],
        });
    }
    if project_dir.join("pyproject.toml").exists() || project_dir.join("setup.py").exists() || project_dir.join("setup.cfg").exists() {
        targets.push(BuildTarget {
            kind: BuildTargetKind::Python,
            label: "python build (wheel)".to_string(),
            command: vec!["python3".into(), "-m".into(), "build".into()],
        });
    }
    if project_dir.join("package.json").exists() {
        targets.push(BuildTarget {
            kind: BuildTargetKind::Node,
            label: "npm run build".to_string(),
            command: vec!["npm".into(), "run".into(), "build".into()],
        });
    }
    if project_dir.join("Dockerfile").exists() || project_dir.join("docker-compose.yml").exists() || project_dir.join("compose.yml").exists() {
        targets.push(BuildTarget {
            kind: BuildTargetKind::Docker,
            label: "docker build".to_string(),
            command: vec!["docker".into(), "build".into(), ".".into()],
        });
    }

    targets
}

/// Run a single build target synchronously. Returns a BuildResult.
pub fn build_artifact(project_dir: &Path, target: &BuildTarget) -> BuildResult {
    let (program, args) = match target.command.split_first() {
        Some((p, a)) => (p.clone(), a.to_vec()),
        None => return BuildResult {
            target: target.clone(),
            status: BuildStatus::Failed,
            output: "Empty command.".to_string(),
        },
    };

    let out = Command::new(&program)
        .args(&args)
        .current_dir(project_dir)
        .output();

    match out {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            let combined = format!("{stdout}{stderr}");
            let status = if o.status.success() { BuildStatus::Success } else { BuildStatus::Failed };
            BuildResult { target: target.clone(), status, output: combined }
        }
        Err(e) => BuildResult {
            target: target.clone(),
            status: BuildStatus::Failed,
            output: format!("Failed to spawn: {e}"),
        },
    }
}

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
