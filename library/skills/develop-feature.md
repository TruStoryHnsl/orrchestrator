---
description: Execute the develop-feature workflow — a dispatch loop that spawns agents, pipes compressed output between steps, and commits results
argument-hint: "<development goal or instruction>" or no args to read from instructions_inbox.md
allowed-tools: Bash, Read, Glob, Grep, Write, Edit, Agent
---

# /develop-feature — Dispatch Loop

You are a MECHANICAL DISPATCHER. Execute each step below in order. Between steps, your ONLY job is to read the previous step's output and feed it into the next step's input.

## Rules (MANDATORY — violating these wastes tokens)

1. **Do NOT reason about the workflow.** Do not analyze the project state, evaluate priorities, suggest next steps, or generate commentary. Just execute steps.
2. **Do NOT generate insights, observations, or explanations.** No "★ Insight" blocks. No "Let me identify..." No "The most impactful..." You are a dispatcher, not a strategist.
3. **Do NOT read files beyond what a step explicitly requires.** If a step says "read instructions_inbox.md", do not also read PLAN.md "for context."
4. **If a step says STOP, stop.** Do not look for alternative work. Report the stop condition to the user and end.
5. **State goes to disk, not your context.** Write outputs to `.orrch/` files. Read them back only when a later step references them.
6. **Compressed output ONLY.** After `workflow_compress`, use ONLY the compressed summary for all subsequent steps. Never pass raw agent output forward.

---

## INIT

```
mkdir -p .orrch
echo '{"workflow":"develop-feature","step":0,"status":"init"}' > .orrch/workflow.json
```

Read `.scope` if it exists → store as `$SCOPE` (default: "private").

**Parse input**: If $ARGUMENTS is a specific goal, use that as `$INSTRUCTIONS`. If $ARGUMENTS is "continue development" or "continue", read `PLAN.md` and collect unchecked `[ ]` items from the lowest incomplete phase.

**If no unchecked items: STOP.** Say "Dev map is complete — no unchecked items in PLAN.md." Do not search elsewhere.

Write `$INSTRUCTIONS` to `.orrch/instructions.md`.

---

## STEP 1 — Codebase brief

```bash
~/projects/orrchestrator/library/tools/codebase_brief.sh "$(pwd)" > .orrch/codebase_brief.txt
```

OR call MCP tool `codebase_brief`. Store result in `.orrch/codebase_brief.txt`.

---

## STEP 2 — PM plans (CONDITIONAL)

Update workflow.json: `{"step":2,"status":"running","agent":"PM"}`.

Check the `plan_ready` flag from workflow_init output (also in `.orrch/workflow.json`).

### If plan_ready = TRUE — Lightweight PM (context bundler)

The PLAN.md tasks already have acceptance criteria and detail. The PM's ONLY job is to reformat them into TASK blocks with exact file paths. The PM must NOT re-plan, re-analyze, or rewrite the logic.

Spawn ONE Agent:

```
prompt: |
  You are the Project Manager in CONVERSION MODE.
  The plan below is already detailed — do NOT rewrite it.
  Your ONLY job: convert each task into a TASK block with exact file paths.

  ## Instructions (already detailed — just reformat)
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

  Rules:
  - Use the codebase brief to determine exact file paths
  - Preserve all acceptance criteria from the plan — do not simplify
  - Preserve all dependency relationships from the plan
  - Do NOT add tasks, remove tasks, or change scope
```

### If plan_ready = FALSE — Full PM (planner)

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

## STEP 3.5 — Bundle file contents per cluster

For each cluster in `.orrch/clusters.txt`:
1. Extract the `Files:` list for all tasks in that cluster
2. Read each unique file using the Read tool (cap: 300 lines per file)
3. Write the bundled contents to `.orrch/cluster_<N>_context.md` in this format:

```markdown
## File: <path>
```<ext>
<file contents>
```
```

This is the **context bundle** — dev agents receive these instead of reading files themselves. This eliminates the single largest source of redundant token spend.

---

## STEP 4 — Implement (parallel per cluster, sequential per wave)

Update workflow.json: `{"step":4,"status":"running"}`.

For each **wave** in `.orrch/clusters.txt` (Wave 1 first, then Wave 2, etc.):

For each **cluster** in the wave, spawn an Agent **in parallel** with other clusters in the same wave:

```
prompt: |
  You are the <cluster's suggested agent role>. Implement the assigned tasks.
  Scope: <$SCOPE> — iterate fast, no over-engineering for private.

  ## Codebase API
  <contents of .orrch/codebase_brief.txt>

  <if Wave 2+>
  ## Prior changes this session
  <contents of .orrch/workspace_state.md>
  </if>

  ## Source files (pre-loaded — do NOT re-read these)
  <contents of .orrch/cluster_<N>_context.md>

  ## Your tasks
  <paste ONLY the TASK blocks from .orrch/plan.md that belong to this cluster>

  ## Rules
  - The source files above are ALREADY loaded — do NOT read them again
  - You MAY read other files not in the bundle if your implementation requires it
  - Follow existing code conventions
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

## STEP 5 — Verify (CONDITIONAL)

Update workflow.json: `{"step":5,"status":"running"}`.

### Determine testing level

Check `.orrch/workspace_state.md` for these signals:

**FULL testing** (2 parallel tester agents) — if ANY of:
- The work completes an entire phase or milestone (multiple tasks across a plan section)
- New traits, modules, or crate boundaries were created (look for "Created:" lines with `trait`, `mod.rs`, new crate paths)
- Safety-critical code was modified: math (projection, rotation, transforms), crypto, auth, coordinate systems
- Architectural wiring changed (e.g. main.rs pipeline, trait definitions, cross-crate interfaces)

**LIGHT testing** (no tester agents) — if NONE of the above:
- Just run: `cargo test --workspace`
- If tests pass: write "LIGHT VERIFY: all tests pass" to `.orrch/security_findings.md` and `.orrch/destructive_findings.md`
- Skip to STEP 6

### FULL testing protocol

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

**Update dev map** — mark completed items in `PLAN.md`:
For each instruction/feature that was implemented (from `.orrch/instructions.md`), find its matching entry in `PLAN.md` and change `[ ]` to `[x]`. Match by instruction ID (e.g., `INS-004`) or feature name keyword. If no matching entry exists in PLAN.md, skip — the dev map only tracks items that were formally planned.

**Clean instruction inbox** — if ALL instructions from the current batch are now implemented:
1. Write a short completion record to the source idea document (the file referenced in the inbox header's `Source:` field): `> Batch complete: N/N instructions implemented (<date>)`
2. Clear the inbox to just its header: `# Orrchestrator — Instruction Inbox\n\nNo pending instructions.`

**Version** — determine directly:
```bash
current=$(git tag -l 'v*' --sort=-v:refname | head -1)
# feat = minor bump, fix = patch, feat! = major
```

**Commit** — execute directly (include PLAN.md and instructions_inbox.md in the staged files):
```bash
git add <files from workspace_state.md>
git commit -m "feat: <summary derived from instructions>"
```

Do NOT tag unless the user requested a release. **Push is required** so the session branch exists on the remote before the merge step.

```bash
git push -u origin HEAD 2>/dev/null || true   # private scope: no remote, ignore
```

---

## STEP 8 — Merge session branch to main (MANDATORY, TIERED)

Update workflow.json: `{"step":8,"status":"merging"}`.

Session branches exist for isolation WHILE WORKING. Once the work is committed, merge back to `main` so follow-up sessions start from the integrated codebase. Leaving branches unmerged is the direct cause of the parallel-session regression cascade. Standing authorization — no per-session user prompt.

Run the tiered merge tool:

```bash
~/projects/orrchestrator/library/tools/merge_to_main.sh
```

The tool handles the common case automatically:
1. **Patience merge** — git's default merge with `-X patience` already auto-resolves disjoint changes in the same file.
2. **Union-merge via `.gitattributes`** — additive files (PLAN.md, DEVLOG.md, instructions_inbox.md, etc.) are concatenated silently, no conflict markers.
3. **LLM per-file resolver** — remaining conflicts get classified as COMBINE (both kept), PICK_OURS, PICK_THEIRS, or ESCALATE. Only ESCALATE surfaces to the user.
4. **Pre-merge checkpoint tag** — main is tagged before the merge so any bad auto-resolve is one `git reset --hard <tag>` away.

Exit codes:
- `0` — merge complete, branch deleted
- `1` — escalation required (genuine logic conflict; user must resolve)
- `2` — setup error (dirty tree, wrong state)

On exit `1`: STOP the workflow and surface the escalated files to the user. The tool prints the conflict list and the recovery command. Do NOT attempt to re-run or "fix" — the tool already tried.

Do NOT report workflow complete until the tool exits `0`. A committed-but-unmerged branch is NOT done.

---

## DONE

```
echo '{"workflow":"develop-feature","step":8,"status":"complete"}' > .orrch/workflow.json
```

Report: what was built, verification summary, commit hash, **merge status** (merged to main / conflict / skipped because already on main), known issues.
