use std::path::{Path, PathBuf};

use crate::plan_parser::{self, FeatureStatus, PlanPhase, parse_status_marker};

/// A roadmap item parsed from PLAN.md.
#[derive(Debug, Clone)]
pub struct RoadmapItem {
    pub number: usize,
    pub title: String,
    pub description: String,
    pub status: FeatureStatus,
    /// Line number in the plan file (1-indexed), used for write-back.
    pub source_line: Option<usize>,
}

impl RoadmapItem {
    /// Backward-compatible: whether the item is considered "done".
    pub fn done(&self) -> bool {
        self.status.is_done()
    }

    pub fn status_icon(&self) -> &'static str {
        self.status.display_icon()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Personal,
    Private,
    Public,
    Commercial,
}

impl Scope {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Private => "private",
            Self::Public => "public",
            Self::Commercial => "commercial",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Personal => "per",
            Self::Private => "prv",
            Self::Public => "pub",
            Self::Commercial => "com",
        }
    }

    fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "personal" => Self::Personal,
            "public" => Self::Public,
            "commercial" => Self::Commercial,
            _ => Self::Private,
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            Self::Personal => Self::Private,
            Self::Private => Self::Public,
            Self::Public => Self::Commercial,
            Self::Commercial => Self::Personal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorTag {
    Red,
    Yellow,
    Green,
    None,
}

impl ColorTag {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::None => "",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Red => "🔴",
            Self::Yellow => "🟡",
            Self::Green => "🟢",
            Self::None => "  ",
        }
    }

    fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "red" => Self::Red,
            "yellow" => Self::Yellow,
            "green" => Self::Green,
            _ => Self::None,
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            Self::None => Self::Green,
            Self::Green => Self::Yellow,
            Self::Yellow => Self::Red,
            Self::Red => Self::None,
        }
    }
}

/// Metadata about what notable files/dirs exist in the project.
#[derive(Debug, Clone, Default)]
pub struct ProjectMeta {
    pub has_claude_md: bool,
    pub has_gemini_md: bool,
    pub has_readme: bool,
    pub has_master_plan: bool,
    pub has_dockerfile: bool,
    pub has_cargo_toml: bool,
    pub has_pyproject: bool,
    pub has_package_json: bool,
    pub has_git: bool,
    pub git_dirty: usize,
    pub version_dirs: Vec<String>,  // "v1", "v2", etc.
    pub current_version: Option<String>,
    pub plan_file: Option<String>,
    pub file_count: usize,
    // Apple platform signals
    pub has_swift: bool,
    pub has_xcodeproj: bool,
    pub has_tauri_ios: bool,
    pub apple_target: bool, // true if any Apple signal detected
}

impl ProjectMeta {
    /// Build a compact metadata line like "CLAUDE.md | Cargo.toml | v2 | master plan"
    pub fn summary_line(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();

        if self.has_claude_md { parts.push("CLAUDE.md"); }
        if self.has_gemini_md { parts.push("GEMINI.md"); }
        if self.has_master_plan { parts.push("master plan"); }
        if self.has_cargo_toml { parts.push("Cargo.toml"); }
        if self.has_pyproject { parts.push("pyproject"); }
        if self.has_package_json { parts.push("package.json"); }
        if self.has_dockerfile { parts.push("Docker"); }
        if self.has_readme { parts.push("README"); }
        if self.apple_target { parts.push("🍎 Apple"); }

        if let Some(ver) = &self.current_version {
            parts.push(ver.as_str());
        }

        if parts.is_empty() {
            if self.file_count > 0 {
                return format!("{} files", self.file_count);
            }
            return "empty".to_string();
        }

        parts.join(" | ")
    }
}

/// OPT-013: Lifecycle stage of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleStage {
    /// Actively developed — new features, frequent commits.
    Active,
    /// Feature-complete; only bug fixes and dependency bumps.
    Maintenance,
    /// No longer receiving changes; preserved for reference or use.
    Archived,
    /// Superseded or abandoned; to be moved to deprecated/ on next cleanup.
    Deprecated,
}

impl LifecycleStage {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Maintenance => "maintenance",
            Self::Archived => "archived",
            Self::Deprecated => "deprecated",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Active => "ACT",
            Self::Maintenance => "MNT",
            Self::Archived => "ARC",
            Self::Deprecated => "DEP",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            Self::Active => Self::Maintenance,
            Self::Maintenance => Self::Archived,
            Self::Archived => Self::Deprecated,
            Self::Deprecated => Self::Active,
        }
    }

    /// Suggested actions appropriate for this lifecycle stage.
    pub fn suggested_actions(&self) -> &'static [&'static str] {
        match self {
            Self::Active => &["continue dev", "run queued", "spawn session"],
            Self::Maintenance => &["fix bugs", "bump deps", "review issues"],
            Self::Archived => &["view history", "export package"],
            Self::Deprecated => &["migrate users", "move to deprecated/"],
        }
    }

    fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "maintenance" => Self::Maintenance,
            "archived" => Self::Archived,
            "deprecated" => Self::Deprecated,
            _ => Self::Active,
        }
    }
}

/// Whether a project is actively being worked on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Temperature {
    Hot,     // actively being worked on
    Cold,    // parked
    Ignored, // hidden at the bottom
}

impl Temperature {
    fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "hot" => Self::Hot,
            "ignored" => Self::Ignored,
            _ => Self::Cold,
        }
    }
    pub fn label(&self) -> &'static str {
        match self { Self::Hot => "hot", Self::Cold => "cold", Self::Ignored => "ignored" }
    }
}

/// A project loaded from ~/projects/<name>/.
#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub scope: Scope,
    pub color_tag: ColorTag,
    pub description: String,
    pub roadmap: Vec<RoadmapItem>,
    pub plan_phases: Vec<PlanPhase>,
    pub queued_prompts: usize,
    pub has_plan: bool,
    pub has_master_plan: bool,
    pub meta: ProjectMeta,
    pub temperature: Temperature,
    pub is_hyperfolder: bool,
    pub sub_projects: Vec<Project>,
    /// Task AP: per-project swappable agent profile filename, e.g. `CLAUDE.md`,
    /// `GEMINI.md`, or a custom profile. Persisted as the `.agent_profile`
    /// dotfile at the project root. When `None`, the runner falls back to
    /// `CLAUDE.md`.
    pub agent_profile: Option<String>,
    /// TOK-002: maximum concurrent sessions for this project.
    /// Persisted as `.max_sessions` at the project root. Defaults to 3.
    pub max_sessions: usize,
    /// OPT-013: lifecycle stage. Persisted as `.orrlifecycle`.
    pub lifecycle_stage: LifecycleStage,
    /// OPT-005: optional logo file path. Persisted as `.orrlogo`.
    pub logo_path: Option<String>,
}

impl Project {
    pub fn load(path: &Path) -> Self {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let scope = load_scope(path);
        let color_tag = load_color_tag(path);
        let meta = scan_project_meta(path);
        let (roadmap, description, has_plan, plan_phases) = if let Some(ref plan_file) = meta.plan_file {
            let plan_path = path.join(plan_file);
            let (rm, desc, hp) = parse_plan_file(&plan_path);
            let phases = if hp {
                if let Ok(content) = std::fs::read_to_string(&plan_path) {
                    plan_parser::parse_plan(&content)
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            (rm, desc, hp, phases)
        } else {
            let desc = read_description_from_claude_md(path);
            (Vec::new(), desc, false, Vec::new())
        };
        let queued_prompts = count_queued_prompts(path);

        let temperature = load_temperature(path);
        let agent_profile = load_agent_profile(path);
        let max_sessions = load_max_sessions(path);
        let lifecycle_stage = load_lifecycle_stage(path);
        let logo_path = load_logo_path(path);
        let is_hyperfolder = name == "admin";

        let sub_projects = if is_hyperfolder {
            load_sub_projects(path)
        } else {
            Vec::new()
        };

        Self {
            name,
            path: path.to_path_buf(),
            scope,
            color_tag,
            description,
            roadmap,
            plan_phases,
            queued_prompts,
            has_plan,
            has_master_plan: meta.has_master_plan,
            meta,
            temperature,
            is_hyperfolder,
            sub_projects,
            agent_profile,
            max_sessions,
            lifecycle_stage,
            logo_path,
        }
    }

    /// Task AP: filename of the agent profile that should be loaded as the
    /// project context when spawning agents. Defaults to `CLAUDE.md` when the
    /// project has not set an explicit profile.
    pub fn agent_profile_filename(&self) -> &str {
        self.agent_profile
            .as_deref()
            .unwrap_or("CLAUDE.md")
    }

    /// Task AP: absolute path to the project's configured agent profile file.
    /// Does NOT check that the file exists — callers should fall back to
    /// their default when it's missing.
    pub fn agent_profile_path(&self) -> PathBuf {
        self.path.join(self.agent_profile_filename())
    }

    /// Persist the current `agent_profile` value to `.agent_profile` at the
    /// project root. Writing `None` removes the file so the project reverts
    /// to the default profile on next load.
    pub fn save_agent_profile(&self) {
        let path = self.path.join(".agent_profile");
        match self.agent_profile.as_deref() {
            None => {
                let _ = std::fs::remove_file(path);
            }
            Some(name) => {
                let _ = std::fs::write(path, name.trim());
            }
        }
    }

    pub fn done_count(&self) -> usize {
        self.roadmap.iter().filter(|r| r.done()).count()
    }

    pub fn open_count(&self) -> usize {
        self.roadmap.iter().filter(|r| !r.done()).count()
    }

    pub fn open_roadmap_items(&self) -> Vec<&RoadmapItem> {
        self.roadmap.iter().filter(|r| !r.done()).collect()
    }

    /// OPT-007: True when the project has a non-empty roadmap and every item is done.
    pub fn roadmap_complete(&self) -> bool {
        !self.roadmap.is_empty() && self.roadmap.iter().all(|r| r.done())
    }

    pub fn next_priority(&self) -> Option<&RoadmapItem> {
        self.roadmap.iter().find(|r| !r.done())
    }

    pub fn default_action(&self) -> &'static str {
        // OPT-013: lifecycle-gated actions override the roadmap-based defaults
        match self.lifecycle_stage {
            LifecycleStage::Maintenance => return "fix bugs",
            LifecycleStage::Archived => return "view history",
            LifecycleStage::Deprecated => return "migrate users",
            LifecycleStage::Active => {} // fall through to roadmap-based logic
        }

        if !self.has_plan && self.description.is_empty() {
            "create plan"
        } else if self.queued_prompts > 0 {
            "run queued"
        } else if !self.roadmap.is_empty() && self.open_count() == 0 {
            // All planned features complete — time to package or audit
            if self.meta.version_dirs.is_empty() {
                "construct package"
            } else {
                "feature audit"
            }
        } else {
            "continue dev"
        }
    }

    pub fn save_temperature(&self) {
        let path = self.path.join(".orrtemp");
        if self.temperature == Temperature::Cold {
            let _ = std::fs::remove_file(path); // cold is default, no file needed
        } else {
            let _ = std::fs::write(path, self.temperature.label());
        }
    }

    /// OPT-013: Persist the lifecycle stage to `.orrlifecycle`.
    /// `Active` (the default) removes the file so clean projects have no extra dotfile.
    pub fn save_lifecycle_stage(&self) {
        let path = self.path.join(".orrlifecycle");
        if self.lifecycle_stage == LifecycleStage::Active {
            let _ = std::fs::remove_file(path);
        } else {
            let _ = std::fs::write(path, self.lifecycle_stage.label());
        }
    }

    /// OPT-005: Persist logo_path to `.orrlogo`. None removes the file.
    pub fn save_logo_path(&self) {
        let path = self.path.join(".orrlogo");
        match self.logo_path.as_deref() {
            None => { let _ = std::fs::remove_file(path); }
            Some(p) => { let _ = std::fs::write(path, p.trim()); }
        }
    }

    pub fn save_scope(&self) {
        let _ = std::fs::write(self.path.join(".scope"), self.scope.label());
    }

    pub fn save_color_tag(&self) {
        let tag_path = self.path.join(".orrtag");
        if self.color_tag == ColorTag::None {
            let _ = std::fs::remove_file(tag_path);
        } else {
            let _ = std::fs::write(tag_path, self.color_tag.label());
        }
    }

    /// List directory contents for the file browser.
    pub fn list_contents(&self) -> Vec<DirEntry> {
        list_directory(&self.path)
    }
}

/// A directory entry for the file browser.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub is_editable: bool,
}

impl DirEntry {
    /// Human-readable size.
    pub fn size_display(&self) -> String {
        if self.is_dir { return "dir".into(); }
        if self.size < 1024 { return format!("{} B", self.size); }
        if self.size < 1024 * 1024 { return format!("{:.1} KB", self.size as f64 / 1024.0); }
        format!("{:.1} MB", self.size as f64 / (1024.0 * 1024.0))
    }

    /// File type description.
    pub fn type_label(&self) -> &'static str {
        if self.is_dir { return "Directory"; }
        let ext = self.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext.to_lowercase().as_str() {
            "rs" => "Rust source",
            "py" => "Python source",
            "js" | "ts" => "JavaScript/TypeScript",
            "md" => "Markdown",
            "toml" => "TOML config",
            "yaml" | "yml" => "YAML config",
            "json" => "JSON",
            "sh" | "bash" => "Shell script",
            "html" | "htm" => "HTML",
            "css" => "Stylesheet",
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => "Image",
            "mp4" | "mkv" | "webm" | "avi" => "Video",
            "mp3" | "flac" | "wav" | "ogg" => "Audio",
            "lock" => "Lock file",
            "txt" => "Text",
            "" => "File",
            other => return "File", // can't return dynamic str, just "File"
        }
    }

    /// Icon for the file type.
    pub fn icon(&self) -> &'static str {
        if self.is_dir { return "📁"; }
        let ext = self.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext.to_lowercase().as_str() {
            "rs" => "🦀",
            "py" => "🐍",
            "js" | "ts" => "📜",
            "md" => "📝",
            "toml" | "yaml" | "yml" | "json" => "⚙",
            "sh" | "bash" | "fish" => "🔧",
            "html" | "htm" | "css" => "🌐",
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => "🖼",
            "mp4" | "mkv" | "webm" => "🎬",
            "mp3" | "flac" | "wav" => "🎵",
            "lock" => "🔒",
            _ if self.is_editable => "📝",
            _ => "📄",
        }
    }
}

pub fn list_directory(path: &Path) -> Vec<DirEntry> {
    let mut entries = Vec::new();
    if let Ok(iter) = std::fs::read_dir(path) {
        for entry in iter.flatten() {
            let meta = entry.metadata().ok();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && name != ".scope" && name != ".orrtag" {
                continue; // hide dotfiles except our config
            }
            let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let is_editable = !is_dir && is_text_file(&name);

            entries.push(DirEntry {
                name,
                path: entry.path(),
                is_dir,
                size,
                is_editable,
            });
        }
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    entries
}

fn is_text_file(name: &str) -> bool {
    let text_exts = [
        "md", "txt", "rs", "py", "js", "ts", "json", "toml", "yaml", "yml",
        "sh", "bash", "fish", "css", "html", "htm", "xml", "csv", "cfg",
        "conf", "ini", "env", "dockerfile", "makefile", "gitignore",
    ];
    let lower = name.to_lowercase();
    // No extension = potentially editable (Makefile, Dockerfile, etc)
    if !lower.contains('.') {
        return matches!(lower.as_str(), "makefile" | "dockerfile" | "readme" | "license");
    }
    lower.rsplit('.').next().is_some_and(|ext| text_exts.contains(&ext))
}

/// Load deprecated project names from ~/projects/deprecated/.
pub fn load_deprecated(projects_dir: &Path) -> Vec<DirEntry> {
    let deprecated_dir = projects_dir.join("deprecated");
    if !deprecated_dir.is_dir() {
        return Vec::new();
    }
    list_directory(&deprecated_dir)
}

fn has_subdirectories(path: &Path) -> bool {
    std::fs::read_dir(path).map(|entries| {
        entries.flatten().any(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            e.path().is_dir() && !name.starts_with('.')
        })
    }).unwrap_or(false)
}

fn load_sub_projects(path: &Path) -> Vec<Project> {
    let mut subs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && !p.read_link().is_ok() {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                if !name.starts_with('.') {
                    subs.push(Project::load(&p));
                }
            }
        }
    }
    subs.sort_by(|a, b| a.name.cmp(&b.name));
    subs
}

/// Package a project as v1 (mark complete). Creates a v1/ directory and moves source into it.
pub fn package_as_v1(project_path: &Path) -> anyhow::Result<()> {
    let v1_dir = project_path.join("v1");
    if v1_dir.exists() {
        anyhow::bail!("v1/ already exists — project may already be versioned");
    }

    // Create v1 directory
    std::fs::create_dir(&v1_dir)?;

    // Move all files/dirs (except v1 itself and dotfiles) into v1/
    let skip = ["v1", ".git", ".scope", ".orrtag", ".retrospect"];
    if let Ok(entries) = std::fs::read_dir(project_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if skip.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            let dest = v1_dir.join(&name);
            std::fs::rename(entry.path(), dest)?;
        }
    }

    Ok(())
}

/// Load all projects from a directory, sorted by color tag then name.
/// Excludes symlinks (e.g. "notes" → obsidian vault), dotfiles, and "deprecated".
pub fn load_projects(projects_dir: &Path) -> Vec<Project> {
    let mut projects = Vec::new();
    if let Ok(entries) = std::fs::read_dir(projects_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Skip symlinks (e.g. notes → obsidian vault)
            if path.read_link().is_ok() {
                continue;
            }
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if !name.starts_with('.') && name != "deprecated" {
                    projects.push(Project::load(&path));
                }
            }
        }
    }
    projects.sort_by(|a, b| a.color_tag.cmp(&b.color_tag).then(a.name.cmp(&b.name)));
    projects
}

// ─── Internal helpers ─────────────────────────────────────────────────

fn load_scope(path: &Path) -> Scope {
    let scope_file = path.join(".scope");
    if let Ok(contents) = std::fs::read_to_string(scope_file) {
        Scope::from_str(&contents)
    } else {
        Scope::Private
    }
}

fn load_temperature(path: &Path) -> Temperature {
    let temp_file = path.join(".orrtemp");
    if let Ok(contents) = std::fs::read_to_string(temp_file) {
        Temperature::from_str(&contents)
    } else {
        Temperature::Cold // default
    }
}

/// Task AP: Load the configured agent profile filename for a project.
/// Reads `.agent_profile` at the project root. Returns `None` when the file
/// is absent or empty; callers fall back to `CLAUDE.md`. The trimmed value
/// is returned verbatim, so either `CLAUDE.md`, `GEMINI.md`, or a custom
/// profile filename is accepted.
fn load_agent_profile(path: &Path) -> Option<String> {
    let file = path.join(".agent_profile");
    let contents = std::fs::read_to_string(file).ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// TOK-002: read `.max_sessions` at the project root. Defaults to 3.
fn load_max_sessions(path: &Path) -> usize {
    let file = path.join(".max_sessions");
    std::fs::read_to_string(file)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(3)
}

/// OPT-013: read `.orrlifecycle` at the project root. Defaults to Active.
fn load_lifecycle_stage(path: &Path) -> LifecycleStage {
    let file = path.join(".orrlifecycle");
    if let Ok(contents) = std::fs::read_to_string(file) {
        LifecycleStage::from_str(&contents)
    } else {
        LifecycleStage::Active
    }
}

/// OPT-005: read `.orrlogo` at the project root. Returns None when absent.
fn load_logo_path(path: &Path) -> Option<String> {
    let file = path.join(".orrlogo");
    let contents = std::fs::read_to_string(file).ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

fn load_color_tag(path: &Path) -> ColorTag {
    let tag_file = path.join(".orrtag");
    if let Ok(contents) = std::fs::read_to_string(tag_file) {
        ColorTag::from_str(&contents)
    } else {
        ColorTag::None
    }
}

/// Scan a project directory for notable files and metadata.
fn scan_project_meta(path: &Path) -> ProjectMeta {
    let mut meta = ProjectMeta::default();

    let Ok(entries) = std::fs::read_dir(path) else {
        return meta;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        meta.file_count += 1;

        match lower.as_str() {
            "claude.md" => meta.has_claude_md = true,
            "gemini.md" => meta.has_gemini_md = true,
            "readme.md" | "readme" => meta.has_readme = true,
            "master_plan.md" | "orrapus_master_plan.md" | "orrapus_master_plan_2.md" => {
                meta.has_master_plan = true;
            }
            "dockerfile" | "docker-compose.yml" | "docker-compose.yaml" => {
                meta.has_dockerfile = true;
            }
            "cargo.toml" => meta.has_cargo_toml = true,
            "pyproject.toml" | "setup.py" => meta.has_pyproject = true,
            "package.json" => meta.has_package_json = true,
            ".git" => meta.has_git = true,
            _ => {}
        }

        // Detect plan files (case-insensitive, multiple names)
        if meta.plan_file.is_none() {
            match lower.as_str() {
                "plan.md" | "development_plan.md" => {
                    meta.plan_file = Some(name.clone());
                }
                _ => {}
            }
        }

        // Detect version directories and Apple signals
        if entry.path().is_dir() {
            if let Some(rest) = lower.strip_prefix('v') {
                if rest.chars().all(|c| c.is_ascii_digit()) && !rest.is_empty() {
                    meta.version_dirs.push(name.clone());
                }
            }
            // Apple: .xcodeproj, src-tauri with iOS
            if lower.ends_with(".xcodeproj") || lower.ends_with(".xcworkspace") {
                meta.has_xcodeproj = true;
            }
            if lower == "src-tauri" {
                // Check for iOS target in tauri config
                let tauri_conf = entry.path().join("tauri.conf.json");
                if let Ok(contents) = std::fs::read_to_string(tauri_conf) {
                    let cl = contents.to_lowercase();
                    if cl.contains("ios") || cl.contains("macos") {
                        meta.has_tauri_ios = true;
                    }
                }
            }
        }

        // Apple: .swift files at top level
        if lower.ends_with(".swift") {
            meta.has_swift = true;
        }
    }

    // Current version = highest vN
    if !meta.version_dirs.is_empty() {
        meta.version_dirs.sort();
        meta.current_version = meta.version_dirs.last().cloned();
    }

    // Deep scan for Swift files if not found at top level (check up to 2 levels)
    if !meta.has_swift {
        meta.has_swift = has_file_recursive(path, "swift", 2);
    }

    // Aggregate Apple target flag
    meta.apple_target = meta.has_swift || meta.has_xcodeproj || meta.has_tauri_ios;

    // Git dirty count (fast — just counts porcelain lines)
    if meta.has_git {
        meta.git_dirty = crate::git::check_status(path).dirty_count;
    }

    meta
}

fn has_file_recursive(path: &Path, ext: &str, max_depth: usize) -> bool {
    if max_depth == 0 { return false; }
    let Ok(entries) = std::fs::read_dir(path) else { return false; };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') { continue; }
        if entry.path().is_file() && name.to_lowercase().ends_with(&format!(".{ext}")) {
            return true;
        }
        if entry.path().is_dir() && has_file_recursive(&entry.path(), ext, max_depth - 1) {
            return true;
        }
    }
    false
}

/// Parse a plan file for roadmap items and description.
fn parse_plan_file(plan_path: &Path) -> (Vec<RoadmapItem>, String, bool) {
    let Ok(contents) = std::fs::read_to_string(plan_path) else {
        return (Vec::new(), String::new(), false);
    };

    let mut items = Vec::new();
    let mut description = String::new();
    let mut in_roadmap = false;
    let mut found_heading = false;
    let mut collecting_desc = false;

    for line in contents.lines() {
        let trimmed = line.trim();

        if !found_heading && trimmed.starts_with("# ") {
            found_heading = true;
            collecting_desc = true;
            continue;
        }
        if collecting_desc {
            if trimmed.is_empty() && description.is_empty() {
                continue;
            }
            if trimmed.starts_with('#') {
                collecting_desc = false;
            } else if !trimmed.is_empty() && description.is_empty() {
                description = trimmed.to_string();
                collecting_desc = false;
            }
        }

        let lower = trimmed.to_lowercase();
        if lower.starts_with("## feature roadmap")
            || lower.starts_with("## roadmap")
            || lower.starts_with("## features")
        {
            in_roadmap = true;
            continue;
        }
        if in_roadmap && trimmed.starts_with("## ") {
            break;
        }
        if !in_roadmap {
            continue;
        }

        if let Some(item) = parse_roadmap_line(trimmed) {
            items.push(item);
        }
    }

    (items, description, true)
}

/// Try to extract a description from CLAUDE.md when no PLAN.md exists.
fn read_description_from_claude_md(path: &Path) -> String {
    let claude_path = path.join("CLAUDE.md");
    let Ok(contents) = std::fs::read_to_string(claude_path) else {
        return String::new();
    };

    let mut found_heading = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if !found_heading && trimmed.starts_with("# ") {
            found_heading = true;
            continue;
        }
        if found_heading {
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('#') {
                return String::new();
            }
            return trimmed.to_string();
        }
    }
    String::new()
}

fn parse_roadmap_line(line: &str) -> Option<RoadmapItem> {
    // Strip numeric prefix "N." if present (e.g. "12. [x] **Title**")
    let rest = line.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
    let rest = rest.trim_start();
    // Strip markdown bullet prefix if present (e.g. "- [x] **Title**", "* [ ] foo").
    // Without this, any project using bulleted checklists (concord, orrapus,
    // orradash, most of orrbeam) has zero items counted in its progress display.
    let rest = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
        .map(str::trim_start)
        .unwrap_or(rest);

    // Check for strikethrough → Deprecated
    let (status, rest) = if rest.starts_with("~~") && rest.ends_with("~~") {
        let inner = &rest[2..rest.len()-2];
        if let Some((_inner_status, consumed)) = parse_status_marker(inner) {
            (FeatureStatus::Deprecated, &inner[consumed..])
        } else {
            (FeatureStatus::Deprecated, inner)
        }
    } else if let Some((s, consumed)) = parse_status_marker(rest) {
        (s, &rest[consumed..])
    } else {
        return None;
    };

    let rest = rest.trim_start();
    let number: usize = line
        .trim_start()
        .split('.')
        .next()
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0);

    let (title, description) = if rest.starts_with("**") {
        let after_open = &rest[2..];
        if let Some(close_pos) = after_open.find("**") {
            let title = after_open[..close_pos].to_string();
            let desc = after_open[close_pos + 2..]
                .trim_start_matches(|c: char| c == ' ' || c == '—' || c == '-' || c == '–')
                .trim()
                .to_string();
            (title, desc)
        } else {
            (rest.to_string(), String::new())
        }
    } else {
        let parts: Vec<&str> = rest.splitn(2, '—').collect();
        if parts.len() == 2 {
            (parts[0].trim().to_string(), parts[1].trim().to_string())
        } else {
            (rest.trim().to_string(), String::new())
        }
    };

    if title.is_empty() {
        return None;
    }

    Some(RoadmapItem {
        number,
        title,
        description,
        status,
        source_line: None,
    })
}

/// Update a feature's status marker in a plan file on disk.
pub fn update_feature_status_in_plan(plan_path: &Path, item_title: &str, new_status: FeatureStatus) -> std::io::Result<()> {
    let contents = std::fs::read_to_string(plan_path)?;
    let mut lines: Vec<String> = contents.lines().map(|l| l.to_string()).collect();
    let new_marker = new_status.write_marker();

    for line in lines.iter_mut() {
        if !line.contains(item_title) {
            continue;
        }
        let trimmed = line.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
        if parse_status_marker(trimmed).is_some() {
            if let Some(bracket_start) = line.find('[') {
                if let Some(bracket_end) = line[bracket_start..].find(']') {
                    let old_marker = &line[bracket_start..bracket_start + bracket_end + 1];
                    if matches!(old_marker, "[ ]" | "[x]" | "[X]" | "[~]" | "[=]" | "[t]" | "[T]" | "[v]" | "[V]")
                        || old_marker == "[✓]"
                    {
                        *line = format!("{}{}{}", &line[..bracket_start], new_marker, &line[bracket_start + old_marker.len()..]);
                        break;
                    }
                }
            }
        }
    }

    let mut output = lines.join("\n");
    if contents.ends_with('\n') {
        output.push('\n');
    }
    std::fs::write(plan_path, output)
}

fn count_queued_prompts(path: &Path) -> usize {
    let inbox_path = path.join("instructions_inbox.md");
    let Ok(contents) = std::fs::read_to_string(inbox_path) else {
        return 0;
    };
    // Two formats coexist:
    //   - New COO format: `### INS-NNN:` headers (one per instruction).
    //     Strikethrough (`### ~~INS-NNN:`) marks completed and is excluded.
    //   - Legacy format: `Executed: pending` line per entry.
    // Count whichever appears.
    let ins_count = contents
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("### INS-") || l.starts_with("### OPT-"))
        .count();
    if ins_count > 0 {
        return ins_count;
    }
    contents
        .lines()
        .filter(|l| l.trim().starts_with("Executed: pending"))
        .count()
}

#[cfg(test)]
mod count_queued_tests {
    use super::*;
    use std::fs;

    fn tmp_project(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "orrch_count_queued_{}_{}",
            std::process::id(),
            name
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("instructions_inbox.md"), body).unwrap();
        dir
    }

    #[test]
    fn counts_ins_headers() {
        let dir = tmp_project(
            "ins",
            "## Instruction\n### INS-001: foo\nbody\n### INS-002: bar\nbody\n",
        );
        assert_eq!(count_queued_prompts(&dir), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn counts_opt_headers() {
        let dir = tmp_project("opt", "### OPT-001: foo\n### INS-001: bar\n");
        assert_eq!(count_queued_prompts(&dir), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn excludes_strikethrough_completed() {
        let dir = tmp_project(
            "strike",
            "### INS-001: open\n### ~~INS-002: done~~\n### INS-003: open\n",
        );
        assert_eq!(count_queued_prompts(&dir), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_legacy_executed_format() {
        let dir = tmp_project(
            "legacy",
            "## Old entry\nExecuted: pending\n## Old entry\nExecuted: pending\n",
        );
        assert_eq!(count_queued_prompts(&dir), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_inbox_returns_zero() {
        let dir = std::env::temp_dir().join(format!(
            "orrch_count_queued_missing_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(count_queued_prompts(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_roadmap_done() {
        let item = parse_roadmap_line("1. [x] **Core process manager** — spawn/kill/monitor").unwrap();
        assert!(item.done());
        assert_eq!(item.status, FeatureStatus::Done);
        assert_eq!(item.number, 1);
        assert_eq!(item.title, "Core process manager");
    }

    #[test]
    fn test_parse_roadmap_open() {
        let item = parse_roadmap_line("5. [ ] **Editor view** — embedded vim").unwrap();
        assert!(!item.done());
        assert_eq!(item.status, FeatureStatus::Planned);
        assert_eq!(item.number, 5);
    }

    #[test]
    fn test_parse_roadmap_implementing() {
        let item = parse_roadmap_line("3. [~] **Feature X** — in progress").unwrap();
        assert_eq!(item.status, FeatureStatus::Implementing);
    }

    #[test]
    fn test_parse_roadmap_testing() {
        let item = parse_roadmap_line("4. [t] **Feature Y** — under test").unwrap();
        assert_eq!(item.status, FeatureStatus::Testing);
    }

    #[test]
    fn test_parse_roadmap_verified() {
        let item = parse_roadmap_line("6. [v] **Feature Z** — verified").unwrap();
        assert_eq!(item.status, FeatureStatus::Verified);
        assert!(item.done()); // verified counts as done
    }

    #[test]
    fn test_parse_roadmap_bulleted_done() {
        // Concord and other projects use "- [x] **Title**" format.
        let item = parse_roadmap_line("- [x] **iOS build pipeline** — Tauri v2 compiles").unwrap();
        assert!(item.done());
        assert_eq!(item.status, FeatureStatus::Done);
        assert_eq!(item.title, "iOS build pipeline");
        assert_eq!(item.number, 0); // unnumbered
    }

    #[test]
    fn test_parse_roadmap_bulleted_open() {
        let item = parse_roadmap_line("- [ ] Sideload test builds to physical iOS devices").unwrap();
        assert!(!item.done());
        assert_eq!(item.status, FeatureStatus::Planned);
        assert_eq!(item.title, "Sideload test builds to physical iOS devices");
    }

    #[test]
    fn test_parse_roadmap_bulleted_star() {
        let item = parse_roadmap_line("* [x] **Something** — done").unwrap();
        assert!(item.done());
        assert_eq!(item.title, "Something");
    }

    #[test]
    fn test_scope_personal() {
        assert_eq!(Scope::from_str("personal"), Scope::Personal);
    }

    #[test]
    fn test_color_tag_cycle() {
        assert_eq!(ColorTag::None.cycle(), ColorTag::Green);
        assert_eq!(ColorTag::Green.cycle(), ColorTag::Yellow);
        assert_eq!(ColorTag::Yellow.cycle(), ColorTag::Red);
        assert_eq!(ColorTag::Red.cycle(), ColorTag::None);
    }

    #[test]
    fn test_color_tag_sort_order() {
        assert!(ColorTag::Red < ColorTag::Yellow);
        assert!(ColorTag::Yellow < ColorTag::Green);
        assert!(ColorTag::Green < ColorTag::None);
    }

    fn test_project(name: &str) -> Project {
        Project {
            name: name.into(),
            path: PathBuf::from(format!("/tmp/{name}")),
            scope: Scope::Private,
            color_tag: ColorTag::None,
            description: String::new(),
            roadmap: Vec::new(),
            plan_phases: Vec::new(),
            queued_prompts: 0,
            has_plan: false,
            has_master_plan: false,
            meta: ProjectMeta::default(),
            temperature: Temperature::Cold,
            is_hyperfolder: false,
            sub_projects: Vec::new(),
            agent_profile: None,
            max_sessions: 3,
            lifecycle_stage: LifecycleStage::Active,
            logo_path: None,
        }
    }

    #[test]
    fn test_default_action() {
        let mut proj = test_project("test");
        assert_eq!(proj.default_action(), "create plan");
        proj.has_plan = true;
        assert_eq!(proj.default_action(), "continue dev");
        proj.queued_prompts = 2;
        assert_eq!(proj.default_action(), "run queued");
    }

    #[test]
    fn test_default_action_with_description() {
        let mut proj = test_project("test");
        proj.description = "Has a CLAUDE.md description".into();
        assert_eq!(proj.default_action(), "continue dev");
    }

    #[test]
    fn test_is_text_file() {
        assert!(is_text_file("README.md"));
        assert!(is_text_file("app.py"));
        assert!(is_text_file("Cargo.toml"));
        assert!(is_text_file("Makefile"));
        assert!(!is_text_file("image.png"));
        assert!(!is_text_file("video.mp4"));
    }

    #[test]
    fn test_meta_summary() {
        let mut meta = ProjectMeta::default();
        meta.has_claude_md = true;
        meta.has_cargo_toml = true;
        meta.current_version = Some("v2".into());
        assert_eq!(meta.summary_line(), "CLAUDE.md | Cargo.toml | v2");
    }
}
