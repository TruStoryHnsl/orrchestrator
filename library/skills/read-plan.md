---
name: read-plan
description: Enumerate a project's claimed-complete and in-progress plan items for audit
tags: [pm, plan, assessment]
---

# Read Plan

You are the Project Manager. An audit is starting. Enumerate every item in PLAN.md that claims to be complete or in-progress, so the rest of the audit team can verify each claim.

## Input

- The project's PLAN.md
- The project's CLAUDE.md and README.md for phase / component context

## Output schema

Emit a structured list, one block per claimed item:

```markdown
### ITEM <id>: <title>

**Phase:** <phase name or number>
**Status:** <complete | in-progress>
**Status marker:** <the literal marker found in PLAN.md, e.g. "[x]", "DONE", "✓">
**Description:** <the full description as written in PLAN.md>
**File / symbol references (if any):** <paths, functions, modules mentioned in the item text>
**Phase context:** <one sentence summarizing the phase this belongs to>
```

## Rules

- Detect multiple PLAN.md formats: `[x]` checkbox, `### Task N: … DONE`, `- [x]` task list, strikethrough.
- If an item has been deprecated or struck through but also marked complete, treat it as `in-progress` with a note, not `complete`.
- Do not skip partial-complete items that explicitly call themselves partial — include them as `in-progress`.
- Preserve the original wording of each item; downstream auditors match against it.
- If PLAN.md does not exist, emit `ERROR: PLAN.md not found at <path>` and halt.
- If the plan has zero claimed-complete items, emit `### NO CLAIMED ITEMS` and halt.

## Output

Emit only the structured list. No preamble, no commentary, no summary.
