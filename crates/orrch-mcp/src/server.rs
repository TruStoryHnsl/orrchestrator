use std::path::PathBuf;

/// Central server state — holds resolved directory paths.
/// All paths are resolved once at startup; files are read fresh on each request.
pub struct OrrchMcpServer {
    /// Agent profiles directory (e.g. ~/projects/orrchestrator/agents/)
    pub agents_dir: PathBuf,
    /// Library directory (e.g. ~/projects/orrchestrator/library/)
    pub library_dir: PathBuf,
    /// Top-level projects directory (e.g. ~/projects/)
    pub projects_dir: PathBuf,
    /// Workflow skill files (e.g. ~/projects/.claude/commands/)
    pub skills_dir: PathBuf,
}

impl OrrchMcpServer {
    /// Build from conventional default paths.
    pub fn from_defaults() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
        let projects_dir = PathBuf::from(&home).join("projects");
        let orrch_dir = projects_dir.join("orrchestrator");

        Self {
            agents_dir: orrch_dir.join("agents"),
            library_dir: orrch_dir.join("library"),
            projects_dir,
            skills_dir: PathBuf::from(&home).join("projects/.claude/commands"),
        }
    }
}
