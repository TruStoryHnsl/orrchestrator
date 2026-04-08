## INSTRUCTION INTAKE

Trigger: user submits a prompt
Blocker: none

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Executive Assistant | skill:triage | triage raw user input — separate development instructions from status inquiries and general conversation, address non-dev items immediately, flag ambiguous items for the user
2 | Chief Operations Officer | skill:clarify | optimize raw development instructions into token-efficient numbered prompts (OPT-NNN), preserving user intent and stripping conversational filler
3 | Chief Operations Officer | tool:write-review | write the raw and optimized instructions to {{WORKSPACE}}/review.json with status pending and the source idea filename
4 | Hypervisor | * | BLOCKING — stop and end turn while the user reviews the side-by-side raw vs optimized comparison in Design > Intentions; the TUI sets review.json status to confirmed or rejected and spawns a fresh continuation session
5 | Chief Operations Officer | skill:parse | determine which project each confirmed instruction routes to by reading project CLAUDE.md/README.md/.scope; split cross-project instructions and route unattached ideas to scratchpad
6 | Chief Operations Officer | tool:copy-file | append each routed instruction to the appropriate project instructions_inbox.md as INS-NNN entries with source idea reference, creating the inbox file if missing
7 | Project Manager | skill:synthesize_instructions | for each affected project, incorporate new INS entries into PLAN.md (extend, modify, add, or flag conflicts) and report what was incorporated

Interrupts: none
