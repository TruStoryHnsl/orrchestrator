---
description: Create a feature branch with conventional naming and link to an issue
argument-hint: "<description> [--issue N] [--type feat|fix|refactor|chore|docs]"
allowed-tools: Bash, Read, Glob, Grep
---

# /feature-branch command

Creates a properly named feature branch following conventional branch naming, optionally linked to a GitHub issue.

## Parse $ARGUMENTS:

- `<description>` (required): short description of the feature/fix (will be slugified)
- `[--issue N]` (optional): GitHub issue number to link
- `[--type feat|fix|refactor|chore|docs]` (optional): branch type prefix. Default: `feat`.

## Execution steps:

### 1. Validate

- Must be inside a project directory with a `.git` directory (not the workspace-level git).
- Check current branch — warn if not on `main` and ask user if they want to branch from current branch or switch to main first.
- Ensure working tree is clean. If dirty, abort: "Uncommitted changes detected. Commit or stash first."

### 2. Create branch name

Convert the description to a conventional branch name:
- Format: `<type>/<slugified-description>` or `<type>/<issue-number>-<slugified-description>`
- Slugify: lowercase, replace spaces/special chars with hyphens, trim to 50 chars
- Examples:
  - `feat/add-voice-channels`
  - `fix/42-thumbnail-loading`
  - `refactor/auth-middleware`

### 3. Create and switch

```bash
git checkout -b <branch-name>
```

### 4. If --issue is provided

- Verify the issue exists using `gh issue view N` (skip if no remote or gh fails)
- Report the issue title for confirmation

### 5. Report

```
Branch created:
  Name:   <type>/<branch-name>
  From:   main @ <short-sha>
  Issue:  #N - <issue title> (if applicable)

  Next steps:
  1. Make your changes
  2. Use conventional commits: git commit -m "feat: description"
  3. When done: /commit-push-pr
```
