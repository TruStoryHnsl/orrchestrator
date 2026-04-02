pub mod template;
pub mod operation;
pub mod parser;
pub mod engine;

pub use template::{Workforce, AgentNode, Connection};
pub use operation::{Operation, Step, TriggerCondition, BlockCondition, InterruptCondition};
pub use engine::{OperationExecution, OperationState, StepResult, load_operations};
