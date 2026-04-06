use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use orrch_core::process_manager::SessionEvent;
use orrch_core::usage::{self, UsageRecord, UsageEvent, UsageTracker};
use orrch_core::{
    analyze_output, infer_state, load_projects, BackendKind, ColorTag,
    OutputSignal, ProcessManager, Project, SessionState, Temperature, CONTINUE_DEV_PROMPT,
    FeedbackItem, FeedbackStatus, load_agents, agents_dir,
};
use orrch_retrospect::{ErrorStore, SolutionTracker};
use tokio::sync::mpsc;

use crate::editor::{VimKind, VimRequest, PendingEditor};

// ─── Panel System ─────────────────────────────────────────────────────

/// Top-level panels navigable with left/right arrows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Design,    // 0 — intentions + workforce editor + library browser
    Oversee,   // 1 — project tracker
    Hypervise, // 2 — interactive multi-session management
    Analyze,   // 3 — token efficiency calibration (placeholder)
    Publish,   // 4 — release packaging + marketing (placeholder)
}

impl Panel {
    pub const ALL: [Panel; 5] = [
        Panel::Design,
        Panel::Oversee,
        Panel::Hypervise,
        Panel::Analyze,
        Panel::Publish,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Design => "Design",
            Self::Oversee => "Oversee",
            Self::Hypervise => "Hypervise",
            Self::Analyze => "Analyze",
            Self::Publish => "Publish",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Design => "Des",
            Self::Oversee => "Ove",
            Self::Hypervise => "Hyp",
            Self::Analyze => "Ana",
            Self::Publish => "Pub",
        }
    }

    pub fn tiny_label(&self) -> &'static str {
        match self {
            Self::Design => "D",
            Self::Oversee => "O",
            Self::Hypervise => "H",
            Self::Analyze => "A",
            Self::Publish => "P",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Self::Design => 0,
            Self::Oversee => 1,
            Self::Hypervise => 2,
            Self::Analyze => 3,
            Self::Publish => 4,
        }
    }

    pub fn next(&self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(&self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Sub-views within the current panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubView {
    /// Default panel list view.
    List,
    /// Project detail (project index).
    ProjectDetail(usize),
    /// Session focus — managed session (global index into pm.sessions).
    SessionFocus(usize),
    /// External session viewer — shows conversation log (PID).
    ExternalSessionView(u32),
    /// Spawn wizard steps.
    SpawnGoal,
    SpawnWorkforce,
    SpawnAgent,
    SpawnBackend,
    SpawnHost,
    /// Confirm feedback delete.
    ConfirmDeleteFeedback(usize),
    /// Routing summary after feedback save.
    RoutingSummary,
    /// Confirm deprecation.
    ConfirmDeprecate(usize),
    /// Confirm marking project as complete (v1 packaging).
    ConfirmComplete(usize),
    /// Deprecated browser (opened from Facilities).
    DeprecatedBrowser,
    /// Context action menu for selected item.
    ActionMenu,
    /// Confirm permanent deletion of a deprecated project.
    ConfirmDeleteDeprecated,
    /// Global app menu (Esc from anywhere).
    AppMenu,
    /// Commit review overlay — shows instruction packages by project, allows correction loop.
    CommitReview(usize), // feedback_items index
    /// Waiting for Claude to finish a correction.
    CommitCorrecting(usize),
    /// New project wizard steps.
    NewProjectName,
    NewProjectScope,
    NewProjectConfirm,
    /// Feedback submission confirmation — shows routing targets, lets user edit.
    FeedbackConfirm(usize), // index into feedback_items
    /// Workflow picker — select a workflow script to run.
    WorkflowPicker,
    /// Add Feature popup in dev map (project index).
    AddFeature(usize),
    /// Add MCP Server registration form.
    AddMcpServer,
}

/// Sub-panels within the Design panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignSub {
    Intentions, // feedback intake + ideas (was ProjectDesign)
    Workforce,  // full-stack .md editor for workflows, teams, agents, skills, tools, etc.
    Library,    // read-only browser (right-justified in tab bar)
}

impl DesignSub {
    pub const ALL: [DesignSub; 3] = [
        DesignSub::Intentions,
        DesignSub::Workforce,
        DesignSub::Library,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Intentions => "Intentions",
            Self::Workforce => "Workforce",
            Self::Library => "Library",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Self::Intentions => 0,
            Self::Workforce => 1,
            Self::Library => 2,
        }
    }

    pub fn next(&self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(&self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Tabs within the Workforce editor (Design > Workforce).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkforceTab {
    Harnesses,     // harness definitions (read-only editor stub)
    Workflows,     // workforce templates (pipelines)
    Teams,         // operation modules (alias)
    Agents,
    Skills,
    Tools,
    McpServers,
    Profiles,      // system-prompt profiles
    TrainingData,  // model training data (coming soon)
    Models,        // model definitions (coming soon)
}

impl WorkforceTab {
    pub const ALL: [WorkforceTab; 10] = [
        WorkforceTab::Harnesses, WorkforceTab::Workflows, WorkforceTab::Teams,
        WorkforceTab::Agents, WorkforceTab::Skills, WorkforceTab::Tools,
        WorkforceTab::McpServers, WorkforceTab::Profiles, WorkforceTab::TrainingData,
        WorkforceTab::Models,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Harnesses => "Harnesses",
            Self::Workflows => "Workflows",
            Self::Teams => "Teams",
            Self::Agents => "Agents",
            Self::Skills => "Skills",
            Self::Tools => "Tools",
            Self::McpServers => "MCP",
            Self::Profiles => "Profiles",
            Self::TrainingData => "Training",
            Self::Models => "Models",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|t| *t == *self).unwrap_or(0)
    }

    pub fn next(&self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(&self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Tabs within the Library browser (Design > Library). Read-only view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySub {
    Agents,
    Models,
    Harnesses,
    McpServers,
    Skills,
    Tools,
}

impl LibrarySub {
    pub const ALL: [LibrarySub; 6] = [
        LibrarySub::Agents, LibrarySub::Models, LibrarySub::Harnesses,
        LibrarySub::McpServers, LibrarySub::Skills, LibrarySub::Tools,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Agents => "Agents",
            Self::Models => "Models",
            Self::Harnesses => "Harnesses",
            Self::McpServers => "MCP",
            Self::Skills => "Skills",
            Self::Tools => "Tools",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|t| *t == *self).unwrap_or(0)
    }

    pub fn next(&self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(&self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Focus within the project detail view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailFocus {
    Roadmap,
    Sessions,
    DevMap,
    Browser,
}

/// Focus pane within the intake review overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntakeReviewFocus {
    Raw,
    Optimized,
}

/// What a project list row maps to.
#[derive(Debug, Clone)]
pub enum ListEntry {
    SectionHeader,
    Project(usize),
    ProductionVersion(usize), // index into production_versions
    DeprecatedFolder,
    Facility(usize),
    SubProject(usize, usize),
}

// ─── Commit Review ──────────────────────────────────────────────────

/// An instruction package that Claude wrote to a project's fb2p.md.
#[derive(Debug, Clone)]
pub struct CommitPackage {
    pub project_name: String,
    pub project_dir: PathBuf,
    pub entry_preview: String, // first ~5 lines of the entry
    pub entry_full: String,    // full entry text
}

// ─── Inline Tree ─────────────────────────────────────────────────────

/// A node in the inline project directory tree.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
    pub icon: &'static str,
    pub is_editable: bool,
}

// ─── Action Menu ─────────────────────────────────────────────────────

/// A context-sensitive action that can be performed on the selected item.
#[derive(Debug, Clone)]
pub struct ActionItem {
    pub key: char,        // accelerator key
    pub label: String,    // display text
    pub action: ActionKind,
}

#[derive(Debug, Clone)]
pub enum ActionKind {
    SpawnSession,
    SpawnAll,       // N — multi-spawn
    NewProject,
    WriteFeedback,
    WriteProjectFeedback(usize),
    MasterPlanAppend(usize),
    CycleTag,
    CycleScope,
    CycleTemp,      // hot/cold/ignored
    IgnoreProject,
    DeprecateProject,
    CompleteProject,
    ReloadProjects,
    GitCommit,     // commit+push selected project via Claude
    GitCommitAll,  // commit+push all dirty projects via Claude
    KillSession(String),
    SubmitFeedback(String),  // filename
    ResumeFeedback(String),
    DeleteFeedback(usize),
}

// ─── App State ────────────────────────────────────────────────────────

pub struct App {
    pub pm: ProcessManager,
    pub panel: Panel,
    pub sub: SubView,
    pub focus_depth: usize, // 0=panel bar, 1=sub bar, 2=sub-sub bar, deeper=content
    pub should_quit: bool,
    pub projects_dir: PathBuf,
    pub event_rx: mpsc::UnboundedReceiver<SessionEvent>,

    // Project data — all projects, plus indices into hot/cold/facilities
    pub projects: Vec<Project>,
    pub hot_indices: Vec<usize>,
    pub cold_indices: Vec<usize>,
    pub ignored_indices: Vec<usize>,
    pub facilities: Vec<Project>,   // hyperfolders (admin, etc.)
    pub project_selected: usize,    // global selection index into the rendered list
    pub session_selected: usize,
    pub roadmap_selected: usize,
    pub expanded_projects: HashSet<usize>,
    pub show_deprecated: bool,      // toggled in facilities section

    // Inline directory tree in expanded project rows
    /// Project index → set of expanded directory paths (relative to project root).
    pub tree_expanded: HashMap<usize, HashSet<PathBuf>>,
    /// Project index → cached directory listing for the project root.
    pub tree_cache: HashMap<usize, Vec<TreeNode>>,
    /// Index into the flat tree listing for the selected project (if browsing tree).
    pub tree_selected: usize,
    /// True when the user is navigating inside an expanded project's tree.
    pub tree_browsing: bool,
    /// Which project index the tree browser is active in.
    pub tree_project: Option<usize>,
    /// Preview content for the currently selected tree file.
    pub tree_preview: String,

    // Design panel sub-navigation
    pub design_sub: DesignSub,

    // Spawn wizard
    pub spawn_project_idx: usize,
    pub spawn_goal_text: String,
    pub spawn_goal_from_roadmap: Option<usize>,
    pub spawn_agent_idx: usize, // 0 = no agent, 1+ = index into agent_profiles
    pub spawn_backend: BackendKind,
    pub spawn_host_idx: usize, // 0 = local, 1+ = remote_hosts index
    pub spawn_workforce_idx: usize,  // 0 = no workforce (solo), 1+ = index into loaded_workforces
    pub loaded_workforces: Vec<orrch_workforce::Workforce>,

    // Agent profiles
    pub agent_profiles: Vec<orrch_core::AgentProfile>,

    // Design > Workforce editor
    pub workforce_tab: WorkforceTab,
    pub wf_selected: usize,
    pub wf_preview_scroll: usize,

    // Design > Library browser (read-only)
    pub library_sub: LibrarySub,
    pub library_selected: usize,
    pub library_scroll: usize,
    pub library_preview_scroll: usize,

    // Shared data (used by both Workforce editor and Library browser)
    pub library_models: Vec<orrch_library::ModelEntry>,
    pub library_harnesses: Vec<orrch_library::HarnessEntry>,
    pub library_mcp_servers: Vec<orrch_library::McpServerEntry>,
    pub library_skills: Vec<(String, PathBuf)>,  // (name, path) from library/skills/
    pub library_tools: Vec<(String, PathBuf)>,   // (name, path) from library/tools/
    pub library_profiles: Vec<(String, PathBuf)>, // system-prompt profiles
    pub valve_store: orrch_library::ValveStore,
    pub usage_tracker: orrch_core::UsageTracker,

    // Workforce Design
    pub workforce_files: Vec<(String, std::path::PathBuf)>, // (name, path)
    pub workforce_selected: usize,
    pub operation_files: Vec<(String, std::path::PathBuf)>, // (name, path)
    pub operation_selected: usize,

    // File browser (two-column: parent dir + child dir/preview)
    pub browser_parent_entries: Vec<orrch_core::DirEntry>,
    pub browser_parent_selected: usize,
    pub browser_child_entries: Vec<orrch_core::DirEntry>,
    pub browser_child_selected: usize,
    pub browser_in_child: bool,
    pub browser_path: PathBuf,
    pub browser_root: PathBuf,
    pub browser_preview: String,

    // Detail view focus: sessions / dev map / browser
    pub detail_focus: DetailFocus,

    // Dev map (parsed Plan.md feature tree)
    pub devmap_phase_idx: usize,    // which phase is expanded (or usize::MAX for none)
    pub devmap_selected: usize,     // flat selection index across all visible items

    // Deprecated panel (two-column browser)
    pub dep_parent_entries: Vec<orrch_core::DirEntry>,
    pub dep_parent_selected: usize,
    pub dep_child_entries: Vec<orrch_core::DirEntry>,
    pub dep_child_selected: usize,
    pub dep_in_child: bool,
    pub dep_path: PathBuf,
    pub dep_root: PathBuf,
    pub dep_preview: String,

    // Ideas panel
    pub ideas: Vec<orrch_core::vault::Idea>,
    pub idea_selected: usize,

    // Production panel
    pub production_versions: Vec<ProductionEntry>,
    pub production_selected: usize,

    // Vim / external editor
    pub vim_request: Option<VimRequest>,
    pub pending_editors: Vec<PendingEditor>,

    // Feedback pipeline
    pub feedback_items: Vec<FeedbackItem>,
    pub feedback_selected: usize,

    // Routing
    pub routing_result: Vec<(String, PathBuf)>,

    // External session log viewer
    pub ext_log_scroll: usize,
    pub ext_log_cache: String,

    // Remote hosts
    pub remote_hosts: Vec<orrch_core::remote::RemoteHost>,
    pub remote_sessions: Vec<orrch_core::ExternalSession>, // sessions from remote machines

    // Retrospect
    pub error_stores: HashMap<String, ErrorStore>,
    pub solution_trackers: HashMap<String, SolutionTracker>,
    pub error_count: usize,
    pub last_notification: Option<(String, std::time::Instant)>,
    pub last_signals: HashMap<String, OutputSignal>,

    /// Target KWin output for orrchestrator-managed windows (e.g. "DP-2").
    /// Detected at startup from the current window's output, or set manually.
    pub target_output: Option<String>,

    // App menu
    pub app_menu_selected: usize,

    // Sessions tab
    pub managed_sessions: Vec<orrch_core::windows::ManagedSession>,
    pub session_tab_selected: usize,

    // New project wizard
    pub new_project_name: String,
    pub new_project_scope: orrch_core::Scope,
    pub new_project_temp: Temperature,
    pub new_project_error: Option<String>,

    // Action menu
    pub action_items: Vec<ActionItem>,
    pub action_selected: usize,
    pub action_return_sub: Option<Box<SubView>>, // where to return on Esc

    // Feedback confirmation
    pub confirm_routes: Vec<(String, PathBuf, bool)>, // (name, path, enabled)
    pub confirm_route_selected: usize,
    pub confirm_feedback_text: String,
    pub confirm_feedback_type: orrch_core::FeedbackType,

    // Commit review
    pub commit_packages: Vec<CommitPackage>,
    pub commit_scroll: usize,
    pub commit_correction_text: String,
    pub commit_typing_correction: bool,
    pub commit_correction_session: Option<String>, // tmux session name

    // Workflow status (live agent tree in Hypervise panel)
    pub workflow_status: Option<orrch_core::WorkflowStatus>,

    // Intake review (instruction audit UI)
    pub intake_review: Option<orrch_core::IntakeReview>,
    pub intake_review_scroll_raw: u16,
    pub intake_review_scroll_opt: u16,
    pub intake_review_focus: IntakeReviewFocus,

    // Split-off vim editors from the orrch-edit session
    pub split_off_editors: Vec<String>,

    // Audit trail expansion in Intentions panel (index of expanded idea, or None)
    pub ideas_audit_expanded: Option<usize>,

    // Workflow picker
    /// Available workflow scripts: (filename, display_name)
    pub workflow_choices: Vec<(String, String)>,
    /// Selected index in workflow picker
    pub workflow_picker_idx: usize,

    // Add Feature popup state (Task 47)
    pub add_feature_title: String,
    pub add_feature_desc: String,
    pub add_feature_field: usize, // 0=title, 1=desc

    // Add MCP Server form state (Task 62)
    pub add_mcp_name: String,
    pub add_mcp_desc: String,
    pub add_mcp_transport: usize,   // 0=stdio, 1=sse
    pub add_mcp_command: String,    // command (stdio) or url (sse)
    pub add_mcp_args: String,       // space-separated args (stdio only)
    pub add_mcp_roles: String,      // comma-separated role names
    pub add_mcp_field: usize,       // 0=name, 1=desc, 2=transport, 3=command/url, 4=args, 5=roles
}

/// A versioned release entry for the Production panel.
#[derive(Debug, Clone)]
pub struct ProductionEntry {
    pub project_name: String,
    pub version: String,
    pub path: PathBuf,
    pub working: bool, // false = marked red
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let projects_dir = default_projects_dir();
        let mut all_projects = load_projects(&projects_dir);
        let mut facilities = Vec::new();
        let mut regular = Vec::new();
        for p in all_projects {
            if p.is_hyperfolder {
                facilities.push(p);
            } else {
                regular.push(p);
            }
        }
        let production_versions = scan_production(&regular);
        let vault = orrch_core::vault::vault_dir(&projects_dir);
        let ideas = orrch_core::vault::load_ideas(&vault);
        let feedback_items = orrch_core::load_feedback_items(&projects_dir);
        let deprecated_path = projects_dir.join("deprecated");
        let workforce_files = scan_md_dir(&projects_dir.join("orrchestrator").join("workforces"));
        let operation_files = scan_md_dir(&projects_dir.join("orrchestrator").join("operations"));
        let loaded_workforces = orrch_workforce::load_workforces(&projects_dir.join("orrchestrator").join("workforces"));
        let library_root = projects_dir.join("orrchestrator").join("library");
        let library_models = orrch_library::load_models(&library_root.join("models"));
        let library_harnesses = orrch_library::load_harnesses(&library_root.join("harnesses"));
        let library_mcp_servers = orrch_library::load_mcp_servers(&library_root.join("mcp_servers"));
        let library_skills = scan_md_dir(&library_root.join("skills"));
        let library_tools = scan_md_dir(&library_root.join("tools"));
        let library_profiles = scan_md_dir(&library_root.join("profiles"));
        let valve_store = orrch_library::ValveStore::load();
        let mut usage_tracker = orrch_core::UsageTracker::new();
        usage_tracker.set_defaults();

        let mut app = Self {
            pm: ProcessManager::new(tx),
            panel: Panel::Oversee,
            sub: SubView::List,
            focus_depth: 1, // start at content level for Oversee panel
            should_quit: false,
            projects_dir,
            event_rx: rx,
            hot_indices: Vec::new(),
            cold_indices: Vec::new(),
            ignored_indices: Vec::new(),
            projects: regular,
            facilities,
            project_selected: 0,
            session_selected: 0,
            roadmap_selected: 0,
            expanded_projects: HashSet::new(),
            show_deprecated: false,
            app_menu_selected: 0,
            managed_sessions: Vec::new(),
            session_tab_selected: 0,
            tree_expanded: HashMap::new(),
            tree_cache: HashMap::new(),
            tree_selected: 0,
            tree_browsing: false,
            tree_project: None,
            tree_preview: String::new(),
            design_sub: DesignSub::Intentions,
            spawn_project_idx: 0,
            spawn_goal_text: String::new(),
            spawn_goal_from_roadmap: None,
            spawn_agent_idx: 0,
            spawn_backend: BackendKind::Claude,
            spawn_host_idx: 0,
            spawn_workforce_idx: 0,
            loaded_workforces,
            agent_profiles: load_agents(&agents_dir()),
            workforce_tab: WorkforceTab::Workflows,
            wf_selected: 0,
            wf_preview_scroll: 0,
            library_sub: LibrarySub::Agents,
            library_selected: 0,
            library_scroll: 0,
            library_preview_scroll: 0,
            library_models,
            library_harnesses,
            library_mcp_servers,
            library_skills,
            library_tools,
            library_profiles,
            valve_store,
            usage_tracker,
            workforce_files,
            workforce_selected: 0,
            operation_files,
            operation_selected: 0,
            browser_parent_entries: Vec::new(),
            browser_parent_selected: 0,
            browser_child_entries: Vec::new(),
            browser_child_selected: 0,
            browser_in_child: false,
            browser_path: PathBuf::new(),
            browser_root: PathBuf::new(),
            browser_preview: String::new(),
            detail_focus: DetailFocus::Sessions,
            devmap_phase_idx: usize::MAX,
            devmap_selected: 0,
            dep_parent_entries: orrch_core::list_directory(&deprecated_path),
            dep_parent_selected: 0,
            dep_child_entries: Vec::new(),
            dep_child_selected: 0,
            dep_in_child: false,
            dep_path: deprecated_path.clone(),
            dep_root: deprecated_path,
            dep_preview: String::new(),
            ideas,
            idea_selected: 0,
            production_versions,
            production_selected: 0,
            vim_request: None,
            pending_editors: Vec::new(),
            feedback_items,
            feedback_selected: 0,
            routing_result: Vec::new(),
            ext_log_scroll: 0,
            ext_log_cache: String::new(),
            remote_hosts: orrch_core::remote::known_hosts(),
            remote_sessions: Vec::new(),
            error_stores: HashMap::new(),
            solution_trackers: HashMap::new(),
            error_count: 0,
            last_notification: None,
            last_signals: HashMap::new(),
            // Auto-detect: orrchestrator-managed windows go to whichever output is currently active
            target_output: detect_current_output(),
            new_project_name: String::new(),
            new_project_scope: orrch_core::Scope::Private,
            new_project_temp: Temperature::Hot,
            new_project_error: None,
            action_items: Vec::new(),
            action_selected: 0,
            action_return_sub: None,
            confirm_routes: Vec::new(),
            confirm_route_selected: 0,
            confirm_feedback_text: String::new(),
            confirm_feedback_type: orrch_core::FeedbackType::Feedback,
            commit_packages: Vec::new(),
            commit_scroll: 0,
            commit_correction_text: String::new(),
            commit_typing_correction: false,
            commit_correction_session: None,
            workflow_status: None,
            intake_review: None,
            intake_review_scroll_raw: 0,
            intake_review_scroll_opt: 0,
            intake_review_focus: IntakeReviewFocus::Raw,
            split_off_editors: Vec::new(),
            ideas_audit_expanded: None,
            workflow_choices: Vec::new(),
            workflow_picker_idx: 0,
            add_feature_title: String::new(),
            add_feature_desc: String::new(),
            add_feature_field: 0,

            add_mcp_name: String::new(),
            add_mcp_desc: String::new(),
            add_mcp_transport: 0,
            add_mcp_command: String::new(),
            add_mcp_args: String::new(),
            add_mcp_roles: String::new(),
            add_mcp_field: 0,
        };
        app.categorize_projects();
        // Expand all projects by default so sessions are visible at a glance
        app.expanded_projects = (0..app.projects.len()).collect();
        // Check for orphaned vim editor windows from a previous orrchestrator session.
        // Draft files persist on disk; the periodic reload will pick up any changes.
        let orphan_count = count_orphaned_editor_windows();
        if orphan_count > 0 {
            app.last_notification = Some((
                format!("Re-adopted {orphan_count} editor window(s) from previous session"),
                std::time::Instant::now(),
            ));
        }
        // Detect orphaned tmux windows from a previous unclean exit
        let orphans = orrch_core::windows::detect_orphaned_sessions();
        if !orphans.is_empty() {
            app.last_notification = Some((
                format!("{} orphaned tmux window(s) from last session detected", orphans.len()),
                std::time::Instant::now(),
            ));
        }
        app
    }

    /// Returns the focus depth at which content (list items) lives for the current panel state.
    /// Depths below this are navigable bar levels.
    pub fn content_depth(&self) -> usize {
        match self.panel {
            Panel::Design => match self.design_sub {
                DesignSub::Intentions => 2,  // panel(0) → sub(1) → content(2)
                DesignSub::Workforce => 3,   // panel(0) → sub(1) → wf tabs(2) → content(3)
                DesignSub::Library => 3,     // panel(0) → sub(1) → lib tabs(2) → content(3)
            },
            _ => 1,                          // panel(0) → content(1)
        }
    }

    pub fn reload_projects(&mut self) {
        let all = load_projects(&self.projects_dir);
        self.facilities.clear();
        self.projects.clear();
        for p in all {
            if p.is_hyperfolder {
                self.facilities.push(p);
            } else {
                self.projects.push(p);
            }
        }
        self.production_versions = scan_production(&self.projects);
        self.categorize_projects();
        // Expand any new projects that weren't in the set (preserves user collapses)
        for i in 0..self.projects.len() {
            self.expanded_projects.insert(i);
        }
    }

    /// Categorize projects into hot (active work) and cold (parked).
    /// Build the flat list mapping for the projects panel.
    pub fn build_list_map(&self) -> Vec<ListEntry> {
        let mut map = Vec::new();
        if !self.hot_indices.is_empty() {
            map.push(ListEntry::SectionHeader);
            for &idx in &self.hot_indices { map.push(ListEntry::Project(idx)); }
        }
        if !self.cold_indices.is_empty() {
            map.push(ListEntry::SectionHeader);
            for &idx in &self.cold_indices { map.push(ListEntry::Project(idx)); }
        }
        // Ignored section
        if !self.ignored_indices.is_empty() {
            map.push(ListEntry::SectionHeader);
            for &idx in &self.ignored_indices { map.push(ListEntry::Project(idx)); }
        }
        // Production section
        if !self.production_versions.is_empty() {
            map.push(ListEntry::SectionHeader);
            for (i, _) in self.production_versions.iter().enumerate() {
                map.push(ListEntry::ProductionVersion(i));
            }
        }
        // Facilities section
        if !self.facilities.is_empty() || self.projects_dir.join("deprecated").is_dir() {
            map.push(ListEntry::SectionHeader);
            if self.projects_dir.join("deprecated").is_dir() {
                map.push(ListEntry::DeprecatedFolder);
            }
            for (fi, facility) in self.facilities.iter().enumerate() {
                map.push(ListEntry::Facility(fi));
                for (si, _) in facility.sub_projects.iter().enumerate() {
                    map.push(ListEntry::SubProject(fi, si));
                }
            }
        }
        map
    }

    /// Resolve the current project_selected (flat list index) to a project index.
    pub fn selected_project_index(&self) -> Option<usize> {
        let map = self.build_list_map();
        match map.get(self.project_selected) {
            Some(ListEntry::Project(idx)) => Some(*idx),
            _ => None,
        }
    }

    pub fn categorize_projects(&mut self) {
        self.hot_indices.clear();
        self.cold_indices.clear();
        self.ignored_indices.clear();
        for (i, proj) in self.projects.iter().enumerate() {
            match proj.temperature {
                orrch_core::Temperature::Hot => self.hot_indices.push(i),
                orrch_core::Temperature::Cold => self.cold_indices.push(i),
                orrch_core::Temperature::Ignored => self.ignored_indices.push(i),
            }
        }
    }

    // ─── Inline Tree Methods ──────────────────────────────────────

    /// Build a flat list of tree nodes for a project, respecting expanded state.
    pub fn build_tree(&mut self, proj_idx: usize) -> Vec<TreeNode> {
        let proj_path = if let Some(p) = self.projects.get(proj_idx) {
            p.path.clone()
        } else {
            return Vec::new();
        };

        let expanded_dirs = self.tree_expanded.entry(proj_idx).or_default().clone();
        let mut nodes = Vec::new();
        self.build_tree_recursive(&proj_path, &proj_path, &expanded_dirs, 0, &mut nodes);
        nodes
    }

    fn build_tree_recursive(
        &self,
        dir: &Path,
        root: &Path,
        expanded: &HashSet<PathBuf>,
        depth: usize,
        out: &mut Vec<TreeNode>,
    ) {
        let entries = orrch_core::list_directory(dir);
        for entry in entries {
            let rel = entry.path.strip_prefix(root).unwrap_or(&entry.path).to_path_buf();
            let is_expanded = entry.is_dir && expanded.contains(&rel);
            out.push(TreeNode {
                name: entry.name.clone(),
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                depth,
                expanded: is_expanded,
                icon: entry.icon(),
                is_editable: entry.is_editable,
            });
            if is_expanded {
                self.build_tree_recursive(&entry.path, root, expanded, depth + 1, out);
            }
        }
    }

    /// Toggle expansion of a directory in the inline tree.
    pub fn tree_toggle_dir(&mut self, proj_idx: usize, dir_rel: &Path) {
        let dirs = self.tree_expanded.entry(proj_idx).or_default();
        if dirs.contains(dir_rel) {
            dirs.remove(dir_rel);
        } else {
            dirs.insert(dir_rel.to_path_buf());
        }
    }

    /// Update the tree preview for the currently selected node.
    pub fn update_tree_preview(&mut self, proj_idx: usize) {
        let nodes = self.build_tree(proj_idx);
        if let Some(node) = nodes.get(self.tree_selected) {
            if node.is_dir {
                let children = orrch_core::list_directory(&node.path);
                let dirs = children.iter().filter(|e| e.is_dir).count();
                self.tree_preview = format!(
                    "{} {}\n\n  Type: Directory\n  Contents: {} dirs, {} files",
                    node.icon, node.name, dirs, children.len() - dirs,
                );
            } else if node.is_editable {
                let content = std::fs::read_to_string(&node.path).unwrap_or_default();
                let lines = content.lines().count();
                let preview: String = content.lines().take(30).collect::<Vec<_>>().join("\n");
                self.tree_preview = format!(
                    "{} {} ({} lines)\n\n{}", node.icon, node.name, lines, preview,
                );
            } else {
                self.tree_preview = format!("{} {}", node.icon, node.name);
            }
        } else {
            self.tree_preview.clear();
        }
    }

    pub fn sessions_for_project(&self, project_path: &Path) -> Vec<&orrch_core::Session> {
        self.pm.sessions().into_iter()
            .filter(|s| s.project_dir == project_path)
            .collect()
    }

    pub fn external_sessions_for_project(&self, project_path: &Path) -> Vec<&orrch_core::ExternalSession> {
        let path_str = project_path.to_string_lossy();
        let proj_name = project_path.file_name().unwrap_or_default().to_string_lossy();
        let mut sessions: Vec<&orrch_core::ExternalSession> = self.pm.external_sessions().iter()
            .filter(|e| e.project_dir == path_str.as_ref() || e.project_dir.starts_with(path_str.as_ref()))
            .collect();
        // Include remote sessions matching by project name (paths differ across machines)
        sessions.extend(self.remote_sessions.iter()
            .filter(|e| e.project_dir.ends_with(&format!("/{proj_name}"))));
        sessions
    }

    /// Sessions at ~/projects/ root — admin/general sessions.
    pub fn admin_sessions(&self) -> Vec<&orrch_core::ExternalSession> {
        let projects_str = self.projects_dir.to_string_lossy();
        self.pm.external_sessions().iter()
            .filter(|e| e.project_dir == projects_str.as_ref())
            .collect()
    }

    /// Get unique active goals (pipelines) for a project, with session count per goal.
    pub fn pipelines_for_project(&self, project_path: &Path) -> Vec<(String, usize, SessionState)> {
        let mut goal_map: HashMap<String, (usize, SessionState)> = HashMap::new();
        for s in self.pm.sessions() {
            if s.project_dir == project_path && s.state != SessionState::Dead {
                let goal = s.goal_display().to_string();
                let entry = goal_map.entry(goal).or_insert((0, s.state));
                entry.0 += 1;
                // Prioritize "worst" state: Waiting > Working > Idle
                if s.state == SessionState::Waiting {
                    entry.1 = SessionState::Waiting;
                }
            }
        }
        let mut pipelines: Vec<(String, usize, SessionState)> = goal_map
            .into_iter()
            .map(|(goal, (count, state))| (goal, count, state))
            .collect();
        pipelines.sort_by(|a, b| a.0.cmp(&b.0));
        pipelines
    }

    /// Check if a goal is already being worked on for a project.
    /// Returns the number of existing sessions with this goal.
    pub fn duplicate_goal_count(&self, project_path: &Path, goal: &str) -> usize {
        let goal_lower = goal.to_lowercase();
        self.pm.sessions().iter()
            .filter(|s| {
                s.project_dir == project_path
                    && s.state != SessionState::Dead
                    && s.goal_display().to_lowercase() == goal_lower
            })
            .count()
    }

    pub fn active_session_count(&self, project_path: &Path) -> usize {
        let managed = self.pm.sessions().iter()
            .filter(|s| s.project_dir == project_path && s.state != SessionState::Dead)
            .count();
        let external = self.external_sessions_for_project(project_path).len();
        managed + external
    }

    /// Spawn a Claude session as a tmux window in the orrch session.
    pub fn spawn_session(&mut self, project_dir: &Path, backend: BackendKind, goal: Option<&str>) -> Result<String> {
        // Check valve state — block spawn if provider valve is closed
        let provider = backend.provider_name();
        let (blocked, reason) = self.valve_store.check_provider(provider);
        if blocked {
            let msg = format!("{} blocked: {}", provider, reason);
            self.notify(msg.clone());
            return Err(anyhow::anyhow!(msg));
        }

        // Check throttle state
        if self.usage_tracker.is_throttled(provider) {
            let reason = self.usage_tracker.throttle_reason(provider).unwrap_or("rate limited");
            let msg = format!("{} throttled: {}", provider, reason);
            self.notify(msg.clone());
            return Err(anyhow::anyhow!(msg));
        }

        let backend_cmd = match self.pm.backends.get_command(backend) {
            Some(cmd) => cmd,
            None => {
                self.notify(format!("{} not available", backend.label()));
                return Err(anyhow::anyhow!("{} not available", backend.label()));
            }
        };

        let proj_name = project_dir.file_name().unwrap_or_default().to_string_lossy();
        let goal_display = goal.unwrap_or("continue development");
        // Sanitize session name: only alphanumeric, hyphens, underscores
        let session_name: String = format!("{}-{}", proj_name, goal_display.chars().take(25).collect::<String>())
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
            .collect();

        match orrch_core::windows::spawn_tmux_session(
            project_dir,
            &backend_cmd,
            goal,
            &session_name,
        ) {
            Ok(window_name) => {
                // Record usage event (JSONL persistence)
                let _ = self.usage_tracker.record(&UsageRecord {
                    timestamp: usage::iso_now(),
                    provider: backend.label().to_string(),
                    event: UsageEvent::SessionStart,
                    session_name: Some(window_name.clone()),
                    project: Some(proj_name.to_string()),
                    duration_secs: None,
                });
                // Record for throttle tracking (in-memory rolling window)
                self.usage_tracker.record_request(provider, 0);
                self.notify(format!("tmux:{} — {} {}", window_name, backend.label(), goal_display));
                Ok(format!("tmux:{window_name}"))
            }
            Err(e) => {
                self.notify(format!("Spawn failed: {e}"));
                Err(e)
            }
        }
    }

    pub fn notify(&mut self, msg: String) {
        self.last_notification = Some((msg, std::time::Instant::now()));
    }

    /// Initialize browser at a project root.
    pub fn browser_open(&mut self, root: &Path) {
        self.browser_root = root.to_path_buf();
        self.browser_path = root.to_path_buf();
        self.browser_parent_entries = orrch_core::list_directory(&self.browser_path);
        self.browser_parent_selected = 0;
        self.browser_in_child = false;
        self.browser_child_selected = 0;
        self.browser_refresh_child();
        self.browser_refresh_preview();
    }

    /// Refresh the child column based on parent selection.
    fn browser_refresh_child(&mut self) {
        if let Some(entry) = self.browser_parent_entries.get(self.browser_parent_selected) {
            if entry.is_dir {
                self.browser_child_entries = orrch_core::list_directory(&entry.path);
            } else {
                self.browser_child_entries.clear();
            }
        } else {
            self.browser_child_entries.clear();
        }
        self.browser_child_selected = 0;
    }

    /// Refresh the preview pane — shows file/dir details, not directory listings.
    fn browser_refresh_preview(&mut self) {
        let entry = if self.browser_in_child {
            self.browser_child_entries.get(self.browser_child_selected)
        } else {
            self.browser_parent_entries.get(self.browser_parent_selected)
        };

        let Some(entry) = entry else {
            self.browser_preview.clear();
            return;
        };

        if entry.is_dir {
            let children = orrch_core::list_directory(&entry.path);
            let dirs = children.iter().filter(|e| e.is_dir).count();
            let files = children.len() - dirs;
            self.browser_preview = format!(
                "{} {}\n\n  Type: Directory\n  Contents: {} dirs, {} files\n  Path: {}",
                entry.icon(), entry.name, dirs, files, entry.path.display()
            );
        } else if entry.is_editable {
            // Show file details + content preview
            let content = std::fs::read_to_string(&entry.path)
                .unwrap_or_else(|e| format!("(error: {e})"));
            let line_count = content.lines().count();
            let preview: String = content.lines().take(40).collect::<Vec<_>>().join("\n");
            self.browser_preview = format!(
                "{} {}\n\n  Type: {}\n  Size: {}\n  Lines: {}\n  Path: {}\n\n{}",
                entry.icon(), entry.name, entry.type_label(), entry.size_display(),
                line_count, entry.path.display(), preview
            );
        } else {
            // Binary/non-editable — metadata only
            self.browser_preview = format!(
                "{} {}\n\n  Type: {}\n  Size: {}\n  Path: {}",
                entry.icon(), entry.name, entry.type_label(), entry.size_display(),
                entry.path.display()
            );
        }
    }

    fn dep_refresh_child(&mut self) {
        if let Some(entry) = self.dep_parent_entries.get(self.dep_parent_selected) {
            if entry.is_dir {
                self.dep_child_entries = orrch_core::list_directory(&entry.path);
            } else {
                self.dep_child_entries.clear();
            }
        } else {
            self.dep_child_entries.clear();
        }
        self.dep_child_selected = 0;
    }

    fn dep_refresh_preview(&mut self) {
        let entry = if self.dep_in_child {
            self.dep_child_entries.get(self.dep_child_selected)
        } else {
            self.dep_parent_entries.get(self.dep_parent_selected)
        };
        let Some(entry) = entry else { self.dep_preview.clear(); return; };

        if entry.is_dir {
            let children = orrch_core::list_directory(&entry.path);
            let dirs = children.iter().filter(|e| e.is_dir).count();
            self.dep_preview = format!("{} {}\n\n  Type: Directory\n  Contents: {} dirs, {} files\n  Path: {}",
                entry.icon(), entry.name, dirs, children.len() - dirs, entry.path.display());
        } else if entry.is_editable {
            let content = std::fs::read_to_string(&entry.path).unwrap_or_default();
            let lines = content.lines().count();
            let preview: String = content.lines().take(40).collect::<Vec<_>>().join("\n");
            self.dep_preview = format!("{} {}\n\n  Type: {}\n  Size: {}\n  Lines: {}\n\n{}",
                entry.icon(), entry.name, entry.type_label(), entry.size_display(), lines, preview);
        } else {
            self.dep_preview = format!("{} {}\n\n  Type: {}\n  Size: {}",
                entry.icon(), entry.name, entry.type_label(), entry.size_display());
        }
    }

    // ─── Event Processing ─────────────────────────────────────────

    pub fn process_events(&mut self) {
        let mut events = Vec::new();
        while let Ok(ev) = self.event_rx.try_recv() {
            events.push(ev);
        }
        for event in events {
            match event {
                SessionEvent::Output { sid, data } => {
                    let project_dir = if let Some(session) = self.pm.get_session_mut(&sid) {
                        session.output_buffer.extend_from_slice(&data);
                        session.last_output_time = Some(std::time::Instant::now());
                        Some(session.project_dir.to_string_lossy().to_string())
                    } else { None };
                    if let Some(dir) = project_dir {
                        let text = String::from_utf8_lossy(&data);
                        let signal = analyze_output(&text);
                        if let Some(session) = self.pm.get_session_mut(&sid) {
                            let new_state = infer_state(session.last_output_time, &signal, 15.0);
                            if session.state != SessionState::Dead {
                                if new_state == SessionState::Waiting && session.state != SessionState::Waiting {
                                    let name = session.display_name().to_string();
                                    self.last_notification = Some((format!("⚠ {name} needs input"), std::time::Instant::now()));
                                }
                                session.state = new_state;
                            }
                        }
                        self.last_signals.insert(sid.clone(), signal);
                        self.analyze_retrospect(&sid, &dir, &data);
                    }
                }
                SessionEvent::Died { sid } => {
                    // Record usage before marking dead
                    if let Some(session) = self.pm.get_session(&sid) {
                        let duration = session.started_at.elapsed().as_secs_f64();
                        let _ = self.usage_tracker.record(&UsageRecord {
                            timestamp: usage::iso_now(),
                            provider: session.backend.label().to_string(),
                            event: UsageEvent::SessionEnd,
                            session_name: Some(sid.clone()),
                            project: Some(session.project_dir.file_name().unwrap_or_default().to_string_lossy().to_string()),
                            duration_secs: Some(duration),
                        });
                    }
                    if let Some(session) = self.pm.get_session_mut(&sid) {
                        session.state = SessionState::Dead;
                    }
                    if let Some(session) = self.pm.get_session(&sid) {
                        let dir_key = session.project_dir.to_string_lossy().to_string();
                        if let Some(tracker) = self.solution_trackers.get_mut(&dir_key) {
                            tracker.on_session_end(&sid);
                        }
                    }
                    self.last_signals.remove(&sid);
                }
            }
        }
        self.pm.reap_children();
    }

    fn analyze_retrospect(&mut self, sid: &str, project_dir: &str, data: &[u8]) {
        let text = String::from_utf8_lossy(data);
        let errors = orrch_retrospect::extract_errors(&text);
        let mut notifications = Vec::new();
        let mut injectable = Vec::new();
        if !errors.is_empty() {
            if let Some(store) = self.error_stores.get_mut(project_dir) {
                for error_text in &errors {
                    let fp = orrch_retrospect::fingerprint(error_text);
                    let category = orrch_retrospect::classify_error(error_text);
                    let known_fix = store.get_resolution(&fp).map(|s| s.to_string());
                    store.append(orrch_retrospect::ErrorRecord::new(fp.clone(), category, error_text.clone(), sid.into(), project_dir.into()));
                    if let Some(fix) = known_fix {
                        notifications.push(format!("Known {}: injecting", category.label()));
                        injectable.push((sid.to_string(), format!("\n[orrchestrator] Known error ({}). Fix:\n{}\n", category.label(), &fix[..fix.len().min(200)])));
                    } else {
                        notifications.push(format!("New error: {}", category.label()));
                    }
                }
            }
            if let Some(tracker) = self.solution_trackers.get_mut(project_dir) {
                for error_text in &errors { tracker.on_error(sid, &orrch_retrospect::fingerprint(error_text)); }
            }
            self.error_count += errors.len();
        } else if let (Some(store), Some(tracker)) = (self.error_stores.get_mut(project_dir), self.solution_trackers.get_mut(project_dir)) {
            let resolved = tracker.on_output(sid, &text, store);
            if !resolved.is_empty() { notifications.push(format!("{} resolved", resolved.len())); }
        }
        for (target, hint) in injectable { let _ = self.pm.write_to_session(&target, hint.as_bytes()); }
        if let Some(msg) = notifications.last() { self.last_notification = Some((msg.clone(), std::time::Instant::now())); }
    }

    // ─── Key Handling ─────────────────────────────────────────────

    /// Handle mouse scroll events.
    pub fn handle_scroll(&mut self, delta: i32) {
        match &self.sub {
            SubView::ExternalSessionView(_) => {
                if delta < 0 {
                    self.ext_log_scroll = self.ext_log_scroll.saturating_sub((-delta) as usize);
                } else {
                    self.ext_log_scroll += delta as usize;
                }
            }
            SubView::List => {
                // Scroll the project list
                if delta < 0 {
                    self.project_selected = self.project_selected.saturating_sub((-delta) as usize);
                } else {
                    let map = self.build_list_map();
                    let max = map.len().saturating_sub(1);
                    self.project_selected = (self.project_selected + delta as usize).min(max);
                }
            }
            SubView::ProjectDetail(pidx) => {
                match self.detail_focus {
                    DetailFocus::Roadmap => {
                        if delta < 0 {
                            self.roadmap_selected = self.roadmap_selected.saturating_sub((-delta) as usize);
                        } else {
                            self.roadmap_selected += delta as usize;
                        }
                    }
                    DetailFocus::Sessions => {
                        if delta < 0 {
                            self.session_selected = self.session_selected.saturating_sub((-delta) as usize);
                        } else {
                            self.session_selected += delta as usize;
                        }
                    }
                    DetailFocus::DevMap => {
                        let total = self.devmap_flat_count(*pidx);
                        if delta < 0 {
                            self.devmap_selected = self.devmap_selected.saturating_sub((-delta) as usize);
                        } else if total > 0 {
                            self.devmap_selected = (self.devmap_selected + delta as usize).min(total.saturating_sub(1));
                        }
                    }
                    DetailFocus::Browser => {
                        if self.browser_in_child {
                            if delta < 0 {
                                self.browser_child_selected = self.browser_child_selected.saturating_sub((-delta) as usize);
                            } else {
                                let max = self.browser_child_entries.len().saturating_sub(1);
                                self.browser_child_selected = (self.browser_child_selected + delta as usize).min(max);
                            }
                        } else {
                            if delta < 0 {
                                self.browser_parent_selected = self.browser_parent_selected.saturating_sub((-delta) as usize);
                            } else {
                                let max = self.browser_parent_entries.len().saturating_sub(1);
                                self.browser_parent_selected = (self.browser_parent_selected + delta as usize).min(max);
                            }
                            self.browser_refresh_child();
                        }
                        self.browser_refresh_preview();
                    }
                }
            }
            _ => {
                // Generic scroll for Design sub-panels
                match self.panel {
                    Panel::Design => match self.design_sub {
                        DesignSub::Workforce => {
                            let count = self.wf_items_for_tab().len();
                            if delta < 0 {
                                self.wf_selected = self.wf_selected.saturating_sub((-delta) as usize);
                            } else {
                                self.wf_selected = (self.wf_selected + delta as usize).min(count.saturating_sub(1));
                            }
                            self.wf_preview_scroll = 0;
                        }
                        DesignSub::Library => {
                            let count = self.library_item_count();
                            if delta < 0 {
                                self.library_selected = self.library_selected.saturating_sub((-delta) as usize);
                            } else {
                                self.library_selected = (self.library_selected + delta as usize).min(count.saturating_sub(1));
                            }
                            self.library_preview_scroll = 0;
                        }
                        DesignSub::Intentions => {
                            if delta < 0 {
                                self.idea_selected = self.idea_selected.saturating_sub((-delta) as usize);
                            } else {
                                self.idea_selected = (self.idea_selected + delta as usize).min(self.ideas.len().saturating_sub(1));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        // Shift+Left/Right always switches panels (from any list view), landing at content depth
        if self.sub == SubView::List && modifiers.contains(KeyModifiers::SHIFT) {
            match code {
                KeyCode::Left => { self.panel = self.panel.prev(); self.focus_depth = self.content_depth(); return Ok(()); }
                KeyCode::Right => { self.panel = self.panel.next(); self.focus_depth = self.content_depth(); return Ok(()); }
                _ => {}
            }
        }

        // Intake review Esc: close review without making a decision
        if code == KeyCode::Esc && self.intake_review.is_some() && self.sub == SubView::List {
            self.intake_review = None;
            self.intake_review_scroll_raw = 0;
            self.intake_review_scroll_opt = 0;
            return Ok(());
        }

        // Global Esc: from List views opens app menu, from app menu closes it
        if code == KeyCode::Esc {
            match &self.sub {
                SubView::List => {
                    if !self.tree_browsing {
                        self.app_menu_selected = 0;
                        self.sub = SubView::AppMenu;
                        return Ok(());
                    }
                    // tree_browsing Esc is handled in key_inside_project
                }
                SubView::AppMenu => {
                    self.sub = SubView::List;
                    return Ok(());
                }
                _ => {} // other subviews handle Esc themselves
            }
        }

        // Normalize vim navigation keys to arrows (except in text inputs)
        let typing_text = matches!(self.sub, SubView::SpawnGoal | SubView::NewProjectName | SubView::AddFeature(_) | SubView::AddMcpServer) || self.commit_typing_correction;
        let key = if !typing_text {
            match code {
                KeyCode::Char('j') => KeyCode::Down,
                KeyCode::Char('k') => KeyCode::Up,
                KeyCode::Char('h') => KeyCode::Left,
                KeyCode::Char('l') => KeyCode::Right,
                other => other,
            }
        } else {
            code
        };

        // Multi-level focus navigation: each tab bar is a navigable level
        if self.sub == SubView::List && self.focus_depth < self.content_depth() {
            let handled = match self.focus_depth {
                // Level 0: Panel bar (Design/Oversee/Hypervise/Analyze/Publish)
                0 => match key {
                    KeyCode::Left => { self.panel = self.panel.prev(); true }
                    KeyCode::Right => { self.panel = self.panel.next(); true }
                    KeyCode::Down | KeyCode::Enter => { self.focus_depth = 1; true }
                    KeyCode::Char('q') => { self.should_quit = true; true }
                    _ => false
                },
                // Level 1: Design sub-bar (Intentions/Workforce/Library)
                1 if self.panel == Panel::Design => match key {
                    KeyCode::Left => { self.design_sub = self.design_sub.prev(); true }
                    KeyCode::Right => { self.design_sub = self.design_sub.next(); true }
                    KeyCode::Up => { self.focus_depth = 0; true }
                    KeyCode::Down | KeyCode::Enter => {
                        // Drop to next level (sub-sub bar or content, depending on design_sub)
                        self.focus_depth = 2;
                        true
                    }
                    KeyCode::Char('q') => { self.should_quit = true; true }
                    _ => false
                },
                // Level 2: Workforce sub-tabs or Library sub-tabs
                2 if self.panel == Panel::Design => match self.design_sub {
                    DesignSub::Workforce => match key {
                        KeyCode::Left => {
                            self.workforce_tab = self.workforce_tab.prev();
                            self.wf_selected = 0; self.wf_preview_scroll = 0; true
                        }
                        KeyCode::Right => {
                            self.workforce_tab = self.workforce_tab.next();
                            self.wf_selected = 0; self.wf_preview_scroll = 0; true
                        }
                        KeyCode::Up => { self.focus_depth = 1; true }
                        KeyCode::Down | KeyCode::Enter => { self.focus_depth = 3; true }
                        KeyCode::Char('q') => { self.should_quit = true; true }
                        _ => false
                    },
                    DesignSub::Library => match key {
                        KeyCode::Left => {
                            self.library_sub = self.library_sub.prev();
                            self.library_selected = 0; self.library_preview_scroll = 0; true
                        }
                        KeyCode::Right => {
                            self.library_sub = self.library_sub.next();
                            self.library_selected = 0; self.library_preview_scroll = 0; true
                        }
                        KeyCode::Up => { self.focus_depth = 1; true }
                        KeyCode::Down | KeyCode::Enter => { self.focus_depth = 3; true }
                        KeyCode::Char('q') => { self.should_quit = true; true }
                        _ => false
                    },
                    _ => false // Intentions has no level 2 bar
                },
                _ => false
            };
            if handled {
                return Ok(());
            }
            // Unhandled key at a bar level: drop to content and process it there
            self.focus_depth = self.content_depth();
        }

        match &self.sub {
            SubView::List => match self.panel {
                Panel::Design => self.key_design(key),
                Panel::Oversee => self.key_projects(key),
                Panel::Hypervise => self.key_sessions_tab(key),
                Panel::Analyze => self.key_placeholder(key),
                Panel::Publish => self.key_placeholder(key),
            },
            SubView::ProjectDetail(_) => self.key_project_detail(key),
            SubView::SessionFocus(_) => self.key_session_focus(key),
            SubView::ExternalSessionView(_) => self.key_external_session_view(key),
            SubView::SpawnGoal => self.key_spawn_goal(key, modifiers),
            SubView::SpawnWorkforce => self.key_spawn_workforce(key),
            SubView::SpawnAgent => self.key_spawn_agent(key),
            SubView::SpawnBackend => self.key_spawn_backend(key),
            SubView::SpawnHost => self.key_spawn_host(key),
            SubView::RoutingSummary => self.key_routing_summary(key),
            SubView::ConfirmDeprecate(idx) => {
                let idx = *idx;
                self.key_confirm_deprecate(key, idx)
            }
            SubView::ConfirmComplete(idx) => {
                let idx = *idx;
                self.key_confirm_complete(key, idx)
            }
            SubView::ConfirmDeleteFeedback(idx) => {
                let idx = *idx;
                self.key_confirm_delete_feedback(key, idx)
            }
            SubView::DeprecatedBrowser => self.key_deprecated_browser(key),
            SubView::AppMenu => self.key_app_menu(key),
            SubView::ActionMenu => self.key_action_menu(key),
            SubView::ConfirmDeleteDeprecated => self.key_confirm_delete_deprecated(key),
            SubView::CommitReview(idx) => { let idx = *idx; self.key_commit_review(key, idx) }
            SubView::CommitCorrecting(idx) => { let idx = *idx; self.key_commit_correcting(key, idx) }
            SubView::NewProjectName => self.key_new_project_name(key),
            SubView::NewProjectScope => self.key_new_project_scope(key),
            SubView::NewProjectConfirm => self.key_new_project_confirm(key),
            SubView::FeedbackConfirm(idx) => {
                let idx = *idx;
                self.key_feedback_confirm(key, idx)
            }
            SubView::WorkflowPicker => self.key_workflow_picker(key),
            SubView::AddFeature(pidx) => { let pidx = *pidx; self.key_add_feature(key, pidx) }
            SubView::AddMcpServer => self.key_add_mcp_server(key),
        }
    }

    fn key_deprecated_browser(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => { self.sub = SubView::List; return Ok(()); }
            KeyCode::Up => {
                if self.dep_in_child {
                    self.dep_child_selected = self.dep_child_selected.saturating_sub(1);
                } else if self.dep_parent_selected == 0 {
                    self.sub = SubView::List;
                    return Ok(());
                } else {
                    self.dep_parent_selected -= 1;
                    self.dep_refresh_child();
                }
                self.dep_refresh_preview();
            }
            KeyCode::Down => {
                if self.dep_in_child {
                    if !self.dep_child_entries.is_empty() && self.dep_child_selected < self.dep_child_entries.len() - 1 {
                        self.dep_child_selected += 1;
                    }
                } else if !self.dep_parent_entries.is_empty() && self.dep_parent_selected < self.dep_parent_entries.len() - 1 {
                    self.dep_parent_selected += 1;
                    self.dep_refresh_child();
                }
                self.dep_refresh_preview();
            }
            KeyCode::Right => {
                if self.dep_in_child {
                    if let Some(entry) = self.dep_child_entries.get(self.dep_child_selected).cloned() {
                        if entry.is_dir {
                            self.dep_path = entry.path;
                            self.dep_parent_entries = orrch_core::list_directory(&self.dep_path);
                            self.dep_parent_selected = 0;
                            self.dep_in_child = false;
                            self.dep_refresh_child();
                        }
                    }
                } else if !self.dep_child_entries.is_empty() {
                    self.dep_in_child = true;
                }
                self.dep_refresh_preview();
            }
            KeyCode::Left => {
                if self.dep_in_child {
                    self.dep_in_child = false;
                } else if self.dep_path != self.dep_root {
                    if let Some(parent) = self.dep_path.parent() {
                        self.dep_path = parent.to_path_buf();
                        self.dep_parent_entries = orrch_core::list_directory(&self.dep_path);
                        self.dep_parent_selected = 0;
                        self.dep_in_child = false;
                        self.dep_refresh_child();
                    }
                }
                self.dep_refresh_preview();
            }
            KeyCode::Enter => {
                let entry = if self.dep_in_child {
                    self.dep_child_entries.get(self.dep_child_selected).cloned()
                } else {
                    self.dep_parent_entries.get(self.dep_parent_selected).cloned()
                };
                if let Some(entry) = entry {
                    if entry.is_dir {
                        self.dep_path = entry.path;
                        self.dep_parent_entries = orrch_core::list_directory(&self.dep_path);
                        self.dep_parent_selected = 0;
                        self.dep_in_child = false;
                        self.dep_refresh_child();
                        self.dep_refresh_preview();
                    } else if entry.is_editable {
                        // Full file view in preview (read-only for deprecated)
                        self.dep_preview = std::fs::read_to_string(&entry.path).unwrap_or_default();
                    }
                }
            }
            KeyCode::Char('d') => {
                // Delete a deprecated project (only at top level of deprecated/)
                if !self.dep_in_child && self.dep_path == self.dep_root {
                    if let Some(entry) = self.dep_parent_entries.get(self.dep_parent_selected) {
                        if entry.is_dir {
                            self.sub = SubView::ConfirmDeleteDeprecated;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_confirm_delete_deprecated(&mut self, key: KeyCode) -> Result<()> {
        if key == KeyCode::Char('y') || key == KeyCode::Char('Y') {
            if let Some(entry) = self.dep_parent_entries.get(self.dep_parent_selected) {
                let name = entry.name.clone();
                let path = entry.path.clone();
                match std::fs::remove_dir_all(&path) {
                    Ok(()) => {
                        self.dep_parent_entries = orrch_core::list_directory(&self.dep_root);
                        self.dep_parent_selected = self.dep_parent_selected.min(self.dep_parent_entries.len().saturating_sub(1));
                        self.dep_refresh_child();
                        self.dep_refresh_preview();
                        self.notify(format!("Permanently deleted deprecated/{name}"));
                    }
                    Err(e) => self.notify(format!("Delete failed: {e}")),
                }
            }
        }
        self.sub = SubView::DeprecatedBrowser;
        Ok(())
    }

    fn key_design(&mut self, key: KeyCode) -> Result<()> {
        // Bar-level navigation (Left/Right for sub-tabs) is handled in handle_key focus dispatch.
        // This only runs at content depth.
        match self.design_sub {
            DesignSub::Intentions => self.key_design_project(key),
            DesignSub::Workforce => self.key_design_workforce(key),
            DesignSub::Library => self.key_library(key),
        }
    }

    fn key_design_project(&mut self, key: KeyCode) -> Result<()> {
        // Project Design merges Ideas + Feedback functionality.
        // For now, delegate to the ideas view (feedback items are also accessible via 'f').
        self.key_ideas(key)
    }

    fn key_design_workforce(&mut self, key: KeyCode) -> Result<()> {
        // At content level: Left/Right switches workforce sub-tabs
        let items = self.wf_items_for_tab();
        let count = items.len();

        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Left => {
                self.workforce_tab = self.workforce_tab.prev();
                self.wf_selected = 0; self.wf_preview_scroll = 0;
            }
            KeyCode::Right => {
                self.workforce_tab = self.workforce_tab.next();
                self.wf_selected = 0; self.wf_preview_scroll = 0;
            }
            KeyCode::Up => {
                if self.wf_selected == 0 { self.focus_depth = self.content_depth() - 1; }
                else { self.wf_selected -= 1; self.wf_preview_scroll = 0; }
            }
            KeyCode::Down => {
                if count > 0 && self.wf_selected < count - 1 {
                    self.wf_selected += 1;
                    self.wf_preview_scroll = 0;
                }
            }
            KeyCode::PageDown => { self.wf_preview_scroll += 10; }
            KeyCode::PageUp => { self.wf_preview_scroll = self.wf_preview_scroll.saturating_sub(10); }
            KeyCode::Char('n') => {
                // Harnesses: editor stub — edit files directly
                if self.workforce_tab == WorkforceTab::Harnesses {
                    self.notify("Harness editor coming soon — edit .md directly in library/harnesses/".into());
                    return Ok(());
                }
                // New from template for current tab
                use orrch_library::templates::{TemplateCategory, create_from_template};
                let category = match self.workforce_tab {
                    WorkforceTab::Harnesses => unreachable!(),
                    WorkforceTab::Workflows => TemplateCategory::Workforce,
                    WorkforceTab::Teams => TemplateCategory::Operation,
                    WorkforceTab::Agents => TemplateCategory::Agent,
                    WorkforceTab::Skills => TemplateCategory::Skill,
                    WorkforceTab::Tools => TemplateCategory::Tool,
                    WorkforceTab::McpServers => TemplateCategory::McpServer,
                    WorkforceTab::Profiles => TemplateCategory::Agent, // reuse agent template for profiles
                    WorkforceTab::TrainingData | WorkforceTab::Models => {
                        self.notify("Coming soon".into());
                        return Ok(());
                    }
                };
                let orrch_dir = self.projects_dir.join("orrchestrator");
                match create_from_template(category, &orrch_dir) {
                    Ok(path) => {
                        let title = format!("[new {}]", self.workforce_tab.label());
                        self.vim_request = Some(VimRequest {
                            file: path,
                            kind: VimKind::NewIdea,
                            title,
                        });
                    }
                    Err(e) => self.notify(format!("Failed: {e}")),
                }
            }
            KeyCode::Char('d') => {
                if self.workforce_tab == WorkforceTab::Harnesses {
                    self.notify("Harness editor coming soon — edit .md directly in library/harnesses/".into());
                    return Ok(());
                }
                let items = self.wf_items_for_tab();
                if let Some((_, path)) = items.get(self.wf_selected) {
                    let p = path.clone();
                    let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                    if std::fs::remove_file(&p).is_ok() {
                        self.notify(format!("Deleted {name}"));
                        self.reload_all_library_data();
                    }
                }
            }
            KeyCode::Enter => {
                if self.workforce_tab == WorkforceTab::Harnesses {
                    self.notify("Harness editor coming soon — edit .md directly in library/harnesses/".into());
                    return Ok(());
                }
                let items = self.wf_items_for_tab();
                if let Some((_, path)) = items.get(self.wf_selected) {
                    let p = path.clone();
                    let title = format!("[{}] {}", self.workforce_tab.label(), p.file_name().unwrap_or_default().to_string_lossy());
                    self.vim_request = Some(VimRequest {
                        file: p,
                        kind: VimKind::NewIdea,
                        title,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Get the list of (name, path) items for the current workforce tab.
    pub fn wf_items_for_tab(&self) -> Vec<(String, PathBuf)> {
        match self.workforce_tab {
            WorkforceTab::Harnesses => self.library_harnesses.iter().map(|h| (h.name.clone(), h.path.clone())).collect(),
            WorkforceTab::Workflows => self.workforce_files.clone(),
            WorkforceTab::Teams => self.operation_files.clone(),
            WorkforceTab::Agents => self.agent_profiles.iter().map(|a| (a.name.clone(), a.path.clone())).collect(),
            WorkforceTab::Skills => self.library_skills.clone(),
            WorkforceTab::Tools => self.library_tools.clone(),
            WorkforceTab::McpServers => self.library_mcp_servers.iter().map(|s| (s.name.clone(), s.path.clone())).collect(),
            WorkforceTab::Profiles => self.library_profiles.clone(),
            WorkforceTab::TrainingData | WorkforceTab::Models => Vec::new(),
        }
    }

    /// Reload all library/workforce data from disk.
    fn reload_all_library_data(&mut self) {
        let orrch_dir = self.projects_dir.join("orrchestrator");
        let library_root = orrch_dir.join("library");
        self.agent_profiles = load_agents(&agents_dir());
        self.library_models = orrch_library::load_models(&library_root.join("models"));
        self.library_harnesses = orrch_library::load_harnesses(&library_root.join("harnesses"));
        self.library_mcp_servers = orrch_library::load_mcp_servers(&library_root.join("mcp_servers"));
        self.library_skills = scan_md_dir(&library_root.join("skills"));
        self.library_tools = scan_md_dir(&library_root.join("tools"));
        self.library_profiles = scan_md_dir(&library_root.join("profiles"));
        self.workforce_files = scan_md_dir(&orrch_dir.join("workforces"));
        self.operation_files = scan_md_dir(&orrch_dir.join("operations"));
    }

    fn key_library(&mut self, key: KeyCode) -> Result<()> {
        // Library is a read-only browser within Design. Left/Right switches sub-tabs at content level.
        // Valve controls (v/V) and MCP toggle (e) are operational, not content editing.
        let count = self.library_item_count();
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Left => {
                self.library_sub = self.library_sub.prev();
                self.library_selected = 0;
                self.library_preview_scroll = 0;
            }
            KeyCode::Right => {
                self.library_sub = self.library_sub.next();
                self.library_selected = 0;
                self.library_preview_scroll = 0;
            }
            KeyCode::Up => {
                if self.library_selected == 0 { self.focus_depth = self.content_depth() - 1; }
                else { self.library_selected -= 1; self.library_preview_scroll = 0; }
            }
            KeyCode::Down => {
                if count > 0 && self.library_selected < count - 1 {
                    self.library_selected += 1;
                    self.library_preview_scroll = 0;
                }
            }
            KeyCode::PageDown => { self.library_preview_scroll += 10; }
            KeyCode::PageUp => { self.library_preview_scroll = self.library_preview_scroll.saturating_sub(10); }
            KeyCode::Char('v') if self.library_sub == LibrarySub::Models => {
                if let Some(model) = self.library_models.get(self.library_selected) {
                    let provider = model.provider.clone();
                    if self.valve_store.is_blocked(&provider) {
                        self.valve_store.open(&provider);
                        self.notify(format!("Valve OPENED: {}", provider));
                    } else {
                        self.valve_store.close(&provider, "manual shutoff", None);
                        self.notify(format!("Valve CLOSED: {}", provider));
                    }
                }
            }
            KeyCode::Char('V') if self.library_sub == LibrarySub::Models => {
                if let Some(model) = self.library_models.get(self.library_selected) {
                    let provider = model.provider.clone();
                    // Close until next Friday at 00:00 UTC (billing cycle reset)
                    self.valve_store.close_until_next_weekday(&provider, 5, 0); // 5 = Friday
                    let valve = self.valve_store.valves.get(&provider);
                    let display = valve.map(|v| v.reopen_display()).unwrap_or_else(|| "?".into());
                    self.notify(format!("Valve CLOSED: {} — reopens {}", provider, display));
                }
            }
            KeyCode::Char('e') if self.library_sub == LibrarySub::McpServers => {
                let idx = self.library_selected;
                if idx < self.library_mcp_servers.len() {
                    self.library_mcp_servers[idx].enabled = !self.library_mcp_servers[idx].enabled;
                    let name = self.library_mcp_servers[idx].name.clone();
                    let status = if self.library_mcp_servers[idx].enabled { "enabled" } else { "disabled" };
                    self.notify(format!("{} {}", name, status));
                }
            }
            // Task 62: Register new external MCP server
            KeyCode::Char('n') if self.library_sub == LibrarySub::McpServers => {
                self.add_mcp_name.clear();
                self.add_mcp_desc.clear();
                self.add_mcp_transport = 0;
                self.add_mcp_command.clear();
                self.add_mcp_args.clear();
                self.add_mcp_roles.clear();
                self.add_mcp_field = 0;
                self.sub = SubView::AddMcpServer;
            }
            _ => {}
        }
        Ok(())
    }

    /// Reload all library data from disk (after create/delete/edit).
    fn reload_library(&mut self) {
        self.reload_all_library_data();
        let count = self.library_item_count();
        self.library_selected = self.library_selected.min(count.saturating_sub(1));
    }

    fn library_item_count(&self) -> usize {
        match self.library_sub {
            LibrarySub::Agents => self.agent_profiles.len(),
            LibrarySub::Models => self.library_models.len(),
            LibrarySub::Harnesses => self.library_harnesses.len(),
            LibrarySub::McpServers => self.library_mcp_servers.len(),
            LibrarySub::Skills => self.library_skills.len(),
            LibrarySub::Tools => self.library_tools.len(),
        }
    }

    fn key_placeholder(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Up => { self.focus_depth = 0; }
            _ => {}
        }
        Ok(())
    }

    fn key_ideas(&mut self, key: KeyCode) -> Result<()> {
        // Intake review mode takes over the Intentions panel
        if self.intake_review.is_some() {
            return self.key_intake_review(key);
        }
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('n') => {
                self.request_vim(VimKind::NewIdea);
            }
            KeyCode::Enter => {
                // Open selected idea in vim for editing
                if let Some(idea) = self.ideas.get(self.idea_selected) {
                    let path = idea.path.clone();
                    let title = format!("[intentions] {}", idea.title);
                    self.vim_request = Some(VimRequest {
                        file: path,
                        kind: VimKind::NewIdea,
                        title,
                    });
                }
            }
            KeyCode::Char('s') => {
                // Submit selected idea to instruction intake pipeline
                if let Some(idea) = self.ideas.get(self.idea_selected) {
                    if idea.pipeline.is_submitted() {
                        self.notify(format!("Already submitted ({}%)", idea.pipeline.progress));
                    } else {
                        let vault = orrch_core::vault::vault_dir(&self.projects_dir);
                        let idea_path = idea.path.clone();
                        let idea_title = idea.title.clone();
                        match orrch_core::vault::submit_to_pipeline(&vault, idea) {
                            Ok(_) => {
                                // Spawn a session that calls the instruction_intake MCP tool
                                let project_dir = self.projects_dir.join("orrchestrator");
                                let goal = format!("Call the instruction_intake tool with file_path: {}", idea_path.display());
                                let _ = self.spawn_session(&project_dir, BackendKind::Claude, Some(&goal));
                                self.notify(format!("Pipeline started: {}", idea_title));
                                self.ideas = orrch_core::vault::load_ideas(&vault);
                            }
                            Err(e) => self.notify(format!("Submit failed: {e}")),
                        }
                    }
                }
            }
            KeyCode::Char('r') => {
                // Review selected idea — show raw idea (left) vs its derived instructions (right)
                if let Some(idea) = self.ideas.get(self.idea_selected) {
                    let raw = std::fs::read_to_string(&idea.path).unwrap_or_default();
                    let inbox_path = self.projects_dir.join("orrchestrator").join("instructions_inbox.md");
                    let optimized = if let Ok(inbox) = std::fs::read_to_string(&inbox_path) {
                        // Find instructions sourced from this idea by matching filename
                        let source_marker = format!("plans/{}", idea.filename);
                        if inbox.contains(&source_marker) {
                            // Extract the package block for this source
                            let mut in_block = false;
                            let mut block = String::new();
                            for line in inbox.lines() {
                                if line.contains(&source_marker) {
                                    in_block = true;
                                }
                                if in_block {
                                    block.push_str(line);
                                    block.push('\n');
                                }
                            }
                            if block.is_empty() { format!("(no instructions found for {})", idea.filename) } else { block }
                        } else {
                            format!("(no instructions found for {})", idea.filename)
                        }
                    } else {
                        "(no instructions_inbox.md found)".into()
                    };
                    self.intake_review = Some(orrch_core::IntakeReview {
                        raw,
                        optimized,
                        source_project: self.projects_dir.join("orrchestrator"),
                        source_path: inbox_path,
                    });
                    self.intake_review_scroll_raw = 0;
                    self.intake_review_scroll_opt = 0;
                    self.intake_review_focus = IntakeReviewFocus::Raw;
                }
            }
            KeyCode::Char('d') => {
                // Delete only if duplicate (warn otherwise)
                if let Some(idea) = self.ideas.get(self.idea_selected) {
                    if idea.pipeline.is_submitted() {
                        self.notify("Cannot delete: already in pipeline. Use only for duplicates.".into());
                    } else {
                        let path = idea.path.clone();
                        let _ = std::fs::remove_file(&path);
                        let vault = orrch_core::vault::vault_dir(&self.projects_dir);
                        self.ideas = orrch_core::vault::load_ideas(&vault);
                        self.idea_selected = self.idea_selected.min(self.ideas.len().saturating_sub(1));
                        self.notify("Deleted (duplicate cleanup)".into());
                    }
                }
            }
            KeyCode::Char('i') => {
                // Toggle audit trail expansion for the selected idea
                if self.ideas_audit_expanded == Some(self.idea_selected) {
                    self.ideas_audit_expanded = None;
                } else {
                    self.ideas_audit_expanded = Some(self.idea_selected);
                }
            }
            KeyCode::Esc => {
                if self.ideas_audit_expanded.is_some() {
                    self.ideas_audit_expanded = None;
                }
            }
            KeyCode::Up => {
                if self.idea_selected == 0 { self.focus_depth = self.content_depth() - 1; }
                else { self.idea_selected -= 1; }
            }
            KeyCode::Down => {
                if !self.ideas.is_empty() && self.idea_selected < self.ideas.len() - 1 {
                    self.idea_selected += 1;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_intake_review(&mut self, key: KeyCode) -> Result<()> {
        // Esc is handled in handle_key before global dispatch
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab => {
                self.intake_review_focus = match self.intake_review_focus {
                    IntakeReviewFocus::Raw => IntakeReviewFocus::Optimized,
                    IntakeReviewFocus::Optimized => IntakeReviewFocus::Raw,
                };
            }
            KeyCode::Up => {
                match self.intake_review_focus {
                    IntakeReviewFocus::Raw => self.intake_review_scroll_raw = self.intake_review_scroll_raw.saturating_sub(1),
                    IntakeReviewFocus::Optimized => self.intake_review_scroll_opt = self.intake_review_scroll_opt.saturating_sub(1),
                }
            }
            KeyCode::Down => {
                match self.intake_review_focus {
                    IntakeReviewFocus::Raw => self.intake_review_scroll_raw += 1,
                    IntakeReviewFocus::Optimized => self.intake_review_scroll_opt += 1,
                }
            }
            KeyCode::Char('e') => {
                // Edit optimized text in vim
                if let Some(review) = &self.intake_review {
                    let edit_path = review.source_project.join(".orrch").join("intake_optimized_edit.md");
                    let _ = std::fs::create_dir_all(edit_path.parent().unwrap());
                    let _ = std::fs::write(&edit_path, &review.optimized);
                    self.vim_request = Some(VimRequest {
                        file: edit_path,
                        kind: VimKind::IntakeReview,
                        title: "[Intake Review] Edit optimized instructions".to_string(),
                    });
                }
            }
            KeyCode::Char('y') => {
                // Confirm review
                if let Some(review) = self.intake_review.take() {
                    match orrch_core::write_intake_decision(&review, "confirmed", &review.optimized) {
                        Ok(_) => self.notify("Intake review confirmed — instructions will be distributed".into()),
                        Err(e) => self.notify(format!("Failed to confirm: {e}")),
                    }
                    self.intake_review_scroll_raw = 0;
                    self.intake_review_scroll_opt = 0;
                }
            }
            KeyCode::Char('N') => {
                // Reject review (capital N to avoid accidental rejection)
                if let Some(review) = self.intake_review.take() {
                    match orrch_core::write_intake_decision(&review, "rejected", &review.optimized) {
                        Ok(_) => self.notify("Intake review rejected".into()),
                        Err(e) => self.notify(format!("Failed to reject: {e}")),
                    }
                    self.intake_review_scroll_raw = 0;
                    self.intake_review_scroll_opt = 0;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_projects(&mut self, key: KeyCode) -> Result<()> {
        self.categorize_projects();

        // If we're inside a project (browsing sessions + files), handle that first
        if self.tree_browsing {
            if let Some(pidx) = self.tree_project {
                return self.key_inside_project(key, pidx);
            }
        }

        let count = self.projects.len() + self.facilities.iter().map(|f| 1 + f.sub_projects.len()).sum::<usize>();
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('n') => {
                if let Some(pidx) = self.selected_project_index() {
                    self.spawn_project_idx = pidx;
                } else if !self.projects.is_empty() {
                    self.spawn_project_idx = 0;
                }
                self.spawn_goal_text.clear();
                self.spawn_goal_from_roadmap = None;
                self.spawn_agent_idx = 0;
                self.spawn_workforce_idx = 0;
                self.sub = SubView::SpawnGoal;
            }
            KeyCode::Char('N') => {
                // Multi-spawn: spawn a session for each open roadmap item
                if let Some(pidx) = self.selected_project_index() {
                    if let Some(proj) = self.projects.get(pidx) {
                        let open_items: Vec<String> = proj.open_roadmap_items()
                            .iter()
                            .map(|item| item.title.clone())
                            .collect();
                        let path = proj.path.clone();
                        let name = proj.name.clone();
                        let count = open_items.len();
                        if count == 0 {
                            self.notify(format!("{name}: no open roadmap items"));
                        } else {
                            let mut spawned = 0;
                            for goal in open_items {
                                if self.spawn_session(&path, BackendKind::Claude, Some(&goal)).is_ok() {
                                    spawned += 1;
                                }
                            }
                            self.notify(format!("{name}: spawned {spawned}/{count} parallel pipelines"));
                        }
                    }
                }
            }
            KeyCode::Char('P') => {
                // New project wizard
                self.new_project_name.clear();
                self.new_project_scope = orrch_core::Scope::Private;
                self.new_project_temp = Temperature::Hot;
                self.new_project_error = None;
                self.sub = SubView::NewProjectName;
            }
            KeyCode::Char('a') => {
                self.open_action_menu();
            }
            KeyCode::Char('g') => {
                // Quick git commit+push for selected project
                if let Some(pidx) = self.selected_project_index() {
                    let (name, path) = {
                        let proj = &self.projects[pidx];
                        (proj.name.clone(), proj.path.clone())
                    };
                    let status = orrch_core::git::check_status(&path);
                    if !status.has_repo {
                        self.notify(format!("{name}: no git repo"));
                    } else if status.dirty_count == 0 && status.unpushed == 0 {
                        self.notify(format!("{name}: clean"));
                    } else {
                        match orrch_core::git::spawn_commit_session(&path, &name) {
                            Ok(session) => self.notify(format!("{name}: committing ({session})")),
                            Err(e) => self.notify(format!("{name}: {e}")),
                        }
                    }
                }
            }
            KeyCode::Char('r') => { self.reload_projects(); self.notify("Reloaded".into()); }
            KeyCode::Char('f') => {
                self.request_vim(VimKind::GlobalFeedback);
            }
            KeyCode::Char('t') => {
                if let Some(pidx) = self.selected_project_index() {
                    let msg = if let Some(proj) = self.projects.get_mut(pidx) {
                        proj.color_tag = proj.color_tag.cycle();
                        proj.save_color_tag();
                        let tag = if proj.color_tag == ColorTag::None { "none" } else { proj.color_tag.label() };
                        Some(format!("{}: {tag}", proj.name))
                    } else { None };
                    if let Some(m) = msg { self.notify(m); }
                }
            }
            KeyCode::Tab => {
                // Tab now does nothing — use → to enter projects
            }
            KeyCode::Char('D') => {
                if let Some(pidx) = self.selected_project_index() {
                    self.sub = SubView::ConfirmDeprecate(pidx);
                }
            }
            KeyCode::Char('C') => {
                if let Some(pidx) = self.selected_project_index() {
                    self.sub = SubView::ConfirmComplete(pidx);
                }
            }
            KeyCode::Char('w') => {
                // Open workflow picker
                self.workflow_choices = orrch_core::windows::list_workflows();
                if self.workflow_choices.is_empty() {
                    self.notify("No workflow scripts found (run_*.sh in library/tools/)".into());
                } else {
                    self.workflow_picker_idx = 0;
                    self.sub = SubView::WorkflowPicker;
                }
            }
            KeyCode::Char('x') => {
                // Remove a production entry (mark as non-release)
                let map = self.build_list_map();
                if let Some(ListEntry::ProductionVersion(vidx)) = map.get(self.project_selected) {
                    let vidx = *vidx;
                    if let Some(v) = self.production_versions.get(vidx) {
                        // Write a marker file to exclude from production
                        let marker = v.path.join(".orrnorelease");
                        let _ = std::fs::write(&marker, "removed from production");
                        let name = format!("{} {}", v.project_name, v.version);
                        self.production_versions.remove(vidx);
                        self.notify(format!("{name} removed from production"));
                    }
                }
            }
            KeyCode::Char('s') => {
                // Toggle hot/cold
                if let Some(pidx) = self.selected_project_index() {
                    if let Some(proj) = self.projects.get_mut(pidx) {
                        proj.temperature = match proj.temperature {
                            Temperature::Hot => Temperature::Cold,
                            Temperature::Cold => Temperature::Hot,
                            Temperature::Ignored => Temperature::Cold,
                        };
                        proj.save_temperature();
                        let label = proj.temperature.label();
                        let name = proj.name.clone();
                        self.categorize_projects();
                        self.notify(format!("{name} → {label}"));
                    }
                }
            }
            KeyCode::Char('S') => {
                // Cycle scope: personal → private → public → commercial
                if let Some(pidx) = self.selected_project_index() {
                    let msg = if let Some(proj) = self.projects.get_mut(pidx) {
                        proj.scope = proj.scope.cycle();
                        proj.save_scope();
                        Some(format!("{}: [{}]", proj.name, proj.scope.label()))
                    } else { None };
                    if let Some(m) = msg { self.notify(m); }
                }
            }
            KeyCode::Char('i') => {
                // Ignore (only for cold projects)
                if let Some(pidx) = self.selected_project_index() {
                    if let Some(proj) = self.projects.get_mut(pidx) {
                        if proj.temperature == Temperature::Cold {
                            proj.temperature = Temperature::Ignored;
                            proj.save_temperature();
                            let name = proj.name.clone();
                            self.categorize_projects();
                            self.notify(format!("{name} → ignored"));
                        }
                    }
                }
            }
            KeyCode::Enter => {
                let map = self.build_list_map();
                if let Some(entry) = map.get(self.project_selected) {
                    match entry {
                        ListEntry::Project(idx) => {
                            let idx = *idx;
                            self.session_selected = 0;
                            self.roadmap_selected = 0;
                            // Default focus to roadmap if it has items, else sessions
                            self.detail_focus = if self.projects.get(idx).map_or(false, |p| !p.roadmap.is_empty()) {
                                DetailFocus::Roadmap
                            } else {
                                DetailFocus::Sessions
                            };
                            if let Some(proj) = self.projects.get(idx) {
                                self.browser_open(&proj.path.clone());
                            }
                            self.sub = SubView::ProjectDetail(idx);
                        }
                        ListEntry::DeprecatedFolder => {
                            let dep_dir = self.projects_dir.join("deprecated");
                            self.dep_path = dep_dir.clone();
                            self.dep_root = dep_dir;
                            self.dep_parent_entries = orrch_core::list_directory(&self.dep_path);
                            self.dep_parent_selected = 0;
                            self.dep_in_child = false;
                            self.dep_refresh_child();
                            self.dep_refresh_preview();
                            self.sub = SubView::DeprecatedBrowser;
                        }
                        ListEntry::SubProject(fi, si) => {
                            if let Some(facility) = self.facilities.get(*fi) {
                                if let Some(sub) = facility.sub_projects.get(*si) {
                                    self.browser_open(&sub.path.clone());
                                    // Use a special detail view — for now open as browser
                                    self.sub = SubView::DeprecatedBrowser; // reuse for read-only browse
                                }
                            }
                        }
                        _ => {} // section headers, facility headers — no action
                    }
                }
            }
            KeyCode::Up => {
                if self.project_selected == 0 { self.focus_depth = 0; }
                else { self.project_selected -= 1; }
            }
            KeyCode::Down => {
                let map = self.build_list_map();
                if !map.is_empty() && self.project_selected < map.len() - 1 { self.project_selected += 1; }
            }
            KeyCode::Right => {
                // → on a project = expand it AND enter it
                if let Some(pidx) = self.selected_project_index() {
                    self.expanded_projects.insert(pidx);
                    self.tree_browsing = true;
                    self.tree_project = Some(pidx);
                    self.tree_selected = 0;
                    self.update_tree_preview(pidx);
                }
            }
            KeyCode::Left => {
                // ← at project list level does nothing (already at top)
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle keys when the cursor is inside an expanded project.
    /// The flat item list is: [sessions...] [directory entries...]
    /// tree_selected indexes into this combined list.
    fn key_inside_project(&mut self, key: KeyCode, proj_idx: usize) -> Result<()> {
        // Build the combined item list: sessions first, then directory tree nodes
        let proj_path = self.projects.get(proj_idx).map(|p| p.path.clone()).unwrap_or_default();
        let session_count = self.sessions_for_project(&proj_path).len()
            + self.external_sessions_for_project(&proj_path).len();
        let tree_nodes = self.build_tree(proj_idx);
        let total_items = session_count + tree_nodes.len();

        let in_sessions = self.tree_selected < session_count;
        let tree_offset = self.tree_selected.saturating_sub(session_count);

        match key {
            KeyCode::Left => {
                if !in_sessions {
                    // In the file tree — check if we can collapse a parent dir
                    if let Some(node) = tree_nodes.get(tree_offset) {
                        if node.depth > 0 {
                            // Collapse the parent directory
                            if let Some(parent) = node.path.parent() {
                                let rel = parent.strip_prefix(&proj_path).unwrap_or(parent).to_path_buf();
                                self.tree_toggle_dir(proj_idx, &rel);
                                // Move selection to the parent dir entry
                                let new_nodes = self.build_tree(proj_idx);
                                let parent_pos = new_nodes.iter()
                                    .position(|n| n.path == *parent)
                                    .unwrap_or(0);
                                self.tree_selected = session_count + parent_pos;
                                self.update_tree_preview(proj_idx);
                                return Ok(());
                            }
                        }
                        // At tree depth 0 with expanded dir — collapse it
                        if node.is_dir && node.expanded {
                            let rel = node.path.strip_prefix(&proj_path).unwrap_or(&node.path).to_path_buf();
                            self.tree_toggle_dir(proj_idx, &rel);
                            self.update_tree_preview(proj_idx);
                            return Ok(());
                        }
                    }
                }
                // At top of project or sessions — exit project, collapse it
                self.tree_browsing = false;
                self.tree_project = None;
                self.tree_preview.clear();
                self.expanded_projects.remove(&proj_idx);
            }
            KeyCode::Esc => {
                // Always exit project immediately
                self.tree_browsing = false;
                self.tree_project = None;
                self.tree_preview.clear();
                self.expanded_projects.remove(&proj_idx);
            }
            KeyCode::Up => {
                if self.tree_selected > 0 {
                    self.tree_selected -= 1;
                    self.update_tree_preview(proj_idx);
                } else {
                    // At the very top — exit project
                    self.tree_browsing = false;
                    self.tree_project = None;
                    self.tree_preview.clear();
                    self.expanded_projects.remove(&proj_idx);
                }
            }
            KeyCode::Down => {
                if total_items > 0 && self.tree_selected < total_items - 1 {
                    self.tree_selected += 1;
                    self.update_tree_preview(proj_idx);
                }
            }
            KeyCode::Right => {
                if !in_sessions {
                    // In file tree — expand directory
                    if let Some(node) = tree_nodes.get(tree_offset) {
                        if node.is_dir && !node.expanded {
                            let rel = node.path.strip_prefix(&proj_path).unwrap_or(&node.path).to_path_buf();
                            self.tree_expanded.entry(proj_idx).or_default().insert(rel);
                            // Move selection into the expanded dir (first child)
                            let new_nodes = self.build_tree(proj_idx);
                            if let Some(next) = new_nodes.iter().position(|n| n.depth > node.depth && n.path.starts_with(&node.path)) {
                                self.tree_selected = session_count + next;
                            }
                            self.update_tree_preview(proj_idx);
                        }
                    }
                }
                // In sessions section, right does nothing (sessions aren't expandable)
            }
            KeyCode::Enter => {
                if in_sessions {
                    // Open session detail / external session log
                    if let Some(proj) = self.projects.get(proj_idx) {
                        let managed = self.sessions_for_project(&proj.path);
                        let managed_count = managed.len();
                        if self.tree_selected < managed_count {
                            if let Some(s) = managed.get(self.tree_selected) {
                                let sid = s.sid.clone();
                                let all = self.pm.sessions();
                                if let Some(gi) = all.iter().position(|gs| gs.sid == sid) {
                                    self.sub = SubView::SessionFocus(gi);
                                }
                            }
                        } else {
                            let ext_idx = self.tree_selected - managed_count;
                            let externals = self.external_sessions_for_project(&proj.path);
                            if let Some(ext) = externals.get(ext_idx) {
                                let pid = ext.pid;
                                self.ext_log_cache = orrch_core::session_log::format_session_log(pid, 50);
                                self.ext_log_scroll = self.ext_log_cache.lines().count().saturating_sub(1);
                                self.sub = SubView::ExternalSessionView(pid);
                            }
                        }
                    }
                } else {
                    // In file tree — open editable files in vim
                    if let Some(node) = tree_nodes.get(tree_offset) {
                        if node.is_editable {
                            let kind = VimKind::ProjectFeedback(proj_idx);
                            let title = self.vim_title(&kind);
                            self.vim_request = Some(VimRequest {
                                file: node.path.clone(),
                                kind,
                                title,
                            });
                        } else if node.is_dir {
                            // Enter on a directory = expand it
                            let rel = node.path.strip_prefix(&proj_path).unwrap_or(&node.path).to_path_buf();
                            self.tree_expanded.entry(proj_idx).or_default().insert(rel);
                            let new_nodes = self.build_tree(proj_idx);
                            if let Some(next) = new_nodes.iter().position(|n| n.depth > node.depth && n.path.starts_with(&node.path)) {
                                self.tree_selected = session_count + next;
                            }
                            self.update_tree_preview(proj_idx);
                        }
                    }
                }
            }
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('a') => self.open_action_menu(),
            KeyCode::Char('n') => {
                self.spawn_project_idx = proj_idx;
                self.spawn_goal_text.clear();
                self.spawn_goal_from_roadmap = None;
                self.spawn_agent_idx = 0;
                self.spawn_workforce_idx = 0;
                self.sub = SubView::SpawnGoal;
            }
            KeyCode::Char('x') => {
                // Kill selected session (only in sessions section)
                if in_sessions {
                    if let Some(proj) = self.projects.get(proj_idx) {
                        let sessions = self.sessions_for_project(&proj.path);
                        if let Some(s) = sessions.get(self.tree_selected) {
                            let sid = s.sid.clone();
                            self.pm.kill_session(&sid);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    // Production is now a section in Projects, not a separate panel.
    #[allow(dead_code)]
    fn key_production(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Up => {
                if self.production_selected == 0 { self.focus_depth = 0; }
                else { self.production_selected -= 1; }
            }
            KeyCode::Down => {
                if !self.production_versions.is_empty() && self.production_selected < self.production_versions.len() - 1 {
                    self.production_selected += 1;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_project_detail(&mut self, key: KeyCode) -> Result<()> {
        let proj_idx = if let SubView::ProjectDetail(idx) = self.sub { idx } else { return Ok(()); };

        // Global commands always available
        match key {
            KeyCode::Esc => { self.sub = SubView::List; return Ok(()); }
            KeyCode::Char('a') => { self.open_action_menu(); return Ok(()); }
            KeyCode::Char('f') | KeyCode::Char('e') => {
                self.request_vim(VimKind::ProjectFeedback(proj_idx));
                return Ok(());
            }
            KeyCode::Char('m') => {
                self.request_vim(VimKind::MasterPlanAppend(proj_idx));
                return Ok(());
            }
            KeyCode::Char('n') => {
                self.spawn_project_idx = proj_idx;
                self.spawn_goal_text.clear();
                self.spawn_goal_from_roadmap = None;
                self.spawn_agent_idx = 0;
                self.spawn_workforce_idx = 0;
                self.sub = SubView::SpawnGoal;
                return Ok(());
            }
            KeyCode::Char('w') => {
                // Show tmux window count for this project
                if let Some(proj) = self.projects.get(proj_idx) {
                    let name = proj.name.clone();
                    let windows = orrch_core::windows::list_tmux_windows();
                    let proj_windows: Vec<_> = windows.iter()
                        .filter(|w| w.name.contains(&name) || w.cwd.ends_with(&format!("/{name}")))
                        .collect();
                    if proj_windows.is_empty() {
                        self.notify(format!("{name}: no tmux sessions"));
                    } else {
                        self.notify(format!("{name}: {} tmux window(s)", proj_windows.len()));
                    }
                }
                return Ok(());
            }
            KeyCode::Tab => {
                // Cycle focus: Roadmap → Sessions → DevMap → Browser → Roadmap
                let has_devmap = self.projects.get(proj_idx).is_some_and(|p| !p.plan_phases.is_empty());
                let has_roadmap = self.projects.get(proj_idx).is_some_and(|p| !p.roadmap.is_empty());
                self.detail_focus = match self.detail_focus {
                    DetailFocus::Roadmap => DetailFocus::Sessions,
                    DetailFocus::Sessions => {
                        if has_devmap { DetailFocus::DevMap } else { DetailFocus::Browser }
                    }
                    DetailFocus::DevMap => DetailFocus::Browser,
                    DetailFocus::Browser => {
                        if has_roadmap { DetailFocus::Roadmap } else { DetailFocus::Sessions }
                    }
                };
                return Ok(());
            }
            _ => {}
        }

        match self.detail_focus {
            DetailFocus::Roadmap => self.key_detail_roadmap(key, proj_idx),
            DetailFocus::Sessions => self.key_detail_sessions(key, proj_idx),
            DetailFocus::DevMap => self.key_detail_devmap(key, proj_idx),
            DetailFocus::Browser => {
                // Down at bottom of browser: don't wrap, just stay
                // Up at top of browser: switch to sessions
                if key == KeyCode::Up && self.browser_parent_selected == 0 && !self.browser_in_child {
                    self.detail_focus = if self.projects.get(proj_idx).is_some_and(|p| !p.plan_phases.is_empty()) {
                        DetailFocus::DevMap
                    } else {
                        DetailFocus::Sessions
                    };
                    return Ok(());
                }
                self.key_browser_in_detail(key, proj_idx)
            }
        }
    }

    fn key_detail_roadmap(&mut self, key: KeyCode, proj_idx: usize) -> Result<()> {
        let roadmap_len = self.projects.get(proj_idx).map(|p| p.roadmap.len()).unwrap_or(0);
        if roadmap_len == 0 {
            self.detail_focus = DetailFocus::Sessions;
            return Ok(());
        }

        match key {
            KeyCode::Up => {
                if self.roadmap_selected == 0 {
                    // Already at top — stay
                } else {
                    self.roadmap_selected = self.roadmap_selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if self.roadmap_selected + 1 < roadmap_len {
                    self.roadmap_selected += 1;
                } else {
                    // Past last roadmap item → move to sessions
                    self.detail_focus = DetailFocus::Sessions;
                }
            }
            KeyCode::Char('s') => {
                // Cycle feature status forward
                if let Some(proj) = self.projects.get_mut(proj_idx) {
                    if let Some(item) = proj.roadmap.get_mut(self.roadmap_selected) {
                        let old = item.status;
                        item.status = old.cycle_forward();
                        if item.status != old {
                            if let Some(ref plan_file) = proj.meta.plan_file {
                                let plan_path = proj.path.join(plan_file);
                                let title = item.title.clone();
                                let new_status = item.status;
                                if let Err(e) = orrch_core::update_feature_status_in_plan(&plan_path, &title, new_status) {
                                    self.notify(format!("Failed to update plan: {e}"));
                                } else {
                                    self.notify(format!("{}: {} → {}", title, old.label(), new_status.label()));
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Char('S') => {
                // Cycle feature status backward
                if let Some(proj) = self.projects.get_mut(proj_idx) {
                    if let Some(item) = proj.roadmap.get_mut(self.roadmap_selected) {
                        let old = item.status;
                        item.status = old.cycle_backward();
                        if item.status != old {
                            if let Some(ref plan_file) = proj.meta.plan_file {
                                let plan_path = proj.path.join(plan_file);
                                let title = item.title.clone();
                                let new_status = item.status;
                                if let Err(e) = orrch_core::update_feature_status_in_plan(&plan_path, &title, new_status) {
                                    self.notify(format!("Failed to update plan: {e}"));
                                } else {
                                    self.notify(format!("{}: {} → {}", title, old.label(), new_status.label()));
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_detail_sessions(&mut self, key: KeyCode, proj_idx: usize) -> Result<()> {
        let session_count = if let Some(proj) = self.projects.get(proj_idx) {
            self.sessions_for_project(&proj.path).len()
                + self.external_sessions_for_project(&proj.path).len()
        } else { 0 };

        match key {
            KeyCode::Up => {
                if self.session_selected == 0 {
                    // At top of sessions → move to roadmap
                    self.detail_focus = DetailFocus::Roadmap;
                } else {
                    self.session_selected = self.session_selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if session_count > 0 && self.session_selected < session_count - 1 {
                    self.session_selected += 1;
                } else {
                    // Past last session → move to dev map (if available) or browser
                    let has_devmap = self.projects.get(proj_idx).is_some_and(|p| !p.plan_phases.is_empty());
                    self.detail_focus = if has_devmap { DetailFocus::DevMap } else { DetailFocus::Browser };
                }
            }
            KeyCode::Enter => {
                if let Some(proj) = self.projects.get(proj_idx) {
                    let managed = self.sessions_for_project(&proj.path);
                    let managed_count = managed.len();

                    if self.session_selected < managed_count {
                        // Managed session — open PTY focus
                        if let Some(s) = managed.get(self.session_selected) {
                            let sid = s.sid.clone();
                            let all = self.pm.sessions();
                            if let Some(global_idx) = all.iter().position(|gs| gs.sid == sid) {
                                self.sub = SubView::SessionFocus(global_idx);
                            }
                        }
                    } else {
                        // External session — open conversation log viewer
                        let ext_idx = self.session_selected - managed_count;
                        let externals = self.external_sessions_for_project(&proj.path);
                        if let Some(ext) = externals.get(ext_idx) {
                            let pid = ext.pid;
                            self.ext_log_cache = orrch_core::session_log::format_session_log(pid, 50);
                            self.ext_log_scroll = self.ext_log_cache.lines().count().saturating_sub(1);
                            self.sub = SubView::ExternalSessionView(pid);
                        }
                    }
                }
            }
            KeyCode::Char('w') => {
                // Select tmux window for the focused session
                let windows = orrch_core::windows::list_tmux_windows();
                if let Some(win) = windows.get(self.session_selected) {
                    orrch_core::windows::select_tmux_window(win.index);
                    self.notify(format!("Selected tmux:{}", win.name));
                }
            }
            KeyCode::Char('x') => {
                // Kill selected session
                if let Some(proj) = self.projects.get(proj_idx) {
                    let sessions = self.sessions_for_project(&proj.path);
                    if let Some(s) = sessions.get(self.session_selected) {
                        let sid = s.sid.clone();
                        self.pm.kill_session(&sid);
                        self.notify(format!("Killed {sid}"));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_detail_devmap(&mut self, key: KeyCode, proj_idx: usize) -> Result<()> {
        let total = self.devmap_flat_count(proj_idx);
        match key {
            KeyCode::Up => {
                if self.devmap_selected == 0 {
                    self.detail_focus = DetailFocus::Sessions;
                } else {
                    self.devmap_selected = self.devmap_selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if total == 0 || self.devmap_selected >= total.saturating_sub(1) {
                    self.detail_focus = DetailFocus::Browser;
                } else {
                    self.devmap_selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some((is_phase, phase_idx, feat_idx)) = self.devmap_item_at(proj_idx, self.devmap_selected) {
                    if is_phase {
                        // Toggle phase expansion
                        if self.devmap_phase_idx == phase_idx {
                            self.devmap_phase_idx = usize::MAX; // collapse
                        } else {
                            self.devmap_phase_idx = phase_idx; // expand
                        }
                    } else {
                        // Quick-spawn: launch develop-feature session for this feature
                        if let Some(proj) = self.projects.get(proj_idx) {
                            if let Some(phase) = proj.plan_phases.get(phase_idx) {
                                if let Some(feat) = phase.features.get(feat_idx) {
                                    self.spawn_project_idx = proj_idx;
                                    self.spawn_goal_text = feat.title.clone();
                                    self.spawn_goal_from_roadmap = None;
                                    self.spawn_agent_idx = 0;
                                    self.spawn_workforce_idx = 0;
                                    self.sub = SubView::SpawnGoal;
                                }
                            }
                        }
                    }
                }
            }
            // Task 46: Reorder features with J (down) / K (up)
            KeyCode::Char('J') => {
                self.devmap_move_feature(proj_idx, orrch_core::MoveDirection::Down);
            }
            KeyCode::Char('K') => {
                self.devmap_move_feature(proj_idx, orrch_core::MoveDirection::Up);
            }
            // Task 47: Add Feature popup
            KeyCode::Char('a') => {
                if let Some((_is_phase, _phase_idx, _)) = self.devmap_item_at(proj_idx, self.devmap_selected) {
                    // 'a' works on both phase headers and features (adds to the selected/containing phase)
                    self.add_feature_title.clear();
                    self.add_feature_desc.clear();
                    self.add_feature_field = 0;
                    self.sub = SubView::AddFeature(proj_idx);
                }
            }
            // Task 3: Mark currently selected feature as user-verified.
            KeyCode::Char('V') => {
                if let Some((is_phase, phase_idx, feat_idx)) = self.devmap_item_at(proj_idx, self.devmap_selected) {
                    if !is_phase {
                        let resolved = self.projects.get(proj_idx).and_then(|proj| {
                            let plan_file = proj.meta.plan_file.as_ref()?;
                            let plan_path = proj.path.join(plan_file);
                            let feat = proj.plan_phases.get(phase_idx)?.features.get(feat_idx)?;
                            Some((plan_path, feat.title.clone()))
                        });
                        if let Some((plan_path, title)) = resolved {
                            if orrch_core::plan_parser::mark_verified_in_plan(&plan_path, &title).is_ok() {
                                self.reload_project_plan(proj_idx);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Move the currently selected feature in the dev map.
    fn devmap_move_feature(&mut self, proj_idx: usize, direction: orrch_core::MoveDirection) {
        let Some((is_phase, phase_idx, feat_idx)) = self.devmap_item_at(proj_idx, self.devmap_selected) else {
            return;
        };
        if is_phase {
            return; // can't move phase headers
        }
        let plan_path = {
            let Some(proj) = self.projects.get(proj_idx) else { return; };
            let Some(ref plan_file) = proj.meta.plan_file else {
                self.notify("No PLAN.md".into());
                return;
            };
            proj.path.join(plan_file)
        };
        match orrch_core::move_feature_in_plan(&plan_path, phase_idx, feat_idx, direction) {
            Ok(true) => {
                // Reload plan phases for this project
                self.reload_project_plan(proj_idx);
                // Adjust selection to follow the moved feature
                match direction {
                    orrch_core::MoveDirection::Up => {
                        self.devmap_selected = self.devmap_selected.saturating_sub(1);
                    }
                    orrch_core::MoveDirection::Down => {
                        self.devmap_selected += 1;
                    }
                }
            }
            Ok(false) => {
                self.notify("Cannot move".into());
            }
            Err(e) => {
                self.notify(format!("Move failed: {e}"));
            }
        }
    }

    /// Reload plan phases for a single project from disk.
    fn reload_project_plan(&mut self, proj_idx: usize) {
        let Some(proj) = self.projects.get_mut(proj_idx) else { return; };
        if let Some(ref plan_file) = proj.meta.plan_file {
            let plan_path = proj.path.join(plan_file);
            if let Ok(content) = std::fs::read_to_string(&plan_path) {
                proj.plan_phases = orrch_core::parse_plan(&content);
            }
        }
    }

    /// Handle key input in the AddFeature popup (Task 47).
    fn key_add_feature(&mut self, key: KeyCode, proj_idx: usize) -> Result<()> {
        match key {
            KeyCode::Esc => {
                self.sub = SubView::ProjectDetail(proj_idx);
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.add_feature_field = if self.add_feature_field == 0 { 1 } else { 0 };
            }
            KeyCode::Enter => {
                let title = self.add_feature_title.trim().to_string();
                if title.is_empty() {
                    self.notify("Title is required".into());
                    return Ok(());
                }
                let desc = self.add_feature_desc.trim().to_string();
                // Determine which phase we're in
                let phase_idx = if let Some((_is_phase, pidx, _)) = self.devmap_item_at(proj_idx, self.devmap_selected) {
                    pidx
                } else if let Some(proj) = self.projects.get(proj_idx) {
                    // Default to last phase
                    proj.plan_phases.len().saturating_sub(1)
                } else {
                    0
                };
                let plan_path = {
                    let Some(proj) = self.projects.get(proj_idx) else {
                        self.sub = SubView::ProjectDetail(proj_idx);
                        return Ok(());
                    };
                    let Some(ref plan_file) = proj.meta.plan_file else {
                        self.notify("No PLAN.md".into());
                        self.sub = SubView::ProjectDetail(proj_idx);
                        return Ok(());
                    };
                    proj.path.join(plan_file)
                };
                match orrch_core::append_feature_to_plan(&plan_path, phase_idx, &title, &desc) {
                    Ok(true) => {
                        self.reload_project_plan(proj_idx);
                        self.notify(format!("Added: {title}"));
                    }
                    Ok(false) => {
                        self.notify("Failed to add feature".into());
                    }
                    Err(e) => {
                        self.notify(format!("Error: {e}"));
                    }
                }
                self.sub = SubView::ProjectDetail(proj_idx);
            }
            KeyCode::Backspace => {
                match self.add_feature_field {
                    0 => { self.add_feature_title.pop(); }
                    _ => { self.add_feature_desc.pop(); }
                }
            }
            KeyCode::Char(c) => {
                match self.add_feature_field {
                    0 => self.add_feature_title.push(c),
                    _ => self.add_feature_desc.push(c),
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle key input in the AddMcpServer form (Task 62).
    fn key_add_mcp_server(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Esc => {
                self.sub = SubView::List;
            }
            KeyCode::Tab => {
                self.add_mcp_field = (self.add_mcp_field + 1) % 6;
            }
            KeyCode::BackTab => {
                self.add_mcp_field = if self.add_mcp_field == 0 { 5 } else { self.add_mcp_field - 1 };
            }
            KeyCode::Enter => {
                if self.add_mcp_field == 2 {
                    // Toggle transport type
                    self.add_mcp_transport = 1 - self.add_mcp_transport;
                    return Ok(());
                }
                // Submit
                let name = self.add_mcp_name.trim().to_string();
                if name.is_empty() {
                    self.notify("Server name is required".into());
                    return Ok(());
                }
                let description = self.add_mcp_desc.trim().to_string();
                let transport = if self.add_mcp_transport == 0 {
                    orrch_library::McpTransport::Stdio {
                        command: self.add_mcp_command.trim().to_string(),
                        args: self.add_mcp_args.split_whitespace().map(|s| s.to_string()).collect(),
                        env: std::collections::HashMap::new(),
                    }
                } else {
                    orrch_library::McpTransport::Sse {
                        url: self.add_mcp_command.trim().to_string(),
                    }
                };
                let assigned_roles: Vec<String> = self.add_mcp_roles
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                let entry = orrch_library::McpServerEntry {
                    name: name.clone(),
                    description,
                    transport,
                    enabled: true,
                    assigned_roles,
                    notes: String::new(),
                    path: std::path::PathBuf::new(),
                };

                let mcp_dir = self.projects_dir.join("orrchestrator/library/mcp_servers");
                match orrch_library::save_mcp_server(&mcp_dir, &entry) {
                    Ok(_) => {
                        // Reload MCP servers
                        self.library_mcp_servers = orrch_library::load_mcp_servers(&mcp_dir);
                        self.notify(format!("Added MCP server: {name}"));
                    }
                    Err(e) => {
                        self.notify(format!("Failed: {e}"));
                    }
                }
                self.sub = SubView::List;
            }
            KeyCode::Backspace => {
                match self.add_mcp_field {
                    0 => { self.add_mcp_name.pop(); }
                    1 => { self.add_mcp_desc.pop(); }
                    2 => {} // transport is a toggle
                    3 => { self.add_mcp_command.pop(); }
                    4 => { self.add_mcp_args.pop(); }
                    5 => { self.add_mcp_roles.pop(); }
                    _ => {}
                }
            }
            KeyCode::Char(c) => {
                if self.add_mcp_field == 2 {
                    // Transport: s=stdio, e=sse
                    match c {
                        's' | 'S' => self.add_mcp_transport = 0,
                        'e' | 'E' => self.add_mcp_transport = 1,
                        _ => {}
                    }
                } else {
                    match self.add_mcp_field {
                        0 => self.add_mcp_name.push(c),
                        1 => self.add_mcp_desc.push(c),
                        3 => self.add_mcp_command.push(c),
                        4 => self.add_mcp_args.push(c),
                        5 => self.add_mcp_roles.push(c),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Count the total number of visible items in the dev map flat list.
    pub fn devmap_flat_count(&self, proj_idx: usize) -> usize {
        let Some(proj) = self.projects.get(proj_idx) else { return 0; };
        let mut count = 0;
        for (i, phase) in proj.plan_phases.iter().enumerate() {
            count += 1; // phase header
            if self.devmap_phase_idx == i {
                count += phase.features.len();
            }
        }
        count
    }

    /// Given a flat index, return (is_phase_header, phase_index, feature_index_within_phase).
    /// For phase headers, feature_index is 0 (unused).
    fn devmap_item_at(&self, proj_idx: usize, flat_idx: usize) -> Option<(bool, usize, usize)> {
        let Some(proj) = self.projects.get(proj_idx) else { return None; };
        let mut pos = 0;
        for (i, phase) in proj.plan_phases.iter().enumerate() {
            if pos == flat_idx {
                return Some((true, i, 0));
            }
            pos += 1;
            if self.devmap_phase_idx == i {
                if flat_idx < pos + phase.features.len() {
                    let feat_idx = flat_idx - pos;
                    return Some((false, i, feat_idx));
                }
                pos += phase.features.len();
            }
        }
        None
    }

    fn key_browser_in_detail(&mut self, key: KeyCode, proj_idx: usize) -> Result<()> {
        match key {
            KeyCode::Esc => self.sub = SubView::List,
            KeyCode::Up => {
                if self.browser_in_child {
                    self.browser_child_selected = self.browser_child_selected.saturating_sub(1);
                } else {
                    self.browser_parent_selected = self.browser_parent_selected.saturating_sub(1);
                    self.browser_refresh_child();
                }
                self.browser_refresh_preview();
            }
            KeyCode::Down => {
                if self.browser_in_child {
                    if !self.browser_child_entries.is_empty()
                        && self.browser_child_selected < self.browser_child_entries.len() - 1
                    {
                        self.browser_child_selected += 1;
                    }
                } else {
                    if !self.browser_parent_entries.is_empty()
                        && self.browser_parent_selected < self.browser_parent_entries.len() - 1
                    {
                        self.browser_parent_selected += 1;
                        self.browser_refresh_child();
                    }
                }
                self.browser_refresh_preview();
            }
            KeyCode::Right => {
                if self.browser_in_child {
                    // Navigate into the selected child directory
                    if let Some(entry) = self.browser_child_entries.get(self.browser_child_selected).cloned() {
                        if entry.is_dir {
                            self.browser_path = entry.path;
                            self.browser_parent_entries = orrch_core::list_directory(&self.browser_path);
                            self.browser_parent_selected = 0;
                            self.browser_in_child = false;
                            self.browser_refresh_child();
                            self.browser_refresh_preview();
                        }
                    }
                } else if !self.browser_child_entries.is_empty() {
                    // Move focus to child column
                    self.browser_in_child = true;
                    self.browser_refresh_preview();
                }
            }
            KeyCode::Left => {
                if self.browser_in_child {
                    // Move focus back to parent column
                    self.browser_in_child = false;
                    self.browser_refresh_preview();
                } else {
                    // Navigate up one directory (if not at root)
                    if self.browser_path != self.browser_root {
                        if let Some(parent) = self.browser_path.parent() {
                            self.browser_path = parent.to_path_buf();
                            self.browser_parent_entries = orrch_core::list_directory(&self.browser_path);
                            self.browser_parent_selected = 0;
                            self.browser_in_child = false;
                            self.browser_refresh_child();
                            self.browser_refresh_preview();
                        }
                    }
                }
            }
            KeyCode::Enter => {
                // Open file in editor (only for editable files)
                let entry = if self.browser_in_child {
                    self.browser_child_entries.get(self.browser_child_selected).cloned()
                } else {
                    self.browser_parent_entries.get(self.browser_parent_selected).cloned()
                };
                if let Some(entry) = entry {
                    if entry.is_editable {
                        // Open file in vim directly
                        let kind = VimKind::ProjectFeedback(proj_idx);
                        let title = self.vim_title(&kind);
                        self.vim_request = Some(VimRequest {
                            file: entry.path.clone(),
                            kind,
                            title,
                        });
                    } else if entry.is_dir {
                        // Enter on a dir = navigate into it
                        self.browser_path = entry.path;
                        self.browser_parent_entries = orrch_core::list_directory(&self.browser_path);
                        self.browser_parent_selected = 0;
                        self.browser_in_child = false;
                        self.browser_refresh_child();
                        self.browser_refresh_preview();
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_session_focus(&mut self, key: KeyCode) -> Result<()> {
        if key == KeyCode::Esc {
            // Return to project detail if we came from one, otherwise list
            // Check if any project detail was active before by looking at session's project
            if let SubView::SessionFocus(idx) = self.sub {
                if let Some(session) = self.pm.sessions().get(idx) {
                    let proj_path = session.project_dir.clone();
                    if let Some(pidx) = self.projects.iter().position(|p| p.path == proj_path) {
                        self.sub = SubView::ProjectDetail(pidx);
                        self.detail_focus = DetailFocus::Sessions;
                        return Ok(());
                    }
                }
            }
            self.sub = SubView::List;
            return Ok(());
        }
        if let SubView::SessionFocus(idx) = self.sub {
            let sid = self.pm.sessions().get(idx).map(|s| s.sid.clone());
            if let Some(sid) = sid {
                let data = match key {
                    KeyCode::Enter => b"\r".to_vec(),
                    KeyCode::Backspace => vec![0x7f],
                    KeyCode::Tab => b"\t".to_vec(),
                    KeyCode::Char(c) => c.to_string().into_bytes(),
                    _ => return Ok(()),
                };
                let _ = self.pm.write_to_session(&sid, &data);
            }
        }
        Ok(())
    }

    fn key_external_session_view(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Esc => {
                if let SubView::ExternalSessionView(pid) = self.sub {
                    for ext in self.pm.external_sessions() {
                        if ext.pid == pid {
                            if let Some(pidx) = self.projects.iter().position(|p| {
                                ext.project_dir == p.path.to_string_lossy().as_ref()
                                    || ext.project_dir.starts_with(p.path.to_string_lossy().as_ref())
                            }) {
                                self.sub = SubView::ProjectDetail(pidx);
                                self.detail_focus = DetailFocus::Sessions;
                                return Ok(());
                            }
                        }
                    }
                }
                self.sub = SubView::List;
            }
            KeyCode::Char('r') => {
                if let SubView::ExternalSessionView(pid) = self.sub {
                    self.ext_log_cache = orrch_core::session_log::format_session_log(pid, 50);
                    self.notify("Log refreshed".into());
                }
            }
            KeyCode::Up => {
                self.ext_log_scroll = self.ext_log_scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                let total = self.ext_log_cache.lines().count();
                if self.ext_log_scroll < total { self.ext_log_scroll += 1; }
            }
            KeyCode::Home => self.ext_log_scroll = 0,
            KeyCode::End => {
                self.ext_log_scroll = self.ext_log_cache.lines().count();
            }
            _ => {}
        }
        Ok(())
    }

    fn key_spawn_goal(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        // Ctrl+C: copy goal text, Ctrl+V: paste into goal
        if modifiers.contains(KeyModifiers::CONTROL) {
            match key {
                KeyCode::Char('c') => {
                    crate::editor::clipboard_set(&self.spawn_goal_text);
                    return Ok(());
                }
                KeyCode::Char('v') => {
                    if let Some(text) = crate::editor::clipboard_get() {
                        self.spawn_goal_from_roadmap = None;
                        // Single-line input: replace newlines with spaces
                        self.spawn_goal_text.push_str(&text.replace('\n', " ").replace('\r', ""));
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        match key {
            KeyCode::Esc => self.sub = SubView::List,
            KeyCode::Enter => {
                self.spawn_workforce_idx = 0;
                if self.loaded_workforces.is_empty() {
                    // No workforces on disk — skip to agent selection
                    self.spawn_agent_idx = 0;
                    self.sub = SubView::SpawnAgent;
                } else {
                    self.sub = SubView::SpawnWorkforce;
                }
            }
            KeyCode::Tab => {
                if let Some(proj) = self.projects.get(self.spawn_project_idx) {
                    let open_count = proj.open_count();
                    if open_count > 0 {
                        let next = match self.spawn_goal_from_roadmap {
                            Some(i) if i + 1 < open_count => Some(i + 1),
                            Some(_) => None,
                            None => Some(0),
                        };
                        self.spawn_goal_from_roadmap = next;
                        if let Some(idx) = next {
                            let open = proj.open_roadmap_items();
                            if let Some(item) = open.get(idx) { self.spawn_goal_text = item.title.clone(); }
                        } else { self.spawn_goal_text.clear(); }
                    }
                }
            }
            KeyCode::Backspace => { self.spawn_goal_from_roadmap = None; self.spawn_goal_text.pop(); }
            KeyCode::Char(c) => { self.spawn_goal_from_roadmap = None; self.spawn_goal_text.push(c); }
            _ => {}
        }
        Ok(())
    }

    fn key_spawn_workforce(&mut self, key: KeyCode) -> Result<()> {
        let count = 1 + self.loaded_workforces.len(); // 0 = no workforce, 1+ = workforces
        match key {
            KeyCode::Esc => self.sub = SubView::List,
            KeyCode::Tab | KeyCode::Down => {
                self.spawn_workforce_idx = (self.spawn_workforce_idx + 1) % count;
            }
            KeyCode::Up => {
                self.spawn_workforce_idx = (self.spawn_workforce_idx + count - 1) % count;
            }
            KeyCode::Enter => {
                // If a workforce is selected, pre-select Hypervisor agent
                if self.spawn_workforce_idx > 0 {
                    self.spawn_agent_idx = self.agent_profiles.iter()
                        .position(|p| p.name == "Hypervisor")
                        .map(|i| i + 1)  // +1 because 0 = no agent
                        .unwrap_or(0);
                } else {
                    self.spawn_agent_idx = 0;
                }
                self.sub = SubView::SpawnAgent;
            }
            _ => {}
        }
        Ok(())
    }

    fn key_spawn_agent(&mut self, key: KeyCode) -> Result<()> {
        let count = 1 + self.agent_profiles.len(); // 0 = no agent, 1+ = profiles
        match key {
            KeyCode::Esc => self.sub = SubView::List,
            KeyCode::Tab | KeyCode::Down => {
                self.spawn_agent_idx = (self.spawn_agent_idx + 1) % count;
            }
            KeyCode::Up => {
                self.spawn_agent_idx = (self.spawn_agent_idx + count - 1) % count;
            }
            KeyCode::Enter => {
                self.spawn_backend = BackendKind::Claude;
                self.sub = SubView::SpawnBackend;
            }
            _ => {}
        }
        Ok(())
    }

    fn key_spawn_backend(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Esc => self.sub = SubView::List,
            KeyCode::Tab => {
                // Cycle through CLI backends
                let cli = BackendKind::cli_backends();
                let cur_idx = cli.iter().position(|b| *b == self.spawn_backend).unwrap_or(0);
                self.spawn_backend = cli[(cur_idx + 1) % cli.len()];
            }
            KeyCode::Enter => {
                self.spawn_host_idx = 0;
                self.sub = SubView::SpawnHost;
            }
            _ => {}
        }
        Ok(())
    }

    fn key_spawn_host(&mut self, key: KeyCode) -> Result<()> {
        // Host list: [0] = local, [1..] = remote_hosts (non-local only)
        let remote_hosts: Vec<&orrch_core::remote::RemoteHost> = self.remote_hosts.iter()
            .filter(|h| !h.is_local)
            .collect();
        let host_count = 1 + remote_hosts.len(); // local + remotes

        match key {
            KeyCode::Esc => self.sub = SubView::List,
            KeyCode::Tab | KeyCode::Down => {
                self.spawn_host_idx = (self.spawn_host_idx + 1) % host_count;
            }
            KeyCode::Up => {
                self.spawn_host_idx = (self.spawn_host_idx + host_count - 1) % host_count;
            }
            KeyCode::Enter => {
                if let Some(proj) = self.projects.get(self.spawn_project_idx) {
                    let path = proj.path.clone();
                    let proj_name = proj.name.clone();
                    let backend = self.spawn_backend;
                    let raw_goal = if self.spawn_goal_text.is_empty() {
                        CONTINUE_DEV_PROMPT.to_string()
                    } else { self.spawn_goal_text.clone() };

                    let goal = if self.spawn_workforce_idx > 0 {
                        // MCP tool path: instruct session to call the develop_feature MCP tool
                        format!("Call the orrchestrator MCP tool 'develop_feature' with goal: {}", raw_goal)
                    } else if self.spawn_agent_idx > 0 {
                        // Agent-only path (existing behavior)
                        if let Some(profile) = self.agent_profiles.get(self.spawn_agent_idx - 1) {
                            profile.as_preamble(&raw_goal)
                        } else { raw_goal }
                    } else {
                        raw_goal
                    };

                    if self.spawn_host_idx == 0 {
                        // Local spawn
                        let _ = self.spawn_session(&path, backend, Some(&goal));
                    } else {
                        // Remote spawn
                        if let Some(host) = remote_hosts.get(self.spawn_host_idx - 1) {
                            let host = (*host).clone();
                            let host_name = host.name.clone();
                            let backend_label = backend.label().to_string();
                            let flags: Vec<String> = if backend == BackendKind::Claude {
                                vec!["--dangerously-skip-permissions".into()]
                            } else {
                                Vec::new()
                            };
                            let goal2 = goal.clone();
                            let proj_name2 = proj_name.clone();
                            tokio::spawn(async move {
                                let _ = orrch_core::remote::spawn_remote_session(
                                    &host, &proj_name2, &backend_label, &goal2, &flags,
                                ).await;
                            });
                            self.notify(format!("Spawning on {host_name}..."));
                        }
                    }
                }
                self.sub = SubView::List;
            }
            _ => {}
        }
        Ok(())
    }

    // ─── Action Menu ─────────────────────────────────────────────

    fn open_action_menu(&mut self) {
        let mut items = Vec::new();
        let saved_sub = self.sub.clone();

        match (&self.panel, &self.sub) {
            (Panel::Oversee, SubView::List) => {
                items.push(ActionItem { key: 'n', label: "Spawn session".into(), action: ActionKind::SpawnSession });
                items.push(ActionItem { key: 'N', label: "Spawn all open roadmap items".into(), action: ActionKind::SpawnAll });
                items.push(ActionItem { key: 'P', label: "Create new project".into(), action: ActionKind::NewProject });
                items.push(ActionItem { key: 'f', label: "Write feedback".into(), action: ActionKind::WriteFeedback });
                items.push(ActionItem { key: 't', label: "Cycle color tag".into(), action: ActionKind::CycleTag });
                items.push(ActionItem { key: 'S', label: "Cycle scope".into(), action: ActionKind::CycleScope });
                items.push(ActionItem { key: 's', label: "Toggle hot/cold".into(), action: ActionKind::CycleTemp });
                items.push(ActionItem { key: 'i', label: "Toggle ignored".into(), action: ActionKind::IgnoreProject });
                items.push(ActionItem { key: 'D', label: "Deprecate project".into(), action: ActionKind::DeprecateProject });
                items.push(ActionItem { key: 'C', label: "Mark complete (v1)".into(), action: ActionKind::CompleteProject });
                items.push(ActionItem { key: 'r', label: "Reload project list".into(), action: ActionKind::ReloadProjects });
                items.push(ActionItem { key: 'g', label: "Git commit+push (Claude)".into(), action: ActionKind::GitCommit });
                items.push(ActionItem { key: 'G', label: "Git commit ALL projects".into(), action: ActionKind::GitCommitAll });
            }
            (_, SubView::ProjectDetail(idx)) => {
                let idx = *idx;
                items.push(ActionItem { key: 'n', label: "Spawn session".into(), action: ActionKind::SpawnSession });
                items.push(ActionItem { key: 'f', label: "Write project feedback".into(), action: ActionKind::WriteProjectFeedback(idx) });
                items.push(ActionItem { key: 'm', label: "Append to master plan".into(), action: ActionKind::MasterPlanAppend(idx) });
                items.push(ActionItem { key: 'S', label: "Cycle scope".into(), action: ActionKind::CycleScope });
                items.push(ActionItem { key: 't', label: "Cycle color tag".into(), action: ActionKind::CycleTag });
                items.push(ActionItem { key: 'g', label: "Git commit+push (Claude)".into(), action: ActionKind::GitCommit });
            }
            (Panel::Design, SubView::List) => {
                if let Some(item) = self.feedback_items.get(self.feedback_selected) {
                    let fname = item.filename.clone();
                    if item.status == FeedbackStatus::Draft {
                        items.push(ActionItem { key: 's', label: "Submit feedback".into(), action: ActionKind::SubmitFeedback(fname.clone()) });
                        items.push(ActionItem { key: 'r', label: "Resume editing".into(), action: ActionKind::ResumeFeedback(fname) });
                    }
                    items.push(ActionItem { key: 'd', label: "Delete feedback".into(), action: ActionKind::DeleteFeedback(self.feedback_selected) });
                }
                items.push(ActionItem { key: 'f', label: "Write new feedback".into(), action: ActionKind::WriteFeedback });
            }
            _ => {}
        }

        if !items.is_empty() {
            self.action_items = items;
            self.action_selected = 0;
            self.action_return_sub = Some(Box::new(saved_sub));
            self.sub = SubView::ActionMenu;
        }
    }

    fn key_action_menu(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Esc => {
                if let Some(prev) = self.action_return_sub.take() {
                    self.sub = *prev;
                } else {
                    self.sub = SubView::List;
                }
            }
            KeyCode::Up => {
                self.action_selected = self.action_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                if self.action_selected + 1 < self.action_items.len() {
                    self.action_selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(item) = self.action_items.get(self.action_selected) {
                    let action = item.action.clone();
                    // Return to previous view first
                    if let Some(prev) = self.action_return_sub.take() {
                        self.sub = *prev;
                    } else {
                        self.sub = SubView::List;
                    }
                    self.execute_action(action)?;
                }
            }
            KeyCode::Char(c) => {
                // Accelerator key — find matching action
                if let Some(item) = self.action_items.iter().find(|i| i.key == c) {
                    let action = item.action.clone();
                    if let Some(prev) = self.action_return_sub.take() {
                        self.sub = *prev;
                    } else {
                        self.sub = SubView::List;
                    }
                    self.execute_action(action)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn execute_action(&mut self, action: ActionKind) -> Result<()> {
        // Pre-extract project index to avoid borrow conflicts
        let pidx = self.selected_project_index();

        match action {
            ActionKind::SpawnSession => {
                self.spawn_project_idx = pidx.unwrap_or(0);
                self.spawn_goal_text.clear();
                self.spawn_goal_from_roadmap = None;
                self.spawn_agent_idx = 0;
                self.spawn_workforce_idx = 0;
                self.sub = SubView::SpawnGoal;
            }
            ActionKind::SpawnAll => {
                if let Some(pidx) = pidx {
                    let (items, path, name) = {
                        let proj = &self.projects[pidx];
                        (proj.open_roadmap_items().iter().map(|i| i.title.clone()).collect::<Vec<_>>(), proj.path.clone(), proj.name.clone())
                    };
                    let count = items.len();
                    let mut spawned = 0;
                    for goal in items { if self.spawn_session(&path, BackendKind::Claude, Some(&goal)).is_ok() { spawned += 1; } }
                    self.notify(format!("{name}: {spawned}/{count} pipelines"));
                }
            }
            ActionKind::NewProject => {
                self.new_project_name.clear();
                self.new_project_scope = orrch_core::Scope::Private;
                self.new_project_temp = Temperature::Hot;
                self.new_project_error = None;
                self.sub = SubView::NewProjectName;
            }
            ActionKind::WriteFeedback => { self.request_vim(VimKind::GlobalFeedback); }
            ActionKind::WriteProjectFeedback(idx) => { self.request_vim(VimKind::ProjectFeedback(idx)); }
            ActionKind::MasterPlanAppend(idx) => { self.request_vim(VimKind::MasterPlanAppend(idx)); }
            ActionKind::CycleTag => {
                if let Some(pidx) = pidx {
                    let proj = &mut self.projects[pidx];
                    proj.color_tag = proj.color_tag.cycle();
                    proj.save_color_tag();
                    let msg = format!("{}: {}", proj.name, proj.color_tag.label());
                    self.notify(msg);
                }
            }
            ActionKind::CycleScope => {
                if let Some(pidx) = pidx {
                    let proj = &mut self.projects[pidx];
                    proj.scope = proj.scope.cycle();
                    proj.save_scope();
                    let msg = format!("{}: [{}]", proj.name, proj.scope.label());
                    self.notify(msg);
                }
            }
            ActionKind::CycleTemp => {
                if let Some(pidx) = pidx {
                    let proj = &mut self.projects[pidx];
                    proj.temperature = match proj.temperature { Temperature::Hot => Temperature::Cold, Temperature::Cold => Temperature::Hot, Temperature::Ignored => Temperature::Cold };
                    proj.save_temperature();
                    let msg = format!("{}: {}", proj.name, proj.temperature.label());
                    self.categorize_projects();
                    self.notify(msg);
                }
            }
            ActionKind::IgnoreProject => {
                if let Some(pidx) = pidx {
                    let proj = &mut self.projects[pidx];
                    proj.temperature = if proj.temperature == Temperature::Ignored { Temperature::Cold } else { Temperature::Ignored };
                    proj.save_temperature();
                    let msg = format!("{}: {}", proj.name, proj.temperature.label());
                    self.categorize_projects();
                    self.notify(msg);
                }
            }
            ActionKind::DeprecateProject => {
                if let Some(pidx) = pidx { self.sub = SubView::ConfirmDeprecate(pidx); }
            }
            ActionKind::CompleteProject => {
                if let Some(pidx) = pidx { self.sub = SubView::ConfirmComplete(pidx); }
            }
            ActionKind::ReloadProjects => { self.reload_projects(); self.notify("Reloaded".into()); }
            ActionKind::GitCommit => {
                if let Some(pidx) = pidx {
                    let (name, path) = {
                        let proj = &self.projects[pidx];
                        (proj.name.clone(), proj.path.clone())
                    };
                    let status = orrch_core::git::check_status(&path);
                    if !status.has_repo {
                        self.notify(format!("{name}: no git repo"));
                    } else if status.dirty_count == 0 && status.unpushed == 0 {
                        self.notify(format!("{name}: nothing to commit"));
                    } else {
                        match orrch_core::git::spawn_commit_session(&path, &name) {
                            Ok(session) => self.notify(format!("{name}: committing via Claude ({session})")),
                            Err(e) => self.notify(format!("{name}: git failed — {e}")),
                        }
                    }
                }
            }
            ActionKind::GitCommitAll => {
                let spawned = orrch_core::git::spawn_commit_all(&self.projects_dir);
                if spawned.is_empty() {
                    self.notify("No projects need committing".into());
                } else {
                    let names: Vec<_> = spawned.iter().map(|(n, _)| n.as_str()).collect();
                    self.notify(format!("Committing {} projects: {}", spawned.len(), names.join(", ")));
                }
            }
            ActionKind::KillSession(sid) => { self.pm.kill_session(&sid); }
            ActionKind::SubmitFeedback(_) => { self.open_feedback_confirm(); }
            ActionKind::ResumeFeedback(_) => {
                if let Some(item) = self.feedback_items.get(self.feedback_selected) {
                    let path = item.path.clone();
                    let kind = VimKind::GlobalFeedback;
                    let title = self.vim_title(&kind);
                    self.vim_request = Some(VimRequest { file: path, kind, title });
                }
            }
            ActionKind::DeleteFeedback(idx) => {
                self.sub = SubView::ConfirmDeleteFeedback(idx);
            }
        }
        Ok(())
    }

    // ─── Feedback Confirmation ────────────────────────────────────

    /// Open the feedback confirmation overlay for the selected draft.
    fn open_feedback_confirm(&mut self) {
        if let Some(item) = self.feedback_items.get(self.feedback_selected) {
            if item.status != FeedbackStatus::Draft {
                return;
            }
            // Read the feedback text
            let text = std::fs::read_to_string(&item.path).unwrap_or_default();
            if text.trim().is_empty() {
                self.notify("Feedback is empty — write something first".into());
                return;
            }

            // Auto-detect target projects (same as current routing)
            let auto_routes = orrch_core::feedback::identify_target_projects_pub(&text, &self.projects_dir);

            // Build the editable route list: auto-detected are enabled, all others disabled
            let mut routes: Vec<(String, PathBuf, bool)> = Vec::new();
            for (name, path) in &auto_routes {
                routes.push((name.clone(), path.clone(), true));
            }
            // Add remaining projects as disabled options
            for proj in &self.projects {
                if !routes.iter().any(|(n, _, _)| *n == proj.name) {
                    routes.push((proj.name.clone(), proj.path.clone(), false));
                }
            }

            self.confirm_routes = routes;
            self.confirm_route_selected = 0;
            self.confirm_feedback_text = text;
            self.confirm_feedback_type = item.feedback_type;
            self.sub = SubView::FeedbackConfirm(self.feedback_selected);
        }
    }

    fn key_feedback_confirm(&mut self, key: KeyCode, item_idx: usize) -> Result<()> {
        match key {
            KeyCode::Esc => { self.sub = SubView::List; }
            KeyCode::Up => {
                if self.confirm_route_selected > 0 {
                    self.confirm_route_selected -= 1;
                }
            }
            KeyCode::Down => {
                if self.confirm_route_selected + 1 < self.confirm_routes.len() {
                    self.confirm_route_selected += 1;
                }
            }
            KeyCode::Char('p') => {
                // Toggle plan mode in the confirmation overlay
                self.confirm_feedback_type = if self.confirm_feedback_type == orrch_core::FeedbackType::Plan {
                    orrch_core::FeedbackType::Feedback
                } else {
                    orrch_core::FeedbackType::Plan
                };
            }
            KeyCode::Char(' ') | KeyCode::Tab => {
                // Toggle selected route on/off
                if let Some(route) = self.confirm_routes.get_mut(self.confirm_route_selected) {
                    route.2 = !route.2;
                }
            }
            KeyCode::Enter => {
                // Confirm: process via Claude
                let enabled_routes: Vec<(String, PathBuf)> = self.confirm_routes.iter()
                    .filter(|(_, _, enabled)| *enabled)
                    .map(|(name, path, _)| (name.clone(), path.clone()))
                    .collect();

                // Get the feedback file info
                if let Some(item) = self.feedback_items.get(item_idx) {
                    let feedback_path = item.path.clone();
                    let filename = item.filename.clone();
                    let projects_dir = self.projects_dir.clone();
                    let text = self.confirm_feedback_text.clone();

                    let route_names: Vec<String> = enabled_routes.iter().map(|(n, _)| n.clone()).collect();
                    let fb_type = self.confirm_feedback_type;

                    // Spawn Claude to process the feedback via /interpret-user-instructions
                    match spawn_feedback_processor(
                        &text,
                        &route_names,
                        &projects_dir,
                        fb_type,
                        &feedback_path,
                    ) {
                        Ok(session_name) => {
                            orrch_core::feedback::mark_as_processing(
                                &filename,
                                &projects_dir,
                                &route_names,
                                fb_type,
                                Some(&session_name),
                            );
                            self.reload_feedback();
                            self.notify(format!("Sent to Claude ({session_name}) — check Processing section"));
                            self.sub = SubView::List;
                        }
                        Err(e) => {
                            self.notify(format!("Failed to spawn processor: {e}"));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ─── App Menu ────────────────────────────────────────────────

    fn key_app_menu(&mut self, key: KeyCode) -> Result<()> {
        const ITEMS: &[(&str, &str)] = &[
            ("q", "Quit orrchestrator"),
            ("r", "Reload all projects"),
            ("g", "Git commit all projects"),
            ("v", "Show version info"),
        ];

        match key {
            KeyCode::Esc => { self.sub = SubView::List; }
            KeyCode::Up => {
                self.app_menu_selected = self.app_menu_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                if self.app_menu_selected + 1 < ITEMS.len() {
                    self.app_menu_selected += 1;
                }
            }
            KeyCode::Enter => {
                self.sub = SubView::List;
                match self.app_menu_selected {
                    0 => self.should_quit = true,
                    1 => { self.reload_projects(); self.notify("Reloaded".into()); }
                    2 => {
                        let spawned = orrch_core::git::spawn_commit_all(&self.projects_dir);
                        let count = spawned.len();
                        self.notify(format!("Committing {count} projects"));
                    }
                    3 => self.notify("orrchestrator v0.1.0".into()),
                    _ => {}
                }
            }
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('r') => { self.sub = SubView::List; self.reload_projects(); self.notify("Reloaded".into()); }
            KeyCode::Char('g') => {
                self.sub = SubView::List;
                let spawned = orrch_core::git::spawn_commit_all(&self.projects_dir);
                self.notify(format!("Committing {} projects", spawned.len()));
            }
            KeyCode::Char('v') => { self.sub = SubView::List; self.notify("orrchestrator v0.1.0".into()); }
            _ => {}
        }
        Ok(())
    }

    // ─── Sessions Tab ────────────────────────────────────────────

    fn key_sessions_tab(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('R') => {
                self.managed_sessions = orrch_core::windows::list_all_sessions();
                self.notify("Sessions refreshed".into());
            }
            KeyCode::Up => {
                if self.session_tab_selected == 0 { self.focus_depth = 0; }
                else { self.session_tab_selected -= 1; }
            }
            KeyCode::Down => {
                if !self.managed_sessions.is_empty() && self.session_tab_selected < self.managed_sessions.len() - 1 {
                    self.session_tab_selected += 1;
                }
            }
            KeyCode::Enter => {
                // Focus the selected session's terminal window + tmux window
                if let Some(s) = self.managed_sessions.get(self.session_tab_selected) {
                    orrch_core::windows::select_and_focus(s.category, s.index);
                }
            }
            KeyCode::Char('m') => {
                // Minimize the category's terminal window
                if let Some(s) = self.managed_sessions.get(self.session_tab_selected) {
                    let title = format!("[orrch] {}", s.category.label());
                    orrch_core::windows::minimize_window(&title);
                    self.notify(format!("Minimized {}", s.category.label()));
                }
            }
            KeyCode::Char('x') => {
                // Kill selected session
                if let Some(s) = self.managed_sessions.get(self.session_tab_selected) {
                    let cat = s.category;
                    let name = s.name.clone();
                    orrch_core::windows::kill_session(cat, &name);
                    self.managed_sessions = orrch_core::windows::list_all_sessions();
                    self.session_tab_selected = self.session_tab_selected.min(self.managed_sessions.len().saturating_sub(1));
                    self.notify(format!("Killed {name}"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ─── Commit Review ──────────────────────────────────────────

    /// Open the commit review overlay — scan fb2p.md files for pending entries.
    pub fn open_commit_review(&mut self, feedback_idx: usize) {
        let item = match self.feedback_items.get(feedback_idx) {
            Some(i) => i,
            None => return,
        };
        if item.status != FeedbackStatus::Processed {
            return;
        }

        // Scan all project fb2p.md files for entries with "Executed: pending"
        let mut packages = Vec::new();
        for proj in &self.projects {
            let fb2p_path = proj.path.join("fb2p.md");
            if let Ok(content) = std::fs::read_to_string(&fb2p_path) {
                // Find all pending entries (split on "---" and look for "Executed: pending")
                for entry in content.split("\n---\n") {
                    if entry.contains("Executed: pending") {
                        let preview: String = entry.lines()
                            .filter(|l| !l.trim().is_empty())
                            .take(5)
                            .collect::<Vec<_>>()
                            .join("\n");
                        packages.push(CommitPackage {
                            project_name: proj.name.clone(),
                            project_dir: proj.path.clone(),
                            entry_preview: preview,
                            entry_full: entry.to_string(),
                        });
                    }
                }
            }
        }

        // Also check workspace-level fb2p.md
        let ws_fb2p = self.projects_dir.join("fb2p.md");
        if let Ok(content) = std::fs::read_to_string(&ws_fb2p) {
            for entry in content.split("\n---\n") {
                if entry.contains("Executed: pending") {
                    let preview: String = entry.lines()
                        .filter(|l| !l.trim().is_empty())
                        .take(5)
                        .collect::<Vec<_>>()
                        .join("\n");
                    packages.push(CommitPackage {
                        project_name: "(workspace)".into(),
                        project_dir: self.projects_dir.clone(),
                        entry_preview: preview,
                        entry_full: entry.to_string(),
                    });
                }
            }
        }

        self.commit_packages = packages;
        self.commit_scroll = 0;
        self.commit_correction_text.clear();
        self.commit_typing_correction = false;
        self.commit_correction_session = None;
        self.sub = SubView::CommitReview(feedback_idx);
    }

    fn key_commit_review(&mut self, key: KeyCode, feedback_idx: usize) -> Result<()> {
        if self.commit_typing_correction {
            // In correction text input mode
            match key {
                KeyCode::Esc => {
                    self.commit_typing_correction = false;
                    self.commit_correction_text.clear();
                }
                KeyCode::Enter => {
                    if !self.commit_correction_text.trim().is_empty() {
                        // Send correction to Claude
                        let correction = self.commit_correction_text.clone();
                        let projects_dir = self.projects_dir.clone();
                        match spawn_correction_processor(&correction, &self.commit_packages, &projects_dir) {
                            Ok(session_name) => {
                                self.commit_correction_session = Some(session_name.clone());
                                self.commit_typing_correction = false;
                                self.commit_correction_text.clear();
                                self.sub = SubView::CommitCorrecting(feedback_idx);
                                self.notify(format!("Correcting via Claude ({session_name})"));
                            }
                            Err(e) => self.notify(format!("Correction failed: {e}")),
                        }
                    }
                }
                KeyCode::Backspace => { self.commit_correction_text.pop(); }
                KeyCode::Char(c) => { self.commit_correction_text.push(c); }
                _ => {}
            }
            return Ok(());
        }

        match key {
            KeyCode::Esc => {
                self.sub = SubView::List;
            }
            KeyCode::Up => {
                self.commit_scroll = self.commit_scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                self.commit_scroll += 1;
            }
            KeyCode::Char('y') | KeyCode::Enter => {
                // Approve — commit to routed
                if let Some(item) = self.feedback_items.get(feedback_idx) {
                    let filename = item.filename.clone();
                    orrch_core::mark_as_routed(&filename, &self.projects_dir);
                    self.reload_feedback();
                    self.reload_projects();
                    let count = self.commit_packages.len();
                    self.notify(format!("Committed {count} instruction package(s) to projects"));
                }
                self.sub = SubView::List;
            }
            KeyCode::Char('n') | KeyCode::Char('e') => {
                // Reject — enter correction mode
                self.commit_typing_correction = true;
                self.commit_correction_text.clear();
            }
            KeyCode::Char('d') => {
                // Deny — remove all pending entries from project fb2p.md files, return to draft
                let removed = self.deny_commit(feedback_idx);
                self.notify(format!("Denied — removed {removed} entries, returned to draft"));
                self.sub = SubView::List;
            }
            _ => {}
        }
        Ok(())
    }

    fn key_commit_correcting(&mut self, key: KeyCode, feedback_idx: usize) -> Result<()> {
        // Check if the correction session has finished
        if let Some(ref session) = self.commit_correction_session {
            let exists = std::process::Command::new("tmux")
                .args(["has-session", "-t", session])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if !exists {
                // Claude finished — refresh packages and return to review
                self.commit_correction_session = None;
                self.open_commit_review(feedback_idx);
                self.notify("Correction complete — review revised packages".into());
                return Ok(());
            }
        }

        match key {
            KeyCode::Esc => {
                // Cancel waiting — go back to review
                if let Some(ref session) = self.commit_correction_session {
                    let _ = std::process::Command::new("tmux")
                        .args(["kill-session", "-t", session])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
                self.commit_correction_session = None;
                self.sub = SubView::CommitReview(feedback_idx);
            }
            _ => {}
        }
        Ok(())
    }

    /// Deny a commit — remove ONLY the specific entries from this processing run
    /// (identified by matching against commit_packages), and return feedback to draft.
    fn deny_commit(&mut self, feedback_idx: usize) -> usize {
        let mut removed = 0;

        // Build a set of entry texts that belong to THIS processing run
        // (the ones displayed in the commit review overlay)
        let package_texts: Vec<String> = self.commit_packages.iter()
            .map(|pkg| pkg.entry_full.trim().to_string())
            .collect();

        // For each project that had packages, scan its fb2p.md and remove only matching entries
        let mut seen_dirs = std::collections::HashSet::new();
        for pkg in &self.commit_packages {
            if !seen_dirs.insert(pkg.project_dir.clone()) {
                continue; // already processed this project
            }
            let fb2p_path = pkg.project_dir.join("fb2p.md");
            let Ok(content) = std::fs::read_to_string(&fb2p_path) else { continue };

            let mut kept = Vec::new();
            let mut current_entry = String::new();
            for line in content.lines() {
                if line == "---" && !current_entry.is_empty() {
                    let trimmed = current_entry.trim().to_string();
                    if package_texts.iter().any(|pt| trimmed.contains(pt.as_str()) || pt.contains(trimmed.as_str())) {
                        removed += 1;
                    } else {
                        kept.push(current_entry.clone());
                    }
                    current_entry.clear();
                    continue;
                }
                current_entry.push_str(line);
                current_entry.push('\n');
            }
            // Handle last entry (no trailing ---)
            if !current_entry.trim().is_empty() {
                let trimmed = current_entry.trim().to_string();
                if package_texts.iter().any(|pt| trimmed.contains(pt.as_str()) || pt.contains(trimmed.as_str())) {
                    removed += 1;
                } else {
                    kept.push(current_entry);
                }
            }

            let new_content = kept.join("\n---\n");
            if new_content.trim().is_empty() {
                let _ = std::fs::remove_file(&fb2p_path);
            } else {
                let _ = std::fs::write(&fb2p_path, new_content);
            }
        }

        // Return feedback file to draft
        if let Some(item) = self.feedback_items.get(feedback_idx) {
            let feedback_dir = self.projects_dir.join(".feedback");
            let mut status_map = orrch_core::feedback::load_status_map_pub(&feedback_dir);
            if let Some(meta) = status_map.get_mut(&item.filename) {
                meta.status = FeedbackStatus::Draft;
                meta.routes.clear();
                meta.submitted_at = None;
                meta.tmux_session = None;
            }
            orrch_core::feedback::save_status_map_pub(&feedback_dir, &status_map);
        }
        self.reload_feedback();

        removed
    }

    // ─── New Project Wizard ──────────────────────────────────────

    fn key_new_project_name(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Esc => { self.sub = SubView::List; }
            KeyCode::Enter => {
                let name = self.new_project_name.trim().to_string();
                if name.is_empty() {
                    self.new_project_error = Some("Name cannot be empty".into());
                    return Ok(());
                }
                // Validate: no spaces, no special chars except hyphens/underscores
                if name.contains(|c: char| c.is_whitespace() || "/$@!#%^&*()+=[]{}|;:',<>?\"".contains(c)) {
                    self.new_project_error = Some("Use lowercase letters, numbers, hyphens only".into());
                    return Ok(());
                }
                let target = self.projects_dir.join(&name);
                if target.exists() {
                    self.new_project_error = Some(format!("'{}' already exists", name));
                    return Ok(());
                }
                self.new_project_error = None;
                self.sub = SubView::NewProjectScope;
            }
            KeyCode::Backspace => { self.new_project_name.pop(); self.new_project_error = None; }
            KeyCode::Char(c) => { self.new_project_name.push(c); self.new_project_error = None; }
            _ => {}
        }
        Ok(())
    }

    fn key_new_project_scope(&mut self, key: KeyCode) -> Result<()> {
        use orrch_core::Scope;
        match key {
            KeyCode::Esc => { self.sub = SubView::NewProjectName; }
            KeyCode::Tab | KeyCode::Down => {
                self.new_project_scope = match self.new_project_scope {
                    Scope::Personal => Scope::Private,
                    Scope::Private => Scope::Public,
                    Scope::Public => Scope::Commercial,
                    Scope::Commercial => Scope::Personal,
                };
            }
            KeyCode::Up => {
                self.new_project_scope = match self.new_project_scope {
                    Scope::Personal => Scope::Commercial,
                    Scope::Private => Scope::Personal,
                    Scope::Public => Scope::Private,
                    Scope::Commercial => Scope::Public,
                };
            }
            KeyCode::Enter => {
                self.sub = SubView::NewProjectConfirm;
            }
            _ => {}
        }
        Ok(())
    }

    fn key_new_project_confirm(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Esc => { self.sub = SubView::NewProjectScope; }
            KeyCode::Char('y') | KeyCode::Enter => {
                let name = self.new_project_name.trim().to_string();
                let path = self.projects_dir.join(&name);
                match self.create_new_project(&path) {
                    Ok(()) => {
                        self.reload_projects();
                        // Select the new project and spawn a "create plan" session
                        if let Some(idx) = self.projects.iter().position(|p| p.name == name) {
                            self.spawn_project_idx = idx;
                            let p = self.projects[idx].path.clone();
                            let _ = self.spawn_session(&p, BackendKind::Claude, Some("Create a master development plan for this project. Read any existing files for context, then write a comprehensive PLAN.md with architecture decisions, feature roadmap, and technical stack."));
                            self.notify(format!("Created {} — spawning plan session", name));
                        } else {
                            self.notify(format!("Created {}", name));
                        }
                    }
                    Err(e) => {
                        self.new_project_error = Some(format!("Failed: {e}"));
                        self.sub = SubView::NewProjectName;
                        return Ok(());
                    }
                }
                self.sub = SubView::List;
            }
            KeyCode::Char('n') => { self.sub = SubView::NewProjectName; }
            _ => {}
        }
        Ok(())
    }

    /// Create the project directory and scaffold files.
    fn create_new_project(&self, path: &std::path::Path) -> anyhow::Result<()> {
        std::fs::create_dir(path)?;

        // Write .scope
        std::fs::write(path.join(".scope"), self.new_project_scope.label())?;

        // Write .orrtemp if hot
        if self.new_project_temp == Temperature::Hot {
            std::fs::write(path.join(".orrtemp"), "hot")?;
        }

        // Create a minimal CLAUDE.md
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        std::fs::write(
            path.join("CLAUDE.md"),
            format!("# {name}\n\nProject instructions for Claude Code.\n"),
        )?;

        Ok(())
    }

    // ─── Vim Integration ─────────────────────────────────────────

    /// Build a descriptive window title for a vim editing session.
    fn vim_title(&self, kind: &VimKind) -> String {
        match kind {
            VimKind::GlobalFeedback => "[orrchestrator] Feedback".into(),
            VimKind::ProjectFeedback(idx) => {
                let name = self.projects.get(*idx).map(|p| p.name.as_str()).unwrap_or("?");
                format!("[orrchestrator] {name} Feedback")
            }
            VimKind::MasterPlanAppend(idx) => {
                let name = self.projects.get(*idx).map(|p| p.name.as_str()).unwrap_or("?");
                format!("[orrchestrator] {name} Master Plan")
            }
            VimKind::NewIdea => "[orrchestrator] New Idea".into(),
            VimKind::IntakeReview => "[orrchestrator] Intake Review".into(),
        }
    }

    /// Request an external vim session. Creates the temp file and sets vim_request.
    fn request_vim(&mut self, kind: VimKind) {
        let file = match &kind {
            VimKind::GlobalFeedback | VimKind::ProjectFeedback(_) => {
                orrch_core::create_draft(&self.projects_dir).ok()
            }
            VimKind::MasterPlanAppend(_) => {
                orrch_core::create_append_draft(&self.projects_dir).ok()
            }
            VimKind::NewIdea => {
                let vault = orrch_core::vault::vault_dir(&self.projects_dir);
                orrch_core::vault::save_idea(&vault, "").ok()
            }
            VimKind::IntakeReview => {
                // Intake review uses an existing file — handled by caller
                None
            }
        };
        if let Some(file) = file {
            let title = self.vim_title(&kind);
            self.vim_request = Some(VimRequest { file, kind, title });
        }
    }

    /// Called by the main loop when a pending editor's vim process exits.
    /// Also called after blocking vim in the same terminal.
    pub fn handle_vim_complete(&mut self, file: &std::path::Path, kind: VimKind) {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        if text.trim().is_empty() {
            // Empty file — clean up
            let _ = std::fs::remove_file(file);
            return;
        }

        match kind {
            VimKind::GlobalFeedback => {
                // Save as draft — user submits from Feedback tab
                self.reload_feedback();
                self.notify("Feedback saved as draft".into());
                self.panel = Panel::Design;
                self.sub = SubView::List;
            }
            VimKind::ProjectFeedback(proj_idx) => {
                // Direct-route to specific project
                if let Some(proj) = self.projects.get(proj_idx) {
                    let name = proj.name.clone();
                    let path = proj.path.clone();
                    let ts = orrch_core::feedback::chrono_lite_timestamp();
                    let _ = orrch_core::feedback::append_to_fb2p_direct(&text, &path, &ts);
                    self.reload_projects();
                    self.notify(format!("Feedback → {name}"));
                }
                // Clean up the temp file since it was directly routed
                let _ = std::fs::remove_file(file);
            }
            VimKind::MasterPlanAppend(proj_idx) => {
                if let Some(proj) = self.projects.get(proj_idx) {
                    let name = proj.name.clone();
                    let path = proj.path.clone();
                    let mp_path = path.join("MASTER_PLAN.md");
                    let ts = orrch_core::feedback::chrono_lite_timestamp();
                    let header = if !mp_path.exists() { format!("# {name} — Master Plan\n") } else { String::new() };
                    let appendix = format!("{header}\n---\n\n## Append: {ts}\n\n{}\n", text.trim());
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&mp_path) {
                        let _ = f.write_all(appendix.as_bytes());
                    }
                    let _ = orrch_core::feedback::append_to_fb2p_direct(
                        &format!("[Master Plan]\n{}", text.trim()), &path, &ts);
                    self.reload_projects();
                    self.notify(format!("Appended to {name} master plan"));
                }
                let _ = std::fs::remove_file(file);
            }
            VimKind::NewIdea => {
                // Already saved in vault dir by request_vim
                let vault = orrch_core::vault::vault_dir(&self.projects_dir);
                self.ideas = orrch_core::vault::load_ideas(&vault);
                self.notify("Saved".into());
            }
            VimKind::IntakeReview => {
                // Read edited optimized text back into the review
                if let Some(ref mut review) = self.intake_review {
                    review.optimized = text.clone();
                }
                self.notify("Optimized text updated — y=confirm N=reject".into());
                // Clean up the temp edit file
                let _ = std::fs::remove_file(file);
            }
        }
        // Always reload library + workforce data after any vim edit
        self.reload_all_library_data();
    }

    /// Check if any pending vim editors have finished (called each tick by main loop).
    pub fn check_pending_editors(&mut self) {
        let mut completed = Vec::new();
        for (i, pe) in self.pending_editors.iter_mut().enumerate() {
            if let Ok(Some(_)) = pe.child.try_wait() {
                completed.push(i);
            }
        }
        for i in completed.into_iter().rev() {
            let pe = self.pending_editors.remove(i);
            self.handle_vim_complete(&pe.file, pe.kind);
        }
    }

    /// Reload feedback items from disk.
    pub fn reload_feedback(&mut self) {
        // Check for processing items that have completed (tmux session gone)
        for item in &self.feedback_items {
            if item.status == FeedbackStatus::Processing {
                if orrch_core::check_processing_complete(&item.filename, &self.projects_dir) {
                    orrch_core::mark_as_processed(&item.filename, &self.projects_dir);
                }
            }
        }
        self.feedback_items = orrch_core::load_feedback_items(&self.projects_dir);
        // Sort by status group so display order matches navigation order:
        // Drafts first, then Processing/Processed, then Routed
        self.feedback_items.sort_by_key(|item| match item.status {
            FeedbackStatus::Draft => 0,
            FeedbackStatus::Processing => 1,
            FeedbackStatus::Processed => 1,
            FeedbackStatus::Routed => 2,
        });
    }

    // ─── Feedback Tab ────────────────────────────────────────────

    fn key_feedback_tab(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('f') => {
                self.request_vim(VimKind::GlobalFeedback);
            }
            KeyCode::Char('R') => {
                self.reload_feedback();
                self.notify("Feedback reloaded".into());
            }
            KeyCode::Char('s') => {
                // Open confirmation overlay for selected draft
                self.open_feedback_confirm();
            }
            KeyCode::Char('r') => {
                // Resume editing — open in vim
                if let Some(item) = self.feedback_items.get(self.feedback_selected) {
                    if item.status == FeedbackStatus::Draft {
                        let path = item.path.clone();
                        let kind = VimKind::GlobalFeedback;
                        let title = self.vim_title(&kind);
                        self.vim_request = Some(VimRequest {
                            file: path,
                            kind,
                            title,
                        });
                    }
                }
            }
            KeyCode::Char('p') => {
                // Toggle plan mode on selected draft
                if let Some(item) = self.feedback_items.get(self.feedback_selected) {
                    if item.status == FeedbackStatus::Draft {
                        let filename = item.filename.clone();
                        let new_type = if item.feedback_type == orrch_core::FeedbackType::Plan {
                            orrch_core::FeedbackType::Feedback
                        } else {
                            orrch_core::FeedbackType::Plan
                        };
                        orrch_core::set_feedback_type(&filename, &self.projects_dir, new_type);
                        self.reload_feedback();
                        self.notify(format!("→ {}", new_type.label()));
                    }
                }
            }
            KeyCode::Char('c') => {
                // Open commit review overlay for processed feedback
                if let Some(item) = self.feedback_items.get(self.feedback_selected) {
                    if item.status == FeedbackStatus::Processed {
                        self.open_commit_review(self.feedback_selected);
                    }
                }
            }
            KeyCode::Char('u') => {
                // Recall — kill tmux if running, remove pending entries, return to draft
                if let Some(item) = self.feedback_items.get(self.feedback_selected) {
                    if item.status != FeedbackStatus::Draft {
                        // Kill tmux session if still alive
                        if let Some(ref session) = item.tmux_session {
                            let _ = std::process::Command::new("tmux")
                                .args(["kill-session", "-t", session.as_str()])
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .status();
                        }
                        let filename = item.filename.clone();
                        let routes = item.routes.clone();

                        // Scan routed projects and remove entries with "Executed: pending"
                        let mut removed = 0;
                        for route_name in &routes {
                            if let Some(proj) = self.projects.iter().find(|p| p.name == *route_name) {
                                let fb2p_path = proj.path.join("fb2p.md");
                                if let Ok(content) = std::fs::read_to_string(&fb2p_path) {
                                    let mut kept = Vec::new();
                                    for entry in content.split("\n---\n") {
                                        if entry.contains("Executed: pending") {
                                            removed += 1;
                                        } else {
                                            kept.push(entry.to_string());
                                        }
                                    }
                                    let new_content = kept.join("\n---\n");
                                    if new_content.trim().is_empty() {
                                        let _ = std::fs::remove_file(&fb2p_path);
                                    } else {
                                        let _ = std::fs::write(&fb2p_path, new_content);
                                    }
                                }
                            }
                        }

                        // Return to draft
                        let feedback_dir = self.projects_dir.join(".feedback");
                        let mut status_map = orrch_core::feedback::load_status_map_pub(&feedback_dir);
                        if let Some(meta) = status_map.get_mut(&filename) {
                            meta.status = FeedbackStatus::Draft;
                            meta.routes.clear();
                            meta.submitted_at = None;
                            meta.tmux_session = None;
                        }
                        orrch_core::feedback::save_status_map_pub(&feedback_dir, &status_map);
                        self.reload_feedback();
                        self.reload_projects();
                        self.notify(format!("Recalled — {removed} pending entries removed, returned to draft"));
                    }
                }
            }
            KeyCode::Char('d') => {
                // Confirm delete
                if !self.feedback_items.is_empty() {
                    self.sub = SubView::ConfirmDeleteFeedback(self.feedback_selected);
                }
            }
            KeyCode::Enter => {
                if let Some(item) = self.feedback_items.get(self.feedback_selected) {
                    match item.status {
                        FeedbackStatus::Draft => self.open_feedback_confirm(),
                        FeedbackStatus::Processed => self.open_commit_review(self.feedback_selected),
                        _ => {}
                    }
                }
            }
            KeyCode::Up => {
                if self.feedback_selected == 0 { self.focus_depth = 0; }
                else { self.feedback_selected -= 1; }
            }
            KeyCode::Down => {
                if !self.feedback_items.is_empty() && self.feedback_selected < self.feedback_items.len() - 1 {
                    self.feedback_selected += 1;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_confirm_delete_feedback(&mut self, key: KeyCode, idx: usize) -> Result<()> {
        if key == KeyCode::Char('y') || key == KeyCode::Char('Y') {
            if let Some(item) = self.feedback_items.get(idx) {
                let filename = item.filename.clone();
                orrch_core::delete_feedback(&filename, &self.projects_dir);
                self.reload_feedback();
                if self.feedback_selected > 0 && self.feedback_selected >= self.feedback_items.len() {
                    self.feedback_selected = self.feedback_items.len().saturating_sub(1);
                }
                self.notify("Deleted".into());
            }
        }
        self.sub = SubView::List;
        Ok(())
    }

    fn key_routing_summary(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Enter => {
                for (_, path) in &self.routing_result.clone() {
                    let _ = self.spawn_session(path, BackendKind::Claude, Some(CONTINUE_DEV_PROMPT));
                }
                self.routing_result.clear();
                self.sub = SubView::List;
            }
            KeyCode::Esc => { self.routing_result.clear(); self.sub = SubView::List; }
            _ => {}
        }
        Ok(())
    }

    fn key_confirm_deprecate(&mut self, key: KeyCode, proj_idx: usize) -> Result<()> {
        if key == KeyCode::Char('y') || key == KeyCode::Char('Y') {
            let proj_data = self.projects.get(proj_idx).map(|p| (p.name.clone(), p.path.clone()));
            if let Some((name, src)) = proj_data {
                let dest = self.projects_dir.join("deprecated").join(&name);
                let _ = std::fs::create_dir_all(self.projects_dir.join("deprecated"));
                if dest.exists() {
                    self.notify(format!("deprecated/{name} already exists"));
                } else if let Err(e) = std::fs::rename(&src, &dest) {
                    self.notify(format!("Move failed: {e}"));
                } else {
                    self.reload_projects();
                    self.project_selected = self.project_selected.min(self.projects.len().saturating_sub(1));
                    self.notify(format!("{name} → deprecated/"));
                }
            }
        }
        self.sub = SubView::List;
        Ok(())
    }

    fn key_confirm_complete(&mut self, key: KeyCode, proj_idx: usize) -> Result<()> {
        if key == KeyCode::Char('y') || key == KeyCode::Char('Y') {
            let proj_data = self.projects.get(proj_idx).map(|p| (p.name.clone(), p.path.clone()));
            if let Some((name, path)) = proj_data {
                if path.join("v1").exists() {
                    self.notify(format!("{name} already has v1/ — use versioning-init for v2"));
                } else {
                    match orrch_core::package_as_v1(&path) {
                        Ok(()) => {
                            self.reload_projects();
                            self.notify(format!("{name} → v1/ (now in Production)"));
                        }
                        Err(e) => self.notify(format!("Failed: {e}")),
                    }
                }
            }
        }
        self.sub = SubView::List;
        Ok(())
    }

    pub fn retrospect_stats_for(&mut self, project_dir: &str) -> Option<orrch_retrospect::store::StoreStats> {
        self.error_stores.get_mut(project_dir).map(|s| s.stats())
    }

    fn key_workflow_picker(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Up => {
                if self.workflow_picker_idx > 0 {
                    self.workflow_picker_idx -= 1;
                }
            }
            KeyCode::Down => {
                if self.workflow_picker_idx + 1 < self.workflow_choices.len() {
                    self.workflow_picker_idx += 1;
                }
            }
            KeyCode::Enter => {
                if let Some((script, display)) = self.workflow_choices.get(self.workflow_picker_idx).cloned() {
                    if let Some(pidx) = self.selected_project_index() {
                        let path = self.projects[pidx].path.clone();
                        let name = self.projects[pidx].name.clone();
                        match orrch_core::windows::spawn_workflow(&path, &script, "continue development") {
                            Ok(window) => {
                                self.notify(format!("{name}: {display} → tmux:{window}"));
                            }
                            Err(e) => {
                                self.notify(format!("{name}: workflow failed: {e}"));
                            }
                        }
                    } else {
                        self.notify("No project selected".into());
                    }
                }
                self.sub = SubView::List;
            }
            KeyCode::Esc => {
                self.sub = SubView::List;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Scan projects for versioned releases (v1/, v2/, etc.)
/// Scan for release versions. v1 = first stable release. Higher = in development.
/// Only v1 (or explicitly tagged releases) appear in production.
fn scan_production(projects: &[Project]) -> Vec<ProductionEntry> {
    let mut entries = Vec::new();
    for proj in projects {
        if proj.meta.version_dirs.is_empty() {
            continue;
        }
        if proj.meta.version_dirs.iter().any(|v| v == "v1") {
            let v1_path = proj.path.join("v1");
            if !v1_path.join(".orrnorelease").exists() {
                entries.push(ProductionEntry {
                    project_name: proj.name.clone(),
                    version: "v1".into(),
                    path: v1_path,
                    working: true,
                });
            }
        }
        // Future: check for .orrrelease marker files in higher versions
    }
    entries
}

/// Count KWin windows with "[orrchestrator]" in their title.
/// These are vim editors from a previous orrchestrator session that survived.
fn count_orphaned_editor_windows() -> usize {
    // Use pgrep to find vim processes whose command line includes [orrchestrator]
    let output = std::process::Command::new("pgrep")
        .args(["-f", r#"\[orrchestrator\]"#])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).lines().count()
        }
        _ => 0,
    }
}

/// Spawn a Claude session to correct feedback routing based on user instructions.
fn spawn_correction_processor(
    correction: &str,
    packages: &[CommitPackage],
    projects_dir: &Path,
) -> anyhow::Result<String> {
    let feedback_dir = projects_dir.join(".feedback");
    std::fs::create_dir_all(&feedback_dir)?;

    let mut context = String::new();
    for pkg in packages {
        context.push_str(&format!("\n### Project: {}\n{}\n", pkg.project_name, pkg.entry_full));
    }

    let prompt_path = feedback_dir.join(".correction-prompt.md");
    std::fs::write(&prompt_path, format!(
        "Correct feedback routing. Current pending entries:\n{context}\n\nUser correction: {correction}\n\n\
         Find 'Executed: pending' entries in project fb2p.md files, apply corrections. \
         Do NOT touch entries without 'Executed: pending'. Do NOT create new project directories.",
    ))?;

    let cmd = format!(
        "cd {} && prompt=$(cat {}) && claude --dangerously-skip-permissions \"$prompt\" && rm -f {}",
        projects_dir.display(), prompt_path.display(), prompt_path.display(),
    );

    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    orrch_core::windows::spawn_in_category(
        orrch_core::windows::SessionCategory::Proc,
        &format!("fix-{}", ts % 100000),
        &cmd,
    )
}

/// Kill an existing tmux session if it exists, then create a new one running a script.
/// Returns Ok(()) on success.
fn tmux_spawn_session(session_name: &str, runner_path: &Path) -> anyhow::Result<()> {
    // Kill any existing session with this name (prevents "duplicate session" errors)
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let status = std::process::Command::new("tmux")
        .args(["new-session", "-d", "-s", session_name])
        .arg("bash")
        .arg(runner_path.to_string_lossy().as_ref())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => anyhow::bail!("tmux exited with {}", s),
        Err(e) => anyhow::bail!("Failed to run tmux: {e}"),
    }
}

fn default_projects_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
    PathBuf::from(home).join("projects")
}

/// Scan a directory for .md files and return (title, path) pairs.
/// Title is extracted from the first `## ` heading or the filename.
fn scan_md_dir(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                let name = if let Ok(content) = std::fs::read_to_string(&path) {
                    // Try to extract name from frontmatter or heading
                    content.lines()
                        .find_map(|line| {
                            let t = line.trim();
                            if let Some(h) = t.strip_prefix("## ") { Some(h.to_string()) }
                            else if let Some(n) = t.strip_prefix("name:") { Some(n.trim().to_string()) }
                            else { None }
                        })
                        .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string())
                } else {
                    path.file_stem().unwrap_or_default().to_string_lossy().to_string()
                };
                items.push((name, path));
            }
        }
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

/// Spawn a hidden tmux session that runs Claude to process feedback.
///
/// Claude reads the raw feedback text and runs the /interpret-user-instructions
/// pipeline: analyze → generate optimized prompts → route to project fb2p.md files.
/// The prompt is written to a temp file to avoid shell escaping issues.
/// Spawn a Claude Code session to process a feedback file.
///
/// Simply runs: claude --dangerously-skip-permissions "/interpret-user-instructions <filepath>"
/// Claude's own CLAUDE.md defines the full pipeline.
fn spawn_feedback_processor(
    _feedback_text: &str,
    _target_projects: &[String],
    projects_dir: &Path,
    _fb_type: orrch_core::FeedbackType,
    feedback_file: &Path,
) -> anyhow::Result<String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let window_name = format!("fb-{}", ts % 100000);
    let file_path = feedback_file.to_string_lossy();
    let projects_dir_str = projects_dir.to_string_lossy();

    let cmd = format!(
        "cd {} && claude --dangerously-skip-permissions '/interpret-user-instructions {}'",
        projects_dir_str, file_path,
    );

    let name = orrch_core::windows::spawn_in_category(
        orrch_core::windows::SessionCategory::Proc,
        &window_name,
        &cmd,
    )?;
    Ok(name)
}

/// Detect which KWin output orrchestrator is currently running on.
/// Uses the active output at startup (orrchestrator should be the focused window).
fn detect_current_output() -> Option<String> {
    let out = std::process::Command::new("qdbus")
        .args(["org.kde.KWin", "/KWin", "org.kde.KWin.activeOutputName"])
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}
