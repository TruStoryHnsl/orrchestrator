---
description: Create a SemVer release — tag, changelog entry, and GitHub Release for a project
argument-hint: "<project> [major|minor|patch] [--pre alpha|beta|rc]"
allowed-tools: Bash, Read, Write, Edit, Glob, Grep
---

# /release command

Creates a versioned release for a project: bumps version, updates CHANGELOG.md, creates an annotated git tag, and publishes a GitHub Release.

## Parse $ARGUMENTS:

- `<project>` (required): project directory name under `~/projects/`
- `[major|minor|patch]` (optional): SemVer bump type. Default: `patch`.
- `[--pre alpha|beta|rc]` (optional): create a pre-release instead of a stable release.

## Execution steps:

### 1. Validate and gather context

- Confirm `~/projects/<project>/` exists and has its own `.git` directory (not the workspace-level git). If no project-level git, abort with: "This project has no git repo. Initialize one with `git init` first, or run `/repo-init <project>` to set up a full GitHub repo."
- Read the project's `.scope` file to calibrate rigor.
- Find the latest git tag matching `v*` in the project repo:
  ```bash
  cd ~/projects/<project> && git tag -l 'v*' --sort=-v:refname | head -1
  ```
- If no tags exist, the current version is `0.0.0` and the next version will be calculated from that.
- Parse the current version and calculate the next version based on bump type.
- If `--pre` is specified, append the pre-release identifier: `vX.Y.Z-alpha.1`, incrementing the pre-release number if a previous pre-release exists.

### 2. Generate changelog entry

- Collect commits since the last tag:
  ```bash
  git log <last-tag>..HEAD --pretty=format:'%h %s' --no-merges
  ```
- If no commits since last tag, abort: "No changes since last release."
- Parse commits using conventional commit format where possible:
  - `feat:` → **Features**
  - `fix:` → **Bug Fixes**
  - `docs:` → **Documentation**
  - `refactor:` → **Refactoring**
  - `perf:` → **Performance**
  - `BREAKING CHANGE` or `!:` → **Breaking Changes**
  - Everything else → **Other Changes**
- Generate a changelog entry in Keep a Changelog format:
  ```markdown
  ## [X.Y.Z] - YYYY-MM-DD

  ### Breaking Changes
  - ...

  ### Features
  - ...

  ### Bug Fixes
  - ...
  ```

### 3. Update files

- If `CHANGELOG.md` exists in the project root, prepend the new entry after the header.
- If `CHANGELOG.md` does not exist, create it with a standard header + the entry.
- If `pyproject.toml` exists, update the `version = "X.Y.Z"` line.
- If `package.json` exists, update the `"version": "X.Y.Z"` field.
- If `Cargo.toml` exists, update the `version = "X.Y.Z"` line in `[package]`.

### 4. Commit and tag

- Stage the changed files (CHANGELOG.md + version files).
- Commit with message: `release: vX.Y.Z`
- Create an annotated tag:
  ```bash
  git tag -a vX.Y.Z -m "Release vX.Y.Z"
  ```

### 5. Push and create GitHub Release

- Check if a remote named `origin` exists. If not, skip this step and report: "No remote configured. Push manually when ready."
- Push the commit and tag:
  ```bash
  git push origin <branch> --follow-tags
  ```
- Create a GitHub Release using `gh`:
  ```bash
  gh release create vX.Y.Z --title "vX.Y.Z" --notes "<changelog entry>" [--prerelease]
  ```
  Add `--prerelease` flag if this is an alpha/beta/rc release.

### 6. Report

```
Release complete:
  Project:  <project>
  Version:  vX.Y.Z
  Tag:      vX.Y.Z
  Commits:  N commits included
  Release:  https://github.com/<owner>/<repo>/releases/tag/vX.Y.Z

  Changelog updated: CHANGELOG.md
  Version files updated: [list of files]
```

## Scope-aware behavior

- `private`: Skip GitHub Release creation. Just tag locally.
- `public`: Full workflow including GitHub Release.
- `commercial`: Full workflow. Warn if CHANGELOG.md was missing. Warn if any commit lacks conventional commit format.
