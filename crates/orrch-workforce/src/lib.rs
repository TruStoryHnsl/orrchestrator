pub mod template;
pub mod operation;
pub mod parser;
pub mod engine;
pub mod compiler;

pub use template::{Workforce, AgentNode, Connection, Team, TeamRef, TeamStep};
pub use operation::{Operation, Step, TriggerCondition, BlockCondition, InterruptCondition};
pub use engine::{
    OperationExecution, OperationState, StepResult, load_operations, load_workforces, load_teams,
    expand_nested_workforce, NestedExpansion, resolve_step_for_dispatch, ResolvedStep,
};
pub use parser::{
    parse_workforce_markdown, serialize_workforce_markdown, serialize_operation_markdown,
    expand_operation_human_readable, parse_team_markdown, serialize_team_markdown,
};
pub use compiler::{compile_team, compile_workflow, CompiledScript, CompiledStep, DEFAULT_CLEANUP_TEAM};
