pub mod agent;
pub mod audit;
pub mod backup;
pub mod compliance;
pub mod backend;
pub mod config;
pub mod diff_log;
pub mod engine;
pub mod feedback;
pub mod file_registry;
pub mod hide;
pub mod loop_review;
pub mod loop_controller;
pub mod loop_watchdog;
pub mod git;
pub mod intake_review;
pub mod output_parser;
pub mod pi_rpc;
pub mod loops;
pub mod plan_parser;
pub mod process_manager;
pub mod process_spawn;
pub mod project;
pub mod provider;
pub mod release;
pub mod remote;
pub mod session;
pub mod session_brief;
pub mod shadow;
pub mod session_log;
pub mod staleness;
pub mod usage;
pub mod windows;
pub mod vault;
pub mod workflow_status;

pub use agent::{AgentProfile, load_agents, agents_dir};
pub use audit::{AuditEntry, ChunkCoordinate, compute_source_hash, write_audit_entry, load_audit_entries};
pub use backend::{BackendKind, BackendsConfig, is_provider_available};
pub use provider::{ProviderConfig, ProviderKind};
pub use engine::{EngineLayers, EngineSource, ResolvedEngineId, resolve_engine, resolve_engine_id, engine_env, engine_selectable, agent_role_engine, project_default_engine, anthropic_url, openai_url, Importance, agent_layer_engine, agent_role_engine_for, agent_engine_pair, EngineDecision, decide_engine, class_default_engine, resolve_loop_engine_id, SUPPORT_DEFAULT_ENGINE, DEV_DEFAULT_ENGINE};
pub use config::Config;
pub use shadow::{ShadowRepo, data_dir, shadow_git_dir, DEFAULT_TRACKED_PATHS};
pub use backup::{BackupConfig, BackupTier, PushOutcome, ProvisionPlan, configure_remote, push_backup, provision_plan};
pub use hide::{HideReport, hide_context, reveal_context, is_orrchestrator_self, CLAUDE_IMPORT_STUB};
pub use feedback::{save_and_route_feedback, CONTINUE_DEV_PROMPT, FeedbackItem, FeedbackStatus, FeedbackType, NewProjectDirective, detect_new_project_directives, load_feedback_items, submit_feedback, delete_feedback, create_draft, create_append_draft, mark_as_processing, mark_as_processed, mark_as_routed, check_processing_complete, write_feedback_metadata, set_feedback_type, tmux_session_status, truncate_inbox_if_large, trim_completed_entries, maintain_all_project_inboxes, InboxMaintenanceReport};
pub use output_parser::{analyze_output, infer_state, OutputSignal};
pub use pi_rpc::{PiRpcSession, PiEvent};
pub use process_manager::ProcessManager;
pub use plan_parser::{parse_plan, parse_status_marker, PlanPhase, PlanFeature, FeatureStatus, RemovalContext, MoveDirection, move_feature_in_plan, append_feature_to_plan, mark_verified_in_plan, rename_feature_in_plan};
pub use diff_log::{DiffEntry, diff_log_path, append_diff, load_diffs, load_all_diffs};
pub use project::{create_project_scaffold, list_directory, load_deprecated, load_projects, package_as_v1, slugify_project_name, update_feature_status_in_plan, ColorTag, DirEntry, LifecycleStage, Project, ProjectMeta, ProjectScaffold, RoadmapItem, Scope, Temperature};
pub use session::{DeviceClass, ExternalSession, Session, SessionState, device_class};
pub use session_brief::{SESSION_BRIEFS_SUBDIR, SessionBrief, SessionBriefInput, list_session_briefs, write_session_brief};
pub use usage::{RateLimitConfig, UsageTracker};
pub use intake_review::{IntakeReview, IntakeReviewFile, load_intake_review, load_review_at, write_intake_decision, distribute_to_inbox_from_intake};
pub use loops::{LoopSchedule, LoopClass, SupportKind, load_loops, save_loops, upsert_loop, toggle_loop, delete_loop, loops_path};
// NOTE: `Clock`/`SystemClock`/`ManualClock`/`Tstamp` collide with file_registry's
// own identically-named types, so they are NOT re-exported flat here — reach them
// via `loop_controller::` (e.g. `orrch_core::loop_controller::ManualClock`).
pub use loop_controller::{
    LoopState, BlockedReason, WorkAssessment, WorkEvaluator,
    WorkforceHandle, RunStatus, WorkflowRunner, RunnerError, ResolvingFeedback, LoopConfig,
    LoopInstance, LoopController,
};
pub use workflow_status::{WorkflowStatus, WorkflowAgentStatus, load_workflow_status};
pub use loop_review::{
    loop_review, loop_review_with, parse_task_blocks, render_ranked_plan, ActionCoeffs, CandidateStep,
    EnergyScorer, HistoryRow, LagrangianScorer, LinearCoeffs, LinearLagrangianScorer, LoopReviewOutcome,
    ScoredStep, ScorerKind, ScoringContext, TermBreakdown, WorkspaceState, DEFAULT_COEFFS_PATH,
    DEFAULT_HISTORY_PATH, DEFAULT_PRELIMINARY_PATH, DEFAULT_RANKED_PATH,
};
pub use compliance::{scan_licenses, check_copyright, LicenseReport, LicenseDep, LicenseStatus, CopyrightReport, MissingHeader};
pub use release::{BumpKind, BuildTarget, BuildTargetKind, BuildResult, BuildStatus, detect_build_targets, build_artifact, bump_version, generate_changelog_entry, next_version_string};
pub use file_registry::{AgentId, ChangeSpec, Clock, EditHandle, EditStatus, FileRegistry, ManualClock, Ownership, RegistryError, SystemClock, CRASH_GRACE, DEFAULT_AUDIT_LOG, DEFAULT_REGISTRY_PATH, SOFT_DOUBLE_READ_ENV};
