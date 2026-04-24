---
name: codebase-audit
description: Find features that exist in the codebase but are not tracked in PLAN.md
tags: [developer, audit, reconciliation]
---

# Codebase Audit

You are the Developer running an audit. Scan the codebase for user-facing features, commands, subsystems, or public APIs that exist in code but are NOT reflected in PLAN.md. These are "undeclared features" — real capabilities the plan has not caught up to.

## Input

- The project's codebase
- The project's PLAN.md (so you know what IS tracked)
- The project's CLAUDE.md and README.md

## Process

1. Read PLAN.md and build a rough mental index of what is tracked.
2. Scan:
   - CLI entry points (argv parsing, command dispatch, `--help` coverage)
   - Public module APIs (`pub fn`, `pub struct`, exported symbols)
   - Route handlers / tool handlers / slash commands / MCP tools
   - UI panels / menu entries / keybindings
   - Configuration keys not mentioned in docs
3. For each candidate, check: does PLAN.md mention it by name, filename, or unambiguous paraphrase?
4. Emit a block per undeclared feature.

## Output schema

```markdown
### UNDECLARED: <short name>
**Kind:** <command | module | handler | panel | config key | api endpoint | other>
**Location:** <file:line or file range>
**What it does:** <one sentence>
**Why this is undeclared:** <PLAN.md has no matching item / PLAN.md has a superseded version / plan entry is too abstract to cover it>
**Suggested plan placement:** <phase name, or "requires user decision">
```

## Rules

- Skip internal helpers, private functions, test fixtures, and generated code. Only surface features a user, operator, or integrator would notice.
- If you find a feature that PLAN.md mentions under a different name, emit it anyway with `Why this is undeclared: name mismatch — PLAN.md calls it "<alt>"`.
- Never edit files. Report only.
- Keep each block terse (~5 lines).
- If you find zero undeclared features, emit `### NO UNDECLARED FEATURES` and halt.
