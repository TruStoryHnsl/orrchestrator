use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// A configured MCP server connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub name: String,
    pub description: String,
    /// How to start this server.
    pub transport: McpTransport,
    /// Whether this server is enabled (user can disable without deleting).
    pub enabled: bool,
    /// Which agent roles should have this server connected.
    /// Empty = available to all agents.
    pub assigned_roles: Vec<String>,
    pub notes: String,
    #[serde(skip)]
    pub path: PathBuf,
}

/// How orrchestrator connects to the MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// stdio-based: run a command, communicate via stdin/stdout.
    Stdio {
        command: String,
        args: Vec<String>,
        env: std::collections::HashMap<String, String>,
    },
    /// SSE-based: connect to an HTTP endpoint.
    Sse { url: String },
}

impl McpServerEntry {
    pub fn summary_line(&self) -> String {
        let status = if self.enabled { "●" } else { "○" };
        format!("{} {} — {}", status, self.name, self.description)
    }
}

/// Load MCP server configs from .md files in a directory.
pub fn load_mcp_servers(dir: &Path) -> Vec<McpServerEntry> {
    let mut servers = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Some(server) = parse_mcp_file(&path) {
                    servers.push(server);
                }
            }
        }
    }
    servers.sort_by(|a, b| b.enabled.cmp(&a.enabled).then(a.name.cmp(&b.name)));
    servers
}

fn parse_mcp_file(path: &Path) -> Option<McpServerEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let (fm, body) = crate::store::parse_frontmatter_pub(&content)?;

    let transport_type = extract(&fm, "transport").unwrap_or_default();
    let transport = if transport_type == "sse" {
        McpTransport::Sse {
            url: extract(&fm, "url").unwrap_or_default(),
        }
    } else {
        McpTransport::Stdio {
            command: extract(&fm, "command").unwrap_or_default(),
            args: extract_list(&fm, "args"),
            env: std::collections::HashMap::new(),
        }
    };

    Some(McpServerEntry {
        name: extract(&fm, "name")?,
        description: extract(&fm, "description").unwrap_or_default(),
        transport,
        enabled: extract(&fm, "enabled").map(|s| s != "false").unwrap_or(true),
        assigned_roles: extract_list(&fm, "assigned_roles"),
        notes: body.trim().to_string(),
        path: path.to_path_buf(),
    })
}

fn extract(fm: &str, key: &str) -> Option<String> {
    crate::store::extract_field_pub(fm, key)
}

fn extract_list(fm: &str, key: &str) -> Vec<String> {
    crate::store::extract_list_pub(fm, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_summary() {
        let server = McpServerEntry {
            name: "orrch-mcp".into(),
            description: "Orrchestrator internal data".into(),
            transport: McpTransport::Stdio {
                command: "orrch-mcp-server".into(),
                args: vec![],
                env: std::collections::HashMap::new(),
            },
            enabled: true,
            assigned_roles: vec![],
            notes: String::new(),
            path: PathBuf::new(),
        };
        assert!(server.summary_line().contains("●"));
    }
}
