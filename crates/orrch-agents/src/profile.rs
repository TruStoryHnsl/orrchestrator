use std::path::{Path, PathBuf};

/// ENG-007: task importance used to pick which engine an agent prefers.
///
/// Defined here (rather than re-exported from orrch-core) to avoid a
/// dependency cycle: orrch-core depends on orrch-agents, not the reverse.
/// The resolver in orrch-core can adapt to / re-export this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Importance {
    /// Highest-stakes work — prefer the optimal (best/expensive) engine.
    Critical,
    /// High-stakes work — prefer the optimal engine.
    Important,
    /// Routine work — prefer the standard (cost-efficient) engine.
    Routine,
}

/// An agent profile loaded from a `.md` file.
#[derive(Debug, Clone)]
pub struct AgentProfile {
    pub name: String,
    pub department: String,
    pub role: String,
    pub description: String,
    /// ENG-007: best/expensive engine id, used for high-importance tasks.
    /// `None` = agent expressed no optimal preference (legacy / inherits resolver default).
    pub optimal_engine: Option<String>,
    /// ENG-007: default cost-efficient engine id, used for routine tasks.
    /// `None` = legacy profile; resolver falls through to project/global/builtin.
    pub standard_engine: Option<String>,
    /// The full markdown body (everything after the YAML frontmatter).
    pub prompt: String,
    /// Path to the source `.md` file.
    pub path: PathBuf,
}

impl AgentProfile {
    /// Load an agent profile from a `.md` file with YAML frontmatter.
    pub fn load(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let (frontmatter, body) = parse_frontmatter(&content)?;

        // ENG-007 new fields (Option => absent key yields None, legacy-safe).
        let standard_engine = extract_field(&frontmatter, "standard_engine")
            // ENG-003 back-compat: a legacy `engine:` field is treated as
            // standard_engine when the new field is absent.
            .or_else(|| extract_field(&frontmatter, "engine"));
        let optimal_engine = extract_field(&frontmatter, "optimal_engine")
            // If only `engine:`/standard is set, optimal defaults to the same
            // id (high-importance tasks reuse the single declared engine rather
            // than silently falling through to a different default).
            .or_else(|| standard_engine.clone());

        Some(Self {
            name: extract_field(&frontmatter, "name")?,
            department: extract_field(&frontmatter, "department").unwrap_or_default(),
            role: extract_field(&frontmatter, "role").unwrap_or_default(),
            description: extract_field(&frontmatter, "description").unwrap_or_default(),
            optimal_engine,
            standard_engine,
            prompt: body.trim().to_string(),
            path: path.to_path_buf(),
        })
    }

    /// ENG-007 getter pair (kept trivial — importance selection lives in
    /// [`AgentProfile::engine_for`] / the orrch-core resolver).
    pub fn standard_engine(&self) -> Option<&str> {
        self.standard_engine.as_deref()
    }

    /// ENG-007 getter for the optimal (best/expensive) engine id.
    pub fn optimal_engine(&self) -> Option<&str> {
        self.optimal_engine.as_deref()
    }

    /// ENG-007: pick the engine id this agent prefers for the given importance.
    /// Optimal for high-importance, standard otherwise. Either may be `None`
    /// (legacy profile) => caller's resolver falls through to lower layers.
    pub fn engine_for(&self, importance: Importance) -> Option<String> {
        agent_layer_engine(
            self.standard_engine.as_deref(),
            self.optimal_engine.as_deref(),
            importance,
        )
    }

    /// Format the agent profile as a prompt preamble to prepend to a task goal.
    pub fn as_preamble(&self, task: &str) -> String {
        format!(
            "{}\n\n---\n\n## Your Task\n\n{}",
            self.prompt, task,
        )
    }

    /// Format the agent profile as a prompt preamble, with a Mentor-generated
    /// references block (library skills + tools) inserted between the profile
    /// body and the task.
    ///
    /// `references_block` is expected to be the string produced by
    /// [`crate::runner::mentor_review_profile`]. If it's empty, the output is
    /// identical to [`AgentProfile::as_preamble`].
    pub fn as_preamble_with_library(&self, task: &str, references_block: &str) -> String {
        if references_block.trim().is_empty() {
            return self.as_preamble(task);
        }
        format!(
            "{}\n\n---\n\n{}\n\n---\n\n## Your Task\n\n{}",
            self.prompt, references_block, task,
        )
    }

    /// Short display label: "Name (role)"
    pub fn label(&self) -> String {
        if self.role.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.role)
        }
    }
}

/// ENG-007 layer-selection: given an agent's declared standard/optimal engine
/// ids and a task importance, return the preferred engine id (or `None` if the
/// agent declared neither — the resolver then falls through to lower layers).
///
/// High importance (Critical/Important) prefers `optimal`, falling back to
/// `standard`. Routine prefers `standard`, falling back to `optimal`.
pub fn agent_layer_engine(
    standard: Option<&str>,
    optimal: Option<&str>,
    importance: Importance,
) -> Option<String> {
    match importance {
        Importance::Critical | Importance::Important => {
            optimal.or(standard).map(str::to_string)
        }
        Importance::Routine => standard.or(optimal).map(str::to_string),
    }
}

/// Load all agent profiles from a directory.
pub fn load_agents(dir: &Path) -> Vec<AgentProfile> {
    let mut agents = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Some(profile) = AgentProfile::load(&path) {
                    agents.push(profile);
                }
            }
        }
    }
    agents.sort_by(|a, b| a.name.cmp(&b.name));
    agents
}

/// Returns the default agent profiles directory.
/// Checks project-local `agents/` first, then `~/.config/orrchestrator/agents/`.
pub fn agents_dir() -> PathBuf {
    // Project-local agents directory (for bundled profiles)
    let local = PathBuf::from("agents");
    if local.is_dir() {
        return local;
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".into());
    PathBuf::from(home)
        .join(".config")
        .join("orrchestrator")
        .join("agents")
}

/// Parse YAML frontmatter delimited by `---` lines.
/// Returns (frontmatter_text, body_text).
fn parse_frontmatter(content: &str) -> Option<(String, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..].trim_start_matches(['\r', '\n']);
    let end = after_first.find("\n---")?;
    let frontmatter = after_first[..end].to_string();
    let body = after_first[end + 4..].to_string();
    Some((frontmatter, body))
}

/// Extract a simple `key: value` field from YAML frontmatter.
/// Handles both single-line values and `>` folded scalars.
fn extract_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let stripped = line.trim();
        if let Some(rest) = stripped.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix(':') {
                let value = value.trim();
                if value == ">" {
                    // Folded scalar — collect indented continuation lines
                    let key_line_idx = frontmatter.find(stripped)?;
                    let after = &frontmatter[key_line_idx + stripped.len()..];
                    let mut parts = Vec::new();
                    for cont_line in after.lines().skip(1) {
                        if cont_line.starts_with(' ') || cont_line.starts_with('\t') {
                            parts.push(cont_line.trim());
                        } else {
                            break;
                        }
                    }
                    return if parts.is_empty() {
                        None
                    } else {
                        Some(parts.join(" "))
                    };
                }
                if value.is_empty() {
                    return None;
                }
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = "---\nname: Hypervisor\nrole: Orchestrator\n---\n\n# Body\nContent here.";
        let (fm, body) = parse_frontmatter(content).unwrap();
        assert!(fm.contains("name: Hypervisor"));
        assert!(body.contains("# Body"));
    }

    #[test]
    fn test_extract_field_simple() {
        let fm = "name: Hypervisor\ndepartment: admin\nrole: Workforce Orchestrator";
        assert_eq!(extract_field(fm, "name"), Some("Hypervisor".into()));
        assert_eq!(extract_field(fm, "department"), Some("admin".into()));
        assert_eq!(extract_field(fm, "role"), Some("Workforce Orchestrator".into()));
    }

    #[test]
    fn test_extract_field_folded() {
        let fm = "description: >\n  This is a long\n  description text\nrole: Test";
        assert_eq!(
            extract_field(fm, "description"),
            Some("This is a long description text".into())
        );
    }

    #[test]
    fn test_agent_preamble() {
        let agent = AgentProfile {
            name: "Hypervisor".into(),
            department: "admin".into(),
            role: "Orchestrator".into(),
            description: "Orchestrates workforces".into(),
            optimal_engine: None,
            standard_engine: None,
            prompt: "You are the Hypervisor.".into(),
            path: PathBuf::from("agents/hypervisor.md"),
        };
        let preamble = agent.as_preamble("continue development");
        assert!(preamble.starts_with("You are the Hypervisor."));
        assert!(preamble.contains("continue development"));
    }

    #[test]
    fn test_agent_label() {
        let agent = AgentProfile {
            name: "Hypervisor".into(),
            department: "admin".into(),
            role: "Orchestrator".into(),
            description: String::new(),
            optimal_engine: None,
            standard_engine: None,
            prompt: String::new(),
            path: PathBuf::new(),
        };
        assert_eq!(agent.label(), "Hypervisor (Orchestrator)");
    }

    use std::io::Write;

    fn write_tmp(name: &str, contents: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("orrch-agents-test-{}-{}.md", std::process::id(), name));
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        p
    }

    #[test]
    fn test_parse_standard_and_optimal_engine() {
        let p = write_tmp(
            "both",
            "---\nname: Developer\nstandard_engine: Claude Sonnet 4.6\noptimal_engine: Claude Opus 4.6\n---\n\nBody.",
        );
        let a = AgentProfile::load(&p).unwrap();
        assert_eq!(a.standard_engine(), Some("Claude Sonnet 4.6"));
        assert_eq!(a.optimal_engine(), Some("Claude Opus 4.6"));
        assert_eq!(a.engine_for(Importance::Routine).as_deref(), Some("Claude Sonnet 4.6"));
        assert_eq!(a.engine_for(Importance::Critical).as_deref(), Some("Claude Opus 4.6"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_legacy_engine_field_maps_to_both() {
        // Legacy ENG-003 `engine:` field => standard==that id and optimal==that id.
        let p = write_tmp("legacy", "---\nname: Legacy\nengine: GPT-4o\n---\n\nBody.");
        let a = AgentProfile::load(&p).unwrap();
        assert_eq!(a.standard_engine(), Some("GPT-4o"));
        assert_eq!(a.optimal_engine(), Some("GPT-4o"));
        assert_eq!(a.engine_for(Importance::Routine).as_deref(), Some("GPT-4o"));
        assert_eq!(a.engine_for(Importance::Critical).as_deref(), Some("GPT-4o"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_no_engine_fields_yields_none_and_loads() {
        let p = write_tmp("none", "---\nname: COO\ndepartment: admin\n---\n\nBody.");
        let a = AgentProfile::load(&p).unwrap();
        assert_eq!(a.standard_engine(), None);
        assert_eq!(a.optimal_engine(), None);
        assert_eq!(a.engine_for(Importance::Routine), None);
        assert_eq!(a.engine_for(Importance::Critical), None);
        let _ = std::fs::remove_file(&p);
    }

    /// ENG-008 acceptance: real agent profiles in the workspace `agents/` dir
    /// carry the role-appropriate engine defaults. Skips gracefully if the dir
    /// isn't reachable from the test cwd (e.g. packaged build).
    #[test]
    fn test_eng008_role_defaults_real_profiles() {
        // Tests run with cwd = crate dir; workspace agents/ is two levels up.
        let base = PathBuf::from("../../agents");
        if !base.is_dir() {
            eprintln!("skipping ENG-008 acceptance: {} not present", base.display());
            return;
        }

        // Reasoning role -> GPT-4o (standard == optimal today).
        let r = AgentProfile::load(&base.join("researcher.md")).unwrap();
        assert_eq!(r.engine_for(Importance::Routine).as_deref(), Some("GPT-4o"));
        assert_eq!(r.engine_for(Importance::Critical).as_deref(), Some("GPT-4o"));

        // Execution role -> Claude Sonnet standard, Opus optimal for critical.
        let d = AgentProfile::load(&base.join("developer.md")).unwrap();
        assert_eq!(d.standard_engine(), Some("Claude Sonnet 4.6"));
        assert_eq!(
            d.engine_for(Importance::Critical).as_deref(),
            Some("Claude Opus 4.6")
        );
        assert_eq!(
            agent_layer_engine(
                d.standard_engine(),
                d.optimal_engine(),
                Importance::Critical
            )
            .as_deref(),
            Some("Claude Opus 4.6")
        );

        // Admin/dispatch role -> no engine binding (legacy None).
        let coo = AgentProfile::load(&base.join("chief_operations_officer.md")).unwrap();
        assert_eq!(coo.standard_engine(), None);
        assert_eq!(coo.optimal_engine(), None);
        assert_eq!(coo.engine_for(Importance::Critical), None);
    }

    #[test]
    fn test_agent_layer_engine_fallbacks() {
        // optimal-only => high importance uses it, routine falls back to it.
        assert_eq!(
            agent_layer_engine(None, Some("Opus"), Importance::Critical).as_deref(),
            Some("Opus")
        );
        assert_eq!(
            agent_layer_engine(None, Some("Opus"), Importance::Routine).as_deref(),
            Some("Opus")
        );
        // standard-only => critical falls back to it.
        assert_eq!(
            agent_layer_engine(Some("Sonnet"), None, Importance::Critical).as_deref(),
            Some("Sonnet")
        );
        assert_eq!(agent_layer_engine(None, None, Importance::Routine), None);
    }
}
