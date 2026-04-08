## KNOWLEDGE INTAKE

Trigger: user submits a prompt
Blocker: none

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Executive Assistant | * | classify the input as a custom library item (skill, tool, agent, mcp_server, model, or harness) and pass it to COO
2 | Chief Operations Officer | skill:classify | determine the item kind and select the matching library subdirectory (library/skills, library/tools, library/mcp_servers, library/models, library/harnesses)
3 | Mentor | skill:review_library | scan the chosen library subdirectory for duplicates, naming collisions, or conflicts with existing items
4 | Repository Manager | tool:write-file | commit the new .md file with YAML frontmatter to the chosen library subdirectory

Interrupts: none
