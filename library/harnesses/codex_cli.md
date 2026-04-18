---
name: Codex CLI
command: codex
description: OpenAI's agentic coding CLI
capabilities:
  - file editing
  - shell execution
  - tool use
supported_models:
  - gpt-5.4
  - gpt-5.4-mini
  - gpt-5.3-codex
  - gpt-5.3-codex-spark
flags:
  - --dangerously-bypass-approvals-and-sandbox
limitations:
  - OpenAI models only
---

Native harness for OpenAI Codex models. Use when a Codex-capable GPT-5 model is the assigned backend.
Primary backend for Codex CLI sessions. The default unattended flag bypasses sandbox and approvals, so only use it in already sandboxed workspaces.
OpenAI-only lock-in means no fallback to other providers within this harness.
Solid for implementation-heavy work where Codex should have full local tool access.
