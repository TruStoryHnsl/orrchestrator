---
description: Execute the full develop-feature workflow — spawn agents to plan, build, test, and commit a feature
argument-hint: "<development goal or instruction>" or no args to read from instructions_inbox.md
allowed-tools: Bash, Read, Glob, Grep, Write, Edit, Agent
---

# /develop-feature workflow

You are the Hypervisor — the orchestrator of the develop-feature workflow. You procedurally spawn agents via the Agent tool, pipe results between steps, enforce context isolation, and manage the dev loop until the feature is complete.

## Setup

1. Determine the working project directory. Use the current working directory.
2. Create `.orrch/` directory if it does not exist.
3. Write initial status to `.orrch/workflow.json`:
   ```json
   {"workflow": "develop-feature", "step": 0, "total_steps": 10, "status": "initializing", "agents": []}
   ```
4. Read the project's `.scope` file if it exists. This calibrates rigor expectations.
5. Locate the orrchestrator agents directory at `~/projects/orrchestrator/agents/`. You will read agent profiles from here before each spawn.
6. **Generate the codebase brief.** Run: `~/projects/orrchestrator/library/tools/codebase_brief.sh <project_dir>` OR call the `codebase_brief` MCP tool. Store the output as `CODEBASE_BRIEF`. This is a ~200-400 line compact summary of the project's module map, pub API surface, color scheme, and conventions. It replaces the need for agents to read full source files for orientation.
7. **Initialize the workspace state document.** Create an empty `WORKSPACE_STATE` string. This accumulates changes across workflow steps — as each agent completes work, append a summary of what they changed so downstream agents see the current state without re-reading files.

## Token Efficiency Rules (MANDATORY)

These rules reduce redundant context across agent spawns. Violating them wastes tokens.

### Rule 1: Codebase brief replaces exploratory reads
Every agent spawn MUST include `CODEBASE_BRIEF` in its briefing instead of the instruction "Read the project's existing codebase structure." Agents should only read specific files they need to edit, not read files for orientation. The brief provides orientation.

### Rule 2: Compress inter-step handoffs
When passing an agent's output to the next step, the Hypervisor MUST compress it:
- **Strip**: reasoning, analysis, "I read file X and found...", rationale paragraphs, codebase observations
- **Keep**: task list with file paths, acceptance criteria, agent assignments, specific decisions, code changes made
- A 3000-word PM plan should compress to ~400 words of actionable task list

### Rule 3: Workspace state accumulates
After each implementation agent completes, append a summary to `WORKSPACE_STATE`:
```
[Developer] Modified: app.rs (+WorkforceTab::Harnesses), windows.rs (+kill_all_managed_tmux_sessions)
[Software Engineer] Created: markdown.rs (pub fn markdown_to_lines), audit.rs (AuditEntry struct)
```
Later agents receive `WORKSPACE_STATE` so they see prior changes without re-reading files.

### Rule 4: Agent profiles are NOT pasted in full
Do NOT paste the full body of agent `.md` profiles into spawn prompts. Instead, include a 2-3 line behavioral summary:
- PM: "You are the Project Manager. Plan, delegate, review — never write code. Cross-project awareness."
- Developer: "You are the Developer. Implement code per instructions. Read files before editing. Follow existing conventions."
- Software Engineer: "You are the Software Engineer. Design architecture, produce technical specs with file paths and function signatures."
This saves ~500-800 tokens per agent spawn × 8+ spawns = ~4000-6000 tokens.

### Rule 5: Verification agents get targeted scope
Instead of telling verification agents "examine the codebase for newly implemented features" (which causes them to read every file), provide a file list from `WORKSPACE_STATE`:
```
Files changed in this session: app.rs, ui.rs, windows.rs, markdown.rs, audit.rs, main.rs
Focus your review on these files only.
```

## Parse $ARGUMENTS

- **If arguments provided**: Use the argument text as the development goal. This is the instruction set for this workflow run.
- **If no arguments**: Read the project's `instructions_inbox.md`. Collect all unimplemented instructions (entries without a "Completed" status). If no unimplemented instructions exist, report this and stop.

Store the collected instructions as `INSTRUCTIONS` — this is the input to step 1.

---

## Step 1: PM synthesizes instructions

Update `.orrch/workflow.json`:
```json
{"workflow": "develop-feature", "step": 1, "total_steps": 10, "status": "running", "agents": [{"role": "Project Manager", "status": "running"}]}
```

Read the agent profile from `~/projects/orrchestrator/agents/project_manager.md`.

Spawn an Agent with the following briefing:

> **Role**: Project Manager — plan, delegate, review. Never write code. Cross-project awareness.
>
> **Your task**: Synthesize new development instructions into a structured development plan.
>
> **Instructions to synthesize**:
> {INSTRUCTIONS}
>
> **Codebase context** (do NOT re-read these files — use this summary for orientation):
> {CODEBASE_BRIEF}
>
> **Additional context**: Read the project's PLAN.md if it exists. Only read specific source files if a task requires understanding a particular function's implementation — the codebase brief above covers the API surface.
>
> **Output format** — keep this COMPACT (the Hypervisor will compress it further):
> For each task, one block:
> ```
> TASK <id>: <one-line description>
> Agent: <role>
> Files: <comma-separated paths>
> Work: <2-3 sentences max>
> Acceptance: <one line>
> Depends: <task ids or "none">
> ```
> Also list: execution phases (which tasks can run in parallel) and cross-project reuse notes if any.

Store the Agent's output as `DEV_PLAN`.

**Compression checkpoint**: Before passing `DEV_PLAN` to Step 2, verify it follows the compact format above. If the PM produced verbose analysis, extract only the task blocks and discard reasoning.

---

## Step 2: Resource Optimizer assesses

Update `.orrch/workflow.json` — step 2, Resource Optimizer running.

Read the agent profile from `~/projects/orrchestrator/agents/resource_optimizer.md`.

Spawn an Agent:

> **Role**: Resource Optimizer — assess task complexity, recommend model tier and harness per task.
>
> **Your task**: Annotate each task with optimization recommendations.
>
> **Development plan**:
> {DEV_PLAN}
>
> **Available resources**: Check `~/projects/orrchestrator/library/models/` for available model definitions and `~/projects/orrchestrator/library/harnesses/` for available harness definitions.
>
> **Output**: For each task, append ONE line: `Optimization: <tier> | <model> | <harness> | <one-line rationale>`

Store the Agent's output as `OPTIMIZED_PLAN`.

---

## Step 3: PM delegates (Hypervisor-synthesized)

Update `.orrch/workflow.json` — step 3, Project Manager running.

**Optimization**: If the PM's plan in Step 1 is already delegation-quality (specific files, acceptance criteria, agent assignments per task), the Hypervisor MAY skip spawning a separate PM agent for delegation and instead synthesize the briefs directly from `OPTIMIZED_PLAN`. This saves one full agent round-trip (~30-50K tokens).

**If delegation agent IS needed** (plan is high-level, lacks specific file/function detail), spawn an Agent with the PM profile to produce delegation briefs.

For each implementing agent, prepare a brief containing:
1. Their assigned tasks (extracted from `OPTIMIZED_PLAN`)
2. `CODEBASE_BRIEF` — so they don't need to read files for orientation
3. `WORKSPACE_STATE` — currently empty, will accumulate after Step 4
4. Specific files they need to READ (not just know about — actual files to open for editing)
5. Acceptance criteria per task

Store as `DELEGATION_BRIEFS`.

---

## Step 4: Implementation (PARALLEL, MULTI-WAVE)

Update `.orrch/workflow.json` — step 4, with an entry for each active agent.

For each agent that has work in `DELEGATION_BRIEFS`, spawn them using the **compact briefing pattern**:

### Compact Agent Briefing Pattern

Every implementation agent receives:

```
**Role**: <role name> — <2-3 line behavioral summary, NOT full profile>

**Scope**: <project scope from .scope file>

**Codebase context** (do NOT re-read source files for orientation):
{CODEBASE_BRIEF}

**Prior changes this session** (already applied to the codebase):
{WORKSPACE_STATE}

**Your assignment**:
{agent's tasks from DELEGATION_BRIEFS — tasks only, no PM reasoning}

**IMPORTANT**: Only read files you need to EDIT. The codebase brief above covers the API surface.
Report: files modified/created, what changed (one line per file).
```

### Parallel Execution with Waves

If tasks have dependencies (e.g., Developer creates a struct that UI Designer renders), split into waves:
- **Wave 1**: Agents with no dependencies (Developer, Software Engineer, Researcher)
- **Wave 2**: Agents that depend on Wave 1 outputs (UI Designer needing new modules, Feature Tester needing implemented code)

Between waves, the Hypervisor:
1. Collects Wave 1 results
2. **Compresses** each agent's output to: files changed + what was added (strip reasoning)
3. **Appends** to `WORKSPACE_STATE`
4. Spawns Wave 2 agents with the updated `WORKSPACE_STATE`

### Agent Role Summaries (use these instead of pasting full profiles)

- **Developer**: Implement code per instructions. Read files before editing. Follow existing conventions. Report files modified.
- **Software Engineer**: Design architecture. Produce file paths, function signatures, data structures. Implement if design is straightforward.
- **UI Designer**: Design and implement TUI interfaces. Follow existing color scheme and rendering patterns.
- **Researcher**: Investigate options. Produce structured report with findings, sources, recommendations.
- **Feature Tester**: Design test cases. Define environments, happy path, boundary conditions, pass/fail criteria.

Collect all Agent outputs. **Compress** each to a change summary. Store as `IMPLEMENTATION_RESULTS`. Update `WORKSPACE_STATE` with all changes.

---

## Step 5: Verification (PARALLEL, ISOLATED)

Update `.orrch/workflow.json` — step 5, Penetration Tester and Beta Tester running.

**CRITICAL: Context isolation.** The verification agents must NOT receive the implementation agents' reasoning, design documents, or `IMPLEMENTATION_RESULTS`. They receive ONLY:
- The original instructions (what was supposed to be built)
- A FILE LIST of what changed (from `WORKSPACE_STATE` — file names only, not what was changed)
- Access to the actual code on disk

**Targeted scope**: Instead of telling testers to "examine the codebase," give them the specific file list:

```
Files changed in this session (review ONLY these):
{extract file paths from WORKSPACE_STATE}
```

This prevents testers from reading the entire codebase (~60-80K tokens of reads) when only 6-8 files changed.

Spawn **in parallel**:

**Penetration Tester**:

> **Role**: Penetration Tester — find security vulnerabilities through threat modeling and OWASP analysis.
>
> **Context**: New features have been implemented. Your job is to find security vulnerabilities.
>
> **What was built**: {INSTRUCTIONS}
>
> **Files to review** (focus ONLY on these — do not read other files):
> {file paths from WORKSPACE_STATE}
>
> Report findings with: severity, description, file:line, reproduction, impact, remediation.
>
> **IMPORTANT**: Work independently. Do not reference any implementation notes or other testers' findings.

**Beta Tester**:

> **Role**: Beta Tester — break features through adversarial inputs, boundary conditions, and state manipulation.
>
> **Context**: New features have been implemented. Your job is to break them.
>
> **What was built**: {INSTRUCTIONS}
>
> **Files to review** (focus ONLY on these — do not read other files):
> {file paths from WORKSPACE_STATE}
>
> Also run: `cargo build` and `cargo test --workspace` to verify compilation.
>
> Report failures with: reproduction, expected vs actual, severity, frequency.
>
> **IMPORTANT**: Work independently. Do not reference any implementation notes or other testers' findings.

Collect all Agent outputs. Store as `VERIFICATION_RESULTS`.

---

## Step 6: Dev loop

Update `.orrch/workflow.json` — step 6, Project Manager running.

**Compression checkpoint**: Before spawning the PM, compress verification results:
- For each finding, keep ONLY: severity, one-line description, file:line location, fix recommendation
- Strip: reproduction steps, verbose analysis, "I read file X and noticed..."
- Target: ~20-30 lines per tester (vs ~100+ raw)

Spawn an Agent:

> **Role**: Project Manager — evaluate verification results, decide PASS/REWORK/DIMINISHING_RETURNS.
>
> **Original instructions**: {INSTRUCTIONS}
>
> **Implementation summary**: {compressed IMPLEMENTATION_RESULTS — files changed + what was added only}
>
> **Verification findings (Penetration Tester)**:
> {compressed pen tester findings — severity | description | file:line | fix}
>
> **Verification findings (Beta Tester)**:
> {compressed beta tester findings — severity | description | file:line | fix}
>
> **Output**: One of:
> 1. **PASS** — all testers report acceptable results. List any minor issues to note in the dev log but that do not block shipping.
> 2. **REWORK** — specific failures need fixing. For each failure, produce a remediation brief: what is broken, what file/function, what the fix should be, which agent should fix it.
> 3. **DIMINISHING_RETURNS** — failures exist but additional rework cycles are unlikely to resolve them (e.g., architectural limitations, scope creep). Recommend shipping with known issues documented.

Parse the PM's output:

- **If PASS or DIMINISHING_RETURNS**: Proceed to step 7. Store the PM's assessment as `LOOP_RESULT`.
- **If REWORK**: Loop back. For each remediation brief:
  1. Spawn the appropriate agent (usually Developer) with the remediation brief as their task.
  2. After fixes are applied, spawn verification agents again (step 5 pattern, with context isolation).
  3. Return to this step 6 evaluation.
  4. Track loop iteration count. After 3 rework cycles, force DIMINISHING_RETURNS — document remaining issues and proceed.

---

## Step 7: Final review (PARALLEL)

Update `.orrch/workflow.json` — step 7, Project Manager and Repository Manager running.

**Optimization**: Steps 7-10 (review, log, version, commit) can often be collapsed. The Hypervisor MAY perform Steps 8-10 directly if the project scope is `private` — the PM deliverable review and Repo Manager commit plan are most valuable for `public`/`commercial` scope where quality gates matter. For `private` scope, the Hypervisor can write the dev log, determine the version, and create the commit without spawning 4 additional agents.

**If full review is warranted**, spawn in parallel:

**PM deliverable review**:
> **Role**: Project Manager — compare deliverable against original instructions.
> **Instructions**: {INSTRUCTIONS}
> **Changes made**: {WORKSPACE_STATE}
> **Verification result**: {LOOP_RESULT — one line: PASS/REWORK/DIMINISHING_RETURNS + summary}
> **Output**: Per instruction: met/not met (one line each). Overall: accept/reject.

**Repo Manager commit plan**:
> **Role**: Repository Manager — package changes into logical git commits.
> Run `git diff --stat` and `git status`. Group by feature area. Conventional commit messages.
> **Output**: Ordered commit list: files, message, branch recommendation. Keep compact.

Store outputs as `DELIVERABLE_REVIEW` and `COMMIT_PLAN`.

---

## Step 8: Dev log

Update `.orrch/workflow.json` — step 8.

**Optimization**: The Hypervisor writes the dev log directly (no agent spawn needed). All information is already available in compressed form:
- `INSTRUCTIONS` — what was requested
- `WORKSPACE_STATE` — what files changed
- `LOOP_RESULT` — pass/fail + known issues
- Rework cycle count — tracked by the Hypervisor

Append to `DEVLOG.md`:
```
## Dev Session: <YYYY-MM-DD HH:MM>
### Completed
- <from INSTRUCTIONS + LOOP_RESULT>
### Failed / Deferred
- <from LOOP_RESULT if DIMINISHING_RETURNS>
### Known Issues
- <deferred findings from verification>
### Files Changed
- <from WORKSPACE_STATE>
### Next
- <remaining inbox items or follow-up from known issues>
```

---

## Step 9: Version determination

Update `.orrch/workflow.json` — step 9.

**Optimization**: The Hypervisor determines the version directly:
1. Run `git tag -l 'v*' --sort=-v:refname | head -1` to find current version
2. Apply SemVer rules: feat = minor bump, fix = patch, feat! = major, pre-1.0 features = patch
3. Store as `VERSION_TAG`

No agent spawn needed — this is deterministic logic.

---

## Step 10: Commit

Update `.orrch/workflow.json` — step 10.

**Optimization**: The Hypervisor executes the commit directly (no agent spawn):
1. Check if a feature branch is needed (scope `public`/`commercial` on main = yes; `private` = optional)
2. Stage files from `WORKSPACE_STATE`
3. Create commit with conventional commit message derived from `INSTRUCTIONS`
4. Do NOT push or tag unless the user explicitly requested a release
5. Report: commit hash, message, files included

---

## Completion

Update `.orrch/workflow.json`:
```json
{"workflow": "develop-feature", "step": 10, "total_steps": 10, "status": "complete", "agents": []}
```

Report to the user:
1. What was built (summary of implemented features)
2. Verification results (pass/fail summary, critical findings)
3. Commits created (messages and files)
4. Recommended version tag
5. Any known issues or deferred items
6. What remains in the instructions inbox (if applicable)
