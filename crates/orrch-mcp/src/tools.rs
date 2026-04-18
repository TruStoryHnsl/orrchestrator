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
            "description": "Load the instruction-intake workflow skill with embedded instructions. Returns the skill content for the harness to execute. The skill writes its working state to the workspace directory provided (per-idea isolation prevents concurrent submissions from clobbering each other).",
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
                    },
                    "workspace": {
                        "type": "string",
                        "description": "Absolute path to the per-idea intake workspace dir. The skill writes workflow.json and review.json here. If omitted, defaults to .orrch/ in the current working directory (legacy behavior)."
                    },
                    "source_idea": {
                        "type": "string",
                        "description": "Filename of the originating idea in the vault (e.g. '2026-04-21-00-14.md'). Embedded in review.json so the TUI can advance vault progress when the user confirms."
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
            "description": "Load an agent profile and combine it with a task to produce a structured prompt via the orrch-agents prompt builder. Supports optional runtime dispatch context: when project_dir is set, the project's core context (CLAUDE.md or the filename named in .agent_profile) is appended; when workforce + operation + step_index are set, the step is resolved via resolve_step_for_dispatch so nested workforce expansion and per-step model overrides are baked into the returned prompt; a bare model_override shortcut is also accepted.",
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
                    },
                    "project_dir": {
                        "type": "string",
                        "description": "Optional: absolute path to the target project. When set, the project's core context file is loaded and appended to the prompt."
                    },
                    "profile_filename": {
                        "type": "string",
                        "description": "Optional: explicit core-context filename (e.g. 'CLAUDE.md', 'GEMINI.md'). When omitted, the project's .agent_profile dotfile is read, falling back to CLAUDE.md."
                    },
                    "workforce": {
                        "type": "string",
                        "description": "Optional: workforce name to resolve the step against (for nested workforce expansion)."
                    },
                    "operation": {
                        "type": "string",
                        "description": "Optional: operation name containing the step. Required together with workforce and step_index."
                    },
                    "step_index": {
                        "type": "string",
                        "description": "Optional: step index within the operation (e.g. '1', '2B'). Required together with workforce and operation."
                    },
                    "model_override": {
                        "type": "string",
                        "description": "Optional shortcut: directly inject a model override directive into the prompt without a full workforce lookup. Ignored if workforce+operation+step_index already produced a resolved step."
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
        serde_json::json!({
            "name": "skill_invoke",
            "description": "Load any skill from the orrchestrator library (`library/skills/<name>.md`) and return its content with arguments substituted. Supports every skill listed by `list_skills` — workflow skills (develop-feature, instruction-intake, pm-plan-edit), agent role skills (agent-pm, agent-developer, agent-tester, …), and meta-skills (bugfix-record, release, feature-branch, repo-init, scope, versioning-init, interpret-user-instructions, commercial-audit). The skill's `$ARGUMENTS` placeholder (Claude Code slash command convention) is replaced with the provided `args` string; when no placeholder exists the args are prepended as an `## Arguments` preamble.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Skill filename without .md extension (e.g. 'develop-feature', 'bugfix-record', 'agent-pm')"
                    },
                    "args": {
                        "type": "string",
                        "description": "Arguments or goal text passed to the skill. Substituted into any `$ARGUMENTS` placeholder; otherwise prepended as a preamble."
                    }
                },
                "required": ["name"]
            }
        }),
        serde_json::json!({
            "name": "remote_list_hosts",
            "description": "List all known remote hosts (orrion, orrgate, orrpheus, …) with SSH target, reachability status, and probed capabilities (OS, session multiplexer, claude CLI presence, codex CLI presence, gemini CLI presence, projects_dir, hostname). Runs the orrch-agent `check` subcommand over SSH for every non-local host. Robust to shell color noise — tolerates themed fish/zsh prompts that emit ANSI/OSC escape sequences on the first line.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        serde_json::json!({
            "name": "remote_discover_sessions",
            "description": "Discover active AI CLI sessions running on a remote host. Returns one JSON-like line per detected claude, codex, or gemini process with PID, command line, and current working directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "host": {
                        "type": "string",
                        "description": "Host name from `remote_list_hosts` (e.g. 'orrpheus', 'orrgate', 'orrion')"
                    }
                },
                "required": ["host"]
            }
        }),
        serde_json::json!({
            "name": "remote_list_sessions",
            "description": "List orrchestrator-managed sessions (those starting with the `orrch-` prefix) on a remote host. Returns one session name per line.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "host": {
                        "type": "string",
                        "description": "Host name from `remote_list_hosts`"
                    }
                },
                "required": ["host"]
            }
        }),
        serde_json::json!({
            "name": "remote_spawn_session",
            "description": "Spawn an orrchestrator-managed Claude CLI session on a remote host. The remote agent auto-detects the best session multiplexer (tmux > screen > nohup) and runs the requested backend with the given goal inside `~/projects/<project>/`. Returns the session name on success.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "host": {
                        "type": "string",
                        "description": "Host name from `remote_list_hosts`"
                    },
                    "project": {
                        "type": "string",
                        "description": "Project directory name under `~/projects/` on the remote host"
                    },
                    "backend": {
                        "type": "string",
                        "description": "Backend command to run (e.g. 'claude', 'gemini')"
                    },
                    "goal": {
                        "type": "string",
                        "description": "Initial goal string passed to the backend as its first prompt"
                    },
                    "flags": {
                        "type": "string",
                        "description": "Optional space-separated CLI flags to pass to the backend (e.g. '--dangerously-skip-permissions')"
                    }
                },
                "required": ["host", "project", "backend", "goal"]
            }
        }),
        serde_json::json!({
            "name": "remote_kill_session",
            "description": "Kill an orrchestrator-managed session on a remote host by name. Uses the remote agent's auto-detected multiplexer.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "host": {
                        "type": "string",
                        "description": "Host name from `remote_list_hosts`"
                    },
                    "session_name": {
                        "type": "string",
                        "description": "Session name to kill (e.g. 'orrch-concord')"
                    }
                },
                "required": ["host", "session_name"]
            }
        }),
        serde_json::json!({
            "name": "create_agent",
            "description": "Create a new agent profile .md file in the agents/ directory from the standard template. Returns the path of the created file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Agent name (used as the filename stem, e.g. 'my_agent')"},
                    "description": {"type": "string", "description": "Optional one-line description to inject into the template"}
                },
                "required": ["name"]
            }
        }),
        serde_json::json!({
            "name": "create_skill",
            "description": "Create a new skill .md file in library/skills/ from the standard template. Returns the path of the created file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Skill name (used as the filename stem)"},
                    "description": {"type": "string", "description": "Optional one-line description to inject into the template"}
                },
                "required": ["name"]
            }
        }),
        serde_json::json!({
            "name": "create_tool",
            "description": "Create a new tool .md file in library/tools/ from the standard template. Returns the path of the created file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Tool name (used as the filename stem)"},
                    "description": {"type": "string", "description": "Optional one-line description to inject into the template"}
                },
                "required": ["name"]
            }
        }),
        serde_json::json!({
            "name": "create_workflow",
            "description": "Create a new workforce/workflow .md file in workforces/ from the standard template. Returns the path of the created file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Workflow name (used as the filename stem)"},
                    "description": {"type": "string", "description": "Optional one-line description to inject into the template"}
                },
                "required": ["name"]
            }
        }),
        serde_json::json!({
            "name": "continue_intake",
            "description": "Continue a confirmed instruction intake session (steps 5-7): read the confirmed review.json from the workspace, embed the optimized instructions, and return the skill content for routing to projects, appending to instructions_inbox.md files, and PM incorporation into PLAN.md.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": {
                        "type": "string",
                        "description": "Absolute path to the per-idea intake workspace directory containing review.json"
                    },
                    "source_idea": {
                        "type": "string",
                        "description": "Optional filename of the originating idea in the vault (e.g. '2026-04-21-00-14.md')"
                    }
                },
                "required": ["workspace"]
            }
        }),
        serde_json::json!({
            "name": "incorporate_inbox",
            "description": "Return a prompt for a PM agent to incorporate all pending INS-NNN / OPT-NNN items from a project's instructions_inbox.md into PLAN.md, then clear the incorporated sections and commit both files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_dir": {
                        "type": "string",
                        "description": "Absolute path to the project directory containing instructions_inbox.md and PLAN.md"
                    }
                },
                "required": ["project_dir"]
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
        "skill_invoke" => skill_invoke(server, args),
        "remote_list_hosts" => remote_list_hosts().await,
        "remote_discover_sessions" => remote_discover_sessions(args).await,
        "remote_list_sessions" => remote_list_sessions(args).await,
        "remote_spawn_session" => remote_spawn_session(args).await,
        "remote_kill_session" => remote_kill_session(args).await,
        "create_agent" => create_library_entry(server, args, "agent"),
        "create_skill" => create_library_entry(server, args, "skill"),
        "create_tool" => create_library_entry(server, args, "tool"),
        "create_workflow" => create_library_entry(server, args, "workflow"),
        "continue_intake" => continue_intake(server, args),
        "incorporate_inbox" => incorporate_inbox(args),
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
         The Hypervisor is a THIN DISPATCHER. Execute exactly these steps:\n\n\
         1. Call MCP tool `workflow_init` with project_dir=\"{project_dir}\"\n\
         2. Ensure you are on the main branch at HEAD before spawning any worktree agents.\n\
            Run `git checkout main` if needed.\n\
         3. Spawn a SINGLE PM agent (subagent_type: general-purpose) with:\n\
            - The full codebase brief + unchecked items from step 1\n\
            - The goal: \"{goal}\"\n\
            - The PM instructions below\n\
         4. When the PM returns, take its final output and commit.\n\
            Update PLAN.md with [x] for completed items. Commit with conventional format.\n\n\
         That's it. The PM manages the entire dev loop. Do NOT:\n\
         - Cluster tasks yourself (PM does it)\n\
         - Spawn developer agents yourself (PM does it)\n\
         - Spawn tester agents yourself (PM does it)\n\
         - Evaluate pass/fail yourself (PM does it)\n\
         - Second-guess or filter the PM's task selection\n\n\
         ---\n\n\
         ## PM AGENT INSTRUCTIONS\n\n\
         You are the Project Manager. You own the entire dev loop for this sprint.\n\n\
         ### Phase 1: Task Selection\n\
         Read the unchecked items from the codebase brief. Select the next batch of\n\
         actionable items (no artificial limit — pick everything that's unblocked).\n\
         Output each as a TASK block:\n\n\
         ```\n\
         TASK <id>: <description>\n\
         Agent: <role>\n\
         Files: <comma-separated paths>\n\
         Work: <2-3 sentences>\n\
         Acceptance: <one measurable criterion>\n\
         Depends: <task ids or none>\n\
         ```\n\n\
         ### Phase 2: Clustering\n\
         Call MCP tool `workflow_cluster` with your TASK blocks.\n\
         The cluster tool groups tasks by shared files and assigns execution waves.\n\n\
         ### Phase 3: Agent Dispatch\n\
         For EACH cluster, spawn a Developer agent (using the Agent tool with\n\
         isolation: \"worktree\") with:\n\
         - All tasks in that cluster\n\
         - The codebase brief for context\n\
         - Instruction to run `cargo build` and `cargo test` after changes\n\
         Spawn clusters in the same wave IN PARALLEL (multiple Agent calls in one message).\n\
         Wait for wave N to complete before starting wave N+1.\n\
         Dispatch ALL tasks. Do not skip or defer any.\n\n\
         ### Phase 4: Compression\n\
         Call MCP tool `workflow_compress` on each developer agent's output.\n\n\
         ### Phase 5: Testing (conditional)\n\
         Spawn tester agents ONLY if the work involves significant structural changes:\n\
         new crates, new traits, modified auth/security code, or completing a full phase.\n\
         Otherwise, the developer's own `cargo test` is sufficient.\n\n\
         ### Phase 6: Evaluation\n\
         Review all compressed outputs. Determine: PASS / REWORK / SHIP_WITH_ISSUES.\n\
         If REWORK: spawn a Developer agent with the fix list (max 3 rework cycles).\n\n\
         ### Phase 7: Report\n\
         Return a final report listing:\n\
         - Each task ID and whether it passed\n\
         - Files changed per task\n\
         - Any worktree paths/branches that contain the changes\n\
         - Which PLAN.md items to mark [x]"
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

    // Workspace + source_idea: substitute into the skill template so the
    // skill writes state files to a per-idea directory and tags review.json
    // with the originating idea filename. Falls back to legacy `.orrch/`
    // behavior if not provided.
    let workspace = args
        .get("workspace")
        .and_then(|v| v.as_str())
        .unwrap_or(".orrch");
    let source_idea = args
        .get("source_idea")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let resolved_skill = skill_content
        .replace("{{WORKSPACE}}", workspace)
        .replace("{{SOURCE_IDEA}}", source_idea);

    let preamble = format!(
        "## Instructions to process\n\n{instructions}\n\n\
         ## Workspace\n\n\
         All state files (workflow.json, review.json) MUST be written to: `{workspace}`\n\
         Do NOT write to `.orrch/` — that path is reserved for in-project workflows.\n\n\
         ## Source Idea\n\n\
         This intake originated from vault idea: `{source_idea}`\n\
         Every JSON file you write must include `\"source_idea\": \"{source_idea}\"`.\n\n\
         ---\n\n"
    );

    format!("{preamble}{resolved_skill}")
}

fn continue_intake(server: &OrrchMcpServer, args: &Value) -> String {
    let workspace = match args.get("workspace").and_then(|v| v.as_str()) {
        Some(w) => w,
        None => return "Error: 'workspace' parameter is required".into(),
    };
    let source_idea = args
        .get("source_idea")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Read and parse review.json
    let review_path = std::path::Path::new(workspace).join("review.json");
    let review_content = match std::fs::read_to_string(&review_path) {
        Ok(c) => c,
        Err(e) => return format!("Error: cannot read {}: {e}", review_path.display()),
    };
    let review: Value = match serde_json::from_str(&review_content) {
        Ok(v) => v,
        Err(e) => return format!("Error: cannot parse review.json: {e}"),
    };

    // Verify confirmed status
    let status = review.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status != "confirmed" {
        return format!(
            "Error: review.json status is '{}', expected 'confirmed'. Confirm the review in the TUI first.",
            status
        );
    }

    // Extract optimized instructions
    let optimized = match review.get("optimized").and_then(|v| v.as_str()) {
        Some(o) => o.to_string(),
        None => return "Error: review.json missing 'optimized' field".into(),
    };

    // Load instruction-intake.md skill
    let skill_path = server.skills_dir.join("instruction-intake.md");
    let skill_content = match std::fs::read_to_string(&skill_path) {
        Ok(c) => c,
        Err(e) => return format!("Error: cannot read instruction-intake.md: {e}"),
    };

    let resolved_skill = skill_content
        .replace("{{WORKSPACE}}", workspace)
        .replace("{{SOURCE_IDEA}}", source_idea);

    format!(
        "## Confirmed intake to distribute\n\n\
         Workspace: {workspace}\n\
         Source idea: {source_idea}\n\n\
         ## Optimized instructions\n\n\
         {optimized}\n\n\
         ---\n\n\
         {resolved_skill}"
    )
}

fn incorporate_inbox(args: &Value) -> String {
    let project_dir = match args.get("project_dir").and_then(|v| v.as_str()) {
        Some(d) => d,
        None => return "Error: 'project_dir' parameter is required".into(),
    };

    let inbox_path = std::path::Path::new(project_dir).join("instructions_inbox.md");
    let inbox_content = match std::fs::read_to_string(&inbox_path) {
        Ok(c) => c,
        Err(e) => return format!("Error: cannot read instructions_inbox.md at {}: {e}", inbox_path.display()),
    };

    // Require at least one INS- or OPT- header
    if !inbox_content.contains("### INS-") && !inbox_content.contains("### OPT-") {
        return "Error: instructions_inbox.md contains no pending ### INS- or ### OPT- items".into();
    }

    format!(
        "## Incorporate inbox into plan\n\n\
         Project: {project_dir}\n\n\
         ## Pending inbox items\n\n\
         {inbox_content}\n\n\
         ## Instructions\n\n\
         You are the Project Manager. Read PLAN.md at {project_dir}/PLAN.md.\n\
         For each inbox item above, determine whether it extends an existing planned\n\
         feature, modifies a prior decision, or adds something new. Update PLAN.md\n\
         accordingly ([ ] status). Then clear the incorporated sections from\n\
         instructions_inbox.md, leaving any un-incorporated items. Commit both files.",
        project_dir = project_dir,
        inbox_content = inbox_content,
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
    use orrch_agents::{AgentProfile, AgentRunner, load_project_core_context};
    use orrch_workforce::{ResolvedStep, load_operations, load_workforces, resolve_step_for_dispatch};

    let agent_name = match args.get("agent").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => return "Error: 'agent' parameter is required".into(),
    };
    let task = match args.get("task").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return "Error: 'task' parameter is required".into(),
    };

    // Normalize: lowercase, spaces → underscores, ensure .md suffix.
    let normalized = agent_name.to_lowercase().replace(' ', "_");
    let filename = if normalized.ends_with(".md") {
        normalized
    } else {
        format!("{normalized}.md")
    };
    let path = server.agents_dir.join(&filename);

    // Load the profile via the orrch-agents loader so we get proper
    // frontmatter parsing + a real AgentProfile struct. On failure
    // (missing frontmatter, etc.) fall back to raw file contents so
    // custom agents without YAML headers still work.
    let profile: AgentProfile = match AgentProfile::load(&path) {
        Some(p) => p,
        None => {
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    return format!(
                        "Error: cannot read agent profile '{}': {e}",
                        path.display()
                    );
                }
            };
            AgentProfile {
                name: agent_name.to_string(),
                department: String::new(),
                role: agent_name.to_string(),
                description: String::new(),
                prompt: content,
                path: path.clone(),
            }
        }
    };

    // Task AP: load the project's core context if `project_dir` was
    // supplied. Honors the `.agent_profile` dotfile override (e.g.
    // GEMINI.md) with a CLAUDE.md fallback. The dotfile read is inlined
    // to keep orrch-mcp free of an orrch-core dep.
    let core_context: Option<String> = args
        .get("project_dir")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .and_then(|project_root| {
            let explicit = args.get("profile_filename").and_then(|v| v.as_str());
            let filename = explicit
                .map(|s| s.to_string())
                .unwrap_or_else(|| read_project_agent_profile_filename(&project_root));
            load_project_core_context(&project_root, &filename)
        });

    // Tasks 35 + 57: this is the runtime dispatch wiring point. When the
    // caller supplies workforce + operation + step_index, we look up the
    // step, run it through `resolve_step_for_dispatch` (which handles
    // nested workforce expansion + model overrides), and feed the result
    // into `build_prompt_for_resolved_step`. Callers that only need a
    // model override can pass `model_override` directly as a shortcut.
    let orrch_root = server.library_dir.parent();
    let resolved: Option<ResolvedStep> = (|| {
        let wf_name = args.get("workforce").and_then(|v| v.as_str())?;
        let op_name = args.get("operation").and_then(|v| v.as_str())?;
        let step_index = args.get("step_index").and_then(|v| v.as_str())?;
        let root = orrch_root?;
        let all_workforces = load_workforces(&root.join("workforces"));
        let workforce = all_workforces.iter().find(|w| w.name == wf_name).cloned()?;
        let operations = load_operations(&root.join("operations"));
        let operation = operations.iter().find(|o| o.name == op_name)?;
        let step = operation.steps.iter().find(|s| s.index == step_index)?;
        Some(resolve_step_for_dispatch(step, &workforce, &all_workforces))
    })()
    .or_else(|| {
        args.get("model_override")
            .and_then(|v| v.as_str())
            .map(|m| ResolvedStep {
                step_index: String::new(),
                agent_profile: profile.name.clone(),
                nested_workforce: None,
                model_override: Some(m.to_string()),
            })
    });

    match resolved {
        Some(r) => AgentRunner::build_prompt_for_resolved_step(
            &profile,
            task,
            core_context.as_deref(),
            &r,
        ),
        None => AgentRunner::build_prompt(&profile, task, core_context.as_deref()),
    }
}

/// Read a project's agent profile filename. Checks the `.agent_profile`
/// dotfile in the project root; falls back to `CLAUDE.md` when missing or
/// empty. Inlined in orrch-mcp to avoid depending on orrch-core.
fn read_project_agent_profile_filename(project_root: &std::path::Path) -> String {
    std::fs::read_to_string(project_root.join(".agent_profile"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "CLAUDE.md".to_string())
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

    // 1b. Check git branch state — warn if not on main.
    let git_branch = std::process::Command::new("git")
        .args(["-C", project_dir, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let git_commit = std::process::Command::new("git")
        .args(["-C", project_dir, "rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let branch_warning = if git_branch != "main" && git_branch != "master" {
        format!("\n⚠ WARNING: Not on main branch (on '{git_branch}'). \
                 Worktree agents will branch from this commit, not main HEAD. \
                 Run `git checkout main` first to avoid merge conflicts.\n")
    } else {
        String::new()
    };

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

    // 3. Read PLAN.md — extract unchecked items and detect plan readiness.
    //
    // plan_ready = true when PLAN.md tasks already have acceptance criteria,
    // file references, and dependency info. In that case the PM should convert
    // task prose to TASK blocks (context bundling), NOT re-plan from scratch.
    let plan_path = project_path.join("PLAN.md");
    let (plan_items, item_count, plan_ready) = match std::fs::read_to_string(&plan_path) {
        Ok(content) => {
            if content.contains("[ ]") {
                // Checkbox format — extract unchecked lines.
                // Skip lines explicitly marked DEPRECATED, deferred, MOVED, or
                // requiring prerequisites that aren't done yet — the PM should
                // not see them as actionable work.
                let lines: Vec<&str> = content
                    .lines()
                    .filter(|l| l.contains("[ ]"))
                    .filter(|l| {
                        let upper = l.to_uppercase();
                        !upper.contains("DEPRECATED")
                            && !upper.contains("MOVED TO CRITICAL PATH")
                            && !upper.contains("DEFERRED")
                            && !l.contains("*deferred:")
                            && !l.contains("_deferred:")
                    })
                    .collect();
                let count = lines.len();
                (lines.join("\n"), count, false)
            } else if content.contains("### Task ") {
                // Task-header format — extract full task sections for unchecked tasks.
                // Also detect plan readiness: do tasks have acceptance criteria + deps?
                let plan_lines: Vec<&str> = content.lines().collect();
                let mut sections = Vec::new();
                let mut current_section = Vec::new();
                let mut in_unchecked = false;
                let mut has_acceptance = 0u32;
                let mut total_tasks = 0u32;

                for line in &plan_lines {
                    if line.contains("### Task ") {
                        // Flush previous section
                        if in_unchecked && !current_section.is_empty() {
                            sections.push(current_section.join("\n"));
                        }
                        current_section.clear();
                        let is_done = line.to_uppercase().contains("DONE")
                            || line.to_uppercase().contains("COMPLETE");
                        in_unchecked = !is_done;
                        if in_unchecked {
                            total_tasks += 1;
                        }
                    }
                    // Detect plan detail signals within task sections
                    if in_unchecked {
                        let upper = line.to_uppercase();
                        if upper.contains("ACCEPTANCE")
                            || upper.contains("CRITERIA")
                            || (upper.contains("**AGENT**") && upper.contains(":"))
                        {
                            has_acceptance += 1;
                        }
                        current_section.push(*line);
                    }
                }
                // Flush final section
                if in_unchecked && !current_section.is_empty() {
                    sections.push(current_section.join("\n"));
                }

                let count = sections.len();
                // Plan is ready if >50% of tasks have acceptance criteria
                let ready = total_tasks > 0 && has_acceptance * 2 >= total_tasks;
                (sections.join("\n\n"), count, ready)
            } else {
                // Unstructured fallback — return first 100 lines.
                let lines: Vec<&str> = content.lines().take(100).collect();
                let joined = lines.join("\n");
                (joined, 0usize, false)
            }
        }
        Err(_) => ("(PLAN.md not found)".to_string(), 0, false),
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
        "plan_ready": plan_ready,
    });
    let _ = std::fs::write(
        orrch_dir.join("workflow.json"),
        serde_json::to_string_pretty(&workflow_json).unwrap_or_default(),
    );

    // 6. Build the Next Step instruction — conditional on plan_ready.
    let next_step = if plan_ready {
        "## Next Step (PLAN READY — skip full PM planning)\n\
         The PLAN.md tasks already have acceptance criteria and detail.\n\
         Spawn a LIGHTWEIGHT PM agent whose ONLY job is to convert these task\n\
         descriptions into TASK blocks with exact file paths. The PM must NOT\n\
         re-plan, re-analyze, or rewrite the tasks — just reformat them.\n\n\
         The PM must output tasks in this EXACT format (cluster_tasks.sh parses it):\n\n\
         TASK <id>: <description>\n\
         Agent: <role>\n\
         Files: <comma-separated paths>\n\
         Work: <2-3 sentences>\n\
         Acceptance: <one line>\n\
         Depends: <task ids or none>"
    } else {
        "## Next Step\n\
         Spawn a PM agent with the instructions and codebase brief above.\n\
         The PM must output tasks in this EXACT format (cluster_tasks.sh parses it):\n\n\
         TASK <id>: <description>\n\
         Agent: <role>\n\
         Files: <comma-separated paths>\n\
         Work: <2-3 sentences>\n\
         Acceptance: <one line>\n\
         Depends: <task ids or none>"
    };

    // 7. Return structured response.
    format!(
        "## Workflow Initialized\n\
         Scope: {scope}\n\
         Branch: {git_branch} ({git_commit})\n\
         Plan ready: {plan_ready}\n\
         Items: {item_count} unchecked\n\
         Inbox stragglers: {stragglers_summary}\n\
         {branch_warning}\n\
         ## Codebase Brief\n\
         {brief_output}\n\
         ## Instructions (unchecked dev map items)\n\
         {plan_items}\n\n\
         ## Inbox Stragglers\n\
         {stragglers}\n\n\
         {next_step}",
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

fn workflow_compress(_server: &OrrchMcpServer, args: &Value) -> String {
    let output = match args.get("output").and_then(|v| v.as_str()) {
        Some(o) => o,
        None => return "Error: 'output' parameter is required".into(),
    };

    if output.trim().is_empty() {
        return "## Compressed Output\n\n(empty input)".into();
    }

    compress_agent_output(output)
}

// ─── Generic skill invocation ───────────────────────────────────────────────

/// Load any skill from `<library>/skills/<name>.md` and return its content
/// with caller-provided arguments substituted.
///
/// Substitution model:
///   - Replace `$ARGUMENTS` placeholder (Claude Code slash-command convention)
///     with the caller's `args` string.
///   - If the skill has no `$ARGUMENTS` placeholder, prepend an
///     `## Arguments` preamble so the agent still sees the input.
fn skill_invoke(server: &OrrchMcpServer, args: &Value) -> String {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.trim(),
        None => return "Error: 'name' parameter is required".into(),
    };

    if name.is_empty() {
        return "Error: 'name' must be a non-empty string".into();
    }

    // Sanitize: reject anything that would escape the skills dir.
    if name.contains('/') || name.contains("..") || name.contains('\\') {
        return format!("Error: invalid skill name '{name}' — must be a plain filename stem");
    }

    let skill_args = args.get("args").and_then(|v| v.as_str()).unwrap_or("");

    // Allow callers to pass either `"develop-feature"` or `"develop-feature.md"`.
    let filename = if name.ends_with(".md") {
        name.to_string()
    } else {
        format!("{name}.md")
    };
    let path = server.skills_dir.join(&filename);

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return format!(
                "Error: cannot read skill '{filename}' from {}: {e}",
                server.skills_dir.display()
            );
        }
    };

    if content.contains("$ARGUMENTS") {
        content.replace("$ARGUMENTS", skill_args)
    } else if skill_args.is_empty() {
        content
    } else {
        format!("## Arguments\n\n{skill_args}\n\n---\n\n{content}")
    }
}

// ─── Remote host tools ──────────────────────────────────────────────────────
//
// Thin wrappers over `orrch_core::remote`. Each tool probes or acts on
// a known host via SSH + the embedded `orrch-agent.sh` script. Host
// names are resolved through `known_hosts()`; callers pass the short
// name (`orrpheus`, `orrgate`, `orrion`) rather than an SSH target.

fn resolve_host(name: &str) -> Result<orrch_core::remote::RemoteHost, String> {
    orrch_core::remote::known_hosts()
        .into_iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| {
            let available: Vec<String> = orrch_core::remote::known_hosts()
                .into_iter()
                .map(|h| h.name)
                .collect();
            format!(
                "Error: unknown host '{name}'. Available: {}",
                available.join(", ")
            )
        })
}

async fn remote_list_hosts() -> String {
    let mut hosts = orrch_core::remote::known_hosts();
    for host in hosts.iter_mut() {
        orrch_core::remote::check_host_reachable(host).await;
    }

    let mut out = String::from("## Remote Hosts\n\n");
    for host in &hosts {
        out.push_str(&format!(
            "- **{}** ({}) — ssh: `{}`\n",
            host.name,
            if host.is_local {
                "local"
            } else if host.reachable {
                "reachable"
            } else {
                "unreachable"
            },
            host.ssh_target
        ));
        if let Some(caps) = &host.capabilities {
            out.push_str(&format!(
                "    - os: {} | mux: {} | claude: {} | codex: {} | gemini: {} | hostname: {} | projects_dir: {}\n",
                caps.os, caps.mux, caps.claude, caps.codex, caps.gemini, caps.hostname, caps.projects_dir
            ));
        } else if !host.is_local && host.reachable {
            out.push_str("    - (reachable but no capability data — agent check parse failed)\n");
        }
    }
    out
}

async fn remote_discover_sessions(args: &Value) -> String {
    let host_name = match args.get("host").and_then(|v| v.as_str()) {
        Some(h) => h,
        None => return "Error: 'host' parameter is required".into(),
    };
    let host = match resolve_host(host_name) {
        Ok(h) => h,
        Err(e) => return e,
    };

    let sessions = orrch_core::remote::discover_remote_sessions(&host).await;
    if sessions.is_empty() {
        return format!("No agent CLI sessions found on {}.", host.name);
    }

    let mut out = format!("## Agent sessions on {}\n\n", host.name);
    for s in &sessions {
        out.push_str(&format!(
            "- pid {} — `{}` @ {}\n",
            s.pid, s.cmdline, s.project_dir
        ));
    }
    out
}

async fn remote_list_sessions(args: &Value) -> String {
    let host_name = match args.get("host").and_then(|v| v.as_str()) {
        Some(h) => h,
        None => return "Error: 'host' parameter is required".into(),
    };
    let host = match resolve_host(host_name) {
        Ok(h) => h,
        Err(e) => return e,
    };

    let sessions = orrch_core::remote::list_remote_managed_sessions(&host).await;
    if sessions.is_empty() {
        return format!("No orrch-managed sessions on {}.", host.name);
    }
    format!(
        "## Managed sessions on {}\n\n{}",
        host.name,
        sessions
            .iter()
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

async fn remote_spawn_session(args: &Value) -> String {
    let host_name = match args.get("host").and_then(|v| v.as_str()) {
        Some(h) => h,
        None => return "Error: 'host' parameter is required".into(),
    };
    let project = match args.get("project").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return "Error: 'project' parameter is required".into(),
    };
    let backend = match args.get("backend").and_then(|v| v.as_str()) {
        Some(b) => b,
        None => return "Error: 'backend' parameter is required".into(),
    };
    let goal = match args.get("goal").and_then(|v| v.as_str()) {
        Some(g) => g,
        None => return "Error: 'goal' parameter is required".into(),
    };
    let flags: Vec<String> = args
        .get("flags")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let host = match resolve_host(host_name) {
        Ok(h) => h,
        Err(e) => return e,
    };

    match orrch_core::remote::spawn_remote_session(&host, project, backend, goal, &flags).await {
        Ok(session_name) => format!(
            "Spawned `{session_name}` on {} (backend: {backend}, project: {project}).",
            host.name
        ),
        Err(e) => format!("Error: spawn failed on {}: {e}", host.name),
    }
}

async fn remote_kill_session(args: &Value) -> String {
    let host_name = match args.get("host").and_then(|v| v.as_str()) {
        Some(h) => h,
        None => return "Error: 'host' parameter is required".into(),
    };
    let session_name = match args.get("session_name").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return "Error: 'session_name' parameter is required".into(),
    };
    let host = match resolve_host(host_name) {
        Ok(h) => h,
        Err(e) => return e,
    };

    if orrch_core::remote::kill_remote_session(&host, session_name).await {
        format!("Killed `{session_name}` on {}.", host.name)
    } else {
        format!("Error: kill failed for `{session_name}` on {}.", host.name)
    }
}

/// Strip markdown formatting: **bold**, `backticks`, leading #, bullet markers.
fn strip_md(s: &str) -> String {
    s.replace("**", "")
        .replace('`', "")
        .replace("\\*", "*")
}

/// Extract file paths from a line. Matches crates/…, src/…, library/…, agents/…, etc.
fn extract_paths(line: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?:crates|src|library|agents|operations|workforces|plans)/[A-Za-z0-9_./-]+\.(?:rs|sh|toml|md|json|yaml)")
            .unwrap()
    });
    re.find_iter(line).map(|m| m.as_str().to_string()).collect()
}

/// Compress raw agent output into a structured summary.
/// Handles both structured agent summaries and raw cargo/test output.
fn compress_agent_output(output: &str) -> String {
    let clean = strip_md(output);
    let lines: Vec<&str> = clean.lines().collect();

    // ── 1. Extract file changes with per-file descriptions ──────────
    let mut file_entries: Vec<(String, String, String)> = Vec::new(); // (path, verb, description)
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Pattern: lines containing a file path followed by a separator and description
    // Matches: "- path/file.rs — Added foo()", "path/file.rs: description", "Modified: path"
    for line in &lines {
        let trimmed = line.trim().trim_start_matches("- ").trim_start_matches("* ");
        let paths = extract_paths(trimmed);
        if paths.is_empty() {
            continue;
        }

        for path in &paths {
            if seen_paths.contains(path) {
                continue;
            }

            // Try to extract description after the path
            let desc = if let Some(after) = trimmed.split(path).nth(1) {
                let after = after.trim().trim_start_matches("—").trim_start_matches("--")
                    .trim_start_matches(':').trim_start_matches(',').trim();
                if after.is_empty() { String::new() } else { after.to_string() }
            } else {
                String::new()
            };

            // Classify as Created or Modified
            let lower = trimmed.to_lowercase();
            let verb = if lower.contains("creat") || lower.contains("new file")
                || lower.contains("added file")
            {
                "Created"
            } else {
                "Modified"
            };

            seen_paths.insert(path.clone());
            file_entries.push((path.clone(), verb.to_string(), desc));
        }
    }

    // ── 2. Extract change descriptions ──────────────────────────────
    // Lines that describe what was done, not just file paths
    let change_verbs = [
        "add", "implement", "introduc", "creat", "wire", "hook", "bind",
        "extend", "expand", "replac", "refactor", "renam", "remov", "delet",
        "updat", "convert", "migrat", "integrat", "support", "enabl", "show",
        "render", "display", "handl", "track", "persist", "configur", "spawn",
        "dispatch", "inject", "pars", "extract", "validat", "check", "block",
        "reject", "auto-clos", "auto-reopen", "throttl",
    ];

    let mut changes: Vec<String> = Vec::new();
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("```")
            || trimmed.starts_with("---") || trimmed.starts_with('|')
        {
            continue;
        }

        let lower = trimmed.to_lowercase();

        // Skip lines that are just file paths with no description
        if extract_paths(trimmed).len() > 0 && trimmed.len() < 60 {
            continue;
        }

        // Lines starting with action verbs or containing them after a bullet
        let is_change = change_verbs.iter().any(|v| lower.contains(v));
        if !is_change {
            continue;
        }

        // Skip lines that are clearly section headers or noise
        if lower.starts_with("files") || lower.starts_with("status")
            || lower.starts_with("build") || lower.starts_with("test")
        {
            continue;
        }

        let entry = trimmed.trim_start_matches("- ").trim_start_matches("* ").to_string();
        if entry.len() > 15 && entry.len() < 300 && !changes.contains(&entry) {
            changes.push(entry);
        }
    }
    // Cap at 25 most relevant changes
    changes.truncate(25);

    // ── 3. Extract build/test status ────────────────────────────────
    let build_status = extract_build_status(&lines);
    let test_status = extract_test_status(&lines);

    // ── 4. Extract issues/concerns ──────────────────────────────────
    let issue_keywords = ["concern", "warning:", "issue:", "bug:", "blocker",
        "todo:", "fixme:", "note:", "error:", "panic", "unwrap"];
    let mut issues: Vec<String> = Vec::new();
    for line in &lines {
        let lower = line.to_lowercase();
        // Skip code lines
        if line.trim().starts_with("//") || line.trim().starts_with("use ")
            || line.trim().starts_with("mod ") || line.trim().starts_with("#[")
        {
            continue;
        }
        if issue_keywords.iter().any(|k| lower.contains(k)) {
            let entry = line.trim().to_string();
            if !issues.contains(&entry) && entry.len() > 5 {
                issues.push(entry);
            }
        }
    }
    issues.truncate(10);

    // ── 5. Format output ────────────────────────────────────────────
    let mut out = String::from("## Compressed Output\n\n");

    // Files
    out.push_str("### Files\n");
    if file_entries.is_empty() {
        out.push_str("(none detected)\n");
    } else {
        for (path, verb, desc) in &file_entries {
            if desc.is_empty() {
                out.push_str(&format!("{verb}: {path}\n"));
            } else {
                out.push_str(&format!("{verb}: {path} — {desc}\n"));
            }
        }
    }
    out.push('\n');

    // Changes
    out.push_str("### Changes\n");
    if changes.is_empty() {
        out.push_str("(none detected)\n");
    } else {
        for c in &changes {
            out.push_str(&format!("- {c}\n"));
        }
    }
    out.push('\n');

    // Status
    out.push_str("### Status\n");
    out.push_str(&format!("Build: {build_status}\n"));
    out.push_str(&format!("Tests: {test_status}\n"));
    if issues.is_empty() {
        out.push_str("Issues: none\n");
    } else {
        out.push_str("Issues:\n");
        for i in &issues {
            out.push_str(&format!("  {i}\n"));
        }
    }

    out
}

fn extract_build_status(lines: &[&str]) -> String {
    // Look for cargo build output or narrative statements
    for line in lines.iter().rev() {
        let lower = line.to_lowercase();
        if lower.contains("error[e") || lower.contains("error:") && lower.contains("could not compile") {
            return format!("FAIL — {}", line.trim());
        }
    }
    for line in lines.iter().rev() {
        let lower = line.to_lowercase();
        if lower.contains("finished") && (lower.contains("dev") || lower.contains("release") || lower.contains("test")) {
            return "pass".into();
        }
        if lower.contains("build") && (lower.contains("pass") || lower.contains("succeed") || lower.contains("clean")) {
            return "pass".into();
        }
        if lower.contains("compil") && (lower.contains("clean") || lower.contains("succeed") || lower.contains("success")) {
            return "pass".into();
        }
    }
    "(no build output detected)".into()
}

fn extract_test_status(lines: &[&str]) -> String {
    // Try to find "test result: ok. N passed" or "N tests pass" or "N/N pass"
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)(?:test result:.*?(\d+)\s*passed|(\d+)\s*(?:tests?\s*)?pass|all\s*(\d+)\s*tests?\s*pass|tests?:\s*(\d+)[\s/]+(\d+))")
            .unwrap()
    });

    let mut best_match = String::new();
    for line in lines {
        if re.is_match(line) {
            best_match = line.trim().to_string();
        }
    }

    if !best_match.is_empty() {
        return best_match;
    }

    // Narrative fallback: "All tests pass", "138 tests pass", "Tests: 90 pass"
    for line in lines.iter().rev() {
        let lower = line.to_lowercase();
        if (lower.contains("test") && lower.contains("pass"))
            || lower.contains("test result")
        {
            return line.trim().to_string();
        }
    }

    "(no test output detected)".into()
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

// ─── Library creation tools ─────────────────────────────────────────────────

// Minimal templates for the create_* tools. Kept in sync with
// orrch-library/src/templates.rs — if those change, update here too.
const CRT_AGENT_TEMPLATE: &str = "---\nname:\ndepartment: development/engineering\nrole:\ndescription: >\n\ncapabilities:\n  -\npreferred_backend: claude\n---\n\n# [Agent Name]\n\nYou are the [Role] — [one sentence describing this agent's purpose].\n\n## Core Behavior\n\n1.\n2.\n3.\n\n## What You Never Do\n\n-\n";
const CRT_SKILL_TEMPLATE: &str = "---\nname:\ndescription: >\n\ntype: skill\ndomain:\nusage: >\n\n---\n\n# [Skill Name]\n\n## Purpose\n\n\n\n## When to Use\n\n\n\n## Implementation\n\n```\n[Skill logic or instructions for the agent using this skill]\n```\n";
const CRT_TOOL_TEMPLATE: &str = "---\nname:\ndescription: >\n\ntype: tool\ncommand:\nargs:\n  -\nrequires:\n  -\n---\n\n# [Tool Name]\n\n## Purpose\n\n\n\n## Usage\n\n```bash\n[command example]\n```\n\n## Output Format\n\n\n";
const CRT_WORKFORCE_TEMPLATE: &str = "---\nname:\ndescription:\noperations:\n  -\n---\n\n## Agents\n\n| ID | Agent Profile | User-Facing |\n|----|---------------|-------------|\n|  |  | no |\n|  |  | yes |\n\n## Connections\n\n| From | To | Data Type |\n|------|----|----------|\n|  |  | instructions |\n";

/// Shared implementation for create_agent / create_skill / create_tool / create_workflow.
/// `kind` is one of "agent", "skill", "tool", "workflow".
fn create_library_entry(server: &OrrchMcpServer, args: &Value, kind: &str) -> String {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) if !n.is_empty() => n,
        _ => return "Error: 'name' parameter is required".into(),
    };
    let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");

    // Sanitize name for use as a filename stem.
    let safe_name: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();

    let (template, subdir): (&str, &str) = match kind {
        "agent"    => (CRT_AGENT_TEMPLATE, "agents"),
        "skill"    => (CRT_SKILL_TEMPLATE, "library/skills"),
        "tool"     => (CRT_TOOL_TEMPLATE, "library/tools"),
        "workflow" => (CRT_WORKFORCE_TEMPLATE, "workforces"),
        other      => return format!("Error: unknown kind '{other}'"),
    };

    // Inject name and optional description into the template.
    let mut content = template.replacen("name:", &format!("name: {safe_name}"), 1);
    if !description.is_empty() {
        content = content.replacen("description: >", &format!("description: {description}"), 1);
    }

    let dir = server.agents_dir.parent()
        .unwrap_or(&server.agents_dir)
        .join(subdir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return format!("Error: could not create directory {}: {e}", dir.display());
    }

    let filename = format!("{safe_name}.md");
    let path = dir.join(&filename);
    if path.exists() {
        return format!("Error: file already exists: {}", path.display());
    }
    match std::fs::write(&path, &content) {
        Ok(_) => format!("Created: {}", path.display()),
        Err(e) => format!("Error: could not write {}: {e}", path.display()),
    }
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
        // 15 base tools + skill_invoke + 5 remote_* tools + 4 create_* tools + continue_intake + incorporate_inbox = 27.
        assert_eq!(tool_definitions().len(), 27);
    }

    #[test]
    fn test_tool_definitions_include_new_tools() {
        let names: Vec<String> = tool_definitions()
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .collect();
        for expected in [
            "skill_invoke",
            "remote_list_hosts",
            "remote_discover_sessions",
            "remote_list_sessions",
            "remote_spawn_session",
            "remote_kill_session",
            "continue_intake",
            "incorporate_inbox",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "missing tool '{expected}' in {names:?}"
            );
        }
    }

    #[test]
    fn test_skill_invoke_missing_name() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({});
        let result = skill_invoke(&server, &args);
        assert!(result.starts_with("Error:"));
    }

    #[test]
    fn test_skill_invoke_path_traversal_rejected() {
        let server = OrrchMcpServer::from_defaults();
        let args = serde_json::json!({"name": "../../../etc/passwd"});
        let result = skill_invoke(&server, &args);
        assert!(result.starts_with("Error:"), "should reject traversal: {result}");
    }

    #[test]
    fn test_skill_invoke_loads_real_skill() {
        // Exercises the live library/skills dir. If orrchestrator is
        // checked out at the expected location we should find
        // develop-feature.md and see its content.
        let server = OrrchMcpServer::from_defaults();
        if !server.skills_dir.join("develop-feature.md").exists() {
            // Skip in environments where the library tree isn't present.
            return;
        }
        let args = serde_json::json!({"name": "develop-feature", "args": "build the thing"});
        let result = skill_invoke(&server, &args);
        assert!(
            !result.starts_with("Error:"),
            "skill_invoke should succeed: {result}"
        );
        // Either $ARGUMENTS was substituted, or the preamble was added.
        assert!(
            result.contains("build the thing"),
            "args not propagated: {result}"
        );
    }

    #[test]
    fn test_resolve_host_unknown() {
        let result = resolve_host("not_a_real_host_xyz").unwrap_err();
        assert!(result.contains("unknown host"));
    }

    #[test]
    fn test_resolve_host_known() {
        // Should find orrpheus (defined in orrch_core::remote::known_hosts).
        let host = resolve_host("orrpheus").expect("orrpheus in known_hosts");
        assert_eq!(host.name, "orrpheus");
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
    fn test_compress_agent_narrative() {
        // Simulate actual agent output format (markdown with em-dashes, bold, backticks)
        let input = r#"## Summary

### Task 64: Valve integration with Resource Optimizer

**`crates/orrch-library/src/model.rs`** -- Added two methods to `ValveStore`:
- `auto_close(provider, reason, duration_secs)` -- closes a valve with a calculated reopen timestamp
- `check_provider(provider) -> (bool, String)` -- returns blocked status and reason tuple

**`crates/orrch-core/src/backend.rs`** -- Added:
- `BackendKind::provider_name()` -- maps Claude to Anthropic, Gemini to Google

**`crates/orrch-core/src/usage.rs`** -- New file: UsageTracker with RateLimitConfig, rolling window

**`crates/orrch-tui/src/app.rs`** -- Wired valve check into `spawn_session()`

**Compilation:** Success (only pre-existing warnings)
**Tests:** 90/90 pass including 2 new tests
"#;
        let result = compress_agent_output(input);
        // Should detect files
        assert!(result.contains("crates/orrch-library/src/model.rs"), "missing model.rs: {result}");
        assert!(result.contains("crates/orrch-core/src/backend.rs"), "missing backend.rs: {result}");
        assert!(result.contains("crates/orrch-core/src/usage.rs"), "missing usage.rs: {result}");
        assert!(result.contains("crates/orrch-tui/src/app.rs"), "missing app.rs: {result}");
        // Should have descriptions
        assert!(result.contains("Added"), "missing change verb: {result}");
        // Should detect created file
        assert!(result.contains("Created") && result.contains("usage.rs"),
            "should detect new file: {result}");
        // Should extract test status
        assert!(result.contains("90") || result.contains("pass"), "missing test status: {result}");
        // Changes section should not be empty
        assert!(!result.contains("### Changes\n(none detected)"), "changes should not be empty: {result}");
    }

    #[test]
    fn test_compress_minimal_input() {
        let result = compress_agent_output("Just some text with no structured content.");
        assert!(result.contains("## Compressed Output"), "missing header: {result}");
        assert!(result.contains("(none detected)"), "should have none detected: {result}");
    }

    #[test]
    fn test_compress_with_file_descriptions() {
        let input = "Files changed:\n\
            - crates/orrch-core/src/plan_parser.rs — Added MoveDirection enum and move_feature_in_plan()\n\
            - crates/orrch-tui/src/ui.rs — Added draw_add_feature() overlay 55x12\n\
            \n\
            Build: passes. Tests: 138 pass.\n";
        let result = compress_agent_output(input);
        // Should capture per-file descriptions
        assert!(result.contains("plan_parser.rs"), "missing plan_parser.rs: {result}");
        assert!(result.contains("MoveDirection"), "missing description: {result}");
        assert!(result.contains("draw_add_feature"), "missing ui desc: {result}");
        // Should find test status
        assert!(result.contains("138") || result.contains("pass"), "missing tests: {result}");
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
