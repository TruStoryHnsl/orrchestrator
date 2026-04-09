use crate::profile::AgentProfile;
use orrch_workforce::{Workforce, Operation};
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

/// Determines if an agent role requires context isolation (verification agents).
pub fn is_verification_role(role_name: &str) -> bool {
    let lower = role_name.to_lowercase();
    lower.contains("tester")
        || lower.contains("penetration")
        || lower.contains("beta")
        || lower.contains("qa")
        || lower.contains("quality assurance")
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
