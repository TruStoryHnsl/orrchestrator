---
name: PI
command: pi
description: Open-source customizable coding harness (badlogic/pi-mono)
capabilities:
  - multi-provider (Anthropic/Google/Mistral)
  - RPC mode
  - extensions
  - skills
  - custom tools
  - thinking levels
  - session persistence
  - sub-agents
supported_models:
  - anthropic/claude-sonnet-4-6
  - google/gemini-2.5-flash
  - mistral/devstral-small-latest
flags:
  - --no-session
  - --mode rpc
  - --provider
  - --model
  - --thinking
limitations:
  - no built-in MCP server (use extensions)
---

Multi-provider harness with first-class RPC mode for programmatic control.
RPC mode accepts JSONL commands on stdin and emits JSONL events on stdout — suitable for
embedding in orchestration pipelines without PTY overhead.
Supports extensions for custom tools and skills, making it the most extensible
open-source harness in the library.
Use `--thinking off` to suppress chain-of-thought in production pipelines where token
efficiency matters.
