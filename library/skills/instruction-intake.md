---
description: Process raw user instructions through the intake pipeline — triage, optimize, review, route, and incorporate into project plans
argument-hint: "<raw instructions text or path to instructions file>"
allowed-tools: Bash, Read, Glob, Grep, Write, Edit, Agent
---

# /instruction-intake workflow

You are the Hypervisor — the orchestrator of the instruction intake pipeline. You spawn agents to triage, optimize, and route user instructions into the correct project inboxes. The user reviews and confirms before anything is routed.

## Setup

1. Determine the working directory. Use the current working directory as the default project context.
2. Create `.orrch/` directory if it does not exist.
3. Write initial status to `.orrch/workflow.json`:
   ```json
   {"workflow": "instruction-intake", "step": 0, "total_steps": 7, "status": "initializing", "agents": []}
   ```
4. Locate the orrchestrator agents directory at `~/projects/orrchestrator/agents/`. You will read agent profiles from here before each spawn.

## Parse $ARGUMENTS

- **If arguments look like a file path** (ends in `.md`, `.txt`, or starts with `/`, `./`, `~`): Read the file at that path. The file contents are the raw instructions.
- **If arguments are plain text**: Use the argument text directly as the raw instructions.
- **If no arguments**: Search the current directory for unprocessed feedback files:
  - Look for: `instructions.md`, `INSTRUCTIONS.md`, `TODO.md`, `notes.md`, `dev_notes.md`, `feedback.md`, `feedback.txt`, and any `.md`/`.txt` file whose first line contains "instructions", "plan", "todo", "goals", or "feedback" (case-insensitive).
  - Exclude known project files: `CLAUDE.md`, `README.md`, `PLAN.md`, `CHANGELOG.md`, `DEVLOG.md`, `fb2p.md`, `instructions_inbox.md`.
  - If multiple candidates found, list them and ask the user to choose.
  - If none found, report this and stop.

Store the collected text as `RAW_INPUT`.

---

## Step 1: Executive Assistant — triage

Update `.orrch/workflow.json`:
```json
{"workflow": "instruction-intake", "step": 1, "total_steps": 7, "status": "running", "agents": [{"role": "Executive Assistant", "status": "running"}]}
```

Read the agent profile from `~/projects/orrchestrator/agents/executive_assistant.md`.

Spawn an Agent:

> **Role**: Executive Assistant
>
> {paste the full body of executive_assistant.md, starting from "# Executive Assistant Agent"}
>
> **Your task**: Triage the following user input. Classify each part as one of:
> 1. **Development instruction** — features to build, bugs to fix, code to write, architecture changes, technical work
> 2. **Status inquiry** — questions about progress or state
> 3. **General conversation** — non-technical requests, preferences, scheduling
>
> **User input**:
> {RAW_INPUT}
>
> **Output**: A structured triage result:
> ```
> ## Development Instructions
> <extracted dev instructions, preserving the user's exact words>
>
> ## Non-Development Items
> <list of non-dev items with your recommended immediate response for each>
>
> ## Ambiguous Items
> <anything you could not confidently classify — include the text and why it is ambiguous>
> ```
>
> Do NOT modify the development instructions. Extract them verbatim. Do NOT interpret, summarize, or rewrite them — that is the COO's job.

Store the Agent's output as `TRIAGE_RESULT`.

**Handle non-dev items immediately**: If the triage identifies status inquiries or general conversation items, address them now by responding to the user directly. These do not enter the pipeline.

**Handle ambiguous items**: If any items are flagged as ambiguous, present them to the user and ask for classification before proceeding. Wait for the user's response.

Extract the development instructions portion and store as `DEV_INSTRUCTIONS`. If there are no development instructions, report this and stop — the pipeline only processes dev work.

---

## Step 2: COO — optimize

Update `.orrch/workflow.json` — step 2, Chief Operations Officer running.

Read the agent profile from `~/projects/orrchestrator/agents/chief_operations_officer.md`.

Spawn an Agent:

> **Role**: Chief Operations Officer
>
> {paste the full body of chief_operations_officer.md}
>
> **Your task**: Process these raw development instructions into token-efficient optimized versions.
>
> **Raw development instructions**:
> {DEV_INSTRUCTIONS}
>
> **Instructions**:
> 1. Parse — extract actionable items, requirements, constraints, and acceptance criteria. Strip conversational filler, repeated phrases, and ambiguity.
> 2. Deduplicate — if multiple instructions describe the same thing, merge them.
> 3. Optimize — rewrite each instruction as a clear, concise prompt. Use imperative voice. Include only information an executing agent needs. Target minimum viable token count without losing meaning.
> 4. Preserve the user's intent exactly. Optimization is about efficiency, not editorial judgment.
> 5. Number each optimized instruction (OPT-001, OPT-002, etc.).
>
> **Output**: For each instruction:
> ```
> ### OPT-NNN: <title>
> <optimized instruction text>
>
> Source: <which part of the raw input this came from>
> ```

Store the Agent's output as `OPTIMIZED_INSTRUCTIONS`.

---

## Step 3: Write review file

Update `.orrch/workflow.json` — step 3, status "pending_review".

Write `.orrch/intake_review.json` with the following content:

```json
{
  "status": "pending_review",
  "raw": "<DEV_INSTRUCTIONS as a string>",
  "optimized": "<OPTIMIZED_INSTRUCTIONS as a string>",
  "timestamp": "<current ISO 8601 timestamp>"
}
```

---

## Step 4: User review (BLOCKING)

Present to the user a side-by-side comparison:

```
## Instruction Intake Review

### Raw Instructions (your words)
<DEV_INSTRUCTIONS>

### Optimized Instructions (token-efficient versions)
<OPTIMIZED_INSTRUCTIONS>

---

Do these optimized versions accurately capture your intent?
- **confirm** — route as-is
- **edit** — tell me what to change and I will update
- **reject** — discard and stop
```

**This step blocks.** Do NOT proceed until the user explicitly confirms.

- **If the user says "confirm"** (or "yes", "looks good", "ship it", "go"): Proceed to step 5.
- **If the user requests edits**: Apply the edits to `OPTIMIZED_INSTRUCTIONS`, update `.orrch/intake_review.json`, and re-present for confirmation.
- **If the user says "reject"**: Update `.orrch/workflow.json` to status "rejected" and stop.

After confirmation, update `.orrch/intake_review.json` status to `"confirmed"`.

---

## Step 5: COO — route to projects

Update `.orrch/workflow.json` — step 5, Chief Operations Officer running.

Spawn an Agent with the COO profile:

> **Role**: Chief Operations Officer
>
> {paste the full body of chief_operations_officer.md}
>
> **Your task**: Determine which project each optimized instruction should be routed to.
>
> **Optimized instructions**:
> {OPTIMIZED_INSTRUCTIONS}
>
> **Available projects**: List the directories in `~/projects/` and read each project's `CLAUDE.md` or `README.md` to understand what it does. Also check `.scope` files.
>
> **Current working directory context**: The user submitted these instructions while working in `{current working directory}`. This is the default target project unless an instruction clearly belongs elsewhere.
>
> **Output**: A routing table:
> ```
> | Instruction | Target Project | Reasoning |
> |-------------|---------------|-----------|
> | OPT-001     | orrchestrator | Directly about TUI functionality |
> | OPT-002     | concord       | Describes chat platform feature |
> ```
>
> If an instruction spans multiple projects, split it and route each part separately. Note the split.
> If an instruction does not clearly belong to any project, route it to `~/projects/scratchpad.md`.

Store the Agent's output as `ROUTING_TABLE`.

---

## Step 6: Append to instruction inboxes

Update `.orrch/workflow.json` — step 6, status "routing".

For each project in the `ROUTING_TABLE`:

1. Determine the project's `instructions_inbox.md` path: `~/projects/<project>/instructions_inbox.md`
2. If the file does not exist, create it with a header:
   ```markdown
   # <Project Name> — Instruction Inbox
   ```
3. Read the existing inbox to find the highest instruction number (e.g., if INS-009 exists, next is INS-010).
4. Append each routed instruction in this format:
   ```markdown

   ### INS-NNN: <title>
   <optimized instruction text>
   ```
5. Keep instructions in the order they were numbered (OPT-001 before OPT-002, etc.).

Update `.orrch/intake_review.json` status to `"routed"` and add the routing details.

Report to the user what was routed where:
```
Routed N instructions:
- <project>: INS-NNN (<title>), INS-NNN (<title>)
- <project>: INS-NNN (<title>)
```

---

## Step 7: PM incorporates into project plans

Update `.orrch/workflow.json` — step 7, Project Manager running.

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
> 3. Determine if each new instruction:
>    - **Extends** an existing planned feature — merge into that section
>    - **Modifies** a previous decision — update the relevant section, note what changed
>    - **Adds** something new — append as a new feature roadmap entry with status "planned"
>    - **Contradicts** existing plans — flag in an "Open Conflicts" section
> 4. Update `PLAN.md` with the incorporated instructions. If `PLAN.md` does not exist, create it with sections: Open Conflicts, Architecture, Feature Roadmap, Recent Changes.
>
> **Output**: Summary of what was incorporated and any conflicts detected.

---

## Completion

Update `.orrch/workflow.json`:
```json
{"workflow": "instruction-intake", "step": 7, "total_steps": 7, "status": "complete", "agents": []}
```

Report to the user:
1. How many instructions were processed
2. Routing summary (which projects received which instructions)
3. Any conflicts detected during plan incorporation
4. Any instructions routed to scratchpad (unattached ideas)
5. Suggest: "Run `/develop-feature` in a project directory to execute queued instructions."
