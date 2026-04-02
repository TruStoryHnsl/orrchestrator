use crate::operation::{Operation, Step, TriggerCondition};

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
}
