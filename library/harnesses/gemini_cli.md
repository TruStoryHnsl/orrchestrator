---
name: Gemini CLI
command: gemini
description: Google's agentic coding CLI
capabilities:
  - code generation
  - file editing
  - shell execution
  - large context
supported_models:
  - gemini-2.5-pro
  - gemini-2.5-flash
flags: []
limitations: []
---

Native harness for Gemini models. Good for research-heavy and context-heavy tasks.
File editing and shell execution work but are less polished than Claude Code.
No MCP server support — tool integrations must be handled externally.
Best used for tasks that benefit from Gemini's massive context window.
