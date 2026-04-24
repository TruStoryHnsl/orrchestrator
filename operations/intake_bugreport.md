## INTAKE BUGREPORT

Trigger: user submits a prompt classified as a bug report (observed-but-not-expected behavior, error, regression)
Blocker: none

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Executive Assistant | skill:triage | classify the raw user input as a bug report (vs idea, feature, status query) and pass the full raw text to COO
2 | Beta Tester | skill:reproduce | attempt to reproduce the bug in a sandbox session; record observed behavior, expected behavior, environment, repro steps, and a proposed severity rating
3 | Chief Operations Officer | skill:bug-parse | structure the raw report plus reproduction notes into fields (title, symptoms, repro, affected components, severity, environment) in token-efficient form
4 | Chief Operations Officer | tool:write-review | write raw report and structured report to {{WORKSPACE}}/review.json with status pending and source idea filename
5 | Hypervisor | * | BLOCKING — stop and end turn while the user reviews the side-by-side raw vs structured bug report in Design > Intentions; the TUI sets review.json status to confirmed or rejected and spawns a fresh continuation session
6 | Chief Operations Officer | skill:parse | determine which project the confirmed bug belongs to by reading project CLAUDE.md/README.md/.scope and matching affected components; flag ambiguous reports for user re-routing
7 | Chief Operations Officer | tool:route-bug | append the confirmed bug to the target project bugs_inbox.md as a BUG-YYYY-MM-DD-NNN entry with source idea reference, creating the inbox file if missing
8 | Repository Manager | tool:ledger-append | append a pending entry to the target project .bugfix-ledger.md matching the existing ledger schema (Solution and Verification fields left blank for later fill-in by develop_feature)
9 | Project Manager | skill:synthesize_instructions | incorporate the new BUG into PLAN.md as a fix task, linking to the BUG-NNN identifier, and report what was incorporated

Interrupts: user rejects the structured report in step 5 (return to step 3 with user correction notes); COO cannot determine target project in step 6 (flag to user as unrouted bug and halt)
