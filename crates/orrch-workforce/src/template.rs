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
    /// Ordered list of teams this workforce runs (workforce/team architectural
    /// split). Each entry references a team .md file in `teams/`.
    /// The cleanup team is the bottom entry in every workforce.
    /// Empty for workforces predating the team split — back-compat.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub teams: Vec<TeamRef>,
}

/// A reference to a team .md file inside a workforce's ordered team list.
/// Multiple entries with the same `team` name are allowed (e.g., a workforce
/// may run `develop_feature` three times in sequence).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRef {
    /// Order index (1-based) — sets the sequential execution order.
    pub order: u32,
    /// Team file stem (e.g., "develop_feature", "cleanup").
    pub team: String,
    /// Optional one-line description of why this team appears here.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A team — a single, complete unit of work with its own agent roster
/// and step pipeline. Teams are the unit a workforce composes; teams
/// themselves are compiled from `teams/<name>.md`.
///
/// Schema mirrors `Workforce` but adds an explicit `steps` table so the
/// compiler can produce a deterministic dispatch script without needing
/// to look up an external operation file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    /// Team name (e.g., "Cleanup", "Develop Feature").
    pub name: String,
    /// Short description.
    pub description: String,
    /// Agents in this team.
    pub agents: Vec<AgentNode>,
    /// Connections between team agents.
    pub connections: Vec<Connection>,
    /// Ordered step pipeline. Steps with the same `index` run in parallel.
    pub steps: Vec<TeamStep>,
    /// Optional summary text appended at the bottom of the team's md file.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
}

/// A single step in a team's pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamStep {
    /// Index — string to allow "1", "1B", "2.1" etc.
    pub index: String,
    /// Agent profile name that executes this step.
    pub agent: String,
    /// Optional tool or skill identifier (e.g. "tool:list_files",
    /// "skill:plan_tasks", "mcp:workflow_init"). `None` = agent decides.
    pub tool_or_skill: Option<String>,
    /// Natural-language description of what this step does.
    pub operation: String,
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
            operations: vec!["DEVELOP FEATURE".into()],
            teams: vec![],
        };
        assert_eq!(wf.agents.len(), 2);
        assert!(wf.agents[0].user_facing);
    }
}
