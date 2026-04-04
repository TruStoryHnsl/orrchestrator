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

> **Role**: Project Manager
>
> {paste the full body of project_manager.md here, starting from "# Project Manager Agent"}
>
> **Your task**: You have received new development instructions. Synthesize them into a structured development plan.
>
> **Instructions to synthesize**:
> {INSTRUCTIONS}
>
> **Context**: Read the project's existing codebase structure to understand conventions, architecture, and current state. Check if a PLAN.md exists and incorporate existing plans. Check other projects in ~/projects/ for reuse opportunities.
>
> **Output**: A structured development plan with:
> 1. Each instruction broken into discrete, delegatable tasks
> 2. For each task: which agent should execute it, what tools/skills to use, acceptance criteria, execution order
> 3. Dependencies between tasks identified
> 4. Cross-project reuse opportunities noted

Store the Agent's output as `DEV_PLAN`.

---

## Step 2: Resource Optimizer assesses

Update `.orrch/workflow.json` — step 2, Resource Optimizer running.

Read the agent profile from `~/projects/orrchestrator/agents/resource_optimizer.md`.

Spawn an Agent:

> **Role**: Resource Optimizer
>
> {paste the full body of resource_optimizer.md}
>
> **Your task**: Read the development plan below and annotate each task with optimization recommendations.
>
> **Development plan**:
> {DEV_PLAN}
>
> **Available resources**: Check `~/projects/orrchestrator/library/models/` for available model definitions and `~/projects/orrchestrator/library/harnesses/` for available harness definitions. Read these files to understand capabilities and costs.
>
> **Output**: The same plan with optimization annotations appended to each task (model tier, suggested model, harness, rationale, estimated savings).

Store the Agent's output as `OPTIMIZED_PLAN`.

---

## Step 3: PM delegates

Update `.orrch/workflow.json` — step 3, Project Manager running.

Spawn an Agent with the PM profile:

> **Role**: Project Manager
>
> {paste the full body of project_manager.md}
>
> **Your task**: Read the optimized plan and produce explicit delegation briefs for each agent that needs to execute work in step 4.
>
> **Optimized plan**:
> {OPTIMIZED_PLAN}
>
> **Output**: For each agent that will execute in step 4 (Developer, Researcher, Software Engineer, UI Designer, Feature Tester), produce a delegation brief containing:
> 1. Exactly what they must do (specific files, patterns, modules)
> 2. Acceptance criteria
> 3. Relevant context (related files, previous decisions, constraints)
> 4. Tool/skill recommendations from the optimization annotations
>
> If a particular agent has no work for this feature, say so explicitly. Not every agent is needed for every feature.

Store the Agent's output as `DELEGATION_BRIEFS`. Parse it to identify which agents have work assigned.

---

## Step 4: Implementation (PARALLEL)

Update `.orrch/workflow.json` — step 4, with an entry for each active agent.

For each agent that has work in `DELEGATION_BRIEFS`, read their profile from `~/projects/orrchestrator/agents/` and spawn them **in parallel** using simultaneous Agent tool calls.

**Developer** (if assigned work):

> **Role**: Developer
>
> {paste the full body of developer.md}
>
> **Your assignment from the Project Manager**:
> {Developer's delegation brief from DELEGATION_BRIEFS}
>
> **Instructions**: Implement the assigned tasks. Read existing code first to understand conventions. Write tests for your implementation. Report what files you created/modified, tests written, and any concerns.

**Researcher** (if assigned work):

> **Role**: Researcher
>
> {paste the full body of researcher.md}
>
> **Your assignment from the Project Manager**:
> {Researcher's delegation brief from DELEGATION_BRIEFS}
>
> **Instructions**: Conduct the research described. Use web search and documentation as needed. Produce a structured report with summary, findings, sources, recommendations, and caveats.

**Software Engineer** (if assigned work):

> **Role**: Software Engineer
>
> {paste the full body of software_engineer.md}
>
> **Your assignment from the Project Manager**:
> {Engineer's delegation brief from DELEGATION_BRIEFS}
>
> **Instructions**: Design the architecture as specified. Produce a technical specification with file paths, function signatures, data structures, and integration points that the Developer can follow.

**UI Designer** (if assigned work):

> **Role**: UI Designer
>
> {paste the full body of ui_designer.md}
>
> **Your assignment from the Project Manager**:
> {UI Designer's delegation brief from DELEGATION_BRIEFS}
>
> **Instructions**: Design the interface as specified. Review existing UI patterns in the project first. Produce a design specification with component hierarchy, layout, visual states, interaction behavior, and accessibility requirements.

**Feature Tester** (if assigned work):

> **Role**: Feature Tester
>
> {paste the full body of feature_tester.md}
>
> **Your assignment from the Project Manager**:
> {Feature Tester's delegation brief from DELEGATION_BRIEFS}
>
> **Instructions**: Design test cases for the features being implemented. Define the test environment setup, test cases (happy path, boundary conditions, error states, integration points), and pass/fail criteria. These test designs will be executed after implementation.

Collect all Agent outputs. Store as `IMPLEMENTATION_RESULTS` — a combined record of all agent outputs keyed by role.

---

## Step 5: Verification (PARALLEL, ISOLATED)

Update `.orrch/workflow.json` — step 5, Penetration Tester and Beta Tester running.

**CRITICAL: Context isolation.** The verification agents must NOT receive the implementation agents' reasoning, design documents, or internal discussion. They receive ONLY:
- The original instructions (what was supposed to be built)
- Access to the actual code/deliverables on disk (they can read files)

Read the agent profiles and spawn **in parallel**:

**Penetration Tester**:

> **Role**: Penetration Tester
>
> {paste the full body of penetration_tester.md}
>
> **Context**: New features have been implemented in this project. Your job is to find security vulnerabilities.
>
> **What was built** (from original instructions only):
> {INSTRUCTIONS}
>
> **Instructions**: Examine the codebase for the newly implemented features. Perform a threat model, OWASP Top 10 sweep, and contextual security testing. Report all findings with severity, description, location, reproduction steps, impact, and remediation recommendations.
>
> **IMPORTANT**: You are working independently. Do not ask for or reference any implementation notes, design documents, or other testers' findings. Read the code directly.

**Beta Tester**:

> **Role**: Beta Tester
>
> {paste the full body of beta_tester.md}
>
> **Context**: New features have been implemented in this project. Your job is to break them.
>
> **What was built** (from original instructions only):
> {INSTRUCTIONS}
>
> **Instructions**: Examine the codebase for the newly implemented features. Try to break them through invalid inputs, boundary conditions, rapid operations, state manipulation, resource exhaustion, and environment variation. Report every failure with reproduction steps, expected vs actual behavior, severity, and frequency.
>
> **IMPORTANT**: You are working independently. Do not ask for or reference any implementation notes, design documents, or other testers' findings. Read the code directly.

Collect all Agent outputs. Store as `VERIFICATION_RESULTS`.

---

## Step 6: Dev loop

Update `.orrch/workflow.json` — step 6, Project Manager running.

Spawn an Agent with the PM profile:

> **Role**: Project Manager
>
> {paste the full body of project_manager.md}
>
> **Your task**: Evaluate verification results and determine if the implementation passes or needs rework.
>
> **Original instructions**:
> {INSTRUCTIONS}
>
> **Implementation results**:
> {IMPLEMENTATION_RESULTS}
>
> **Verification results (Penetration Tester)**:
> {Pen Tester's output from VERIFICATION_RESULTS}
>
> **Verification results (Beta Tester)**:
> {Beta Tester's output from VERIFICATION_RESULTS}
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

Spawn **in parallel**:

**Project Manager — deliverable review**:

> **Role**: Project Manager
>
> {paste the full body of project_manager.md}
>
> **Your task**: Compare the completed deliverable against the original instructions. Verify that all requirements are met, acceptance criteria satisfied, and the feature is complete.
>
> **Original instructions**:
> {INSTRUCTIONS}
>
> **Implementation results**:
> {IMPLEMENTATION_RESULTS}
>
> **Loop result**:
> {LOOP_RESULT}
>
> **Output**: A deliverable assessment — requirements met (yes/no per requirement), quality assessment, and recommendation (accept/reject with reasoning).

**Repository Manager — commit review**:

> **Role**: Repository Manager
>
> {paste the full body of repository_manager.md}
>
> **Your task**: Review the changes made during this development session. Recommend how to package them into git commits.
>
> **Instructions**: Run `git diff` and `git status` to see all changes. Group related changes into logical commits. For each proposed commit, specify: files to include, conventional commit message, and reasoning for the grouping. Also recommend branch strategy (feature branch vs direct to main).
>
> **Output**: A commit plan — ordered list of commits with files, messages, and branch recommendation.

Store outputs as `DELIVERABLE_REVIEW` and `COMMIT_PLAN`.

---

## Step 8: Dev log

Update `.orrch/workflow.json` — step 8, Project Manager running.

Spawn an Agent with the PM profile:

> **Role**: Project Manager
>
> {paste the full body of project_manager.md}
>
> **Your task**: Write a development session log entry.
>
> **Session summary**:
> - Instructions: {INSTRUCTIONS}
> - Implementation results: {IMPLEMENTATION_RESULTS (summary only — file changes, test results)}
> - Verification results: {VERIFICATION_RESULTS (summary only — pass/fail counts, critical findings)}
> - Loop iterations: {number of rework cycles}
> - Deliverable review: {DELIVERABLE_REVIEW}
>
> **Output**: A log entry in this format:
> ```
> ## Dev Session: <YYYY-MM-DD HH:MM>
> ### Completed
> - <list of completed features/fixes>
> ### Failed / Deferred
> - <list of items that did not pass, if any>
> ### Known Issues
> - <issues documented during DIMINISHING_RETURNS, if any>
> ### Files Changed
> - <list of files created/modified/deleted>
> ### Next
> - <what should happen next based on remaining instructions>
> ```
>
> Append this entry to the project's `DEVLOG.md` (create if it does not exist).

---

## Step 9: Version determination

Update `.orrch/workflow.json` — step 9, Repository Manager running.

Spawn an Agent with the Repository Manager profile:

> **Role**: Repository Manager
>
> {paste the full body of repository_manager.md}
>
> **Your task**: Determine the appropriate semantic version tag for this set of changes.
>
> **Commit plan**:
> {COMMIT_PLAN}
>
> **Current version**: Run `git tag -l 'v*' --sort=-v:refname | head -1` to find the latest version tag. If no tags exist, this is v0.1.0.
>
> **Rules**:
> - Breaking changes (feat! or BREAKING CHANGE) = major bump
> - New features (feat) = minor bump
> - Fixes only (fix) = patch bump
> - If pre-1.0.0, treat feature additions as patch bumps unless explicitly breaking
>
> **Output**: The recommended version tag (e.g., v0.4.0) with reasoning.

Store the output as `VERSION_TAG`.

---

## Step 10: Commit

Update `.orrch/workflow.json` — step 10, Repository Manager running.

Spawn an Agent with the Repository Manager profile:

> **Role**: Repository Manager
>
> {paste the full body of repository_manager.md}
>
> **Your task**: Execute the commit plan.
>
> **Commit plan**:
> {COMMIT_PLAN}
>
> **Version tag**:
> {VERSION_TAG}
>
> **Instructions**:
> 1. Check if a feature branch is needed (based on commit plan's branch recommendation). If so, create it.
> 2. Stage and commit each logical change group per the commit plan, using conventional commit messages.
> 3. Do NOT force-push, commit secrets, or skip conventional commit format.
> 4. Do NOT push to remote or create tags unless the user explicitly requested a release. Just commit locally.
> 5. Report what was committed: commit hashes, messages, files included.

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
