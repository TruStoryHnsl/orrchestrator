use serde_json::Value;
use std::path::Path;

use crate::server::OrrchMcpServer;

// ─── Tool definitions (JSON Schema for tools/list) ─────────────────────────

pub fn tool_definitions() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": "library_search",
            "description": "Search the orrchestrator library for models, harnesses, skills, tools, or MCP servers by keyword. Returns matching item names.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search keyword to match against filenames and content"
                    },
                    "kind": {
                        "type": "string",
                        "description": "Optional: restrict to a subdirectory (models, harnesses, skills, tools, mcp_servers)",
                        "enum": ["models", "harnesses", "skills", "tools", "mcp_servers"]
                    }
                },
                "required": ["query"]
            }
        }),
        serde_json::json!({
            "name": "library_get",
            "description": "Read a specific file from the orrchestrator library. Returns full markdown content.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "description": "Subdirectory: models, harnesses, skills, tools, or mcp_servers",
                        "enum": ["models", "harnesses", "skills", "tools", "mcp_servers"]
                    },
                    "name": {
                        "type": "string",
                        "description": "Filename without extension (e.g. 'claude_opus')"
                    }
                },
                "required": ["kind", "name"]
            }
        }),
        serde_json::json!({
            "name": "list_agents",
            "description": "List all agent profiles with name, role, and department extracted from YAML frontmatter.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        serde_json::json!({
            "name": "list_skills",
            "description": "List all workflow skill files with descriptions extracted from YAML frontmatter.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        serde_json::json!({
            "name": "develop_feature",
            "description": "Load the develop-feature workflow skill, prepend the goal, and return the full skill content for the harness to execute.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal": {
                        "type": "string",
                        "description": "The development goal or feature description"
                    },
                    "project_dir": {
                        "type": "string",
                        "description": "Optional project directory path"
                    }
                },
                "required": ["goal"]
            }
        }),
        serde_json::json!({
            "name": "instruction_intake",
            "description": "Load the instruction-intake workflow skill with embedded instructions. Returns the skill content for the harness to execute.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "instructions": {
                        "type": "string",
                        "description": "Raw instruction text to process"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Path to a file containing instructions"
                    }
                }
            }
        }),
        serde_json::json!({
            "name": "workflow_status",
            "description": "Read the active workflow status from a project's .orrch/workflow.json file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_dir": {
                        "type": "string",
                        "description": "Absolute path to the project directory"
                    }
                },
                "required": ["project_dir"]
            }
        }),
        serde_json::json!({
            "name": "project_state",
            "description": "Get a summary of a project's current state: first 50 lines of PLAN.md and instruction inbox count.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": {
                        "type": "string",
                        "description": "Project name (directory name under ~/projects/)"
                    }
                },
                "required": ["project"]
            }
        }),
        serde_json::json!({
            "name": "inbox_append",
            "description": "Append new instructions to a project's instructions_inbox.md with a timestamp.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": {
                        "type": "string",
                        "description": "Project name (directory name under ~/projects/)"
                    },
                    "instructions": {
                        "type": "string",
                        "description": "Instruction text to append"
                    }
                },
                "required": ["project", "instructions"]
            }
        }),
        serde_json::json!({
            "name": "agent_invoke",
            "description": "Load an agent profile and combine it with a task to produce a structured prompt. Returns the agent's full profile body with the task appended.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "agent": {
                        "type": "string",
                        "description": "Agent name (e.g. 'developer', 'Software Engineer', 'hypervisor')"
                    },
                    "task": {
                        "type": "string",
                        "description": "The task to assign to the agent"
                    }
                },
                "required": ["agent", "task"]
            }
        }),
    ]
}

// ─── Dispatch ───────────────────────────────────────────────────────────────

pub async fn dispatch(server: &OrrchMcpServer, name: &str, args: &Value) -> String {
    match name {
        "library_search" => library_search(server, args),
        "library_get" => library_get(server, args),
        "list_agents" => list_agents(server),
        "list_skills" => list_skills(server),
        "develop_feature" => develop_feature(server, args),
        "instruction_intake" => instruction_intake(server, args),
        "workflow_status" => workflow_status(args),
        "project_state" => project_state(server, args),
        "inbox_append" => inbox_append(server, args),
        "agent_invoke" => agent_invoke(server, args),
        _ => format!("Error: unknown tool '{name}'"),
    }
}

// ─── Tool implementations ───────────────────────────────────────────────────

fn library_search(server: &OrrchMcpServer, args: &Value) -> String {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q.to_lowercase(),
        None => return "Error: 'query' parameter is required".into(),
    };

    let subdirs: Vec<&str> = match args.get("kind").and_then(|v| v.as_str()) {
        Some(kind) => vec![kind],
        None => vec!["models", "harnesses", "skills", "tools", "mcp_servers"],
    };

    let mut matches: Vec<String> = Vec::new();

    for subdir in subdirs {
        let dir = server.library_dir.join(subdir);
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                let filename = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();

                let mut matched = filename.contains(&query);

                if !matched {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        matched = content.to_lowercase().contains(&query);
                    }
                }

                if matched {
                    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                    matches.push(format!("{subdir}/{stem}"));
                }
            }
        }
    }

    if matches.is_empty() {
        format!("No matches for '{}'", args.get("query").and_then(|v| v.as_str()).unwrap_or(&query))
    } else {
        matches.sort();
        matches.join("\n")
    }
}

fn library_get(server: &OrrchMcpServer, args: &Value) -> String {
    let kind = match args.get("kind").and_then(|v| v.as_str()) {
        Some(k) => k,
        None => return "Error: 'kind' parameter is required".into(),
    };
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return "Error: 'name' parameter is required".into(),
    };

    let path = server.library_dir.join(kind).join(format!("{name}.md"));
    match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) => format!("Error: cannot read {}: {e}", path.display()),
    }
}

fn list_agents(server: &OrrchMcpServer) -> String {
    let entries = match std::fs::read_dir(&server.agents_dir) {
        Ok(e) => e,
        Err(e) => return format!("Error: cannot read agents directory: {e}"),
    };

    let mut agents: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let name = extract_frontmatter_field(&content, "name")
                    .unwrap_or_else(|| {
                        path.file_stem().unwrap_or_default().to_string_lossy().into()
                    });
                let role = extract_frontmatter_field(&content, "role").unwrap_or_default();
                let dept = extract_frontmatter_field(&content, "department").unwrap_or_default();
                agents.push(format!("- {name} | {role} | {dept}"));
            }
        }
    }

    agents.sort();

    if agents.is_empty() {
        "No agent profiles found.".into()
    } else {
        format!("Agents ({} total):\n{}", agents.len(), agents.join("\n"))
    }
}

fn list_skills(server: &OrrchMcpServer) -> String {
    let entries = match std::fs::read_dir(&server.skills_dir) {
        Ok(e) => e,
        Err(e) => return format!("Error: cannot read skills directory: {e}"),
    };

    let mut skills: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md") {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let desc = if let Ok(content) = std::fs::read_to_string(&path) {
                extract_frontmatter_field(&content, "description").unwrap_or_default()
            } else {
                String::new()
            };
            skills.push(format!("- {stem}: {desc}"));
        }
    }

    skills.sort();

    if skills.is_empty() {
        "No skill files found.".into()
    } else {
        format!("Skills ({} total):\n{}", skills.len(), skills.join("\n"))
    }
}

fn develop_feature(server: &OrrchMcpServer, args: &Value) -> String {
    let goal = match args.get("goal").and_then(|v| v.as_str()) {
        Some(g) => g,
        None => return "Error: 'goal' parameter is required".into(),
    };

    let skill_path = server.skills_dir.join("develop-feature.md");
    let skill_content = match std::fs::read_to_string(&skill_path) {
        Ok(c) => c,
        Err(e) => return format!("Error: cannot read develop-feature.md: {e}"),
    };

    let project_ctx = match args.get("project_dir").and_then(|v| v.as_str()) {
        Some(dir) => format!("\n\nProject directory: {dir}"),
        None => String::new(),
    };

    format!("## Goal\n\n{goal}{project_ctx}\n\n---\n\n{skill_content}")
}

fn instruction_intake(server: &OrrchMcpServer, args: &Value) -> String {
    let skill_path = server.skills_dir.join("instruction-intake.md");
    let skill_content = match std::fs::read_to_string(&skill_path) {
        Ok(c) => c,
        Err(e) => return format!("Error: cannot read instruction-intake.md: {e}"),
    };

    // Resolve the instructions text from either inline or file.
    let instructions = if let Some(text) = args.get("instructions").and_then(|v| v.as_str()) {
        text.to_string()
    } else if let Some(path) = args.get("file_path").and_then(|v| v.as_str()) {
        match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return format!("Error: cannot read {path}: {e}"),
        }
    } else {
        return "Error: either 'instructions' or 'file_path' must be provided".into();
    };

    format!(
        "## Instructions to process\n\n{instructions}\n\n---\n\n{skill_content}"
    )
}

fn workflow_status(args: &Value) -> String {
    let project_dir = match args.get("project_dir").and_then(|v| v.as_str()) {
        Some(d) => d,
        None => return "Error: 'project_dir' parameter is required".into(),
    };

    let path = Path::new(project_dir).join(".orrch").join("workflow.json");
    match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => "No active workflow.".into(),
    }
}

fn project_state(server: &OrrchMcpServer, args: &Value) -> String {
    let project = match args.get("project").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return "Error: 'project' parameter is required".into(),
    };

    let project_dir = server.projects_dir.join(project);
    if !project_dir.is_dir() {
        return format!("Error: project directory '{}' not found", project_dir.display());
    }

    let mut output = format!("# Project: {project}\n\n");

    // PLAN.md — first 50 lines.
    let plan_path = project_dir.join("PLAN.md");
    match std::fs::read_to_string(&plan_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().take(50).collect();
            output.push_str("## PLAN.md (first 50 lines)\n\n");
            output.push_str(&lines.join("\n"));
            output.push('\n');
        }
        Err(_) => {
            output.push_str("No PLAN.md found.\n");
        }
    }

    // instructions_inbox.md — line count.
    let inbox_path = project_dir.join("instructions_inbox.md");
    match std::fs::read_to_string(&inbox_path) {
        Ok(content) => {
            let line_count = content.lines().count();
            output.push_str(&format!(
                "\n## instructions_inbox.md\n\n{line_count} lines in inbox.\n"
            ));
        }
        Err(_) => {
            output.push_str("\nNo instructions_inbox.md found.\n");
        }
    }

    output
}

fn inbox_append(server: &OrrchMcpServer, args: &Value) -> String {
    let project = match args.get("project").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return "Error: 'project' parameter is required".into(),
    };
    let instructions = match args.get("instructions").and_then(|v| v.as_str()) {
        Some(i) => i,
        None => return "Error: 'instructions' parameter is required".into(),
    };

    let project_dir = server.projects_dir.join(project);
    if !project_dir.is_dir() {
        return format!("Error: project directory '{}' not found", project_dir.display());
    }

    let inbox_path = project_dir.join("instructions_inbox.md");
    let timestamp = now_iso8601();

    let entry = format!("\n---\n\n## Instruction ({timestamp})\n\n{instructions}\n");

    use std::fs::OpenOptions;
    use std::io::Write;

    match OpenOptions::new().create(true).append(true).open(&inbox_path) {
        Ok(mut file) => match file.write_all(entry.as_bytes()) {
            Ok(()) => format!("Appended to {}", inbox_path.display()),
            Err(e) => format!("Error: write failed: {e}"),
        },
        Err(e) => format!("Error: cannot open {}: {e}", inbox_path.display()),
    }
}

fn agent_invoke(server: &OrrchMcpServer, args: &Value) -> String {
    let agent_name = match args.get("agent").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => return "Error: 'agent' parameter is required".into(),
    };
    let task = match args.get("task").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return "Error: 'task' parameter is required".into(),
    };

    // Normalize: lowercase, spaces → underscores, ensure .md suffix.
    let normalized = agent_name
        .to_lowercase()
        .replace(' ', "_");
    let filename = if normalized.ends_with(".md") {
        normalized
    } else {
        format!("{normalized}.md")
    };

    let path = server.agents_dir.join(&filename);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return format!("Error: cannot read agent profile '{}': {e}", path.display()),
    };

    // Extract body after frontmatter.
    let body = match extract_body(&content) {
        Some(b) => b,
        None => &content,
    };

    format!("{body}\n\n---\n\n## Your Task\n\n{task}")
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Parse YAML frontmatter and return a specific field value.
fn extract_frontmatter_field(content: &str, key: &str) -> Option<String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = trimmed[3..].trim_start_matches(['\r', '\n']);
    let end = after_first.find("\n---")?;
    let frontmatter = &after_first[..end];

    for line in frontmatter.lines() {
        let stripped = line.trim();
        if let Some(rest) = stripped.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix(':') {
                let value = value.trim();
                if value == ">" {
                    // Folded scalar — collect indented continuation lines.
                    let key_line_idx = frontmatter.find(stripped)?;
                    let after = &frontmatter[key_line_idx + stripped.len()..];
                    let mut parts = Vec::new();
                    for cont_line in after.lines().skip(1) {
                        if cont_line.starts_with(' ') || cont_line.starts_with('\t') {
                            parts.push(cont_line.trim());
                        } else {
                            break;
                        }
                    }
                    return if parts.is_empty() {
                        None
                    } else {
                        Some(parts.join(" "))
                    };
                }
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Extract the body content after YAML frontmatter.
fn extract_body(content: &str) -> Option<&str> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..];
    let end = after_first.find("\n---")?;
    let body = &after_first[end + 4..];
    Some(body.trim_start_matches(['\r', '\n']))
}

/// Simple ISO 8601 timestamp without external deps.
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert epoch seconds to date-time components.
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01 to Y-M-D (simplified, handles leap years).
    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            month = i as u64 + 1;
            break;
        }
        days -= md;
    }

    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_frontmatter_field_simple() {
        let content = "---\nname: Hypervisor\nrole: Orchestrator\n---\n\nBody";
        assert_eq!(
            extract_frontmatter_field(content, "name"),
            Some("Hypervisor".into())
        );
        assert_eq!(
            extract_frontmatter_field(content, "role"),
            Some("Orchestrator".into())
        );
    }

    #[test]
    fn test_extract_frontmatter_field_folded() {
        let content = "---\ndescription: >\n  This is a long\n  description text\nrole: Test\n---\n\nBody";
        assert_eq!(
            extract_frontmatter_field(content, "description"),
            Some("This is a long description text".into())
        );
    }

    #[test]
    fn test_extract_body() {
        let content = "---\nname: Test\n---\n\n# Heading\n\nBody content.";
        let body = extract_body(content).unwrap();
        assert!(body.starts_with("# Heading"));
        assert!(body.contains("Body content."));
    }

    #[test]
    fn test_extract_body_no_frontmatter() {
        let content = "# Just a heading\n\nSome text.";
        assert!(extract_body(content).is_none());
    }

    #[test]
    fn test_days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_known_date() {
        // 2024-01-01 is day 19723 since epoch.
        let (y, m, d) = days_to_ymd(19723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }

    #[test]
    fn test_now_iso8601_format() {
        let ts = now_iso8601();
        // Should look like "2026-04-03T12:34:56Z"
        assert!(ts.len() == 20, "timestamp length: {} ({})", ts.len(), ts);
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
    }

    #[test]
    fn test_tool_definitions_count() {
        assert_eq!(tool_definitions().len(), 10);
    }

    #[test]
    fn test_tool_definitions_have_schemas() {
        for tool in tool_definitions() {
            assert!(tool.get("name").is_some(), "tool missing name");
            assert!(tool.get("description").is_some(), "tool missing description");
            let schema = tool.get("inputSchema").expect("tool missing inputSchema");
            assert_eq!(schema["type"], "object");
        }
    }

    #[test]
    fn test_dispatch_unknown_tool() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({});
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(dispatch(&server, "nonexistent", &args));
        assert!(result.starts_with("Error: unknown tool"));
    }

    #[test]
    fn test_library_search_no_query() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({});
        let result = library_search(&server, &args);
        assert!(result.starts_with("Error:"));
    }

    #[test]
    fn test_library_get_missing_params() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({"kind": "models"});
        let result = library_get(&server, &args);
        assert!(result.starts_with("Error:"));
    }

    #[test]
    fn test_agent_invoke_normalization() {
        // Test that name normalization works (won't find the file in test env,
        // but exercises the path).
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({"agent": "Software Engineer", "task": "do something"});
        let result = agent_invoke(&server, &args);
        // In test env this either finds the file or returns an error with the normalized path.
        assert!(
            result.contains("software_engineer") || result.contains("Your Task"),
            "result: {result}"
        );
    }

    #[test]
    fn test_inbox_append_missing_project() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({"project": "nonexistent_project_xyz_123", "instructions": "test"});
        let result = inbox_append(&server, &args);
        assert!(result.starts_with("Error:"));
    }

    #[test]
    fn test_project_state_missing() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({"project": "nonexistent_project_xyz_123"});
        let result = project_state(&server, &args);
        assert!(result.starts_with("Error:"));
    }

    #[test]
    fn test_workflow_status_no_workflow() {
        let args = serde_json::json!({"project_dir": "/tmp/nonexistent_dir_xyz"});
        let result = workflow_status(&args);
        assert_eq!(result, "No active workflow.");
    }
}
