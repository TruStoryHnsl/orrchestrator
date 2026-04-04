---
description: View, set, or auto-detect project scope classification (private/public/commercial)
argument-hint: "[project-name] [private|public|commercial|detect]" or "detect-all" or no args to list all
allowed-tools: Bash, Read, Glob, Grep, Write, Edit, Agent
---

# /scope command

You manage project scope tags stored as `.scope` files in project root directories.

## Parse $ARGUMENTS:

- **No arguments**: List all projects in `~/projects/` with their current `.scope` values. Show "unset" for projects missing a `.scope` file.
- **`detect-all`**: For every project directory in `~/projects/` that lacks a `.scope` file, spawn the `scope-detector` agent to analyze it. Present results as a table and ask the user to confirm before writing.
- **`<project> detect`**: Spawn the `scope-detector` agent on `~/projects/<project>/` and present the result. Ask user to confirm before writing.
- **`<project> <private|public|commercial>`**: Write the scope value directly to `~/projects/<project>/.scope`. No confirmation needed.

## When writing a .scope file:

1. Write a single word (`private`, `public`, or `commercial`) to `~/projects/<project>/.scope` with no trailing newline.
2. If the project has version subdirectories (v1/, v2/, etc.), copy the `.scope` file into the highest version directory as well.
3. Report what was written.

## When listing scopes:

Show a clean table:
```
Project              Scope
─────────────────────────────────
orrapus              private
concord              public
orracle-trainer      unset
```

Skip hidden directories, `node_modules`, `deprecated/`, and non-directory entries.

## When detecting:

Use the `scope-detector` agent. Pass it the full path to the project directory. Parse its structured output (SCOPE, CONFIDENCE, REASONING, KEY_SIGNALS). Present as a table with all fields. Only write the `.scope` file after user confirmation.
