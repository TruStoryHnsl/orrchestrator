## INSTRUCTION INTAKE

Trigger: user submits a prompt
Blocker: none

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Executive Assistant | * | separate development instructions from other input and pass them to COO
1 | Executive Assistant | * | immediately address input unrelated to direct software development
2 | Chief Operations Officer | skill:clarify | process raw instructions into optimized token-efficient instructions
3 | Chief Operations Officer | skill:parse | determine which project each instruction should be sent to
4 | Chief Operations Officer | tool:copy-file | append the new instructions to the appropriate project instruction_inbox.md files
5 | Project Manager | skill:synthesize_instructions | incorporate the new instructions into the project plan

Interrupts: none
