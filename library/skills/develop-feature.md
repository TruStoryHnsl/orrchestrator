---
description: Execute the develop-feature workflow — a dispatch loop that spawns agents, pipes compressed output between steps, and commits results
argument-hint: "<development goal or instruction>" or no args to read from instructions_inbox.md
allowed-tools: Bash, Read, Glob, Grep, Write, Edit, Agent
---

# /develop-feature — Dispatch Loop

Execute each step below IN ORDER. Do not reason about the workflow — just run each step, store the result, and move to the next. All compression and batching is handled by tools, not by you.

Variables are written to `.orrch/` files. Read them back when a later step references them.

---

## INIT

```
mkdir -p .orrch
echo '{"workflow":"develop-feature","step":0,"status":"init"}' > .orrch/workflow.json
```

Read `.scope` if it exists → store as `$SCOPE` (default: "private").

**Parse input**: If $ARGUMENTS provided, that is `$INSTRUCTIONS`. Otherwise read `instructions_inbox.md`, collect unimplemented entries. If none, stop.

Write `$INSTRUCTIONS` to `.orrch/instructions.md`.

---

## STEP 1 — Codebase brief

```bash
~/projects/orrchestrator/library/tools/codebase_brief.sh "$(pwd)" > .orrch/codebase_brief.txt
```

OR call MCP tool `codebase_brief`. Store result in `.orrch/codebase_brief.txt`.

---

## STEP 2 — PM plans

Update workflow.json: `{"step":2,"status":"running","agent":"PM"}`.

Spawn ONE Agent:

```
prompt: |
  You are the Project Manager. Plan and delegate — never write code.

  Synthesize these instructions into a task list.

  ## Instructions
  <contents of .orrch/instructions.md>

  ## Codebase
  <contents of .orrch/codebase_brief.txt>

  ## Output format (MANDATORY — downstream tools parse this)
  For each task, output EXACTLY this block format:

  TASK <id>: <one-line description>
  Agent: <role — Developer, Software Engineer, UI Designer, Researcher, or Feature Tester>
  Files: <comma-separated paths of files this task will read or modify>
  Work: <what to do, 2-3 sentences max>
  Acceptance: <one-line pass/fail criteria>
  Depends: <comma-separated task ids, or "none">

  After all tasks, output:
  REUSE: <any cross-project reuse notes, or "none">
```

Write agent output to `.orrch/plan.md`.

---

## STEP 3 — Compress + cluster

```bash
cat .orrch/plan.md | ~/projects/orrchestrator/library/tools/compress_output.sh > .orrch/plan_compressed.md
cat .orrch/plan.md | ~/projects/orrchestrator/library/tools/cluster_tasks.sh > .orrch/clusters.txt
```

Read `.orrch/clusters.txt` to see the cluster assignments and wave structure.

---

## STEP 4 — Implement (parallel per cluster, sequential per wave)

Update workflow.json: `{"step":4,"status":"running"}`.

For each **wave** in `.orrch/clusters.txt` (Wave 1 first, then Wave 2, etc.):

For each **cluster** in the wave, spawn an Agent **in parallel** with other clusters in the same wave:

```
prompt: |
  You are the <cluster's suggested agent role>. Implement the assigned tasks.
  Scope: <$SCOPE> — iterate fast, no over-engineering for private.

  ## Codebase (do NOT read files for orientation — only read files you will EDIT)
  <contents of .orrch/codebase_brief.txt>

  <if Wave 2+>
  ## Prior changes this session
  <contents of .orrch/workspace_state.md>
  </if>

  ## Your tasks
  <paste ONLY the TASK blocks from .orrch/plan.md that belong to this cluster>

  ## Rules
  - Read each target file before editing
  - Follow existing code conventions
  - Only read files listed in your tasks' Files: field
  - Report: files modified/created, one line per file describing the change
```

After ALL agents in a wave complete:

```bash
# Compress each agent's output and append to workspace state
echo "--- Wave N results ---" >> .orrch/workspace_state.md
for each agent_output:
  echo "$agent_output" | ~/projects/orrchestrator/library/tools/compress_output.sh >> .orrch/workspace_state.md
done
```

Run `cargo build` to verify. If build fails, report error and stop.

Repeat for next wave.

---

## STEP 5 — Verify (parallel, isolated)

Update workflow.json: `{"step":5,"status":"running"}`.

Extract the file list from `.orrch/workspace_state.md` (just the paths, not what changed).

Spawn TWO Agents **in parallel** (context isolation — they share NO implementation details):

**Agent A (security)**:
```
prompt: |
  You are a security tester. Find vulnerabilities in the recently changed code.

  What was built: <contents of .orrch/instructions.md>

  Files to review (ONLY these): <file list from workspace_state>

  Report each finding as: SEVERITY | description | file:line | remediation
  One finding per line. No prose.
```

**Agent B (destructive)**:
```
prompt: |
  You are a destructive tester. Break the recently implemented features.

  What was built: <contents of .orrch/instructions.md>

  Files to review (ONLY these): <file list from workspace_state>

  Also run: cargo build && cargo test --workspace

  Report each failure as: SEVERITY | description | file:line | fix
  One failure per line. No prose.
```

Compress both outputs:
```bash
echo "$security_output" | ~/projects/orrchestrator/library/tools/compress_output.sh > .orrch/security_findings.md
echo "$destructive_output" | ~/projects/orrchestrator/library/tools/compress_output.sh > .orrch/destructive_findings.md
```

---

## STEP 6 — Evaluate

Update workflow.json: `{"step":6,"status":"running"}`.

Spawn ONE Agent:

```
prompt: |
  You are the Project Manager. Evaluate these verification findings and decide: PASS, REWORK, or SHIP_WITH_ISSUES.

  Instructions: <contents of .orrch/instructions.md>
  Changes made: <contents of .orrch/workspace_state.md>
  Security findings: <contents of .orrch/security_findings.md>
  Destructive findings: <contents of .orrch/destructive_findings.md>

  Output EXACTLY one of:
  VERDICT: PASS
  VERDICT: SHIP_WITH_ISSUES
  Known issues: <one per line>
  VERDICT: REWORK
  <for each fix needed, one line: FIX | file:line | what to fix | severity>
```

Write output to `.orrch/verdict.md`.

**If REWORK**: Spawn a Developer agent with the FIX lines as tasks. Then re-run STEP 5 and STEP 6. Max 3 rework cycles — after 3, force SHIP_WITH_ISSUES.

**If PASS or SHIP_WITH_ISSUES**: Continue to STEP 7.

---

## STEP 7 — Log + Commit

Update workflow.json: `{"step":7,"status":"finishing"}`.

**Dev log** — write directly to `DEVLOG.md` (no agent needed):
```
## Dev Session: <date>
### Completed
<from instructions.md — list each instruction>
### Known Issues
<from verdict.md if SHIP_WITH_ISSUES>
### Files Changed
<from workspace_state.md — file list>
### Next
<remaining inbox items, or "inbox clear">
```

**Version** — determine directly:
```bash
current=$(git tag -l 'v*' --sort=-v:refname | head -1)
# feat = minor bump, fix = patch, feat! = major
```

**Commit** — execute directly:
```bash
git add <files from workspace_state.md>
git commit -m "feat: <summary derived from instructions>"
```

Do NOT push or tag unless user requested a release.

---

## DONE

```
echo '{"workflow":"develop-feature","step":7,"status":"complete"}' > .orrch/workflow.json
```

Report: what was built, verification summary, commit hash, known issues.
