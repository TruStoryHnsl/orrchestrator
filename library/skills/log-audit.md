---
name: log-audit
description: Write a full assessment report summarizing reconciliation decisions and reviewer findings
tags: [pm, audit, reporting]
---

# Log Audit

You are the Project Manager. The audit is finished. Write a persistent report so the user and future sessions can see exactly what the audit found and what was reconciled.

## Inputs

- The `RECONCILIATION SUMMARY` emitted by `reconcile-plan`
- The raw findings from `verify-claim`, `codebase-audit`, `gap-audit`, `exercise`
- The before/after PLAN.md diff (you may `git diff PLAN.md` to get it)

## Output

Write the report to `.orrch/assess_report_<YYYY-MM-DD-HHMM>.md` (create `.orrch/` if it does not exist).

```markdown
# Assessment Report — <project name>

**Date:** <YYYY-MM-DD HH:MM>
**Session branch:** <branch name>
**Commit before audit:** <short hash>

## Executive summary

<2-4 sentences: overall plan health, number of false-complete items, number of undeclared features, any structural concerns>

## Reconciliation decisions

| Item ID | Before | After | Reason |
|---|---|---|---|
| <id> | complete | in-progress | <short reason, cite evidence> |
| ... | ... | ... | ... |

## Backfilled from codebase

| Feature | Location | Phase placement |
|---|---|---|
| <name> | <file> | <phase or "Pending Reconciliation"> |
| ... | ... | ... |

## Stale references flagged

| Plan item | Referenced artifact | Status |
|---|---|---|
| <id> | <artifact> | <not found / renamed / moved> |
| ... | ... | ... |

## Discrepancies requiring user decision

<bullet per item where the audit could not conclude and the user must rule>

## Raw findings (for audit trail)

### Verify-claim verdicts
<paste or summarize>

### Codebase-audit findings
<paste or summarize>

### Gap-audit findings
<paste or summarize>

### Exercise findings
<paste or summarize>

## Next actions

<bulleted list — typically: "user review Pending Reconciliation items", "dispatch develop_feature for downgraded items", etc.>
```

## Rules

- The report is permanent — do not delete prior assess reports. Each audit gets its own timestamped file.
- Be concrete. Every table row cites evidence.
- Keep the executive summary readable by a user who did not run the audit.
- If any section is empty (no findings of that kind), write `_no findings_` rather than deleting the section.
