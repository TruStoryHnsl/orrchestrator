pub mod template;
pub mod operation;
pub mod parser;
pub mod engine;

pub use template::{Workforce, AgentNode, Connection};
pub use operation::{Operation, Step, TriggerCondition, BlockCondition, InterruptCondition};
pub use engine::{OperationExecution, OperationState, StepResult, load_operations, load_workforces, expand_nested_workforce, NestedExpansion, resolve_step_for_dispatch, ResolvedStep};
pub use parser::{parse_workforce_markdown, serialize_workforce_markdown, serialize_operation_markdown, expand_operation_human_readable};
