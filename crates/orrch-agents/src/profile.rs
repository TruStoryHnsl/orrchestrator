use std::path::{Path, PathBuf};

/// An agent profile loaded from a `.md` file.
#[derive(Debug, Clone)]
pub struct AgentProfile {
    pub name: String,
    pub department: String,
    pub role: String,
    pub description: String,
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

        Some(Self {
            name: extract_field(&frontmatter, "name")?,
            department: extract_field(&frontmatter, "department").unwrap_or_default(),
            role: extract_field(&frontmatter, "role").unwrap_or_default(),
            description: extract_field(&frontmatter, "description").unwrap_or_default(),
            prompt: body.trim().to_string(),
            path: path.to_path_buf(),
        })
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

    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
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
            prompt: String::new(),
            path: PathBuf::new(),
        };
        assert_eq!(agent.label(), "Hypervisor (Orchestrator)");
    }
}
