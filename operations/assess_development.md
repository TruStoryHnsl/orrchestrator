## ASSESS DEVELOPMENT

Trigger: manually spawned from the Oversee panel (on-demand audit of a project's claimed-complete features vs actual state)
Blocker: none

### Order of Operations
#### <index> | <agent> | <tool or skill> | <operation>

1 | Project Manager | skill:read-plan | enumerate every plan item marked complete ([x], DONE, ✓) or in-progress, and produce a structured list with phase, id, title, file/symbol references
2 | Feature Tester | skill:verify-claim | for each claimed-complete item, attempt to verify that referenced files and symbols exist, tests pass, and the described behavior can be observed; emit pass/fail per item with evidence
2 | Developer | skill:codebase-audit | scan the codebase for features, modules, commands, or user-facing behavior that exist in code but are not tracked in PLAN.md; emit a list of undeclared features with file references
2 | Researcher | skill:gap-audit | scan PLAN.md for items that reference files, modules, functions, or external artifacts that do not exist in the project; emit a list of stale or orphaned plan references
3 | Beta Tester | skill:exercise | live-exercise the top-N user-facing claimed-complete features (N=5 default, configurable), record observed behavior vs claimed behavior; emit discrepancies with evidence
4 | Project Manager | skill:reconcile-plan | merge findings from steps 2 and 3 into PLAN.md: downgrade false-complete items to partial with notes, promote undocumented-complete items to tracked, flag stale plan references, add missing items with appropriate phase assignment
5 | Project Manager | skill:log-audit | write a full assessment report to .orrch/assess_report_<timestamp>.md summarizing reconciliation decisions, reviewers' raw findings, and open discrepancies requiring user input
6 | Repository Manager | mcp:github | commit the updated PLAN.md and assessment report with a docs(plan) reconciliation message
7 | Repository Manager | skill:merge-to-main | MANDATORY — merge the session branch back to main so subsequent development starts from the reconciled plan state; abort and escalate on conflicts

Interrupts: reviewer in step 2 reports that the project is in an inconsistent state preventing audit (partially refactored, broken build) — halt and report to user; merge-to-main conflict in step 7 requires user resolution
