---
name: Claude Opus 4.6
provider: Anthropic
model_id: claude-opus-4-6
tier: enterprise
pricing: per_token
max_context: 1000000
api_key_env: ANTHROPIC_API_KEY
capabilities:
  - full agentic coding
  - tool use
  - subagent spawning
  - 1M context
  - complex architecture
  - security analysis
limitations:
  - expensive
  - rate limited at scale
---

Best model for complex, multi-file architecture tasks and security-sensitive work.
Use when the task requires deep reasoning across large codebases or spawning subagents.
Overkill for routine implementation — prefer Sonnet for straightforward coding tasks.
Reserve for tasks where correctness matters more than speed or cost.
