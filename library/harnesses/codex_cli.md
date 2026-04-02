---
name: Codex CLI
command: codex
description: OpenAI's agentic coding CLI
capabilities:
  - file editing
  - shell execution
  - tool use
supported_models:
  - gpt-4o
  - o3
  - o4-mini
flags:
  - --full-auto
limitations:
  - OpenAI models only
---

Native harness for OpenAI models. Use when GPT-4o or o3/o4-mini is the assigned model.
--full-auto flag enables unattended execution — use only for trusted, well-scoped tasks.
OpenAI-only lock-in means no fallback to other providers within this harness.
Solid for straightforward implementation tasks paired with GPT-4o.
