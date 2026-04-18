---
description: Process raw user instructions through the intake pipeline — triage, optimize, review, route, and incorporate into project plans
argument-hint: "<raw instructions text or path to instructions file>"
allowed-tools: Bash, Read, Glob, Grep, Write, Edit, Agent
---

# /instruction-intake workflow

You are the Hypervisor — the orchestrator of the instruction intake pipeline. You spawn agents to triage, optimize, and route user instructions into the correct project inboxes. The user reviews and confirms before anything is routed.

## Workspace and source idea (CRITICAL)

The MCP tool injects two values into the prompt above this body:

- **WORKSPACE_DIR** — absolute path to a per-idea directory you must use for ALL state files. Look for the `## Workspace` section above for the exact path. If no path is given, fall back to `.orrch/` in the current working directory.
- **SOURCE_IDEA** — vault filename (e.g. `2026-04-21-00-14.md`) that originated this intake. Look for the `## Source Idea` section above. Embed this verbatim in every JSON file you write.

Throughout this skill, the literal token `{{WORKSPACE}}` refers to the workspace directory and `{{SOURCE_IDEA}}` refers to the source idea filename. The MCP server replaces these placeholders before handing the prompt to you. **Use the substituted values, never the literal placeholders.**

## Setup

1. Create the workspace directory if it does not exist:
   ```
   mkdir -p {{WORKSPACE}}
   ```
2. Write initial status to `{{WORKSPACE}}/workflow.json`:
   ```json
   {"workflow": "instruction-intake", "step": 0, "total_steps": 7, "status": "initializing", "agents": [], "source_idea": "{{SOURCE_IDEA}}"}
   ```
3. Locate the orrchestrator agents directory at `~/projects/orrchestrator/agents/`. You will read agent profiles from here before each spawn.

## Parse $ARGUMENTS

The instructions text was already loaded for you and is included in the `## Instructions to process` section above. Use that as `RAW_INPUT`. Do not search for files — the orchestrator already supplied the content.

---

## Step 1: Executive Assistant — triage

Update `{{WORKSPACE}}/workflow.json`:
```json
{"workflow": "instruction-intake", "step": 1, "total_steps": 7, "status": "running", "agents": [{"role": "Executive Assistant", "status": "running"}], "source_idea": "{{SOURCE_IDEA}}"}
```

Read the agent profile from `~/projects/orrchestrator/agents/executive_assistant.md`.

Spawn an Agent:

> **Role**: Executive Assistant
>
> {paste the full body of executive_assistant.md, starting from "# Executive Assistant Agent"}
>
> **Your task**: Triage the following user input. Classify each part as one of:
> 1. **Development instruction** — features to build, bugs to fix, code to write, architecture changes, technical work that is actionable now
> 2. **Status inquiry** — questions about progress or state
> 3. **General conversation** — non-technical requests, preferences, scheduling
> 4. **Deferred idea** — bold ideas, out-of-scope brainstorms, "someday" features, or anything the user explicitly frames as future/post-launch. These are NOT ready to execute but must NOT be dropped. They get parked in the target project's PLAN.md for later.
>
> **Signs something is a deferred idea** (not a dev instruction):
> - Scoped as "after we finish X" / "once Y is done" / "in the future"
> - A brainstorm list with no explicit requirement or acceptance criteria
> - Bold/ambitious ideas that clearly exceed the project's current maturity
> - Anything framed as an out-there idea, vision, or "what if"
>
> **User input**:
> {RAW_INPUT}
>
> **Output**: A structured triage result:
> ```
> ## Development Instructions
> <extracted dev instructions, preserving the user's exact words>
>
> ## Deferred Ideas
> <brainstorms, bold ideas, and out-of-scope items — with the target project each belongs to>
>
> ## Non-Development Items
> <list of non-dev items with your recommended immediate response for each>
>
> ## Ambiguous Items
> <anything you could not confidently classify — include the text and why it is ambiguous>
> ```
>
> Do NOT modify the development instructions or deferred ideas. Extract them verbatim. Do NOT interpret, summarize, or rewrite them — that is the COO's job.

Store the Agent's output as `TRIAGE_RESULT`.

**Handle non-dev items immediately**: If the triage identifies status inquiries or general conversation items, address them now by responding to the user directly. These do not enter the pipeline.

**Handle ambiguous items**: If any items are flagged as ambiguous, present them to the user and ask for classification before proceeding. Wait for the user's response.

Extract the development instructions portion and store as `DEV_INSTRUCTIONS`.
Extract the deferred ideas portion and store as `DEFERRED_IDEAS` (may be empty).

If there are no development instructions AND no deferred ideas, mark the workflow as `failed` in `{{WORKSPACE}}/workflow.json` and stop.

---

## Step 2: COO — optimize

Update `{{WORKSPACE}}/workflow.json` — step 2, Chief Operations Officer running, source_idea preserved.

Read the agent profile from `~/projects/orrchestrator/agents/chief_operations_officer.md`.

Spawn an Agent:

> **Role**: Chief Operations Officer
>
> {paste the full body of chief_operations_officer.md}
>
> **Your task**: Process these raw development instructions into token-efficient optimized versions. Also summarize any deferred ideas for PLAN.md storage.
>
> **Raw development instructions** (actionable):
> {DEV_INSTRUCTIONS}
>
> **Raw deferred ideas** (to be parked, not executed):
> {DEFERRED_IDEAS}
>
> **Instructions for actionable items**:
> 1. Parse — extract actionable items, requirements, constraints, and acceptance criteria. Strip conversational filler, repeated phrases, and ambiguity.
> 2. Deduplicate — if multiple instructions describe the same thing, merge them.
> 3. Optimize — rewrite each instruction as a clear, concise prompt. Use imperative voice. Include only information an executing agent needs. Target minimum viable token count without losing meaning.
> 4. Preserve the user's intent exactly. Optimization is about efficiency, not editorial judgment.
> 5. Number each optimized instruction (OPT-001, OPT-002, etc.).
>
> **Instructions for deferred ideas**:
> - Preserve the user's exact words. Do not optimize or rewrite.
> - Give each a short title (DEF-001, DEF-002, etc.).
> - These will be stored verbatim in PLAN.md — no compression needed.
>
> **Output**:
>
> For each actionable instruction:
> ```
> ### OPT-NNN: <title>
> <optimized instruction text>
>
> Source: <which part of the raw input this came from>
> ```
>
> For each deferred idea:
> ```
> ### DEF-NNN: <title>
> <user's original brainstorm text, preserved verbatim>
>
> Source: <which part of the raw input this came from>
> ```

Store the Agent's output as `OPTIMIZED_INSTRUCTIONS`.

---

## Step 3: Write review file

Update `{{WORKSPACE}}/workflow.json` — step 3, status `"pending_review"`, source_idea preserved.

Write `{{WORKSPACE}}/review.json` with the following content (note: status is `"pending"`, NOT `"pending_review"` — the TUI loader only matches `"pending"`):

```json
{
  "raw": "<DEV_INSTRUCTIONS as a string>",
  "optimized": "<OPTIMIZED_INSTRUCTIONS as a string>",
  "deferred": "<DEFERRED_IDEAS (DEF-NNN blocks) as a string, or empty string if none>",
  "status": "pending",
  "source_idea": "{{SOURCE_IDEA}}"
}
```

Be careful to JSON-escape newlines and quotes inside `raw`, `optimized`, and `deferred`.

---

## Step 4: User review (BLOCKING)

After writing `{{WORKSPACE}}/review.json` you MUST stop and end your turn. The user reviews and confirms via the orrchestrator TUI (`Design > Intentions` panel) — they do NOT type a confirmation into your chat. The TUI will write back to `{{WORKSPACE}}/review.json` with `"status": "confirmed"` or `"status": "rejected"`.

Print this message to the user before stopping:

```
Step 3 complete. Review written to {{WORKSPACE}}/review.json.

Open the orrchestrator TUI → Design → Intentions panel to review and confirm
the optimized instructions for source idea {{SOURCE_IDEA}}.

This session will exit. The TUI will spawn a fresh continuation session
once you confirm the review.
```

Then exit. Do not poll. Do not loop. Steps 5-7 will be executed by a separate, fresh session that the TUI launches when the user confirms.

---

## Steps 5-7: continuation (only run if invoked specifically for steps 5-7)

If you were invoked with the explicit instruction "continue intake from confirmed review at {{WORKSPACE}}/review.json", read that file, verify `"status": "confirmed"`, then run steps 5-7 below using the `optimized` field as the input. Otherwise these steps are not yours to run — they belong to the continuation session.

### Step 5: COO — route to projects

Update `{{WORKSPACE}}/workflow.json` — step 5, Chief Operations Officer running.

**BEFORE spawning the COO**: Read the source idea file at `~/projects/orrchestrator/plans/{{SOURCE_IDEA}}` and check its **first line** for an explicit project directive. If the first line names a project (e.g. `ORRAPUS - orragen notes`, `CONCORD: voice fixes`, `orrchestrator`), extract that as `PRIMARY_PROJECT` and pass it to the COO as the default routing target. Individual instructions may still be split to other projects if their content clearly demands it, but the COO must treat `PRIMARY_PROJECT` as the strong default and require a concrete reason to deviate.

Spawn an Agent with the COO profile:

> **Role**: Chief Operations Officer
>
> {paste the full body of chief_operations_officer.md}
>
> **Your task**: Determine which project each optimized instruction and deferred idea should be routed to.
>
> **Primary project** (from source idea file header): {PRIMARY_PROJECT if detected, otherwise "none — infer from content"}
>
> When a primary project is set, default all instructions to it. Only route an instruction elsewhere if its content explicitly and unambiguously concerns a different project — and state your concrete reason in the Reasoning column.
>
> **Optimized instructions**:
> {OPTIMIZED_INSTRUCTIONS from review.json}
>
> **Deferred ideas** (not actionable yet — route to project PLAN.md deferred sections):
> {DEFERRED_IDEAS from review.json}
>
> **Available projects**: List the directories in `~/projects/` and read each project's `CLAUDE.md` or `README.md` to understand what it does. Also check `.scope` files.
>
> **Output**: Two routing tables:
>
> **Actionable instructions** (go to instructions_inbox.md):
> ```
> | Instruction | Target Project | Reasoning |
> |-------------|---------------|-----------|
> | OPT-001     | orrchestrator | Primary project (default) |
> | OPT-002     | concord       | Explicitly about Matrix chat — unambiguously not orrchestrator |
> ```
>
> **Deferred ideas** (go to PLAN.md deferred section):
> ```
> | Idea | Target Project | Reasoning |
> |------|---------------|-----------|
> | Game Center brainstorm | concord | Chat platform gaming feature |
> ```
>
> If an instruction spans multiple projects, split it and route each part separately. Note the split.
> If an instruction does not clearly belong to any project, route it to `~/projects/scratchpad.md`.
> Deferred ideas with no clear project also go to `~/projects/scratchpad.md`.

Store the Agent's output as `ROUTING_TABLE`.

### Step 6: Append to instruction inboxes

Update `{{WORKSPACE}}/workflow.json` — step 6, status "routing".

For each project in the `ROUTING_TABLE`:

1. Determine the project's `instructions_inbox.md` path: `~/projects/<project>/instructions_inbox.md`
2. If the file does not exist, create it with a header:
   ```markdown
   # <Project Name> — Instruction Inbox
   ```
3. Read the existing inbox to find the highest instruction number (e.g., if INS-009 exists, next is INS-010).
4. Append each routed instruction in this format. Include the source idea so the vault sync can later detect implementation:
   ```markdown

   ### INS-NNN: <title> (source: plans/{{SOURCE_IDEA}})
   <optimized instruction text>
   ```
5. Keep instructions in the order they were numbered (OPT-001 before OPT-002, etc.).

Report to the user what was routed where:
```
Routed N instructions:
- <project>: INS-NNN (<title>), INS-NNN (<title>)
- <project>: INS-NNN (<title>)
```

### Step 7: PM incorporates into project plans

Update `{{WORKSPACE}}/workflow.json` — step 7, Project Manager running.

For each project that received new instructions, read the agent profile from `~/projects/orrchestrator/agents/project_manager.md` and spawn an Agent. If multiple projects are affected, spawn them **in parallel**.

> **Role**: Project Manager
>
> {paste the full body of project_manager.md}
>
> **Your task**: Incorporate newly routed instructions into the project's development plan.
>
> **Project**: {project name} at `~/projects/{project}/`
>
> **New instructions added to inbox**:
> {the instructions that were routed to this project}
>
> **Instructions**:
> 1. Read the project's `PLAN.md` (if it exists).
> 2. Read the project's `CLAUDE.md` for context on what the project does.
> 3. Determine if each new **actionable instruction**:
>    - **Extends** an existing planned feature — merge into that section
>    - **Modifies** a previous decision — update the relevant section, note what changed
>    - **Adds** something new — append as a new feature roadmap entry with status "planned"
>    - **Contradicts** existing plans — flag in an "Open Conflicts" section
> 4. For each **deferred idea** routed to this project: add it to a `### Deferred / Post-Primary-Goals` subsection inside the Feature Roadmap. Format:
>    ```markdown
>    ### Deferred / Post-Primary-Goals
>    _Ideas parked here are not actionable yet. Revisit when the primary roadmap is complete._
>
>    - **<idea title>** (captured <date>, source: <source_idea file>): <preserved brainstorm text>
>    ```
>    If the section already exists, append to it. Do not promote deferred ideas into the active roadmap — they stay deferred until the user explicitly acts on them.
> 5. Update `PLAN.md`. If `PLAN.md` does not exist, create it with sections: Open Conflicts, Architecture, Feature Roadmap (including a Deferred subsection), Recent Changes.
>
> **Output**: Summary of what was incorporated, what was deferred, and any conflicts detected.

---

## Completion

Update `{{WORKSPACE}}/workflow.json`:
```json
{"workflow": "instruction-intake", "step": 7, "total_steps": 7, "status": "complete", "agents": [], "source_idea": "{{SOURCE_IDEA}}"}
```

Report to the user:
1. How many instructions were processed
2. Routing summary (which projects received which instructions)
3. Any conflicts detected during plan incorporation
4. Any instructions routed to scratchpad (unattached ideas)
5. Suggest: "Run `/develop-feature` in a project directory to execute queued instructions."
# test
