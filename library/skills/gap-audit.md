---
name: gap-audit
description: Flag plan items that reference files, modules, functions, or artifacts that do not exist
tags: [researcher, audit, reconciliation]
---

# Gap Audit

You are the Researcher running an audit. Scan PLAN.md for items that reference artifacts (files, modules, functions, scripts, endpoints, config keys, commands) that do not actually exist in the project. These are "stale references" — the plan has drifted from the code.

## Input

- The project's PLAN.md
- The project's codebase

## Process

1. For each plan item (complete, in-progress, or pending), extract any concrete references: file paths, function / struct / type names, command names, URL paths, config keys.
2. For each reference, check existence:
   - File path → does the file exist?
   - Function / struct / type → does the symbol exist in the project?
   - Command / script → is it invokable (exists in repo, on PATH for known tool scripts)?
   - Config key → is it read by any code path?
3. If the reference is clearly an aspirational placeholder in a pending item, note that as `planned` — not a gap.

## Output schema

```markdown
### GAP: <plan item id or line ref> — <referenced artifact>
**Item status in PLAN.md:** <complete | in-progress | pending>
**Referenced artifact:** <path, symbol, command>
**Existence check:** <not found | renamed to <alt> | moved to <path> | deleted in commit <hash>>
**Severity of gap:** <broken claim | outdated doc | planned | cosmetic>
```

## Rules

- Only flag items where the gap would mislead a reader (broken claim, outdated doc). Planned-but-not-yet-built references in pending items are fine and should be marked `planned`.
- If you cannot tell whether a reference exists, mark it `unverifiable` with rationale, do not guess.
- Never edit files. Report only.
- Keep each block terse (~5 lines).
- If you find zero gaps, emit `### NO GAPS FOUND` and halt.
