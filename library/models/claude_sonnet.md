---
name: Claude Sonnet 4.6
provider: Anthropic
model_id: claude-sonnet-4-6
tier: enterprise
pricing: per_token
max_context: 200000
api_key_env: ANTHROPIC_API_KEY
capabilities:
  - fast agentic coding
  - tool use
  - good for routine dev tasks
limitations:
  - less capable than Opus for complex reasoning
---

Default workhorse for most coding tasks — fast, reliable, cost-effective.
Handles file edits, test writing, refactoring, and standard feature work well.
Swap to Opus only when Sonnet struggles with multi-step reasoning or large-scale architecture.
Context window is smaller than Opus — split large codebases across sessions if needed.
