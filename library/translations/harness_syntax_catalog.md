---
name: Harness Syntax Catalog
description: First-slice comparison of prompt, tool-call, file-read, and agent-spawn syntax across supported harnesses.
kind: translation_catalog
version: 1
last_updated: 2026-04-08
---

# Harness Syntax Catalog

This is the first cataloging slice for the Syntax Translation Engine (PLAN.md item 63).
It captures the externally-observable syntax differences between the five harnesses
currently registered in `library/harnesses/`, so that per-harness variants of context
files (agent profiles, CLAUDE.md equivalents, skill prompts) can be generated later.

Cells marked `TBD` require a research pass (likely dispatched via the Mentor ->
Researcher loop defined in PLAN item 58) before downstream translation tooling can
rely on them.

## Comparison Table

| Harness    | Prompt Delimiter            | Tool-Call Format                             | File Read Syntax            | Agent Spawn Syntax                    | Context File Name |
|------------|-----------------------------|----------------------------------------------|-----------------------------|---------------------------------------|-------------------|
| Claude Code | stdin / `--print` / interactive REPL | Anthropic `tool_use` JSON blocks via MCP + built-in `Read`/`Edit`/`Bash` tools | `Read` tool with `file_path` arg | `Task` subagent tool (general-purpose / specialized subagent_type) | `CLAUDE.md` |
| Codex CLI   | stdin / `--full-auto` batch prompt | OpenAI function-call schema (`tools: [{type: function, ...}]`) | built-in filesystem tool (`read_file` / shell `cat`) | no first-class subagent; orchestrate via shell / parallel `codex` invocations | `AGENTS.md` (TBD — verify) |
| Crush CLI   | stdin prompt; Ollama chat template | Ollama tool-use JSON (model-dependent; falls back to prompt-engineered `<tool>` tags) | shell `cat` / built-in read tool (TBD — verify name) | no subagent primitive; spawn parallel `crush` processes | `CRUSH.md` (TBD — verify) |
| Gemini CLI  | stdin / flag-based prompt   | Gemini function-calling JSON (`function_declarations`) | built-in read tool (TBD — exact name) | no first-class subagent; parallel `gemini` processes | `GEMINI.md` |
| OpenCode    | stdin / interactive REPL    | Provider-dependent — forwards native tool schema of the routed backend (Anthropic / OpenAI / Gemini / Ollama) | built-in read tool (TBD — verify) | native parallel-session support (`opencode session ...`, TBD exact subcommand) | `OPENCODE.md` (TBD — verify) |

## Legend / Notes

- **Prompt Delimiter** — how the harness ingests the initial prompt (stdin, CLI flag, REPL input).
- **Tool-Call Format** — wire format the model must emit for the harness to execute a tool. This is the
  single biggest translation axis between providers.
- **File Read Syntax** — the canonical way an agent asks the harness to read a file. For downstream
  translation, references like "use the Read tool" need to be rewritten per-harness.
- **Agent Spawn Syntax** — whether the harness has a first-class subagent primitive (only Claude Code
  does, today) and what it's called. Everything else fakes it with parallel processes.
- **Context File Name** — the `CLAUDE.md`-equivalent file each harness auto-loads at session start,
  if any. Needed so that per-harness context files can be generated with the correct filename.

## Known TBDs (follow-up research)

1. Verify exact context file name for Codex CLI, Crush CLI, and OpenCode (current entries are best-guesses).
2. Verify the exact `read_file` / read-tool name used by Crush CLI, Gemini CLI, and OpenCode at the
   tool-schema level (not just the user-facing command name).
3. Confirm OpenCode's subcommand surface for parallel sessions (`opencode session create ...` vs
   `opencode run --parallel ...`).
4. Confirm whether Crush CLI's tool-use JSON is standardized or varies per underlying Ollama model.

## Next Slice

Once the TBDs above are resolved, the translation engine should start emitting per-harness
variants of the agent profiles in `agents/*.md`, storing them at
`library/translations/agents/<harness>/<agent>.md`. This catalog is the source-of-truth lookup
table the generator will use.
