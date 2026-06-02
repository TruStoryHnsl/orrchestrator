---
name: Claude Code
command: claude
description: Full agentic coding CLI by Anthropic
capabilities:
  - tool use
  - subagent spawning
  - file editing
  - shell execution
  - MCP server integration
  - hooks
  - 1M context
supported_models:
  - claude-opus-4-6
  - claude-sonnet-4-6
flags:
  - --dangerously-skip-permissions
limitations: []
env_injection: true
rpc_steering: false
prompt_injection: true
mcp_tools: true
tool_gating: true
system_prompt_override: false
mid_run_steering: false
---

Primary harness for all Claude-model tasks. Most mature agentic coding environment available.
Supports MCP servers for extended tool access (GitHub, databases, etc.).
Subagent spawning enables parallel workloads within a single session.
Use --dangerously-skip-permissions only for fully automated pipelines with trusted prompts.
