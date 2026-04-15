use std::path::{Path, PathBuf};

pub const AGENT_TEMPLATE: &str = r#"---
name:
department: development/engineering
role:
description: >

capabilities:
  -
preferred_backend: claude
---

# [Agent Name]

You are the [Role] — [one sentence describing this agent's purpose].

## Core Behavior

1.
2.
3.

## What You Never Do

-
"#;

pub const MODEL_TEMPLATE: &str = r#"---
name:
provider:
model_id:
tier: mid-tier
pricing: per_token
max_context: 128000
api_key_env:
capabilities:
  -
limitations:
  -
---

Notes about when to use this model and what it's best at.
"#;

pub const HARNESS_TEMPLATE: &str = r#"---
name:
command:
description:
capabilities:
  -
limitations:
  -
supported_models:
  -
flags:
  -
---

Practical guidance for the Resource Optimizer on when to choose this harness.
"#;

pub const MCP_SERVER_TEMPLATE: &str = r#"---
name:
description:
transport: stdio
command:
args:
  -
enabled: true
assigned_roles:
  -
---

Configuration notes for this MCP server.
"#;

pub const WORKFORCE_TEMPLATE: &str = r#"---
name:
description:
operations:
  -
---

## Agents

| ID | Agent Profile | User-Facing |
|----|---------------|-------------|
|  |  | no |
|  |  | yes |

## Connections

| From | To | Data Type |
|------|----|-----------|
|  |  | instructions |
"#;

pub const OPERATION_TEMPLATE: &str = r#"## [OPERATION NAME]

Trigger:
Blocker: none

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 |  | * |
2 |  | * |
3 |  | * |

Interrupts: none
"#;

pub const SKILL_TEMPLATE: &str = r#"---
name:
description: >

type: skill
domain:
usage: >

---

# [Skill Name]

## Purpose



## When to Use



## Implementation

```
[Skill logic or instructions for the agent using this skill]
```
"#;

pub const TOOL_TEMPLATE: &str = r#"---
name:
description: >

type: tool
command:
args:
  -
requires:
  -
---

# [Tool Name]

## Purpose



## Usage

```bash
[command example]
```

## Output Format


"#;

pub const PI_EXTENSION_TEMPLATE: &str = r#"import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

// {{name}} — orrchestrator PI extension
// Translated from: {{source}}
// Description: {{description}}

export default function (pi: ExtensionAPI) {
  // System prompt injection
  pi.on("session_start", async (_event, ctx) => {
    // TODO: add session_start logic
  });

  // Custom tool
  // pi.registerTool({ name: "{{slug}}", description: "{{description}}", parameters: ..., async execute(id, params, signal, onUpdate, ctx) { ... } });
}
"#;

/// Create a new file from template and return its path.
/// The file is created in the appropriate directory with a timestamp name.
pub fn create_from_template(category: TemplateCategory, base_dir: &Path) -> std::io::Result<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    match category {
        TemplateCategory::PiExtension => {
            let dir = base_dir.join("library/pi-extensions");
            std::fs::create_dir_all(&dir)?;
            let filename = format!("new_extension_{}.ts", ts);
            let path = dir.join(&filename);
            std::fs::write(&path, PI_EXTENSION_TEMPLATE)?;
            Ok(path)
        }
        _ => {
            let (template, subdir, prefix) = match category {
                TemplateCategory::Agent => (AGENT_TEMPLATE, "agents", "new_agent"),
                TemplateCategory::Model => (MODEL_TEMPLATE, "library/models", "new_model"),
                TemplateCategory::Harness => (HARNESS_TEMPLATE, "library/harnesses", "new_harness"),
                TemplateCategory::McpServer => (MCP_SERVER_TEMPLATE, "library/mcp_servers", "new_mcp"),
                TemplateCategory::Workforce => (WORKFORCE_TEMPLATE, "workforces", "new_workforce"),
                TemplateCategory::Operation => (OPERATION_TEMPLATE, "operations", "new_operation"),
                TemplateCategory::Skill => (SKILL_TEMPLATE, "library/skills", "new_skill"),
                TemplateCategory::Tool => (TOOL_TEMPLATE, "library/tools", "new_tool"),
                TemplateCategory::PiExtension => unreachable!(),
            };
            let dir = base_dir.join(subdir);
            std::fs::create_dir_all(&dir)?;
            let filename = format!("{}_{}.md", prefix, ts);
            let path = dir.join(&filename);
            std::fs::write(&path, template)?;
            Ok(path)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TemplateCategory {
    Agent,
    Model,
    Harness,
    McpServer,
    Workforce,
    Operation,
    Skill,
    Tool,
    PiExtension,
}
