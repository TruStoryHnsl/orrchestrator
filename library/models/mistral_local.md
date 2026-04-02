---
name: Mistral Local (Ollama)
provider: Ollama
model_id: mistral
tier: local
pricing: local
max_context: 32000
api_key_env: null
capabilities:
  - free
  - no API dependency
  - decent for simple tasks with rigid instructions
limitations:
  - small context
  - needs very explicit instructions
  - no tool use without harness
---

Fallback option when offline or when all API quotas are exhausted.
Only assign tasks that are small, self-contained, and have extremely clear instructions.
32K context means single-file work only — no multi-file reasoning.
Requires Ollama running locally. Pair with a harness that provides tool use scaffolding.
