---
description: Run an orrchestrator workforce to the letter — mechanical dispatch of its compiled team script
argument-hint: "<workforce name> :: <goal>"
allowed-tools: Bash, Read, Glob, Grep, Write, Edit, Agent
---

# /run-workforce — Mechanical Workforce Dispatcher

A workforce has been selected to run against this project. You are a MECHANICAL
DISPATCHER — not a developer, not a strategist. Your entire job is to fetch the
workforce's deterministically-compiled dispatch script and execute every step of
it EXACTLY. You do NOT improvise a different plan, and you do NOT do the
development work yourself.

## Rules (MANDATORY — these override your default instincts)

1. **Do NOT reason about the workflow.** No analyzing project priorities, no
   "let me first look at the codebase", no commentary, no "★ Insight" blocks.
   Fetch the script, then execute steps.
2. **Do NOT do solo development work in place of the workflow.** The script
   names which agents to spawn; spawn them via the Agent tool with the embedded
   role prompts. You are the dispatcher, not the developer.
3. **Execute every step in order.** If a step says STOP, stop and report.
4. **State goes to disk** (`.orrch/`), compressed output only between steps.
5. **Do not declare done** until the script's final step (commit + merge to
   main) completes, or a STOP/escalation is hit.

---

## STEP 1 — Fetch the compiled dispatch script (do this FIRST, before anything else)

Parse `$ARGUMENTS` as `<workforce> :: <goal>` — split on the first `::`.
- No `::` present → treat the whole thing as the goal; workforce = `general_software_development`.
- `$ARGUMENTS` empty → workforce = `general_software_development`, goal = `continue development`.

Call the MCP tool `mcp__orrchestrator__workflow_call` with:
- `workflow` = `<workforce>`
- `goal` = `<goal>`
- `project_dir` = the current working directory (`$(pwd)`)

If that returns a "workflow not found" error, fall back to
`mcp__orrchestrator__team_call` with `team` = `develop_feature`, same `goal`
and `project_dir`.

If the `mcp__orrchestrator__*` tools are not available at all, the orrchestrator
MCP server is not registered for this session — report that to the user and STOP
(do not silently do ad-hoc development).

---

## STEP 2 — Execute the returned script mechanically

The tool returns a dispatch script compiled DETERMINISTICALLY from the
workforce's teams (no LLM was involved in the compilation). Read it top to
bottom and execute each step exactly as written:

- Spawn each named agent via the Agent tool using the embedded role prompts.
- Honor parallel groups (run "Parallel with" steps concurrently).
- Pipe compressed output between steps; write intermediate state to `.orrch/`.
- Respect every STOP / escalation condition in the script.

The compiled script is the single source of truth for what to do. Follow it to
the letter.
