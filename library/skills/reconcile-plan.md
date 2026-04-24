---
name: reconcile-plan
description: Merge audit findings into PLAN.md — downgrade false-complete items, promote undocumented features, flag stale references
tags: [pm, plan, reconciliation, audit]
---

# Reconcile Plan

You are the Project Manager. The audit team has produced findings. Merge them into PLAN.md so the plan honestly represents the current state of the project.

## Inputs

- `read-plan` output (the items the audit examined)
- `verify-claim` findings (per-item verdicts)
- `codebase-audit` findings (undeclared features)
- `gap-audit` findings (stale references)
- `exercise` findings (user-perspective outcomes)
- The project's current PLAN.md

## Reconciliation rules

Apply in this order:

1. **Downgrade false-complete items.** For every item with `verify-claim` verdict `false` or `exercise` verdict `broken`:
   - Change status marker from complete to in-progress (`[x]` → `[ ]`, `DONE` → `IN PROGRESS`).
   - Append a dated note: `<!-- audit 2026-MM-DD: downgraded — <one-line reason, cite evidence> -->`

2. **Downgrade partials.** For `verify-claim` verdict `partial` or `exercise` verdict `partial`:
   - Keep complete marker but split the item: extract the working portion as complete, add a new in-progress sub-item for the gap with a dated note.

3. **Promote undocumented features.** For each `codebase-audit` UNDECLARED entry:
   - If the suggested phase is unambiguous: add a new `[x]` item with title, brief description, and file reference, and note: `<!-- audit 2026-MM-DD: backfilled from codebase, was not tracked -->`.
   - If the phase is ambiguous: add to a new section at the bottom titled `## Pending Reconciliation (audit <date>)` — the user decides placement.

4. **Flag stale references.** For each `gap-audit` GAP entry with severity `broken claim` or `outdated doc`:
   - Append an inline note to the offending item: `<!-- audit 2026-MM-DD: stale reference — <artifact> not found -->`.
   - Do not delete the item — the user reviews stale items before removal.

5. **Leave `planned` gap references alone.** Pending items referencing not-yet-built artifacts are expected.

## Output

1. Edit PLAN.md in place applying every rule above.
2. Emit a concise summary of reconciliation decisions:

```markdown
### RECONCILIATION SUMMARY
- Downgraded: <count> items (<comma-separated ids>)
- Split: <count> items (<ids>)
- Backfilled: <count> features (<short names>)
- Flagged stale: <count> references
- Awaiting user decision: <count> items (<ids>)
```

## Rules

- Never delete an item. Convert it or flag it.
- Preserve all existing formatting conventions (numbering, bold, indentation).
- Every change you make gets a dated `<!-- audit YYYY-MM-DD: ... -->` comment so a reviewer can see what the audit touched.
- If findings conflict (e.g., `verify-claim` verified but `exercise` broken), trust `exercise` — it is closer to the user's reality.
- If findings are insufficient to decide (`unverifiable` everywhere), add the item to `## Pending Reconciliation` rather than guessing.
