---
description: Initialize a project as its own git repo with GitHub remote, replacing monorepo membership
argument-hint: "<project> [--visibility private|public] [--description 'text']"
allowed-tools: Bash, Read, Write, Edit, Glob, Grep
---

# /repo-init command

Initializes a project directory as its own independent git repository with a GitHub remote. This is the migration path from the old workspace monorepo to per-project repos.

## Parse $ARGUMENTS:

- `<project>` (required): directory name under `~/projects/`
- `[--visibility private|public]` (optional): GitHub repo visibility. Default: read from `.scope` — private scope → private repo, public/commercial scope → public repo.
- `[--description 'text']` (optional): repo description for GitHub.

## Execution steps:

### 1. Validate

- Confirm `~/projects/<project>/` exists.
- Check if it already has its own `.git` directory (not a submodule of the workspace repo):
  ```bash
  test -d ~/projects/<project>/.git && echo "HAS_GIT" || echo "NO_GIT"
  ```
- If it already has `.git`, check if it has a GitHub remote. If yes, report "Already initialized" and show remote URL. If no remote, skip to step 3.
- Read `.scope` file for visibility default.

### 2. Initialize git

```bash
cd ~/projects/<project>
git init
git add -A
git commit -m "feat: initial commit"
```

**Exclude from initial commit** (add to .gitignore first if not present):
- `.env`, `*.env`, `.env.*`
- `node_modules/`, `venv/`, `__pycache__/`, `.pyc`
- `build/`, `dist/`, `.next/`
- `.DS_Store`, `._*`
- Media files over 100MB

### 3. Create GitHub repo

```bash
gh repo create TruStoryHnsl/<project> \
  --<visibility> \
  --source ~/projects/<project> \
  --description "<description or project purpose from CLAUDE.md>" \
  --push
```

### 4. Set up initial version tag

- If the project is functional/deployed, tag as `v1.0.0`:
  ```bash
  git tag -a v1.0.0 -m "Initial versioned release"
  ```
- If the project is still in development, tag as `v0.1.0`:
  ```bash
  git tag -a v0.1.0 -m "Initial development version"
  ```
- Push the tag:
  ```bash
  git push origin --tags
  ```

### 5. Create initial CHANGELOG.md (if scope is public or commercial)

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [X.Y.Z] - YYYY-MM-DD

### Added
- Initial release
```

### 6. Report

```
Repository initialized:
  Project:    <project>
  Repo:       https://github.com/TruStoryHnsl/<project>
  Visibility: <private|public>
  Version:    vX.Y.Z
  Branch:     main

  Next steps:
  1. Verify at: https://github.com/TruStoryHnsl/<project>
  2. Use conventional commits going forward
  3. Use /release to create future releases
  4. Use /feature-branch to start new work
```
