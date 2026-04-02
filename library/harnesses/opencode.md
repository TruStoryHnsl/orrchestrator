---
name: OpenCode
command: opencode
description: Experimental agentic coding CLI with parallel session management
capabilities:
  - parallel sessions
  - file editing
  - shell execution
  - multi-provider
supported_models:
  - claude
  - gpt-4o
  - gemini
  - ollama models
flags: []
limitations:
  - experimental
  - may have stability issues
---

Multi-provider harness — can route to any model backend from a single CLI.
Parallel session support makes it interesting for orchestrated workloads.
Still experimental — expect rough edges and occasional failures under load.
Consider as a secondary harness when Claude Code or Gemini CLI are unavailable for a given model.
