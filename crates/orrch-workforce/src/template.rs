use serde::{Deserialize, Serialize};

/// A workforce template — a team of agents organized for a specific kind of task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workforce {
    /// Template name (e.g., "General Software Development").
    pub name: String,
    /// Short description of what this workforce is optimized for.
    pub description: String,
    /// Agent nodes in this workforce.
    pub agents: Vec<AgentNode>,
    /// Connections between agents (data flow / reporting lines).
    pub connections: Vec<Connection>,
    /// Operation modules this workforce can execute.
    pub operations: Vec<String>, // references to Operation names
}

/// An agent's place in a workforce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNode {
    /// Unique ID within this workforce (e.g., "pm", "dev-1", "researcher").
    pub id: String,
    /// Reference to an agent profile name (e.g., "Project Manager").
    pub agent_profile: String,
    /// Whether this agent is the user-facing output of the workforce.
    pub user_facing: bool,
    /// Optional reference to a nested workforce by name. When set, this agent
    /// delegates to another workforce instead of (or in addition to) its profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nested_workforce: Option<String>,
}

/// A directed connection between two agent nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// Source agent ID.
    pub from: String,
    /// Target agent ID.
    pub to: String,
    /// What kind of data flows along this connection.
    pub data_type: DataFlow,
}

/// The type of data that flows between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataFlow {
    /// Instructions / task delegation.
    Instructions,
    /// Code or artifact delivery.
    Deliverable,
    /// Test results or verification reports.
    Report,
    /// Research findings.
    Research,
    /// General communication.
    Message,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workforce_creation() {
        let wf = Workforce {
            name: "General Software Development".into(),
            description: "Full dev team with PM, engineers, testers".into(),
            agents: vec![
                AgentNode { id: "pm".into(), agent_profile: "Project Manager".into(), user_facing: true, nested_workforce: None },
                AgentNode { id: "dev".into(), agent_profile: "Developer".into(), user_facing: false, nested_workforce: None },
            ],
            connections: vec![
                Connection { from: "pm".into(), to: "dev".into(), data_type: DataFlow::Instructions },
            ],
            operations: vec!["DEVELOP FEATURE".into()],
        };
        assert_eq!(wf.agents.len(), 2);
        assert!(wf.agents[0].user_facing);
    }
}
