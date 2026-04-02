use serde::{Deserialize, Serialize};

/// An operation module — a structured pipeline that a workforce can execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// Operation name (e.g., "INSTRUCTION INTAKE", "DEVELOP FEATURE").
    pub name: String,
    /// What triggers this operation to start.
    pub trigger: TriggerCondition,
    /// Condition that prevents the operation from running.
    pub blocker: Option<BlockCondition>,
    /// Ordered steps to execute.
    pub steps: Vec<Step>,
    /// Conditions that cancel the operation mid-run.
    pub interrupts: Vec<InterruptCondition>,
}

/// A single step in an operation's order of operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Step index (e.g., "1", "1B", "2"). String to support sub-steps.
    pub index: String,
    /// Agent profile name that executes this step.
    pub agent: String,
    /// Tool or skill to use (None = agent decides).
    pub tool_or_skill: Option<String>,
    /// Natural language description of what this step does.
    pub operation: String,
    /// Steps with the same parallel group run concurrently.
    pub parallel_group: Option<u32>,
}

/// What triggers an operation to start.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerCondition {
    /// User submits input of a specific type.
    UserSubmit { input_type: String },
    /// Unprocessed items exist in a project's instruction inbox.
    InboxNotEmpty { project: String },
    /// Manual trigger by the user.
    Manual,
    /// Triggered by another operation completing.
    OperationComplete { operation: String },
}

/// Condition that blocks an operation from running.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockCondition {
    /// Intelligence Resources Manager says API limits are reached.
    ApiRateLimited { provider: String },
    /// Another operation is currently running on the same project.
    OperationInProgress { operation: String },
    /// Custom condition described in natural language.
    Custom { description: String },
}

/// Condition that interrupts (cancels) a running operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptCondition {
    /// API rate limit hit mid-operation — pause after current step.
    ApiRateLimited { provider: String },
    /// User requests cancellation.
    UserCancel,
    /// Custom condition.
    Custom { description: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instruction_intake_module() {
        let op = Operation {
            name: "INSTRUCTION INTAKE".into(),
            trigger: TriggerCondition::UserSubmit { input_type: "instructions".into() },
            blocker: None,
            steps: vec![
                Step {
                    index: "1".into(),
                    agent: "Executive Assistant".into(),
                    tool_or_skill: None,
                    operation: "separate dev instructions from other input".into(),
                    parallel_group: None,
                },
                Step {
                    index: "2".into(),
                    agent: "Chief Operations Officer".into(),
                    tool_or_skill: Some("skill:clarify".into()),
                    operation: "process raw instructions into optimized instructions".into(),
                    parallel_group: None,
                },
                Step {
                    index: "3".into(),
                    agent: "Chief Operations Officer".into(),
                    tool_or_skill: Some("skill:parse".into()),
                    operation: "determine which project each instruction goes to".into(),
                    parallel_group: None,
                },
            ],
            interrupts: vec![],
        };
        assert_eq!(op.steps.len(), 3);
        assert_eq!(op.steps[0].agent, "Executive Assistant");
    }

    #[test]
    fn test_develop_feature_parallel_steps() {
        let op = Operation {
            name: "DEVELOP FEATURE".into(),
            trigger: TriggerCondition::InboxNotEmpty { project: "*".into() },
            blocker: Some(BlockCondition::ApiRateLimited { provider: "any".into() }),
            steps: vec![
                Step { index: "1".into(), agent: "Project Manager".into(), tool_or_skill: None, operation: "synthesize instructions".into(), parallel_group: None },
                Step { index: "2".into(), agent: "Developer".into(), tool_or_skill: None, operation: "execute coding tasks".into(), parallel_group: Some(1) },
                Step { index: "2".into(), agent: "Researcher".into(), tool_or_skill: None, operation: "conduct research".into(), parallel_group: Some(1) },
                Step { index: "2".into(), agent: "Feature Tester".into(), tool_or_skill: None, operation: "design tests".into(), parallel_group: Some(1) },
            ],
            interrupts: vec![InterruptCondition::UserCancel],
        };
        let parallel: Vec<_> = op.steps.iter().filter(|s| s.parallel_group == Some(1)).collect();
        assert_eq!(parallel.len(), 3);
    }
}
