use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextScope {
    Project,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Artifact {
    /// Local: the implementation plan is specific to one project.
    Plan,
    /// Local: the development log records work history for one project.
    Devlog,
    /// Local: project events describe one project's lifecycle and activity.
    Events,
    /// Local: architecture notes describe one project's codebase.
    Architecture,
    /// Local: licensing notes depend on one project's dependencies and package policy.
    Licensing,
    /// Local: a codebase brief summarizes one project for agents.
    CodebaseBrief,
    /// Local: queued instructions are consumed by one project workflow.
    InstructionsInbox,
    /// Local: backup settings configure one project's shadow backup target.
    BackupConfig,
    /// Local: session briefs summarize work performed in one project.
    SessionBriefs,
    /// Local: the file registry coordinates ownership within one project.
    FileRegistry,
    /// Local: file registry audit entries describe one project's coordination history.
    FileRegistryAudit,
    /// Local: workflow status reflects one project's active operation.
    WorkflowStatus,
    /// Local: intake review state is tied to one project instruction pipeline.
    IntakeReview,
    /// Local: loop-review preliminary plans are generated for one project workspace.
    LoopPreliminary,
    /// Local: loop-review ranked plans are generated for one project workspace.
    LoopRanked,
    /// Local: loop-review scoring history is about one project workspace.
    LagrangianHistory,
    /// Local: loop-review coefficients tune selection for one project workspace.
    LagrangianCoeffs,
    /// Local: the default engine override applies to one project.
    ProjectEngine,
    /// Local: project config overrides apply to one project.
    ProjectConfig,
    /// Global: application settings span all projects and describe the user environment.
    Config,
    /// Global: provider API valves span all projects for one user.
    Valves,
    /// Global: usage history spans providers and projects for one user.
    Usage,
    /// Global: loop registry schedules workflows across many projects.
    LoopRegistry,
    /// Global: shadow repositories are app-owned data stored outside project repos.
    ShadowRepo,
}

impl Artifact {
    pub const ALL: &'static [Artifact] = &[
        Artifact::Plan,
        Artifact::Devlog,
        Artifact::Events,
        Artifact::Architecture,
        Artifact::Licensing,
        Artifact::CodebaseBrief,
        Artifact::InstructionsInbox,
        Artifact::BackupConfig,
        Artifact::SessionBriefs,
        Artifact::FileRegistry,
        Artifact::FileRegistryAudit,
        Artifact::WorkflowStatus,
        Artifact::IntakeReview,
        Artifact::LoopPreliminary,
        Artifact::LoopRanked,
        Artifact::LagrangianHistory,
        Artifact::LagrangianCoeffs,
        Artifact::ProjectEngine,
        Artifact::ProjectConfig,
        Artifact::Config,
        Artifact::Valves,
        Artifact::Usage,
        Artifact::LoopRegistry,
        Artifact::ShadowRepo,
    ];

    pub fn scope(&self) -> ContextScope {
        match self {
            Artifact::Plan
            | Artifact::Devlog
            | Artifact::Events
            | Artifact::Architecture
            | Artifact::Licensing
            | Artifact::CodebaseBrief
            | Artifact::InstructionsInbox
            | Artifact::BackupConfig
            | Artifact::SessionBriefs
            | Artifact::FileRegistry
            | Artifact::FileRegistryAudit
            | Artifact::WorkflowStatus
            | Artifact::IntakeReview
            | Artifact::LoopPreliminary
            | Artifact::LoopRanked
            | Artifact::LagrangianHistory
            | Artifact::LagrangianCoeffs
            | Artifact::ProjectEngine
            | Artifact::ProjectConfig => ContextScope::Project,
            Artifact::Config
            | Artifact::Valves
            | Artifact::Usage
            | Artifact::LoopRegistry
            | Artifact::ShadowRepo => ContextScope::Global,
        }
    }

    pub fn file_name(&self) -> &'static str {
        match self {
            Artifact::Plan => "PLAN.md",
            Artifact::Devlog => "DEVLOG.md",
            Artifact::Events => "events.jsonl",
            Artifact::Architecture => "ARCHITECTURE.md",
            Artifact::Licensing => "LICENSING.md",
            Artifact::CodebaseBrief => "CODEBASE_BRIEF.md",
            Artifact::InstructionsInbox => "instructions_inbox.md",
            Artifact::BackupConfig => "backup.json",
            Artifact::SessionBriefs => "session_briefs",
            Artifact::FileRegistry => "file_registry.json",
            Artifact::FileRegistryAudit => "file_registry_audit.log",
            Artifact::WorkflowStatus => "workflow.json",
            Artifact::IntakeReview => "intake_review.json",
            Artifact::LoopPreliminary => "plan_preliminary.md",
            Artifact::LoopRanked => "plan_ranked.md",
            Artifact::LagrangianHistory => "lagrangian_history.jsonl",
            Artifact::LagrangianCoeffs => "lagrangian_coeffs.json",
            Artifact::ProjectEngine => "engine",
            Artifact::ProjectConfig => "config.json",
            Artifact::Config => "config.json",
            Artifact::Valves => "valves.json",
            Artifact::Usage => "usage.jsonl",
            Artifact::LoopRegistry => "loops.json",
            Artifact::ShadowRepo => "shadow",
        }
    }

    fn global_root(&self) -> Option<PathBuf> {
        match self {
            Artifact::Config | Artifact::Valves => Some(config::config_dir()),
            Artifact::Usage | Artifact::LoopRegistry | Artifact::ShadowRepo => {
                Some(config::data_dir())
            }
            _ => None,
        }
    }
}

pub fn artifact_path(a: Artifact, project_dir: Option<&Path>) -> Result<PathBuf> {
    match a.scope() {
        ContextScope::Project => {
            let project_dir = project_dir.ok_or_else(|| {
                anyhow!("project directory is required for project-scoped artifact {a:?}")
            })?;
            Ok(project_dir.join(".orrch").join(a.file_name()))
        }
        ContextScope::Global => Ok(a
            .global_root()
            .expect("global artifact must have a global root")
            .join(a.file_name())),
    }
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

    fn fresh_home(prefix: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn artifacts_have_documented_scopes() {
        let project = [
            Artifact::Plan,
            Artifact::Devlog,
            Artifact::Events,
            Artifact::Architecture,
            Artifact::Licensing,
            Artifact::CodebaseBrief,
            Artifact::InstructionsInbox,
            Artifact::BackupConfig,
            Artifact::SessionBriefs,
            Artifact::FileRegistry,
            Artifact::FileRegistryAudit,
            Artifact::WorkflowStatus,
            Artifact::IntakeReview,
            Artifact::LoopPreliminary,
            Artifact::LoopRanked,
            Artifact::LagrangianHistory,
            Artifact::LagrangianCoeffs,
            Artifact::ProjectEngine,
            Artifact::ProjectConfig,
        ];
        let global = [
            Artifact::Config,
            Artifact::Valves,
            Artifact::Usage,
            Artifact::LoopRegistry,
            Artifact::ShadowRepo,
        ];

        assert_eq!(Artifact::ALL.len(), project.len() + global.len());
        for artifact in project {
            assert_eq!(artifact.scope(), ContextScope::Project, "{artifact:?}");
        }
        for artifact in global {
            assert_eq!(artifact.scope(), ContextScope::Global, "{artifact:?}");
        }
    }

    #[test]
    fn project_artifacts_resolve_under_dot_orrch() {
        let project_dir = PathBuf::from("/tmp/example-project");

        for artifact in Artifact::ALL
            .iter()
            .copied()
            .filter(|a| a.scope() == ContextScope::Project)
        {
            let path = artifact_path(artifact, Some(&project_dir)).unwrap();
            assert_eq!(path, project_dir.join(".orrch").join(artifact.file_name()));
        }
    }

    #[test]
    fn project_artifact_requires_project_dir() {
        let err = artifact_path(Artifact::Plan, None).unwrap_err().to_string();
        assert!(err.contains("project directory is required"));
    }

    #[test]
    fn global_artifacts_resolve_under_config_or_data_dir() {
        let _lock = crate::shadow::TEST_ENV_LOCK.lock().unwrap();
        let home = fresh_home("orrch-context-location-home");
        let _guard = HomeGuard::set(&home);
        let config_dir = home.join(".config").join("orrchestrator");
        let data_dir = home.join(".local").join("share").join("orrchestrator");

        assert_eq!(
            artifact_path(Artifact::Config, None).unwrap(),
            config_dir.join("config.json")
        );
        assert_eq!(
            artifact_path(Artifact::Valves, None).unwrap(),
            config_dir.join("valves.json")
        );
        assert_eq!(
            artifact_path(Artifact::Usage, None).unwrap(),
            data_dir.join("usage.jsonl")
        );
        assert_eq!(
            artifact_path(Artifact::LoopRegistry, None).unwrap(),
            data_dir.join("loops.json")
        );
        assert_eq!(
            artifact_path(Artifact::ShadowRepo, None).unwrap(),
            data_dir.join("shadow")
        );

        let _ = std::fs::remove_dir_all(home);
    }
}
