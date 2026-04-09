use crate::profile::AgentProfile;
use orrch_workforce::{Operation, ResolvedStep, Workforce};
use std::path::Path;

/// Convert an agent profile name to its expected filename.
/// "Project Manager" → "project_manager"
fn profile_name_to_filename(name: &str) -> String {
    name.to_lowercase().replace(' ', "_")
}

/// Builds a lean context string for the Hypervisor agent.
///
/// Includes only: workforce header, team roster with file paths (NOT profile bodies),
/// operation step tables with trigger/blocker/interrupt, and an instruction to load
/// profiles on demand via the Read tool.
///
/// Target: ~2,000 tokens vs ~15,000 with the old build_workforce_context().
pub fn build_hypervisor_context(
    workforce: &Workforce,
    operations: &[Operation],
    agents_dir: &Path,
) -> String {
    let mut out = String::new();

    // --- Workforce header ---
    out.push_str(&format!("## Workforce: {}\n\n", workforce.name));
    out.push_str(&workforce.description);
    out.push_str("\n\n");

    // --- Team roster (names + paths only, NO profile bodies) ---
    out.push_str("### Team Roster\n\n");
    out.push_str("| ID | Role | User-Facing | Profile Path |\n");
    out.push_str("|----|------|-------------|-------------|\n");
    for node in &workforce.agents {
        let facing = if node.user_facing { "yes" } else { "no" };
        let filename = profile_name_to_filename(&node.agent_profile);
        let path = agents_dir.join(format!("{}.md", filename));
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            node.id, node.agent_profile, facing, path.display(),
        ));
    }
    out.push('\n');

    // --- Operation step tables ---
    let wf_ops_lower: Vec<String> = workforce
        .operations
        .iter()
        .map(|o| o.to_lowercase())
        .collect();

    for op in operations {
        if !wf_ops_lower.contains(&op.name.to_lowercase()) {
            continue;
        }
        out.push_str(&format!("### Operation: {}\n\n", op.name));
        out.push_str(&format!("Trigger: {}\n", op.trigger));
        if let Some(blocker) = &op.blocker {
            out.push_str(&format!("Blocker: {}\n", blocker));
        }
        out.push('\n');

        out.push_str("| Step | Agent | Tool/Skill | Action |\n");
        out.push_str("|------|-------|------------|--------|\n");
        for step in &op.steps {
            let tool = step.tool_or_skill.as_deref().unwrap_or("");
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                step.index, step.agent, tool, step.operation,
            ));
        }
        if !op.interrupts.is_empty() {
            let descs: Vec<String> = op.interrupts.iter().map(|i| i.to_string()).collect();
            out.push_str(&format!("\nInterrupts: {}\n", descs.join("; ")));
        }
        out.push('\n');
    }

    // --- On-demand loading instruction ---
    out.push_str("### Loading Agent Profiles\n\n");
    out.push_str("Agent profile bodies are NOT included here to conserve tokens. ");
    out.push_str("When you need to spawn a subagent, use the Read tool to load their ");
    out.push_str("profile from the path in the Team Roster table above. Extract the ");
    out.push_str("prompt body (everything after the YAML frontmatter), then pass it ");
    out.push_str("as the agent's system prompt when spawning via the Agent tool.\n");

    out
}

/// Constructs the full prompt for launching an agent-driven session.
///
/// The agent's profile body becomes a preamble that shapes Claude's behavior,
/// followed by a separator and the actual task to execute.
pub struct AgentRunner;

impl AgentRunner {
    /// Build the full prompt for a session driven by an agent profile.
    ///
    /// Returns the prompt string to pass to the AI backend's `-p` flag.
    /// The prompt structure:
    /// 1. Agent identity and behavioral rules (from profile .md body)
    /// 2. Core context (project summary, if provided)
    /// 3. Task instruction
    pub fn build_prompt(
        agent: &AgentProfile,
        task: &str,
        core_context: Option<&str>,
    ) -> String {
        let mut parts = Vec::with_capacity(3);

        // Agent identity preamble
        parts.push(agent.prompt.clone());

        // Core context (shared reference info, never current-task state)
        if let Some(ctx) = core_context {
            if !ctx.is_empty() {
                parts.push(format!("## Core Context\n\n{}", ctx));
            }
        }

        // Task instruction
        parts.push(format!("## Your Task\n\n{}", task));

        parts.join("\n\n---\n\n")
    }

    /// Build a prompt for a verification agent with context isolation.
    ///
    /// Verification agents (Feature Tester, Beta Tester, Penetration Tester)
    /// receive ONLY the deliverable — no implementation reasoning, no other
    /// verifier's results. This forces genuine independent assessment.
    pub fn build_verification_prompt(
        agent: &AgentProfile,
        deliverable_description: &str,
        core_context: Option<&str>,
    ) -> String {
        let mut parts = Vec::with_capacity(3);

        // Agent identity
        parts.push(agent.prompt.clone());

        // Core context (historical only)
        if let Some(ctx) = core_context {
            if !ctx.is_empty() {
                parts.push(format!("## Core Context\n\n{}", ctx));
            }
        }

        // Deliverable only — no implementation notes
        parts.push(format!(
            "## Verification Task\n\n\
            You are performing independent verification. You have NOT seen any other \
            agent's assessment of this work. Form your own conclusions.\n\n\
            ### Deliverable to Verify\n\n{}",
            deliverable_description
        ));

        parts.join("\n\n---\n\n")
    }

    /// Build a prompt for a step whose model selection has been resolved at
    /// runtime via [`orrch_workforce::resolve_step_for_dispatch`].
    ///
    /// This is the Task 35 + Task 57 runtime entry point: it threads the
    /// optional per-step model override into the prompt as a directive block
    /// that downstream backends can key on, and preserves nested-workforce
    /// provenance for dispatchers that want to expand inner workforces.
    ///
    /// Behavior:
    /// - Always calls [`build_prompt`] for the baseline agent prompt.
    /// - If `resolved.model_override.is_some()`, prepends a `## Model Override`
    ///   block so the backend/runner layer sees the directive even when the
    ///   raw backend API doesn't have a structured model field.
    /// - If `resolved.nested_workforce.is_some()`, appends a short marker so
    ///   hypervisors can detect the nested unit boundary.
    pub fn build_prompt_for_resolved_step(
        agent: &AgentProfile,
        task: &str,
        core_context: Option<&str>,
        resolved: &ResolvedStep,
    ) -> String {
        let base = Self::build_prompt(agent, task, core_context);

        let mut parts: Vec<String> = Vec::with_capacity(3);

        if let Some(model) = resolved.model_override.as_ref() {
            parts.push(format!(
                "## Model Override\n\n\
                This step has been dispatched with an explicit model selection. \
                The effective model is `{}`. Backends that accept a model \
                directive should honor this selection.",
                model
            ));
        }

        parts.push(base);

        if let Some(inner) = resolved.nested_workforce.as_ref() {
            parts.push(format!(
                "## Nested Workforce\n\n\
                This step was resolved through a nested workforce expansion. \
                The parent step targeted an agent node that delegates to the \
                inner workforce `{}`. The output agent of that inner workforce \
                is running now. If you need to coordinate across the inner \
                workforce, spawn its team members via the Agent tool.",
                inner.name
            ));
        }

        parts.join("\n\n---\n\n")
    }

    /// Build a prompt for an inter-agent handoff within a workflow.
    ///
    /// The receiving agent gets the previous agent's output as handoff context,
    /// injected via prompt. This is the "prompt injection" communication path
    /// used within tightly-coupled workflows.
    pub fn build_handoff_prompt(
        agent: &AgentProfile,
        task: &str,
        handoff_from: &str,
        handoff_content: &str,
        core_context: Option<&str>,
    ) -> String {
        let mut parts = Vec::with_capacity(4);

        parts.push(agent.prompt.clone());

        if let Some(ctx) = core_context {
            if !ctx.is_empty() {
                parts.push(format!("## Core Context\n\n{}", ctx));
            }
        }

        // Handoff from previous agent (compressed to drop reasoning/preamble)
        let compressed = compress_handoff(handoff_content);
        parts.push(format!(
            "## Handoff from {}\n\n{}",
            handoff_from, compressed
        ));

        parts.push(format!("## Your Task\n\n{}", task));

        parts.join("\n\n---\n\n")
    }
}

/// Compress an agent's output for handoff to the next agent.
/// Strips reasoning blocks (<thinking>...</thinking>), drops common preamble
/// lines that don't carry decision content, and collapses excessive blank lines.
/// Preserves substantive output: code blocks, file paths, conclusions.
pub fn compress_handoff(text: &str) -> String {
    // --- Step 1: Strip <thinking>...</thinking> blocks (multiline-safe) ---
    let stripped = strip_thinking_blocks(text);

    // --- Step 2: Drop preamble lines (unless inside a fenced code block) ---
    const PREAMBLE_PREFIXES: &[&str] = &[
        "let me ",
        "i'll start by",
        "i'll begin",
        "first, i need to",
        "first, let me",
        "looking at this",
        "looking at the",
        "i'm going to",
        "let's ",
        "okay, ",
        "ok, ",
        "alright",
    ];

    let mut kept: Vec<String> = Vec::new();
    let mut in_code_block = false;
    for line in stripped.lines() {
        let trimmed = line.trim_start();
        // Toggle code-block state on fence lines
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            kept.push(line.to_string());
            continue;
        }
        if in_code_block {
            kept.push(line.to_string());
            continue;
        }
        // Outside code block: check for preamble match
        let lower = trimmed.to_lowercase();
        let is_preamble = PREAMBLE_PREFIXES.iter().any(|p| lower.starts_with(p));
        if is_preamble {
            continue;
        }
        kept.push(line.to_string());
    }

    // --- Step 3: Collapse 3+ consecutive blank lines into a single blank line ---
    let mut result: Vec<String> = Vec::with_capacity(kept.len());
    let mut blank_run = 0usize;
    for line in kept {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                result.push(line);
            }
            // drop additional blanks
        } else {
            blank_run = 0;
            result.push(line);
        }
    }

    result.join("\n")
}

/// Strip `<thinking>...</thinking>` blocks from text via a simple state machine.
/// Non-greedy, multiline-safe. Unclosed opening tags drop everything after them.
fn strip_thinking_blocks(text: &str) -> String {
    const OPEN: &str = "<thinking>";
    const CLOSE: &str = "</thinking>";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        match rest.find(OPEN) {
            None => {
                out.push_str(rest);
                break;
            }
            Some(open_idx) => {
                out.push_str(&rest[..open_idx]);
                let after_open = &rest[open_idx + OPEN.len()..];
                match after_open.find(CLOSE) {
                    None => {
                        // Unclosed: drop the rest
                        break;
                    }
                    Some(close_idx) => {
                        rest = &after_open[close_idx + CLOSE.len()..];
                    }
                }
            }
        }
    }
    out
}

/// Task AP: load a project's core context file by filename, relative to the
/// project root. Accepts any profile filename — typically `CLAUDE.md`,
/// `GEMINI.md`, or a custom per-project profile — and reads its contents.
///
/// Returns `None` when the file is missing or unreadable. Callers that need a
/// hard default should pair this with `Project::agent_profile_filename()` in
/// orrch-core, which returns `"CLAUDE.md"` when the project has no explicit
/// profile set.
///
/// This helper deliberately takes a plain `&str` filename instead of importing
/// `orrch_core::Project`, so `orrch-agents` does not need to depend on
/// `orrch-core`.
pub fn load_project_core_context(
    project_root: &Path,
    profile_filename: &str,
) -> Option<String> {
    let path = project_root.join(profile_filename);
    std::fs::read_to_string(path).ok()
}

/// Determines if an agent role requires context isolation (verification agents).
pub fn is_verification_role(role_name: &str) -> bool {
    let lower = role_name.to_lowercase();
    lower.contains("tester")
        || lower.contains("penetration")
        || lower.contains("beta")
        || lower.contains("qa")
        || lower.contains("quality assurance")
}

// ─── Task 32: Mentor auto-assignment ─────────────────────────────────────────
//
// `mentor_review_profile` scans an agent profile's prompt body for topic
// keywords, then returns a formatted markdown "references" block listing the
// most likely-relevant skills and tools from the library. This is the content
// the Mentor injects into the agent's prompt via
// `AgentProfile::as_preamble_with_library()` so the agent sees suggestions for
// which skills/tools to consult for its task.
//
// Matching is intentionally simple keyword intersection (no LLM): both the
// profile text and the item filename/name are lowercased, split into word-like
// tokens, and we emit any library item whose tokens intersect the profile's
// "topic keywords". Topic keywords are a curated subset of common domain terms
// (test, commit, release, review, debug, ...) plus any word from the profile's
// role/department fields.

/// Format a "References" block listing library skills and tools the Mentor
/// believes are relevant to the given agent profile.
///
/// `library_skills` and `library_tools` are `(display_name, path)` tuples as
/// produced by the TUI's `scan_md_dir` helper. Returns an empty string if no
/// items match — callers can pass the result directly to
/// [`crate::profile::AgentProfile::as_preamble_with_library`].
pub fn mentor_review_profile(
    profile: &crate::profile::AgentProfile,
    library_skills: &[(String, std::path::PathBuf)],
    library_tools: &[(String, std::path::PathBuf)],
) -> String {
    let topics = extract_profile_topics(profile);
    if topics.is_empty() {
        return String::new();
    }

    let skills_hits: Vec<&(String, std::path::PathBuf)> = library_skills
        .iter()
        .filter(|(name, path)| item_matches_topics(name, path, &topics))
        .collect();
    let tools_hits: Vec<&(String, std::path::PathBuf)> = library_tools
        .iter()
        .filter(|(name, path)| item_matches_topics(name, path, &topics))
        .collect();

    if skills_hits.is_empty() && tools_hits.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("## Mentor-Suggested Library References\n\n");
    out.push_str(
        "Based on this agent's role and profile, the following library items look relevant. \
         Consider loading them (via the Read tool) before you start work:\n\n",
    );

    if !skills_hits.is_empty() {
        out.push_str("**Skills:**\n");
        for (name, path) in skills_hits {
            out.push_str(&format!("- `{}` — {}\n", name, path.display()));
        }
        out.push('\n');
    }
    if !tools_hits.is_empty() {
        out.push_str("**Tools:**\n");
        for (name, path) in tools_hits {
            out.push_str(&format!("- `{}` — {}\n", name, path.display()));
        }
        out.push('\n');
    }

    out
}

/// Curated set of domain topic keywords. If any of these show up in the
/// agent's profile body, library items whose name/path contain them become
/// candidates.
const MENTOR_TOPIC_KEYWORDS: &[&str] = &[
    "test", "commit", "release", "review", "debug", "deploy", "build",
    "branch", "pr", "pull request", "scope", "plan", "develop", "feature",
    "instruction", "intake", "audit", "security", "penetration", "research",
    "beta", "pm", "engineer", "developer", "tester", "coo", "ux", "ui",
    "mentor", "repo", "repository", "interpret", "compress", "route",
    "cluster", "workflow",
];

fn extract_profile_topics(profile: &crate::profile::AgentProfile) -> Vec<String> {
    let mut topics: Vec<String> = Vec::new();

    // Role / department / name contribute directly as tokens.
    for field in [&profile.role, &profile.department, &profile.name] {
        for word in tokenize(field) {
            if !topics.contains(&word) {
                topics.push(word);
            }
        }
    }

    // Scan the profile body for curated topic keywords.
    let body_lower = profile.prompt.to_lowercase();
    for kw in MENTOR_TOPIC_KEYWORDS {
        if body_lower.contains(kw) && !topics.iter().any(|t| t == kw) {
            topics.push((*kw).to_string());
        }
    }

    // Drop trivially short tokens that would produce noise.
    topics.retain(|t| t.len() >= 3);
    topics
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn item_matches_topics(
    name: &str,
    path: &std::path::Path,
    topics: &[String],
) -> bool {
    let hay = format!(
        "{} {}",
        name.to_lowercase(),
        path.to_string_lossy().to_lowercase()
    );
    topics.iter().any(|t| hay.contains(t.as_str()))
}

// ─── Task 58: Mentor resource updates (Researcher dispatch scaffolding) ──────

/// Kind of library resource the Mentor can ask a Researcher to refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Model,
    Harness,
}

impl ResourceKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Harness => "harness",
        }
    }
}

/// A Mentor-originated request for the Researcher to investigate changes to a
/// specific library resource.
#[derive(Debug, Clone)]
pub struct ResourceUpdateRequest {
    pub kind: ResourceKind,
    pub target_name: String,
    /// ISO 8601 date/time the resource was last verified. `None` means it
    /// has never been checked.
    pub last_checked: Option<String>,
    pub note: Option<String>,
}

impl ResourceUpdateRequest {
    pub fn new(kind: ResourceKind, target_name: impl Into<String>) -> Self {
        Self {
            kind,
            target_name: target_name.into(),
            last_checked: None,
            note: None,
        }
    }

    pub fn with_last_checked(mut self, ts: impl Into<String>) -> Self {
        self.last_checked = Some(ts.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Build the Researcher-facing task prompt that asks for a structured diff on
/// the given resource. The output is a self-contained task string — pass it to
/// [`AgentRunner::build_prompt`] with a Researcher `AgentProfile` to produce
/// the full prompt for the session.
pub fn build_researcher_resource_prompt(request: &ResourceUpdateRequest) -> String {
    let since = request
        .last_checked
        .as_deref()
        .unwrap_or("(never — initial catalog pass)");
    let note_line = request
        .note
        .as_deref()
        .map(|n| format!("\nMentor note: {}\n", n))
        .unwrap_or_default();

    format!(
        "Investigate changes to the {kind} **{name}** since {since}.\n\
        {note}\n\
        Check for and report on:\n\
        \n\
        1. **Pricing changes** — input/output rates, subscription tiers, free-tier limits.\n\
        2. **API endpoint changes** — base URL, auth scheme, breaking request/response shape changes.\n\
        3. **Capability changes** — new features (e.g., tool use, structured output, vision, context-window bumps).\n\
        4. **Deprecations** — model IDs or flags that are being retired, with end-of-life dates.\n\
        5. **Availability / rate limits** — regional availability, new per-minute / per-day caps.\n\
        \n\
        Return your findings as a structured markdown diff with the following shape:\n\
        \n\
        ```\n\
        ## {name} — Update Report\n\
        Date: <today>\n\
        \n\
        ### Changed\n\
        - <field>: <old> -> <new>\n\
        \n\
        ### Added\n\
        - <new capability or field>\n\
        \n\
        ### Deprecated\n\
        - <field> (EOL: <date>)\n\
        \n\
        ### Unchanged\n\
        - <summary>\n\
        ```\n\
        \n\
        If nothing has changed, still return the block with all sections empty\n\
        except `Unchanged`. Do not speculate — only report facts you can back\n\
        with primary sources (official docs, provider changelog, release notes).",
        kind = request.kind.label(),
        name = request.target_name,
        since = since,
        note = note_line,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::AgentProfile;
    use std::path::PathBuf;

    fn test_agent() -> AgentProfile {
        AgentProfile {
            name: "Developer".into(),
            department: "development".into(),
            role: "Developer".into(),
            description: "Implements code".into(),
            prompt: "You are the Developer. Write clean code.".into(),
            path: PathBuf::from("agents/developer.md"),
        }
    }

    #[test]
    fn test_build_prompt_basic() {
        let agent = test_agent();
        let prompt = AgentRunner::build_prompt(&agent, "implement the login form", None);
        assert!(prompt.contains("You are the Developer"));
        assert!(prompt.contains("implement the login form"));
        assert!(!prompt.contains("Core Context"));
    }

    #[test]
    fn test_build_prompt_with_context() {
        let agent = test_agent();
        let prompt = AgentRunner::build_prompt(
            &agent,
            "implement the login form",
            Some("Project uses React + TypeScript"),
        );
        assert!(prompt.contains("Core Context"));
        assert!(prompt.contains("React + TypeScript"));
    }

    #[test]
    fn test_verification_prompt_isolation() {
        let agent = AgentProfile {
            name: "Feature Tester".into(),
            department: "development".into(),
            role: "Feature Tester".into(),
            description: "Tests features".into(),
            prompt: "You are the Feature Tester.".into(),
            path: PathBuf::new(),
        };
        let prompt = AgentRunner::build_verification_prompt(
            &agent,
            "Login form at src/components/Login.tsx",
            None,
        );
        assert!(prompt.contains("independent verification"));
        assert!(prompt.contains("NOT seen any other agent's assessment"));
        assert!(prompt.contains("Login.tsx"));
    }

    #[test]
    fn test_handoff_prompt() {
        let agent = test_agent();
        let prompt = AgentRunner::build_handoff_prompt(
            &agent,
            "implement the changes",
            "Software Engineer",
            "Architecture: use a service layer pattern with dependency injection",
            None,
        );
        assert!(prompt.contains("Handoff from Software Engineer"));
        assert!(prompt.contains("service layer pattern"));
        assert!(prompt.contains("implement the changes"));
    }

    #[test]
    fn test_compress_handoff_strips_thinking() {
        let input = "<thinking>internal reasoning here</thinking>\nFinal answer.";
        let out = compress_handoff(input);
        assert!(!out.contains("internal reasoning"), "thinking block not stripped: {:?}", out);
        assert!(out.contains("Final answer."), "final answer missing: {:?}", out);
    }

    #[test]
    fn test_compress_handoff_strips_preamble() {
        let input = "Let me check the code.\nLooking at this file, I see the issue.\nThe fix is in foo.rs:42.";
        let out = compress_handoff(input);
        assert!(!out.contains("Let me check"), "preamble 'Let me' not stripped: {:?}", out);
        assert!(!out.contains("Looking at this"), "preamble 'Looking at this' not stripped: {:?}", out);
        assert!(out.contains("The fix is in foo.rs:42."), "substantive line missing: {:?}", out);
    }

    #[test]
    fn test_compress_handoff_preserves_code_blocks() {
        let input = "Summary:\n```rust\nlet me reassign x = 5;\n```\nDone.";
        let out = compress_handoff(input);
        // The "let me reassign" line is inside a fenced code block, so it must be kept.
        assert!(out.contains("let me reassign x = 5;"), "code-block line was stripped: {:?}", out);
        assert!(out.contains("```"), "code fence missing: {:?}", out);
        assert!(out.contains("Summary:"));
        assert!(out.contains("Done."));
    }

    #[test]
    fn test_compress_handoff_collapses_blank_lines() {
        let input = "line one\n\n\n\n\n\nline two";
        let out = compress_handoff(input);
        // Should collapse the run of blank lines to a single blank line:
        // "line one\n\nline two"
        assert_eq!(out, "line one\n\nline two", "blank lines not collapsed: {:?}", out);
    }

    #[test]
    fn test_compress_handoff_multiple_thinking_blocks() {
        let input = "A<thinking>one</thinking>B<thinking>two</thinking>C";
        let out = compress_handoff(input);
        assert_eq!(out, "ABC");
    }

    #[test]
    fn test_is_verification_role() {
        assert!(is_verification_role("Feature Tester"));
        assert!(is_verification_role("Beta Tester"));
        assert!(is_verification_role("Penetration Tester"));
        assert!(!is_verification_role("Developer"));
        assert!(!is_verification_role("Project Manager"));
    }

    // ── Task 32: mentor_review_profile ──────────────────────────────────

    fn tester_profile() -> crate::profile::AgentProfile {
        crate::profile::AgentProfile {
            name: "Feature Tester".into(),
            department: "development".into(),
            role: "QA / Testing".into(),
            description: "Runs cargo test and validates behavior".into(),
            prompt: "You are the Feature Tester. You run tests and review failures.".into(),
            path: std::path::PathBuf::from("agents/feature_tester.md"),
        }
    }

    fn empty_profile() -> crate::profile::AgentProfile {
        crate::profile::AgentProfile {
            name: "Zzz".into(),
            department: "".into(),
            role: "".into(),
            description: "".into(),
            // Body uses words that are NOT in MENTOR_TOPIC_KEYWORDS.
            prompt: "Do abstract zzzz things. Consider xyzqplk carefully.".into(),
            path: std::path::PathBuf::from("agents/zzz.md"),
        }
    }

    #[test]
    fn test_mentor_review_profile_matches_test_topics() {
        let profile = tester_profile();
        let skills = vec![
            ("agent-feature-tester".to_string(), std::path::PathBuf::from("library/skills/agent-feature-tester.md")),
            ("agent-pm".to_string(),             std::path::PathBuf::from("library/skills/agent-pm.md")),
            ("release".to_string(),              std::path::PathBuf::from("library/skills/release.md")),
        ];
        let tools = vec![
            ("workflow_status.sh".to_string(), std::path::PathBuf::from("library/tools/workflow_status.sh")),
            ("route_instructions.sh".to_string(), std::path::PathBuf::from("library/tools/route_instructions.sh")),
        ];

        let block = mentor_review_profile(&profile, &skills, &tools);
        assert!(!block.is_empty(), "tester profile should produce a non-empty references block");
        assert!(block.contains("Mentor-Suggested Library References"));
        assert!(block.contains("Skills"));
        // agent-feature-tester matches "tester" / "feature" / "test"
        assert!(block.contains("agent-feature-tester"));
    }

    #[test]
    fn test_mentor_review_profile_no_matches_returns_empty() {
        let profile = empty_profile();
        let skills = vec![
            ("unrelated-item-xyz".to_string(), std::path::PathBuf::from("library/skills/unrelated-item-xyz.md")),
        ];
        let tools: Vec<(String, std::path::PathBuf)> = vec![];

        let block = mentor_review_profile(&profile, &skills, &tools);
        assert!(
            block.is_empty(),
            "profile with no matching topics should yield an empty block, got: {:?}",
            block
        );
    }

    #[test]
    fn test_as_preamble_with_library_empty_is_same_as_preamble() {
        let agent = test_agent();
        let a = agent.as_preamble("do the thing");
        let b = agent.as_preamble_with_library("do the thing", "");
        assert_eq!(a, b);
    }

    #[test]
    fn test_as_preamble_with_library_inserts_block() {
        let agent = test_agent();
        let block = "## Mentor-Suggested Library References\n\n- foo\n";
        let p = agent.as_preamble_with_library("do the thing", block);
        assert!(p.contains("You are the Developer"));
        assert!(p.contains("Mentor-Suggested Library References"));
        assert!(p.contains("## Your Task"));
        assert!(p.contains("do the thing"));
        // Ordering: profile body, then references, then task
        let profile_idx = p.find("You are the Developer").unwrap();
        let refs_idx = p.find("Mentor-Suggested Library References").unwrap();
        let task_idx = p.find("## Your Task").unwrap();
        assert!(profile_idx < refs_idx);
        assert!(refs_idx < task_idx);
    }

    // ── Task 58: ResourceUpdateRequest + build_researcher_resource_prompt ──

    #[test]
    fn test_resource_update_request_builder() {
        let req = ResourceUpdateRequest::new(ResourceKind::Model, "Claude Opus 4.6")
            .with_last_checked("2026-01-15")
            .with_note("pricing may be stale");
        assert_eq!(req.kind, ResourceKind::Model);
        assert_eq!(req.target_name, "Claude Opus 4.6");
        assert_eq!(req.last_checked.as_deref(), Some("2026-01-15"));
        assert_eq!(req.note.as_deref(), Some("pricing may be stale"));
    }

    #[test]
    fn test_build_researcher_resource_prompt_mentions_since_and_target() {
        let req = ResourceUpdateRequest::new(ResourceKind::Harness, "Codex CLI")
            .with_last_checked("2025-12-01");
        let prompt = build_researcher_resource_prompt(&req);
        assert!(prompt.contains("Codex CLI"));
        assert!(prompt.contains("harness"));
        assert!(prompt.contains("2025-12-01"));
        assert!(prompt.contains("Pricing"));
        assert!(prompt.contains("Deprecations"));
        assert!(prompt.contains("Update Report"));
    }

    #[test]
    fn test_build_researcher_resource_prompt_never_checked() {
        let req = ResourceUpdateRequest::new(ResourceKind::Model, "Mistral Large");
        let prompt = build_researcher_resource_prompt(&req);
        assert!(prompt.contains("never"));
        assert!(prompt.contains("Mistral Large"));
    }

    #[test]
    fn test_build_hypervisor_context() {
        use orrch_workforce::{Workforce, AgentNode, Connection, Operation, Step, TriggerCondition};
        use orrch_workforce::template::DataFlow;

        let workforce = Workforce {
            name: "Test Workforce".into(),
            description: "A minimal test workforce".into(),
            agents: vec![
                AgentNode {
                    id: "pm".into(),
                    agent_profile: "Project Manager".into(),
                    user_facing: true,
                    nested_workforce: None,
                },
                AgentNode {
                    id: "dev".into(),
                    agent_profile: "Developer".into(),
                    user_facing: false,
                    nested_workforce: None,
                },
            ],
            connections: vec![Connection {
                from: "pm".into(),
                to: "dev".into(),
                data_type: DataFlow::Instructions,
            }],
            operations: vec!["BUILD FEATURE".into()],
        };

        let operations = vec![
            Operation {
                name: "BUILD FEATURE".into(),
                trigger: TriggerCondition::Manual,
                blocker: None,
                steps: vec![
                    Step {
                        index: "1".into(),
                        agent: "Project Manager".into(),
                        tool_or_skill: None,
                        operation: "plan the work".into(),
                        parallel_group: None,
                        model_override: None,
                    },
                    Step {
                        index: "2".into(),
                        agent: "Developer".into(),
                        tool_or_skill: Some("skill:code".into()),
                        operation: "implement the feature".into(),
                        parallel_group: None,
                        model_override: None,
                    },
                ],
                interrupts: vec![],
            },
            Operation {
                name: "UNRELATED OP".into(),
                trigger: TriggerCondition::Manual,
                blocker: None,
                steps: vec![],
                interrupts: vec![],
            },
        ];

        let agents_dir = PathBuf::from("/home/test/agents");
        let ctx = build_hypervisor_context(&workforce, &operations, &agents_dir);

        // Roster present with paths
        assert!(ctx.contains("## Workforce: Test Workforce"));
        assert!(ctx.contains("### Team Roster"));
        assert!(ctx.contains("/home/test/agents/project_manager.md"));
        assert!(ctx.contains("/home/test/agents/developer.md"));

        // Operation table present
        assert!(ctx.contains("### Operation: BUILD FEATURE"));
        assert!(ctx.contains("plan the work"));
        assert!(ctx.contains("skill:code"));

        // Filtered operation excluded
        assert!(!ctx.contains("UNRELATED OP"));

        // Agent profile BODIES are NOT present (this is the key assertion)
        assert!(!ctx.contains("Plan carefully"));
        assert!(!ctx.contains("Write clean code"));

        // On-demand loading instruction present
        assert!(ctx.contains("Loading Agent Profiles"));
        assert!(ctx.contains("Read tool"));
    }
}
