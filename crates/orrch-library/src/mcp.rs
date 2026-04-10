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

/// Save an MCP server entry as a `.md` file with YAML frontmatter.
///
/// Creates the directory if it doesn't exist. Filename is derived from the
/// server name (lowercased, spaces to hyphens).
pub fn save_mcp_server(dir: &Path, entry: &McpServerEntry) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;

    let filename = entry.name.to_lowercase().replace(' ', "-");
    let path = dir.join(format!("{filename}.md"));

    let mut content = String::from("---\n");
    content.push_str(&format!("name: {}\n", entry.name));
    content.push_str(&format!("description: {}\n", entry.description));

    match &entry.transport {
        McpTransport::Stdio { command, args, env: _ } => {
            content.push_str("transport: stdio\n");
            content.push_str(&format!("command: {}\n", command));
            if !args.is_empty() {
                content.push_str("args:\n");
                for arg in args {
                    content.push_str(&format!("  - {}\n", arg));
                }
            }
        }
        McpTransport::Sse { url } => {
            content.push_str("transport: sse\n");
            content.push_str(&format!("url: {}\n", url));
        }
    }

    content.push_str(&format!("enabled: {}\n", entry.enabled));

    if !entry.assigned_roles.is_empty() {
        content.push_str("assigned_roles:\n");
        for role in &entry.assigned_roles {
            content.push_str(&format!("  - {}\n", role));
        }
    }

    content.push_str("---\n");

    if !entry.notes.is_empty() {
        content.push('\n');
        content.push_str(&entry.notes);
        content.push('\n');
    }

    std::fs::write(&path, &content)?;
    Ok(path)
}

/// Load MCP servers from Claude JSON config files.
///
/// Scans (in order): `~/.claude.json`, `~/.config/claude/mcp.json`,
/// and `<project_root>/.mcp.json`. Each file contains a top-level
/// `mcpServers` object where keys are server names and values contain
/// `type`, `command`, `args`, and optionally `env`.
///
/// Servers from later files override earlier ones (by name).
pub fn load_mcp_servers_from_claude_configs(project_root: Option<&Path>) -> Vec<McpServerEntry> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
    let home_path = Path::new(&home);

    let mut paths = vec![
        home_path.join(".claude.json"),
        home_path.join(".config/claude/mcp.json"),
    ];
    if let Some(root) = project_root {
        paths.push(root.join(".mcp.json"));
    }

    let mut servers = std::collections::HashMap::<String, McpServerEntry>::new();

    for config_path in &paths {
        if let Ok(content) = std::fs::read_to_string(config_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(mcp_servers) = json.get("mcpServers").and_then(|v| v.as_object()) {
                    for (name, cfg) in mcp_servers {
                        if let Some(entry) = parse_claude_mcp_entry(name, cfg, config_path) {
                            servers.insert(name.clone(), entry);
                        }
                    }
                }
            }
        }
    }

    let mut result: Vec<McpServerEntry> = servers.into_values().collect();
    result.sort_by(|a, b| b.enabled.cmp(&a.enabled).then(a.name.cmp(&b.name)));
    result
}

/// Parse a single MCP server entry from Claude JSON config format.
fn parse_claude_mcp_entry(
    name: &str,
    cfg: &serde_json::Value,
    source_path: &Path,
) -> Option<McpServerEntry> {
    let transport_type = cfg.get("type").and_then(|v| v.as_str()).unwrap_or("stdio");

    let transport = if transport_type == "sse" {
        let url = cfg.get("url").and_then(|v| v.as_str()).unwrap_or_default();
        McpTransport::Sse { url: url.to_string() }
    } else {
        let command = cfg.get("command").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let args = cfg.get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let env = cfg.get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        McpTransport::Stdio { command, args, env }
    };

    // Derive a human-readable description from the command/url
    let description = match &transport {
        McpTransport::Stdio { command, .. } => {
            let cmd_name = Path::new(command)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| command.clone());
            format!("Claude config: {cmd_name}")
        }
        McpTransport::Sse { url } => format!("SSE: {url}"),
    };

    let source_label = source_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    Some(McpServerEntry {
        name: name.to_string(),
        description,
        transport,
        enabled: true, // Claude config entries are always enabled
        assigned_roles: vec![],
        notes: format!("Source: {source_label}"),
        path: source_path.to_path_buf(),
    })
}

/// Load all MCP servers from both library .md files and Claude JSON configs,
/// merged and deduplicated by name (library entries take precedence).
pub fn load_all_mcp_servers(library_dir: &Path, project_root: Option<&Path>) -> Vec<McpServerEntry> {
    let mut by_name = std::collections::HashMap::<String, McpServerEntry>::new();

    // Load from Claude configs first (lower precedence)
    for entry in load_mcp_servers_from_claude_configs(project_root) {
        by_name.insert(entry.name.clone(), entry);
    }

    // Load from library .md files (higher precedence — overwrites)
    for entry in load_mcp_servers(library_dir) {
        by_name.insert(entry.name.clone(), entry);
    }

    let mut result: Vec<McpServerEntry> = by_name.into_values().collect();
    result.sort_by(|a, b| b.enabled.cmp(&a.enabled).then(a.name.cmp(&b.name)));
    result
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
