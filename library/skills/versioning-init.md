---
description: Archive current project build as vN and scaffold a fresh vN+1 directory for new development
argument-hint: "<project-name> [version-number]"
allowed-tools: Bash, Read, Glob, Grep, Write, Edit
---

# /versioning-init command

Archives the current active version of a project and scaffolds a fresh version directory.

## Parse $ARGUMENTS:

- `<project-name>` (required): directory name under `~/projects/`
- `[version-number]` (optional): the NEW version to create. Default: 2.

## Execution steps:

### 1. Validate

- Confirm `~/projects/<project>/` exists.
- Find existing version directories (`v1/`, `v2/`, etc.) by listing `v*/` directories.
- If the target `v<N>/` already exists, abort with an error.
- Determine the "current active version":
  - If version directories exist, the highest `vN/` is the current active version.
  - If no version directories exist, the project root is the current active version.

### 2. Check disk space

- Calculate the size of the source (active version) excluding `.git/`, `node_modules/`, `venv/`, `__pycache__/`, `.env`, `*.pyc`, `build/`, `dist/`, `.next/`, media files over 100MB.
- If archive would exceed 1GB, warn the user and ask for confirmation.
- Display: "Archive size: ~X MB. Proceed? (y/n)"

### 3. Create archive (vN-1 or v1)

- If archiving from project root (no existing versions):
  ```bash
  mkdir -p ~/projects/<project>/v1
  rsync -a --exclude='.git' --exclude='node_modules' --exclude='venv' \
    --exclude='__pycache__' --exclude='.env' --exclude='*.pyc' \
    --exclude='build' --exclude='dist' --exclude='.next' \
    --exclude='v1' --exclude='v2' \
    ~/projects/<project>/ ~/projects/<project>/v1/
  ```
- If archiving from an existing version directory (e.g., v2 → creating v3):
  - The existing highest version IS the archive already. No copy needed.
  - Just create the new version directory.

### 4. Scaffold new version (vN)

Create `~/projects/<project>/v<N>/` with:

**CLAUDE.md:**
```markdown
# <Project Name> v<N>

This is a fresh build of the feature-package detailed in this version folder's parent project directory.

## Context
- Previous version archived in `../v<N-1>/`
- Read `../v<N-1>/CLAUDE.md` for historical context and architecture decisions
- Read `../../CLAUDE.md` for workspace-level guidance

## Scope
Inherits from parent: <read ../.scope or "unset">

## What Changed (v<N-1> → v<N>)
<!-- Why does v<N> exist? What is being rebuilt/rethought? -->

## Architecture
<!-- Define the new architecture here -->

## Quick Start
<!-- How to run v<N> -->
```

**Copy from parent:**
- `.scope` file (if exists in parent project root)
- `.gitignore` (if exists)

### 5. Update parent CLAUDE.md

If `~/projects/<project>/CLAUDE.md` exists, prepend:
```
> **Active development is in v<N>/.** v<N-1> is archived.
```

### 6. Report

Print a summary:
```
versioning-init complete:
  Project:     <project>
  Archived:    v<N-1>/ (<size>)
  New version: v<N>/ (empty, scaffolded)
  Scope:       <scope value> (inherited)

  Next steps:
  1. Edit v<N>/CLAUDE.md to document what changed
  2. Agents can reference v<N-1>/ for code to port forward
  3. Run /interpret-user-instructions if you have a plan doc
```
