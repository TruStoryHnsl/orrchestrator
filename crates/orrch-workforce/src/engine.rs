use crate::operation::{Operation, Step, BlockCondition};

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
                Step { index: "1".into(), agent: "PM".into(), tool_or_skill: None, operation: "plan".into(), parallel_group: None },
                Step { index: "2".into(), agent: "Dev".into(), tool_or_skill: None, operation: "code".into(), parallel_group: Some(1) },
                Step { index: "2".into(), agent: "Researcher".into(), tool_or_skill: None, operation: "research".into(), parallel_group: Some(1) },
                Step { index: "3".into(), agent: "Tester".into(), tool_or_skill: None, operation: "test".into(), parallel_group: None },
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
}
