use crate::profile::AgentProfile;

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

        // Handoff from previous agent
        parts.push(format!(
            "## Handoff from {}\n\n{}",
            handoff_from, handoff_content
        ));

        parts.push(format!("## Your Task\n\n{}", task));

        parts.join("\n\n---\n\n")
    }
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
    fn test_is_verification_role() {
        assert!(is_verification_role("Feature Tester"));
        assert!(is_verification_role("Beta Tester"));
        assert!(is_verification_role("Penetration Tester"));
        assert!(!is_verification_role("Developer"));
        assert!(!is_verification_role("Project Manager"));
    }
}
