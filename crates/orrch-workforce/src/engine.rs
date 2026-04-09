use crate::operation::{BlockCondition, Operation, Step};
use crate::template::Workforce;

/// Execution state of an operation module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationState {
    /// Waiting for trigger condition.
    Idle,
    /// Blocked by a condition (e.g., API rate limit).
    Blocked(String),
    /// Currently executing a step.
    Running {
        step_index: usize,
        parallel_remaining: usize,
    },
    /// All steps completed successfully.
    Complete,
    /// Interrupted mid-execution.
    Interrupted(String),
}

/// Tracks the execution progress of an operation module.
#[derive(Debug)]
pub struct OperationExecution {
    pub operation: Operation,
    pub state: OperationState,
    /// Results from completed steps (step index string → output text).
    pub step_results: Vec<StepResult>,
    /// Current step pointer.
    pub current_step: usize,
}

/// Result of a completed step.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub step_index: String,
    pub agent: String,
    pub output: String,
    pub success: bool,
}

impl OperationExecution {
    pub fn new(operation: Operation) -> Self {
        Self {
            operation,
            state: OperationState::Idle,
            step_results: Vec::new(),
            current_step: 0,
        }
    }

    /// Get the next batch of steps to execute.
    ///
    /// Returns steps that share the same index (parallel group) or a single
    /// sequential step. Returns empty vec if operation is complete.
    pub fn next_steps(&self) -> Vec<&Step> {
        if self.current_step >= self.operation.steps.len() {
            return Vec::new();
        }

        let current = &self.operation.steps[self.current_step];

        // Check if this step is part of a parallel group
        if let Some(group) = current.parallel_group {
            self.operation.steps[self.current_step..]
                .iter()
                .take_while(|s| s.parallel_group == Some(group))
                .collect()
        } else {
            vec![current]
        }
    }

    /// Advance past the current step(s) after they complete.
    pub fn advance(&mut self, results: Vec<StepResult>) {
        let batch_size = self.next_steps().len();
        self.step_results.extend(results);
        self.current_step += batch_size;

        if self.current_step >= self.operation.steps.len() {
            self.state = OperationState::Complete;
        } else {
            self.state = OperationState::Running {
                step_index: self.current_step,
                parallel_remaining: 0,
            };
        }
    }

    /// Check if a blocker condition is active.
    pub fn check_blocker(&self) -> Option<&BlockCondition> {
        self.operation.blocker.as_ref()
    }

    /// Start execution.
    pub fn start(&mut self) {
        if self.operation.steps.is_empty() {
            self.state = OperationState::Complete;
        } else {
            self.state = OperationState::Running {
                step_index: 0,
                parallel_remaining: self.next_steps().len(),
            };
        }
    }

    /// Interrupt the operation.
    pub fn interrupt(&mut self, reason: String) {
        self.state = OperationState::Interrupted(reason);
    }

    /// Progress display: "OPERATION NAME [3/7] — Agent executing"
    pub fn progress_display(&self) -> String {
        let total = self.operation.steps.len();
        let completed = self.step_results.len();
        let current_agent = self.next_steps()
            .first()
            .map(|s| s.agent.as_str())
            .unwrap_or("done");

        match &self.state {
            OperationState::Idle => format!("{} [idle]", self.operation.name),
            OperationState::Blocked(reason) => format!("{} [BLOCKED: {}]", self.operation.name, reason),
            OperationState::Running { .. } => format!(
                "{} [{}/{}] — {} executing",
                self.operation.name, completed, total, current_agent
            ),
            OperationState::Complete => format!("{} [complete]", self.operation.name),
            OperationState::Interrupted(reason) => format!("{} [interrupted: {}]", self.operation.name, reason),
        }
    }

    /// Whether all steps failed verification (dev loop should retry).
    pub fn has_failures(&self) -> bool {
        self.step_results.iter().any(|r| !r.success)
    }
}

/// Result of resolving a nested workforce reference on an agent node.
#[derive(Debug, Clone)]
pub struct NestedExpansion {
    /// The inner workforce that the parent node expands into.
    pub inner_workforce: crate::template::Workforce,
    /// The id of the agent in the inner workforce whose output is the unit's result.
    pub output_agent_id: String,
    /// The agent profile name of that output agent.
    pub output_agent_profile: String,
}

/// Resolve a nested workforce reference on a parent workforce node.
///
/// Lookup logic:
/// 1. Find the node in `parent.agents` matching `node_id`.
/// 2. If `node.nested_workforce` is `None`, return `None`.
/// 3. Find a workforce in `all_workforces` whose `name` matches the nested reference.
///    If not found, return `None`.
/// 4. Identify the inner workforce's "output" agent: the agent with `user_facing == true`.
///    If none has `user_facing`, fall back to the FIRST agent in `inner.agents`.
///    If `inner.agents` is empty, return `None`.
/// 5. Return `Some(NestedExpansion { inner_workforce, output_agent_id, output_agent_profile })`.
pub fn expand_nested_workforce(
    parent: &crate::template::Workforce,
    all_workforces: &[crate::template::Workforce],
    node_id: &str,
) -> Option<NestedExpansion> {
    let node = parent.agents.iter().find(|a| a.id == node_id)?;
    let nested_ref = node.nested_workforce.as_ref()?;
    let inner = all_workforces.iter().find(|w| &w.name == nested_ref)?;

    if inner.agents.is_empty() {
        return None;
    }

    let output_agent = inner
        .agents
        .iter()
        .find(|a| a.user_facing)
        .unwrap_or(&inner.agents[0]);

    Some(NestedExpansion {
        inner_workforce: inner.clone(),
        output_agent_id: output_agent.id.clone(),
        output_agent_profile: output_agent.agent_profile.clone(),
    })
}

/// A step resolved for runtime dispatch: carries the effective agent profile
/// name (possibly resolved through a nested workforce expansion) and any
/// per-step model override pulled off `Step::model_override`.
///
/// Task 35 + Task 57 runtime wiring. Produced by [`resolve_step_for_dispatch`]
/// and consumed by agent dispatch layers (e.g. the hypervisor in orrch-agents)
/// so both nested workforce expansion and mixed-model execution happen at the
/// actual spawn site rather than in ad-hoc callers.
#[derive(Debug, Clone)]
pub struct ResolvedStep {
    /// Step index (e.g. "1", "2B") — preserved verbatim from the source step.
    pub step_index: String,
    /// Final agent profile name to spawn. If the step's agent node pointed at a
    /// nested workforce, this is the nested workforce's user-facing output
    /// agent profile; otherwise it equals `step.agent`.
    pub agent_profile: String,
    /// Optional: the inner workforce that expansion resolved to. When `Some`,
    /// the dispatcher can further walk the nested workforce. When `None`, the
    /// step is a flat single-agent spawn.
    pub nested_workforce: Option<Workforce>,
    /// Per-step model override from `Step::model_override`. Passed through
    /// verbatim so dispatchers can route to the correct backend.
    pub model_override: Option<String>,
}

/// Resolve a `Step` against its executing workforce for runtime dispatch.
///
/// Lookup logic:
/// 1. Find an `AgentNode` in `workforce.agents` whose `agent_profile` matches
///    `step.agent`. If none matches, return a direct resolution (flat step).
/// 2. If that node has `nested_workforce: Some(...)`, call
///    [`expand_nested_workforce`] to resolve the inner workforce's output
///    agent. If expansion succeeds, the resolved profile is the inner output
///    agent's profile and `nested_workforce` is populated.
/// 3. Otherwise, the resolved profile is `step.agent` (unchanged).
/// 4. `model_override` is always forwarded from `step.model_override`.
///
/// This is the single point where Task 35 (nested workforce expansion) and
/// Task 57 (mixed-model overrides) feed runtime dispatch. Dispatchers should
/// call this for every step before spawning an agent.
pub fn resolve_step_for_dispatch(
    step: &Step,
    workforce: &Workforce,
    all_workforces: &[Workforce],
) -> ResolvedStep {
    // Try to find the agent node by profile match. Workforces may have
    // multiple nodes with the same profile (e.g. duplicate Developers per
    // file-cluster batching); pick the first that has a nested_workforce if
    // any, otherwise the first match.
    let node = workforce
        .agents
        .iter()
        .find(|a| a.agent_profile == step.agent && a.nested_workforce.is_some())
        .or_else(|| {
            workforce
                .agents
                .iter()
                .find(|a| a.agent_profile == step.agent)
        });

    if let Some(node) = node {
        if node.nested_workforce.is_some() {
            if let Some(expansion) = expand_nested_workforce(workforce, all_workforces, &node.id) {
                return ResolvedStep {
                    step_index: step.index.clone(),
                    agent_profile: expansion.output_agent_profile,
                    nested_workforce: Some(expansion.inner_workforce),
                    model_override: step.model_override.clone(),
                };
            }
            // Expansion failed (missing inner, empty, etc.) — fall through to
            // direct dispatch so the step still runs rather than silently
            // disappearing.
        }
    }

    ResolvedStep {
        step_index: step.index.clone(),
        agent_profile: step.agent.clone(),
        nested_workforce: None,
        model_override: step.model_override.clone(),
    }
}

/// Load operation modules from markdown files in a directory.
pub fn load_operations(dir: &std::path::Path) -> Vec<Operation> {
    let mut ops = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(op) = crate::parser::parse_operation_markdown(&content) {
                        ops.push(op);
                    }
                }
            }
        }
    }
    ops
}

/// Load workforce templates from markdown files in a directory.
pub fn load_workforces(dir: &std::path::Path) -> Vec<crate::template::Workforce> {
    let mut workforces = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(wf) = crate::parser::parse_workforce_markdown(&content) {
                        workforces.push(wf);
                    }
                }
            }
        }
    }
    workforces
}

/// Serialize a workforce to markdown and write it to `path`.
///
/// Overwrites any existing file at `path` without prompting.
/// Returns an `io::Error` (with context) on any filesystem failure.
pub fn export_workforce_to_path(
    wf: &crate::template::Workforce,
    path: &std::path::Path,
) -> std::io::Result<()> {
    let md = crate::parser::serialize_workforce_markdown(wf);
    std::fs::write(path, md).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("failed to write workforce to {}: {}", path.display(), e),
        )
    })
}

/// Read a workforce markdown file from `path` and parse it into a `Workforce`.
///
/// Returns an `io::Error` with clear context on:
/// - file not found
/// - unreadable file (permissions, other IO errors)
/// - invalid markdown (parser returned `None`) — mapped to `InvalidData`
pub fn import_workforce_from_path(
    path: &std::path::Path,
) -> std::io::Result<crate::template::Workforce> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("failed to read workforce from {}: {}", path.display(), e),
        )
    })?;
    crate::parser::parse_workforce_markdown(&content).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "failed to parse workforce markdown at {}: invalid format",
                path.display()
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::*;

    fn test_operation() -> Operation {
        Operation {
            name: "TEST OP".into(),
            trigger: TriggerCondition::Manual,
            blocker: None,
            steps: vec![
                Step { index: "1".into(), agent: "PM".into(), tool_or_skill: None, operation: "plan".into(), parallel_group: None, model_override: None },
                Step { index: "2".into(), agent: "Dev".into(), tool_or_skill: None, operation: "code".into(), parallel_group: Some(1), model_override: None },
                Step { index: "2".into(), agent: "Researcher".into(), tool_or_skill: None, operation: "research".into(), parallel_group: Some(1), model_override: None },
                Step { index: "3".into(), agent: "Tester".into(), tool_or_skill: None, operation: "test".into(), parallel_group: None, model_override: None },
            ],
            interrupts: vec![],
        }
    }

    #[test]
    fn test_execution_start() {
        let mut exec = OperationExecution::new(test_operation());
        exec.start();
        assert!(matches!(exec.state, OperationState::Running { .. }));
    }

    #[test]
    fn test_next_steps_sequential() {
        let exec = OperationExecution::new(test_operation());
        let steps = exec.next_steps();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].agent, "PM");
    }

    #[test]
    fn test_next_steps_parallel() {
        let mut exec = OperationExecution::new(test_operation());
        exec.current_step = 1; // move to step index "2"
        let steps = exec.next_steps();
        assert_eq!(steps.len(), 2); // Dev + Researcher
        assert_eq!(steps[0].agent, "Dev");
        assert_eq!(steps[1].agent, "Researcher");
    }

    #[test]
    fn test_advance_through_operation() {
        let mut exec = OperationExecution::new(test_operation());
        exec.start();

        // Complete step 1 (PM)
        exec.advance(vec![StepResult { step_index: "1".into(), agent: "PM".into(), output: "plan done".into(), success: true }]);
        assert_eq!(exec.current_step, 1);

        // Complete step 2 (parallel: Dev + Researcher)
        exec.advance(vec![
            StepResult { step_index: "2".into(), agent: "Dev".into(), output: "code done".into(), success: true },
            StepResult { step_index: "2".into(), agent: "Researcher".into(), output: "research done".into(), success: true },
        ]);
        assert_eq!(exec.current_step, 3);

        // Complete step 3 (Tester)
        exec.advance(vec![StepResult { step_index: "3".into(), agent: "Tester".into(), output: "tests pass".into(), success: true }]);
        assert_eq!(exec.state, OperationState::Complete);
    }

    #[test]
    fn test_progress_display() {
        let mut exec = OperationExecution::new(test_operation());
        assert!(exec.progress_display().contains("[idle]"));

        exec.start();
        assert!(exec.progress_display().contains("[0/4]"));
        assert!(exec.progress_display().contains("PM executing"));

        exec.advance(vec![StepResult { step_index: "1".into(), agent: "PM".into(), output: "done".into(), success: true }]);
        assert!(exec.progress_display().contains("[1/4]"));
    }

    #[test]
    fn test_interrupt() {
        let mut exec = OperationExecution::new(test_operation());
        exec.start();
        exec.interrupt("API rate limit".into());
        assert!(matches!(exec.state, OperationState::Interrupted(_)));
    }

    #[test]
    fn test_export_import_workforce_round_trip() {
        use crate::parser::parse_workforce_markdown;

        let md = r#"---
name: Round Trip Test
description: Export/import file I/O round trip
operations:
  - INSTRUCTION INTAKE
  - DEVELOP FEATURE
---

## Agents

| ID | Agent Profile | User-Facing |
|----|---------------|-------------|
| pm | Project Manager | yes |
| dev | Developer | no |
| res | Researcher | no |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | dev | instructions |
| pm | res | instructions |
| res | dev | research |
| dev | pm | deliverable |
"#;
        let wf_in = parse_workforce_markdown(md).expect("fixture parses");

        // Unique tempfile path (env::temp_dir + pid + nanos) — no external crate
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut path = std::env::temp_dir();
        path.push(format!("orrch_wf_roundtrip_{}_{}.md", std::process::id(), nanos));

        // Export
        super::export_workforce_to_path(&wf_in, &path).expect("export succeeds");
        assert!(path.exists(), "file was not written");

        // Import
        let wf_out = super::import_workforce_from_path(&path).expect("import succeeds");

        // Field-by-field assertions
        assert_eq!(wf_in.name, wf_out.name);
        assert_eq!(wf_in.agents.len(), wf_out.agents.len());
        assert_eq!(wf_in.connections.len(), wf_out.connections.len());
        assert_eq!(wf_in.operations.len(), wf_out.operations.len());
        assert_eq!(wf_out.name, "Round Trip Test");
        assert_eq!(wf_out.agents.len(), 3);
        assert_eq!(wf_out.connections.len(), 4);
        assert_eq!(wf_out.operations.len(), 2);

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_import_workforce_file_not_found() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "orrch_wf_nonexistent_{}_{}.md",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let err = super::import_workforce_from_path(&path).expect_err("should fail");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert!(err.to_string().contains("failed to read workforce"));
    }

    fn make_agent(id: &str, profile: &str, user_facing: bool, nested: Option<&str>) -> crate::template::AgentNode {
        crate::template::AgentNode {
            id: id.into(),
            agent_profile: profile.into(),
            user_facing,
            nested_workforce: nested.map(|s| s.into()),
        }
    }

    fn make_workforce(name: &str, agents: Vec<crate::template::AgentNode>) -> crate::template::Workforce {
        crate::template::Workforce {
            name: name.into(),
            description: "test".into(),
            agents,
            connections: vec![],
            operations: vec![],
        }
    }

    #[test]
    fn test_expand_nested_valid_ref() {
        let parent = make_workforce(
            "parent",
            vec![make_agent("node_a", "Project Manager", false, Some("inner_team"))],
        );
        let inner = make_workforce(
            "inner_team",
            vec![
                make_agent("dev1", "Developer", false, None),
                make_agent("lead", "Software Engineer", true, None),
            ],
        );
        let all = vec![inner];
        let expansion = super::expand_nested_workforce(&parent, &all, "node_a")
            .expect("should resolve");
        assert_eq!(expansion.output_agent_id, "lead");
        assert_eq!(expansion.output_agent_profile, "Software Engineer");
        assert_eq!(expansion.inner_workforce.name, "inner_team");
    }

    #[test]
    fn test_expand_nested_none_ref() {
        let parent = make_workforce(
            "parent",
            vec![make_agent("node_a", "Project Manager", true, None)],
        );
        let all = vec![];
        assert!(super::expand_nested_workforce(&parent, &all, "node_a").is_none());
    }

    #[test]
    fn test_expand_nested_missing_inner() {
        let parent = make_workforce(
            "parent",
            vec![make_agent("node_a", "Project Manager", false, Some("missing_team"))],
        );
        let other = make_workforce(
            "other_team",
            vec![make_agent("x", "Developer", true, None)],
        );
        let all = vec![other];
        assert!(super::expand_nested_workforce(&parent, &all, "node_a").is_none());
    }

    #[test]
    fn test_expand_nested_fallback_first_agent() {
        let parent = make_workforce(
            "parent",
            vec![make_agent("node_a", "Project Manager", false, Some("inner_team"))],
        );
        let inner = make_workforce(
            "inner_team",
            vec![
                make_agent("first", "Researcher", false, None),
                make_agent("second", "Developer", false, None),
            ],
        );
        let all = vec![inner];
        let expansion = super::expand_nested_workforce(&parent, &all, "node_a")
            .expect("should resolve");
        assert_eq!(expansion.output_agent_id, "first");
        assert_eq!(expansion.output_agent_profile, "Researcher");
    }

    #[test]
    fn test_import_workforce_invalid_markdown() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut path = std::env::temp_dir();
        path.push(format!("orrch_wf_invalid_{}_{}.md", std::process::id(), nanos));

        // Write obvious garbage (no frontmatter) to trigger parser None
        std::fs::write(&path, "this is not a workforce markdown file\n").expect("write garbage");

        let err = super::import_workforce_from_path(&path).expect_err("should fail");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("invalid format"));

        let _ = std::fs::remove_file(&path);
    }
}
