---
name: DeepSeek V4 Flash (Ollama)
provider: Ollama
model_id: deepseek-v4-flash
tier: local
pricing: local
base_url: http://localhost:11434/v1
location: gateway
api_format:
  - openai
capabilities:
  - local OpenAI-compatible serving via Ollama
  - no API cost
  - rigid scope-limited workflows
limitations:
  - bounded by local hardware (HWF fit check required)
  - smaller effective context than cloud
  - no Anthropic-compatible endpoint
---

DeepSeek V4 Flash served locally through Ollama's OpenAI-compatible endpoint at
`http://localhost:11434/v1`. Location is `gateway` (runs on a reachable host, not
the cloud) — Local/Gateway engines require an HWF fit check for the target
machine before they are offered. No API key. Speaks only the OpenAI wire format.
