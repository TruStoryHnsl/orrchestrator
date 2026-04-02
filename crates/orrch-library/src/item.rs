use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The kind of item stored in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Agent,
    Skill,
    Tool,
    McpServer,
    WorkforceTemplate,
    ApiKey,
}

impl ItemKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::Skill => "Skill",
            Self::Tool => "Tool",
            Self::McpServer => "MCP Server",
            Self::WorkforceTemplate => "Workforce Template",
            Self::ApiKey => "API Key",
        }
    }

    pub fn directory(&self) -> &'static str {
        match self {
            Self::Agent => "agents",
            Self::Skill => "skills",
            Self::Tool => "tools",
            Self::McpServer => "mcp_servers",
            Self::WorkforceTemplate => "workforce_templates",
            Self::ApiKey => "api_keys",
        }
    }
}

/// A library item — a reusable component for AI workflows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryItem {
    /// Display name.
    pub name: String,
    /// What kind of item this is.
    pub kind: ItemKind,
    /// Short description.
    pub description: String,
    /// Tags for search/filtering.
    pub tags: Vec<String>,
    /// Full content (the .md body after frontmatter).
    pub content: String,
    /// Path to the source file.
    pub path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_kind_labels() {
        assert_eq!(ItemKind::Agent.label(), "Agent");
        assert_eq!(ItemKind::McpServer.label(), "MCP Server");
        assert_eq!(ItemKind::WorkforceTemplate.directory(), "workforce_templates");
    }
}
