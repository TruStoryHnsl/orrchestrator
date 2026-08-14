pub mod agent;
pub mod audit;
pub mod backend;
pub mod backup;
pub mod compliance;
pub mod config;
pub mod connections;
pub mod context_location;
pub mod context_migrate;
pub mod diff_log;
pub mod engine;
pub mod feedback;
pub mod file_registry;
pub mod git;
pub mod hide;
pub mod intake_review;
pub mod loop_controller;
pub mod loop_review;
pub mod loop_watchdog;
pub mod loops;
pub mod output_parser;
pub mod pi_rpc;
pub mod plan_parser;
pub mod process_manager;
pub mod process_spawn;
pub mod project;
pub mod provider;
pub mod release;
pub mod remote;
pub mod session;
pub mod session_brief;
pub mod session_log;
pub mod shadow;
pub mod staleness;
pub mod usage;
pub mod vault;
pub mod windows;
pub mod workflow_status;

pub use agent::{AgentProfile, agents_dir, load_agents};
pub use audit::{
    AuditEntry, ChunkCoordinate, compute_source_hash, load_audit_entries, write_audit_entry,
};
pub use backend::{BackendKind, BackendsConfig, is_provider_available};
pub use backup::{
    BackupConfig, BackupTier, ProvisionPlan, PushOutcome, configure_remote, provision_plan,
    push_backup,
};
pub use config::Config;
pub use connections::{
    Connection, ConnectionKind, ConnectionStore, connections_path, mask_key, presets,
    test_connection,
};
pub use context_location::{Artifact, ContextScope, artifact_path};
pub use context_migrate::{MigrationReport, migrate_context};
pub use diff_log::{DiffEntry, append_diff, diff_log_path, load_all_diffs, load_diffs};
pub use engine::{
    DEV_DEFAULT_ENGINE, EngineDecision, EngineLayers, EngineSource, Importance, ResolvedEngineId,
    SUPPORT_DEFAULT_ENGINE, agent_engine_pair, agent_layer_engine, agent_role_engine,
    agent_role_engine_for, anthropic_url, class_default_engine, decide_engine, engine_env,
    engine_selectable, openai_url, project_default_engine, resolve_engine, resolve_engine_id,
    resolve_loop_engine_id,
};
pub use feedback::{
    CONTINUE_DEV_PROMPT, FeedbackItem, FeedbackStatus, FeedbackType, InboxMaintenanceReport,
    NewProjectDirective, check_processing_complete, create_append_draft, create_draft,
    delete_feedback, detect_new_project_directives, load_feedback_items,
    maintain_all_project_inboxes, mark_as_processed, mark_as_processing, mark_as_routed,
    save_and_route_feedback, set_feedback_type, submit_feedback, tmux_session_status,
    trim_completed_entries, truncate_inbox_if_large, write_feedback_metadata,
};
pub use hide::{
    CLAUDE_IMPORT_STUB, HideReport, hide_context, is_orrchestrator_self, reveal_context,
};
pub use intake_review::{
    IntakeReview, IntakeReviewFile, distribute_to_inbox_from_intake, load_intake_review,
    load_review_at, write_intake_decision,
};
pub use loops::{
    LoopClass, LoopSchedule, SupportKind, delete_loop, load_loops, loops_path, save_loops,
    toggle_loop, upsert_loop,
};
pub use output_parser::{OutputSignal, analyze_output, infer_state};
pub use pi_rpc::{PiEvent, PiRpcSession};
pub use plan_parser::{
    FeatureStatus, MoveDirection, PlanFeature, PlanPhase, RemovalContext, append_feature_to_plan,
    lint_plan, mark_verified_in_plan, move_feature_in_plan, parse_plan, parse_status_marker,
    rename_feature_in_plan,
};
pub use process_manager::ProcessManager;
pub use project::{
    ColorTag, DirEntry, LifecycleStage, Project, ProjectMeta, ProjectScaffold, RoadmapItem, Scope,
    Temperature, create_project_scaffold, list_directory, load_deprecated, load_projects,
    package_as_v1, slugify_project_name, update_feature_status_in_plan,
};
pub use provider::{ProviderConfig, ProviderKind};
pub use session::{DeviceClass, ExternalSession, Session, SessionState, device_class};
pub use session_brief::{
    SESSION_BRIEFS_SUBDIR, SessionBrief, SessionBriefInput, list_session_briefs,
    write_session_brief,
};
pub use shadow::{DEFAULT_TRACKED_PATHS, ShadowRepo, data_dir, shadow_git_dir};
pub use usage::{RateLimitConfig, UsageTracker};
// NOTE: `Clock`/`SystemClock`/`ManualClock`/`Tstamp` collide with file_registry's
// own identically-named types, so they are NOT re-exported flat here — reach them
// via `loop_controller::` (e.g. `orrch_core::loop_controller::ManualClock`).
pub use compliance::{
    CopyrightReport, LicenseDep, LicenseReport, LicenseStatus, MissingHeader, check_copyright,
    scan_licenses,
};
pub use file_registry::{
    AgentId, CRASH_GRACE, ChangeSpec, Clock, DEFAULT_AUDIT_LOG, DEFAULT_REGISTRY_PATH, EditHandle,
    EditStatus, FileRegistry, ManualClock, Ownership, RegistryError, SOFT_DOUBLE_READ_ENV,
    SystemClock,
};
pub use loop_controller::{
    BlockedReason, LoopConfig, LoopController, LoopInstance, LoopState, ResolvingFeedback,
    RunStatus, RunnerError, WorkAssessment, WorkEvaluator, WorkflowRunner, WorkforceHandle,
};
pub use loop_review::{
    ActionCoeffs, CandidateStep, DEFAULT_COEFFS_PATH, DEFAULT_HISTORY_PATH,
    DEFAULT_PRELIMINARY_PATH, DEFAULT_RANKED_PATH, EnergyScorer, HistoryRow, LagrangianScorer,
    LinearCoeffs, LinearLagrangianScorer, LoopReviewOutcome, ScoredStep, ScorerKind,
    ScoringContext, TermBreakdown, WorkspaceState, loop_review, loop_review_with,
    parse_task_blocks, render_ranked_plan,
};
pub use release::{
    BuildResult, BuildStatus, BuildTarget, BuildTargetKind, BumpKind, build_artifact, bump_version,
    detect_build_targets, generate_changelog_entry, next_version_string,
};
pub use workflow_status::{WorkflowAgentStatus, WorkflowStatus, load_workflow_status};
