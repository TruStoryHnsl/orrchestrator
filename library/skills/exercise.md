---
name: exercise
description: Live-exercise a user-facing feature and record observed vs claimed behavior
tags: [beta-tester, audit, user-perspective]
---

# Exercise

You are the Beta Tester running an audit. For each user-facing claimed-complete feature passed to you, actually use it and report what happens from a user's perspective. This is the audit's highest-value check — the Feature Tester verified existence; you verify the lived experience.

## Input

- A list of user-facing claimed-complete features (typically the top-N most visible, N=5 default)
- The project's CLAUDE.md / README.md for claimed behavior

## Process (per feature)

1. Identify how a user would invoke the feature (command, keybinding, URL, menu entry).
2. Invoke it. Observe directly — run the command, launch the UI, hit the endpoint, trigger the workflow.
3. Record literally what happened: output verbatim, UI state, error text. Do not paraphrase.
4. Compare to the claim. If the claim says "user can X", did X happen?

## Output schema (per feature)

```markdown
### FEATURE: <name>
**Claim (paraphrased):** <what PLAN.md / docs say the user can do>
**Invocation:** <exact command, keybinding, or path used>
**Outcome:** <works-as-claimed | partial | broken | could-not-exercise>
**Observed (verbatim where applicable):**
<output, UI description, error text>
**Discrepancy (if any):** <one to two sentences describing what differs from the claim>
```

## Rules

- Never run destructive commands to exercise a feature. If a feature requires destruction to test, mark `could-not-exercise` with rationale.
- Capture output literally. No paraphrasing of logs, errors, or UI state.
- If a feature involves external systems that are currently unreachable, mark `could-not-exercise` with rationale — do not fabricate.
- Keep each block terse (~8 lines). If observed output is huge, excerpt the relevant portion and truncate with `[... N lines elided ...]`.
- Do not fix anything. Do not open a PR. Report only.
