use crate::operation::{Operation, Step, TriggerCondition};
use crate::template::{Workforce, AgentNode, Connection, DataFlow};

/// Parse YAML frontmatter delimited by `---` lines.
/// Returns (frontmatter_text, body_text).
fn parse_frontmatter(content: &str) -> Option<(String, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..].trim_start_matches(['\r', '\n']);
    let end = after_first.find("\n---")?;
    let frontmatter = after_first[..end].to_string();
    let body = after_first[end + 4..].to_string();
    Some((frontmatter, body))
}

/// Extract a simple `key: value` field from YAML frontmatter.
fn extract_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let stripped = line.trim();
        if let Some(rest) = stripped.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix(':') {
                let value = value.trim();
                if value.is_empty() {
                    return None;
                }
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Extract a YAML list field (lines starting with `  - `) after the key line.
fn extract_list_field(frontmatter: &str, key: &str) -> Vec<String> {
    let mut found_key = false;
    let mut items = Vec::new();
    for line in frontmatter.lines() {
        let stripped = line.trim();
        if !found_key {
            if let Some(rest) = stripped.strip_prefix(key) {
                let rest = rest.trim_start();
                if rest.starts_with(':') {
                    found_key = true;
                }
            }
        } else if let Some(item) = stripped.strip_prefix("- ") {
            items.push(item.trim().to_string());
        } else {
            // Non-list-item line after key means list ended
            break;
        }
    }
    items
}

/// Parse a data type string from a connection table into a DataFlow variant.
fn parse_data_flow(s: &str) -> DataFlow {
    match s.to_lowercase().as_str() {
        "instructions" => DataFlow::Instructions,
        "deliverable" => DataFlow::Deliverable,
        "report" => DataFlow::Report,
        "research" => DataFlow::Research,
        "message" => DataFlow::Message,
        _ => DataFlow::Message, // fallback
    }
}

/// Parse a workforce markdown file into a Workforce struct.
///
/// Expected format:
/// ```markdown
/// ---
/// name: Workforce Name
/// description: Short description
/// operations:
///   - OPERATION ONE
///   - OPERATION TWO
/// ---
///
/// ## Agents
///
/// | ID | Agent Profile | User-Facing |
/// |----|---------------|-------------|
/// | pm | Project Manager | yes |
///
/// ## Connections
///
/// | From | To | Data Type |
/// |------|----|-----------|
/// | pm | dev | instructions |
/// ```
pub fn parse_workforce_markdown(content: &str) -> Option<Workforce> {
    let (frontmatter, body) = parse_frontmatter(content)?;

    let name = extract_field(&frontmatter, "name")?;
    let description = extract_field(&frontmatter, "description").unwrap_or_default();
    let operations = extract_list_field(&frontmatter, "operations");

    let mut agents = Vec::new();
    let mut connections = Vec::new();

    #[derive(PartialEq)]
    enum Section {
        None,
        Agents,
        Connections,
    }
    let mut section = Section::None;

    for line in body.lines() {
        let trimmed = line.trim();

        // Detect section headings
        if trimmed.starts_with("## ") {
            let heading = trimmed[3..].trim().to_lowercase();
            if heading == "agents" {
                section = Section::Agents;
            } else if heading == "connections" {
                section = Section::Connections;
            } else {
                section = Section::None;
            }
            continue;
        }

        // Skip header/separator rows in pipe tables
        if trimmed.starts_with('|') && (trimmed.contains("---") || trimmed.contains("ID") || trimmed.contains("From")) {
            continue;
        }

        // Parse pipe-delimited table rows
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let parts: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(|s| s.trim())
                .collect();

            match section {
                Section::Agents if parts.len() >= 3 => {
                    let id = parts[0].to_string();
                    let agent_profile = parts[1].to_string();
                    let user_facing = parts[2].to_lowercase() == "yes";
                    if !id.is_empty() {
                        agents.push(AgentNode {
                            id,
                            agent_profile,
                            user_facing,
                        });
                    }
                }
                Section::Connections if parts.len() >= 3 => {
                    let from = parts[0].to_string();
                    let to = parts[1].to_string();
                    let data_type = parse_data_flow(parts[2]);
                    if !from.is_empty() {
                        connections.push(Connection {
                            from,
                            to,
                            data_type,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(Workforce {
        name,
        description,
        agents,
        connections,
        operations,
    })
}

/// Parse a structured markdown workforce operation into an Operation struct.
///
/// Expected format:
/// ```markdown
/// ## OPERATION NAME
///
/// Trigger: <trigger description>
/// Blocker: <blocker description or "none">
///
/// ### Order of Operations
/// #### <index> | <agent> | <tool or skill> | <operation>
///
/// 1 | Agent Name | skill:name | description of what to do
/// 2 | Agent Name | * | description
///
/// Interrupts: <interrupt description or "none">
/// ```
pub fn parse_operation_markdown(content: &str) -> Option<Operation> {
    let mut name = String::new();
    let mut trigger = TriggerCondition::Manual;
    let mut steps = Vec::new();

    let mut in_steps = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Extract operation name from ## heading
        if let Some(heading) = trimmed.strip_prefix("## ") {
            if !heading.starts_with('#') {
                name = heading.trim().to_string();
                continue;
            }
        }

        // Extract trigger
        if let Some(trigger_text) = trimmed.strip_prefix("Trigger:") {
            let t = trigger_text.trim();
            if t.contains("user submits") || t.contains("user submit") {
                trigger = TriggerCondition::UserSubmit { input_type: "prompt".into() };
            } else if t.contains("unprocessed instructions") || t.contains("inbox") {
                trigger = TriggerCondition::InboxNotEmpty { project: "*".into() };
            } else {
                trigger = TriggerCondition::Manual;
            }
            continue;
        }

        // Detect start of step table
        if trimmed.contains("<index>") && trimmed.contains("<agent>") {
            in_steps = true;
            continue;
        }

        // Parse pipe-delimited step lines
        if in_steps && trimmed.contains('|') {
            let parts: Vec<&str> = trimmed.split('|').map(|s| s.trim()).collect();
            if parts.len() >= 4 {
                let index = parts[0].to_string();
                let agent = parts[1].to_string();
                let tool = if parts[2] == "*" || parts[2] == "?" {
                    None
                } else {
                    Some(parts[2].to_string())
                };
                let operation = parts[3..].join(" | ").to_string();

                if !index.is_empty() && !agent.is_empty() {
                    steps.push(Step {
                        index,
                        agent,
                        tool_or_skill: tool,
                        operation,
                        parallel_group: None, // TODO: detect from duplicate indices
                    });
                }
            }
        }

        // End of steps section
        if in_steps && (trimmed.starts_with("Interrupts:") || trimmed.is_empty() && !steps.is_empty()) {
            if trimmed.starts_with("Interrupts:") {
                in_steps = false;
            }
        }
    }

    if name.is_empty() || steps.is_empty() {
        return None;
    }

    // Auto-detect parallel groups: steps with the same index run in parallel
    let mut group_counter = 0u32;
    let mut i = 0;
    while i < steps.len() {
        let idx = &steps[i].index;
        let same_idx: Vec<usize> = (i..steps.len())
            .take_while(|&j| steps[j].index == *idx)
            .collect();
        if same_idx.len() > 1 {
            group_counter += 1;
            for j in same_idx.iter() {
                steps[*j].parallel_group = Some(group_counter);
            }
            i += same_idx.len();
        } else {
            i += 1;
        }
    }

    Some(Operation {
        name,
        trigger,
        blocker: None,
        steps,
        interrupts: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_instruction_intake() {
        let md = r#"
## INSTRUCTION INTAKE

Trigger: user submits a prompt

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Executive Assistant | * | separate dev instructions from other input
1B | Executive Assistant | * | immediately address non-dev input
2 | Chief Operations Officer | skill:clarify | process raw instructions into optimized instructions
3 | Chief Operations Officer | skill:parse | determine which project each instruction goes to
4 | Chief Operations Officer | tool:copy-file | append to appropriate project instruction_inbox.md
5 | Project Manager | skill:synthesize_instructions | incorporate into project plan

Interrupts: none
"#;
        let op = parse_operation_markdown(md).unwrap();
        assert_eq!(op.name, "INSTRUCTION INTAKE");
        assert_eq!(op.steps.len(), 6);
        assert_eq!(op.steps[0].agent, "Executive Assistant");
        assert_eq!(op.steps[2].tool_or_skill, Some("skill:clarify".into()));
    }

    #[test]
    fn test_parse_parallel_steps() {
        let md = r#"
## DEVELOP FEATURE

Trigger: unprocessed instructions in project queue

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Project Manager | * | synthesize instructions
2 | Developer | * | execute coding tasks
2 | Researcher | * | conduct research
2 | Software Engineer | * | design architecture
3 | Project Manager | * | review deliverable

Interrupts: none
"#;
        let op = parse_operation_markdown(md).unwrap();
        assert_eq!(op.name, "DEVELOP FEATURE");
        // Steps at index "2" should have the same parallel_group
        let group: Vec<Option<u32>> = op.steps.iter().map(|s| s.parallel_group).collect();
        assert_eq!(group[0], None); // step 1
        assert_eq!(group[1], group[2]); // step 2s share a group
        assert_eq!(group[2], group[3]); // all three
        assert!(group[1].is_some());
        assert_eq!(group[4], None); // step 3
    }

    #[test]
    fn test_parse_workforce_general_software_development() {
        let md = r#"---
name: General Software Development
description: Full dev team with PM, engineers, testers, and DevOps. Suitable for most software projects.
operations:
  - INSTRUCTION INTAKE
  - DEVELOP FEATURE
---

## Agents

| ID | Agent Profile | User-Facing |
|----|---------------|-------------|
| ea | Executive Assistant | yes |
| coo | Chief Operations Officer | no |
| pm | Project Manager | yes |
| eng | Software Engineer | no |
| dev | Developer | no |
| res | Researcher | no |
| ui | UI Designer | no |
| ft | Feature Tester | no |
| pt | Penetration Tester | no |
| bt | Beta Tester | no |
| rm | Repository Manager | no |

## Connections

| From | To | Data Type |
|------|----|-----------|
| ea | coo | instructions |
| coo | pm | instructions |
| pm | eng | instructions |
| pm | dev | instructions |
| pm | res | instructions |
| pm | ui | instructions |
| dev | ft | deliverable |
| dev | pt | deliverable |
| dev | bt | deliverable |
| ft | pm | report |
| pt | pm | report |
| bt | pm | report |
| eng | dev | instructions |
| res | eng | research |
| pm | rm | deliverable |
"#;
        let wf = parse_workforce_markdown(md).unwrap();
        assert_eq!(wf.name, "General Software Development");
        assert_eq!(wf.description, "Full dev team with PM, engineers, testers, and DevOps. Suitable for most software projects.");
        assert_eq!(wf.agents.len(), 11);
        assert_eq!(wf.connections.len(), 15);
        assert_eq!(wf.operations.len(), 2);
        assert_eq!(wf.operations[0], "INSTRUCTION INTAKE");
        assert_eq!(wf.operations[1], "DEVELOP FEATURE");

        // Verify first agent
        assert_eq!(wf.agents[0].id, "ea");
        assert_eq!(wf.agents[0].agent_profile, "Executive Assistant");
        assert!(wf.agents[0].user_facing);

        // Verify a non-user-facing agent
        assert_eq!(wf.agents[4].id, "dev");
        assert_eq!(wf.agents[4].agent_profile, "Developer");
        assert!(!wf.agents[4].user_facing);

        // Verify connection data types
        assert!(matches!(wf.connections[0].data_type, DataFlow::Instructions));
        assert!(matches!(wf.connections[6].data_type, DataFlow::Deliverable));
        assert!(matches!(wf.connections[9].data_type, DataFlow::Report));
        assert!(matches!(wf.connections[13].data_type, DataFlow::Research));
    }
}
