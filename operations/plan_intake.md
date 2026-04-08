## PLAN INTAKE

Trigger: user submits a prompt
Blocker: none

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Executive Assistant | * | separate plan/design input from other content and pass it to COO
2 | Chief Operations Officer | skill:clarify | parse the plan, determine scope, and decide whether to scaffold a new project or extend an existing one
3 | Chief Operations Officer | skill:parse | identify the target project (existing) or generate a project slug (new)
4 | Chief Operations Officer | tool:copy-file | route the parsed plan to the target project workspace
5 | Project Manager | skill:synthesize_instructions | incorporate the plan into the existing PLAN.md or scaffold a new project PLAN.md

Interrupts: none
