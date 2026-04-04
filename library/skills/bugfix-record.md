---
description: Record a confirmed bug and its working solution to the project bugfix ledger
argument-hint: "[project-name] or no args for umbrella/admin issues"
allowed-tools: Bash, Read, Write, Edit, Glob, Grep, Agent
---

# /bugfix-record — Record a confirmed bugfix to the project ledger

You record bugs and their confirmed solutions into project-specific ledger files. This creates an institutional knowledge base of what broke, why, and what fixed it.

## Context gathering

1. Review the current conversation to identify:
   - **The bug**: What went wrong, symptoms, error messages, affected components
   - **Root cause**: Why it happened (if determined)
   - **The solution**: Exact steps/commands/changes that fixed it
   - **Confirmation**: How we verified the fix worked

2. If `$ARGUMENTS` names a project, the ledger goes in `~/projects/<project>/.bugfix-ledger.md`.
   If no arguments, the ledger goes in `~/projects/.bugfix-ledger.md` (umbrella/admin/infra issues).

## Ledger format

Each entry follows this structure. Append to the file (create if it doesn't exist).

```markdown
---

### BUG-<YYYY-MM-DD>-<seq> — <short title>

**Date:** <YYYY-MM-DD>
**Severity:** <critical | high | medium | low>
**Affected:** <machine/service/component>
**Trigger:** <what caused the bug to manifest>

#### Symptoms
<bulleted list of observable symptoms>

#### Root cause
<concise explanation of why this happened>

#### Solution
<exact steps, commands, or code changes that fixed it — copy-pasteable>

#### Verification
<how the fix was confirmed working>

#### Prevention
<what was put in place to prevent recurrence, if anything>
```

## Sequence numbering

Read the existing ledger file to find the last `BUG-<date>-<N>` entry for today's date. Increment N for the next entry. Start at 001 if this is the first entry for today.

## After recording

1. Show the user the recorded entry for review
2. Suggest running `/bugfix-analyze <project>` to update the architectural companion report
