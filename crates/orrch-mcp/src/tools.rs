use regex::Regex;
use serde_json::Value;
use std::path::Path;
use std::sync::OnceLock;

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
        serde_json::json!({
            "name": "module_api",
            "description": "Extract the public API surface (pub structs, enums, fns, consts, mods) from a Rust source file. Returns a compact summary instead of the full file contents.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "crate_name": {
                        "type": "string",
                        "description": "Crate name (e.g. 'orrch-tui', 'orrch-core')"
                    },
                    "module": {
                        "type": "string",
                        "description": "Module filename without .rs extension (e.g. 'app', 'windows', 'lib')"
                    }
                },
                "required": ["crate_name", "module"]
            }
        }),
        serde_json::json!({
            "name": "codebase_brief",
            "description": "Generate a compact summary of the orrchestrator codebase: module map with pub API surface, color scheme, conventions, and crate dependencies. Use this at the start of any task instead of reading source files for orientation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": {
                        "type": "string",
                        "description": "Project name (default: 'orrchestrator')"
                    }
                }
            }
        }),
        serde_json::json!({
            "name": "workflow_init",
            "description": "Initialize a develop-feature workflow: generate codebase brief, read PLAN.md for unchecked items, check instruction inbox for stragglers. Returns everything needed to spawn the PM agent.",
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
            "name": "workflow_cluster",
            "description": "Cluster PM's task plan by file overlap for parallel execution. Takes the PM's raw plan output, runs cluster_tasks.sh, returns cluster assignments with wave ordering.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "plan": {
                        "type": "string",
                        "description": "The PM's task plan output (must contain TASK blocks)"
                    }
                },
                "required": ["plan"]
            }
        }),
        serde_json::json!({
            "name": "workflow_compress",
            "description": "Compress agent output to a structured summary: files changed, key changes, build/test status. Strips reasoning and verbose analysis.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "output": {
                        "type": "string",
                        "description": "Raw agent output to compress"
                    }
                },
                "required": ["output"]
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
        "module_api" => module_api(server, args),
        "codebase_brief" => codebase_brief(server, args),
        "workflow_init" => workflow_init(server, args),
        "workflow_cluster" => workflow_cluster(server, args),
        "workflow_compress" => workflow_compress(server, args),
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

fn develop_feature(_server: &OrrchMcpServer, args: &Value) -> String {
    let goal = args
        .get("goal")
        .and_then(|v| v.as_str())
        .unwrap_or("continue development");
    let project_dir = args
        .get("project_dir")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    format!(
        "Develop-feature workflow for: {goal}\n\
         Project: {project_dir}\n\n\
         Execute these steps using tools and Agent spawns:\n\n\
         1. Call MCP tool `workflow_init` with project_dir=\"{project_dir}\"\n\
         2. Spawn Agent (PM) with the instructions + brief from step 1\n\
         3. Call MCP tool `workflow_cluster` with the PM's plan output\n\
         4. For each cluster, spawn Agent in parallel with cluster tasks + brief\n\
         5. Call MCP tool `workflow_compress` on each agent's output\n\
         6. Spawn 2 Agents (testers) in parallel with file list from compressed output\n\
         7. Call MCP tool `workflow_compress` on tester outputs\n\
         8. Spawn Agent (PM) to evaluate — PASS/REWORK/SHIP_WITH_ISSUES\n\
         9. If REWORK: spawn Developer with fix list, re-verify (max 3 cycles)\n\
         10. Update PLAN.md [x], write DEVLOG.md, git commit"
    )
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

fn module_api(server: &OrrchMcpServer, args: &Value) -> String {
    let crate_name = match args.get("crate_name").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return "Error: 'crate_name' parameter is required".into(),
    };
    let module = match args.get("module").and_then(|v| v.as_str()) {
        Some(m) => m,
        None => return "Error: 'module' parameter is required".into(),
    };

    let orrch_dir = server.projects_dir.join("orrchestrator");
    let file_path = orrch_dir
        .join("crates")
        .join(crate_name)
        .join("src")
        .join(format!("{module}.rs"));

    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => return format!("Error: cannot read {}: {e}", file_path.display()),
    };

    let line_count = content.lines().count();
    let api = extract_pub_api(&content);

    format!("# {crate_name}::{module} ({line_count} lines)\n\n{api}")
}

fn codebase_brief(server: &OrrchMcpServer, args: &Value) -> String {
    let project = args
        .get("project")
        .and_then(|v| v.as_str())
        .unwrap_or("orrchestrator");

    let project_dir = server.projects_dir.join(project);
    if !project_dir.is_dir() {
        return format!("Error: project directory '{}' not found", project_dir.display());
    }

    let crates_dir = project_dir.join("crates");
    let mut output = format!("# {project} codebase brief\n\n");
    let mut total_lines = 0usize;

    // Collect crate dirs sorted.
    let mut crate_dirs: Vec<_> = match std::fs::read_dir(&crates_dir) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect(),
        Err(_) => Vec::new(),
    };
    crate_dirs.sort();

    for crate_path in &crate_dirs {
        let crate_name = crate_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Read Cargo.toml for dependency list.
        let deps = {
            let cargo_toml = crate_path.join("Cargo.toml");
            if let Ok(toml_content) = std::fs::read_to_string(&cargo_toml) {
                extract_cargo_deps(&toml_content)
            } else {
                Vec::new()
            }
        };

        output.push_str(&format!("## {crate_name}"));
        if !deps.is_empty() {
            output.push_str(&format!(" (deps: {})", deps.join(", ")));
        }
        output.push('\n');

        // Enumerate .rs source files under src/.
        let src_dir = crate_path.join("src");
        let mut rs_files: Vec<_> = match std::fs::read_dir(&src_dir) {
            Ok(rd) => rd
                .flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .is_some_and(|ext| ext == "rs")
                })
                .map(|e| e.path())
                .collect(),
            Err(_) => Vec::new(),
        };
        rs_files.sort();

        for rs_path in &rs_files {
            let module_name = rs_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let content = match std::fs::read_to_string(rs_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let line_count = content.lines().count();
            total_lines += line_count;

            let api = extract_pub_api(&content);

            output.push_str(&format!("\n### {crate_name}::{module_name} ({line_count} lines)\n"));
            output.push_str(&api);
            output.push('\n');

            // Cap total output to keep it manageable.
            if output.lines().count() >= 380 {
                output.push_str("\n... (truncated — use module_api for remaining modules)\n");
                break;
            }
        }

        if output.lines().count() >= 380 {
            break;
        }
    }

    // Append conventions footer.
    output.push_str(&format!(
        "\n---\n\n## Conventions\n\
         - Language: Rust (edition 2024), private scope — iterate fast\n\
         - Commits: conventional format (feat/fix/refactor/chore)\n\
         - One session per workflow — token efficiency is a core principle\n\
         - Workforce format: structured markdown with pipe-delimited step tables\n\
         - TUI: ratatui + crossterm, depth-level nav (Up/Down between bars, Left/Right within bars)\n\
         - Total indexed: ~{total_lines} lines across {} crates\n",
        crate_dirs.len()
    ));

    output
}

fn workflow_init(server: &OrrchMcpServer, args: &Value) -> String {
    let project_dir = match args.get("project_dir").and_then(|v| v.as_str()) {
        Some(d) => d,
        None => return "Error: 'project_dir' parameter is required".into(),
    };

    let project_path = std::path::Path::new(project_dir);

    // 1. Read .scope (default "private").
    let scope = std::fs::read_to_string(project_path.join(".scope"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "private".to_string());

    // 2. Run codebase_brief.sh.
    let brief_script = server.library_dir.join("tools/codebase_brief.sh");
    let brief_output = if brief_script.exists() {
        match std::process::Command::new("bash")
            .arg(&brief_script)
            .arg(project_dir)
            .output()
        {
            Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
            Err(e) => format!("(codebase_brief.sh failed: {e})"),
        }
    } else {
        "(codebase_brief.sh not found — skipping brief)".to_string()
    };

    // 3. Read PLAN.md and extract unchecked items.
    let plan_path = project_path.join("PLAN.md");
    let (plan_items, item_count) = match std::fs::read_to_string(&plan_path) {
        Ok(content) => {
            if content.contains("[ ]") {
                // Checkbox format — extract unchecked lines.
                let lines: Vec<&str> = content
                    .lines()
                    .filter(|l| l.contains("[ ]"))
                    .collect();
                let count = lines.len();
                (lines.join("\n"), count)
            } else if content.contains("### Task ") {
                // Task-header format — exclude DONE/COMPLETE lines.
                let lines: Vec<&str> = content
                    .lines()
                    .filter(|l| {
                        l.contains("### Task ")
                            && !l.to_uppercase().contains("DONE")
                            && !l.to_uppercase().contains("COMPLETE")
                    })
                    .collect();
                let count = lines.len();
                (lines.join("\n"), count)
            } else {
                // Unstructured fallback — return first 100 lines.
                let lines: Vec<&str> = content.lines().take(100).collect();
                let joined = lines.join("\n");
                (joined, 0usize)
            }
        }
        Err(_) => ("(PLAN.md not found)".to_string(), 0),
    };

    // 4. Read instructions_inbox.md — find straggler headings not yet implemented.
    let inbox_path = project_path.join("instructions_inbox.md");
    let stragglers = match std::fs::read_to_string(&inbox_path) {
        Ok(content) => {
            let lines: Vec<&str> = content
                .lines()
                .filter(|l| {
                    l.starts_with("### ")
                        && !l.contains("~~")
                        && !l.to_uppercase().contains("IMPLEMENTED")
                })
                .collect();
            if lines.is_empty() {
                "none".to_string()
            } else {
                lines.join("\n")
            }
        }
        Err(_) => "none".to_string(),
    };

    // 5. Create .orrch/ dir and write workflow.json at step 0.
    let orrch_dir = project_path.join(".orrch");
    let _ = std::fs::create_dir_all(&orrch_dir);
    let workflow_json = serde_json::json!({
        "step": 0,
        "status": "initialized",
        "project_dir": project_dir,
        "scope": scope,
    });
    let _ = std::fs::write(
        orrch_dir.join("workflow.json"),
        serde_json::to_string_pretty(&workflow_json).unwrap_or_default(),
    );

    // 6. Return structured response.
    format!(
        "## Workflow Initialized\n\
         Scope: {scope}\n\
         Items: {item_count} unchecked\n\
         Inbox stragglers: {stragglers_summary}\n\n\
         ## Codebase Brief\n\
         {brief_output}\n\
         ## Instructions (unchecked dev map items)\n\
         {plan_items}\n\n\
         ## Inbox Stragglers\n\
         {stragglers}\n\n\
         ## Next Step\n\
         Spawn a PM agent with the instructions and codebase brief above.\n\
         The PM must output tasks in this EXACT format (cluster_tasks.sh parses it):\n\n\
         TASK <id>: <description>\n\
         Agent: <role>\n\
         Files: <comma-separated paths>\n\
         Work: <2-3 sentences>\n\
         Acceptance: <one line>\n\
         Depends: <task ids or none>",
        stragglers_summary = if stragglers == "none" { "none".to_string() } else {
            format!("{} item(s)", stragglers.lines().count())
        },
    )
}

fn workflow_cluster(server: &OrrchMcpServer, args: &Value) -> String {
    let plan = match args.get("plan").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return "Error: 'plan' parameter is required".into(),
    };

    let script = server.library_dir.join("tools/cluster_tasks.sh");
    if !script.exists() {
        return format!("Error: cluster_tasks.sh not found at {}", script.display());
    }

    let tmp = std::env::temp_dir().join(format!("orrch-cluster-{}.txt", std::process::id()));
    if let Err(e) = std::fs::write(&tmp, plan) {
        return format!("Error: cannot write temp file: {e}");
    }

    let tmp_file = match std::fs::File::open(&tmp) {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return format!("Error: cannot open temp file: {e}");
        }
    };

    let result = std::process::Command::new("bash")
        .arg(&script)
        .stdin(tmp_file)
        .output();

    let _ = std::fs::remove_file(&tmp);

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if stdout.is_empty() && !stderr.is_empty() {
                format!("Error from cluster_tasks.sh:\n{stderr}")
            } else {
                stdout
            }
        }
        Err(e) => format!("Error: failed to run cluster_tasks.sh: {e}"),
    }
}

fn workflow_compress(server: &OrrchMcpServer, args: &Value) -> String {
    let output = match args.get("output").and_then(|v| v.as_str()) {
        Some(o) => o,
        None => return "Error: 'output' parameter is required".into(),
    };

    let script = server.library_dir.join("tools/compress_output.sh");
    if !script.exists() {
        return format!("Error: compress_output.sh not found at {}", script.display());
    }

    let tmp = std::env::temp_dir().join(format!("orrch-compress-{}.txt", std::process::id()));
    if let Err(e) = std::fs::write(&tmp, output) {
        return format!("Error: cannot write temp file: {e}");
    }

    let tmp_file = match std::fs::File::open(&tmp) {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return format!("Error: cannot open temp file: {e}");
        }
    };

    let result = std::process::Command::new("bash")
        .arg(&script)
        .stdin(tmp_file)
        .output();

    let _ = std::fs::remove_file(&tmp);

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if stdout.is_empty() && !stderr.is_empty() {
                format!("Error from compress_output.sh:\n{stderr}")
            } else {
                stdout
            }
        }
        Err(e) => format!("Error: failed to run compress_output.sh: {e}"),
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Extract the public API surface from Rust source content.
/// Uses simple line-by-line regex matching — not a full parser.
fn extract_pub_api(content: &str) -> String {
    // Compile patterns once per call via OnceLock not possible in fn scope easily,
    // so we use local Regex::new (cheap for short-lived calls; cached via OnceLock at module level).
    static PUB_ITEM: OnceLock<Regex> = OnceLock::new();
    static FIELD_LINE: OnceLock<Regex> = OnceLock::new();
    static VARIANT_LINE: OnceLock<Regex> = OnceLock::new();
    static COLOR_RGB: OnceLock<Regex> = OnceLock::new();

    let pub_item = PUB_ITEM.get_or_init(|| {
        Regex::new(r"^\s*pub\s+(fn|struct|enum|const|static|type|mod|trait)\s+(\w+)").unwrap()
    });
    let field_line = FIELD_LINE.get_or_init(|| {
        Regex::new(r"^\s+pub\s+(\w+)\s*:").unwrap()
    });
    let variant_line = VARIANT_LINE.get_or_init(|| {
        Regex::new(r"^\s+([A-Z][A-Za-z0-9_]*)\s*[,\{(]?$").unwrap()
    });
    let color_rgb = COLOR_RGB.get_or_init(|| {
        Regex::new(r"(?:const\s+\w+.*Rgb|Color::Rgb\s*\()").unwrap()
    });

    let lines: Vec<&str> = content.lines().collect();
    let n = lines.len();

    let mut fns: Vec<String> = Vec::new();
    let mut structs: Vec<String> = Vec::new();
    let mut enums: Vec<String> = Vec::new();
    let mut consts: Vec<String> = Vec::new();
    let mut mods: Vec<String> = Vec::new();
    let mut colors: Vec<String> = Vec::new();

    let mut i = 0;
    while i < n {
        let line = lines[i];

        // Color constants.
        if color_rgb.is_match(line) {
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() {
                colors.push(trimmed);
            }
            i += 1;
            continue;
        }

        if let Some(cap) = pub_item.captures(line) {
            let kind = cap.get(1).map_or("", |m| m.as_str());
            let name = cap.get(2).map_or("", |m| m.as_str());

            match kind {
                "fn" => {
                    // Collect the full signature up to the opening `{`.
                    let mut sig = line.trim().to_string();
                    // If line already contains `{`, truncate there.
                    if let Some(brace) = sig.find('{') {
                        sig = sig[..brace].trim_end().to_string();
                    } else {
                        // Multi-line sig — keep collecting until we hit `{` or `;`.
                        let mut j = i + 1;
                        while j < n && j < i + 8 {
                            let cont = lines[j].trim();
                            if cont.starts_with("//") {
                                j += 1;
                                continue;
                            }
                            let combined = format!("{sig} {cont}");
                            if cont.contains('{') {
                                let brace = combined.find('{').unwrap_or(combined.len());
                                sig = combined[..brace].trim_end().to_string();
                                break;
                            }
                            if cont.ends_with(';') {
                                sig = combined;
                                break;
                            }
                            sig = combined;
                            j += 1;
                        }
                    }
                    fns.push(sig);
                }
                "struct" => {
                    // Try to collect public field names.
                    let mut fields: Vec<String> = Vec::new();
                    // Check if it's a tuple/unit struct on one line.
                    if line.contains(';') || line.contains('(') {
                        structs.push(format!("struct {name} {{ ... }}"));
                    } else {
                        let mut j = i + 1;
                        while j < n && j < i + 40 {
                            let cont = lines[j];
                            if cont.trim() == "}" {
                                break;
                            }
                            if let Some(fcap) = field_line.captures(cont) {
                                fields.push(
                                    fcap.get(1).map_or("", |m| m.as_str()).to_string(),
                                );
                            }
                            j += 1;
                        }
                        if fields.is_empty() {
                            structs.push(format!("struct {name} {{ ... }}"));
                        } else if fields.len() > 8 {
                            let preview: Vec<_> = fields[..6].to_vec();
                            structs.push(format!(
                                "struct {name} {{ {}, ... }}",
                                preview.join(", ")
                            ));
                        } else {
                            structs.push(format!("struct {name} {{ {} }}", fields.join(", ")));
                        }
                    }
                }
                "enum" => {
                    // Collect variant names.
                    let mut variants: Vec<String> = Vec::new();
                    let mut j = i + 1;
                    while j < n && j < i + 60 {
                        let cont = lines[j];
                        if cont.trim() == "}" {
                            break;
                        }
                        if let Some(vcap) = variant_line.captures(cont) {
                            variants.push(
                                vcap.get(1).map_or("", |m| m.as_str()).to_string(),
                            );
                        }
                        j += 1;
                    }
                    if variants.is_empty() {
                        enums.push(format!("enum {name} {{ ... }}"));
                    } else if variants.len() > 10 {
                        let preview: Vec<_> = variants[..8].to_vec();
                        enums.push(format!(
                            "enum {name} {{ {}, ... ({} total) }}",
                            preview.join(", "),
                            variants.len()
                        ));
                    } else {
                        enums.push(format!("enum {name} {{ {} }}", variants.join(", ")));
                    }
                }
                "const" | "static" => {
                    consts.push(line.trim().trim_end_matches('{').trim().to_string());
                }
                "type" => {
                    consts.push(line.trim().trim_end_matches('{').trim().to_string());
                }
                "mod" => {
                    mods.push(format!("mod {name}"));
                }
                "trait" => {
                    structs.push(format!("trait {name} {{ ... }}"));
                }
                _ => {}
            }
        }

        i += 1;
    }

    let mut out = String::new();

    if !enums.is_empty() {
        out.push_str("## pub enums\n");
        for e in &enums {
            out.push_str(e);
            out.push('\n');
        }
        out.push('\n');
    }

    if !structs.is_empty() {
        out.push_str("## pub structs / traits\n");
        for s in &structs {
            out.push_str(s);
            out.push('\n');
        }
        out.push('\n');
    }

    if !fns.is_empty() {
        out.push_str("## pub fns\n");
        for f in &fns {
            out.push_str(f);
            out.push('\n');
        }
        out.push('\n');
    }

    if !consts.is_empty() {
        out.push_str("## pub consts / types\n");
        for c in &consts {
            out.push_str(c);
            out.push('\n');
        }
        out.push('\n');
    }

    if !mods.is_empty() {
        out.push_str("## pub mods\n");
        for m in &mods {
            out.push_str(m);
            out.push('\n');
        }
        out.push('\n');
    }

    if !colors.is_empty() {
        out.push_str("## color constants\n");
        for c in colors.iter().take(20) {
            out.push_str(c);
            out.push('\n');
        }
        out.push('\n');
    }

    if out.is_empty() {
        out.push_str("(no pub items found)\n");
    }

    out
}

/// Extract `[dependencies]` keys from a Cargo.toml content string.
fn extract_cargo_deps(toml_content: &str) -> Vec<String> {
    let mut in_deps = false;
    let mut deps: Vec<String> = Vec::new();

    for line in toml_content.lines() {
        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            in_deps = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_deps = false;
        }
        if in_deps {
            if let Some(raw_key) = trimmed.split('=').next() {
                // Strip workspace-style suffix: "tokio.workspace" → "tokio"
                let key = raw_key.trim().trim_matches('"');
                let key = key.split('.').next().unwrap_or(key).trim();
                if !key.is_empty() && !key.starts_with('#') {
                    deps.push(key.to_string());
                }
            }
        }
    }

    deps
}

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
        assert_eq!(tool_definitions().len(), 15);
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

    #[test]
    fn test_module_api_missing_crate_name() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({"module": "lib"});
        let result = module_api(&server, &args);
        assert!(result.starts_with("Error:"), "result: {result}");
    }

    #[test]
    fn test_module_api_missing_module() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({"crate_name": "orrch-core"});
        let result = module_api(&server, &args);
        assert!(result.starts_with("Error:"), "result: {result}");
    }

    #[test]
    fn test_module_api_nonexistent_file() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({"crate_name": "orrch-core", "module": "nonexistent_xyz"});
        let result = module_api(&server, &args);
        assert!(result.starts_with("Error:"), "result: {result}");
    }

    #[test]
    fn test_codebase_brief_nonexistent_project() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({"project": "nonexistent_project_xyz_123"});
        let result = codebase_brief(&server, &args);
        assert!(result.starts_with("Error:"), "result: {result}");
    }

    #[test]
    fn test_extract_pub_api_basic() {
        let src = r#"
pub struct Foo {
    pub name: String,
    pub count: u32,
}

pub enum Bar {
    Alpha,
    Beta,
    Gamma,
}

pub fn do_thing(x: u32) -> bool {
    x > 0
}

pub const MAX: u32 = 100;

pub mod inner;
"#;
        let api = extract_pub_api(src);
        assert!(api.contains("struct Foo"), "missing struct Foo: {api}");
        assert!(api.contains("name") || api.contains("count"), "missing fields: {api}");
        assert!(api.contains("enum Bar"), "missing enum Bar: {api}");
        assert!(api.contains("Alpha"), "missing variant Alpha: {api}");
        assert!(api.contains("do_thing"), "missing fn: {api}");
        assert!(api.contains("MAX"), "missing const: {api}");
        assert!(api.contains("mod inner"), "missing mod: {api}");
    }

    #[test]
    fn test_extract_pub_api_empty() {
        let src = "fn private_fn() {}\nstruct PrivateStruct {}\n";
        let api = extract_pub_api(src);
        assert!(api.contains("(no pub items found)"), "result: {api}");
    }

    #[test]
    fn test_extract_cargo_deps_basic() {
        let toml = "[dependencies]\ntokio.workspace = true\nserde_json = \"1\"\n\n[dev-dependencies]\ntempfile = \"3\"\n";
        let deps = extract_cargo_deps(toml);
        assert!(deps.contains(&"tokio".to_string()), "deps: {deps:?}");
        assert!(deps.contains(&"serde_json".to_string()), "deps: {deps:?}");
        // dev-dependencies should not be included.
        assert!(!deps.contains(&"tempfile".to_string()), "deps: {deps:?}");
    }
}
