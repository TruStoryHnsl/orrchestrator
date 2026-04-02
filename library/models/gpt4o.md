---
name: GPT-4o
provider: OpenAI
model_id: gpt-4o
tier: enterprise
pricing: per_token
max_context: 128000
api_key_env: OPENAI_API_KEY
capabilities:
  - strong reasoning
  - code generation
  - tool use
limitations:
  - less context than Claude/Gemini
---

Strong general-purpose model — good reasoning and reliable tool use.
Use as an alternative to Claude when Anthropic API is down or rate limited.
128K context is sufficient for most tasks but limits whole-codebase analysis.
Pairs well with Codex CLI harness for OpenAI-native agentic workflows.
