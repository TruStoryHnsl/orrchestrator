---
description: Read raw user instructions, generate optimized LLM prompts, route to correct projects, maintain feedback records, and optionally execute
argument-hint: "[path-to-instructions-file]"
allowed-tools: Bash, Read, Glob, Grep, Write, Edit, Agent
---

# /interpret-user-instructions command

Transforms raw, conversational user instructions into structured LLM-optimized development prompts. Routes feedback to the correct project(s) and maintains a master development plan per project.

## Parse $ARGUMENTS:

- If a file path is provided, read that file.
- If no arguments, search the current directory for feedback files:
  - Look for: `instructions.md`, `INSTRUCTIONS.md`, `TODO.md`, `plan.md`, `PLAN.md`, `*.instructions.md`, `dev_notes.md`, `notes.md`, `feedback.md`, `feedback.txt`, `*.feedback.md`, `*.feedback.txt`
  - Also check any `.md` or `.txt` file whose first non-empty line contains "instructions", "plan", "todo", "goals", or "feedback" (case-insensitive)
  - If multiple candidates found, list them and ask the user to choose.
  - If none found, tell the user and stop.

## Processing steps:

### 1. Read raw instructions

Read the entire file. These are stream-of-consciousness user notes — informal, possibly contradictory, with tangential thoughts mixed in. The user may be writing about MULTIPLE projects in a single file.

### 2. Determine context: single project or workspace-level

**If running inside a specific project directory** (e.g., `~/projects/concord/`):
- Process as single-project feedback. All content targets this project.
- Proceed to step 3.

**If running in the root `~/projects/` directory (or a non-project directory):**
- Activate **multi-project routing mode** (step 2b).

### 2b. Multi-project routing (workspace-level only)

When processing feedback at the workspace level:

1. Read the full document and identify which project(s) each section/idea targets.
2. Use contextual clues: project names, technology references, feature descriptions that match known projects.
3. Scan `~/projects/*/CLAUDE.md` and `~/projects/*/.scope` to understand what each project does.
4. Split the document into discrete **feedback packets** — each packet targets one project.
5. Any ideas that don't clearly belong to an existing project go into a `~/projects/scratchpad.md` file for loose ideas.
6. For each packet, route it to the correct project's feedback pipeline (step 3 onwards, targeting that project's directory).
7. Report the routing: "Routed 3 packets to concord, 2 to orrapus, 1 to scratchpad."

### 3. Read project scope

Check for `.scope` file in the project root. The scope informs how much rigor to inject:
- `private`: Keep the prompt lean. Focus on getting it done.
- `public`: Add requirements for docs, tests, clean code.
- `commercial`: Add requirements for comprehensive tests, error handling, security, docs.

### 4. Generate optimized prompt

Transform the raw text into a structured prompt with these sections:

```markdown
## Objective
<1-2 sentence summary of what needs to be built/changed>

## Requirements
<Numbered list of concrete deliverables extracted from the instructions>

## Constraints
<Technical constraints, compatibility requirements, things to avoid>

## Technical Decisions
<Specific tech choices the user made — frameworks, patterns, approaches>
<Preserve these exactly as stated, even if informal>

## Open Questions
<Anything ambiguous, contradictory, or marked with "maybe"/"probably"/"I think">
<List these as decision points that need resolution>

## Acceptance Criteria
<How to know the work is done — derived from the requirements>
<For public/commercial scope: include testing and documentation criteria>
```

**Interpretation rules:**
- Numbered/bulleted items → Requirements
- Parenthetical asides → Constraints or notes
- "Maybe"/"probably"/"I think" → Open Questions (do NOT resolve these, flag them)
- References to other projects → Resolve to actual paths
- Contradictions → Flag explicitly in Open Questions
- Repetition → Deduplicate, keep the most specific version
- Emotional emphasis ("THIS IS IMPORTANT", caps, exclamation marks) → Elevate to top of relevant section
- Code snippets or examples → Preserve verbatim in Technical Decisions

### 5. Update the project feedback record (fb2p.md)

Append to `fb2p.md` in the target project directory (create if it doesn't exist).

Format:
```markdown
---

## Entry: <YYYY-MM-DD HH:MM> — <source-filename>

### Raw Input
<Full original text if under 200 lines, otherwise:>
Source: `<relative-path-to-file>` (<line count> lines)

### Optimized Prompt
<The generated structured prompt from step 4>

### Status
Generated: <timestamp>
Executed: <pending|timestamp>
Queued: <position in queue, e.g., "3 of 5">
```

### 6. Incorporate into master development plan

Each project should have a `PLAN.md` in its root (create if it doesn't exist). This is the **master development plan** — a living document that evolves with every feedback intake.

After generating the optimized prompt:

1. Read the project's `PLAN.md`.
2. Determine if the new feedback:
   - **Extends** an existing planned feature → merge into that section
   - **Modifies** a previous decision → update the relevant section, note what changed
   - **Adds** something entirely new → append as a new section
   - **Contradicts** existing plans → flag in an "Open Conflicts" section at the top
3. Write the updated `PLAN.md`.
4. The plan should always have these sections:
   ```markdown
   # <Project Name> — Master Development Plan

   ## Open Conflicts
   <Any contradictions between feedback entries that need user resolution>

   ## Architecture
   <Current architectural decisions, updated as feedback refines them>

   ## Feature Roadmap
   <Ordered list of features/changes, with status: planned/in-progress/done>

   ## Recent Changes
   <What changed in this plan update and why, with date>
   ```

### 7. Preserve the original feedback file

After the feedback has been:
- Committed to `fb2p.md`
- Incorporated into `PLAN.md`

Do NOT delete the original feedback file. It stays in place as a permanent record alongside the development log. The original text is the user's source of truth — `fb2p.md` contains the optimized version, but the raw input is preserved for auditing and reference.

Report: "Processed and archived. Original `<filename>` preserved. See fb2p.md and PLAN.md."

### 8. Present and confirm

Show the user:
1. The optimized prompt(s)
2. Any Open Questions that need resolution
3. Changes made to PLAN.md
4. Ask: "Run this prompt now, resolve open questions first, or save for later?"

- If **run now**: Execute the optimized prompt as the next task. Update fb2p.md status to "Executed: <timestamp>".
- If **resolve first**: Present each open question and ask the user to decide. Update the prompt with their answers, re-save to fb2p.md, then ask again.
- If **save for later**: Just confirm fb2p.md was written. The prompt stays in the queue.

## Automatic feedback intake (background monitoring)

Claude should proactively check for new feedback files when:
- Starting a session in a project directory
- The user says "continue development" or "what's new"
- Any new `.txt` or `.md` file appears in the project root that looks like feedback (not code, not config)

When new feedback files are found:
1. Process them through this pipeline automatically
2. Report what was found and incorporated
3. Ask if the user wants to execute any queued prompts

## The "continue development" trigger

When the user says **"continue development"** (or "continue", "what's next", "pick up where we left off"):

1. Read the project's `PLAN.md` for the current state
2. Read `fb2p.md` for any queued (unexecuted) prompts
3. Check for any new raw feedback files that haven't been processed
4. Present a summary:
   - Recent plan changes from feedback
   - Queued prompts ready to execute
   - Current feature roadmap status
5. Start executing the next queued prompt, or ask the user which to prioritize
