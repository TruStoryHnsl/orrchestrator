---
name: DeepSeek V4 Flash
provider: DeepSeek
model_id: deepseek-v4-flash
tier: mid-tier
pricing: per_token
base_url: https://api.deepseek.com
api_key_env: DEEPSEEK_API_KEY
max_context: 1000000
location: cloud
api_format:
  - anthropic
  - openai
capabilities:
  - fast structured-instruction coding
  - tool use
  - long 1M context
  - dual Anthropic/OpenAI-compatible endpoints
limitations:
  - weaker complex reasoning than enterprise tier
  - rate limited at scale
---

DeepSeek V4 Flash — a fast, low-cost mid-tier engine that serves BOTH an
Anthropic-compatible and an OpenAI-compatible endpoint at `api.deepseek.com`.
Drive it via either wire protocol. Good for high-volume structured-instruction
workflows where cost matters; prefer an enterprise engine for deep multi-file
architecture or security-sensitive work.

Pricing: $0.14 / $0.28 per 1M tokens (input / output).
