---
name: OpenAI-Compatible Endpoint (template)
provider: Custom
model_id: REPLACE-ME
tier: mid-tier
pricing: per_token
base_url: https://REPLACE-ME.example.com/v1
api_key_env: OPENAI_COMPATIBLE_API_KEY
max_context: 128000
location: cloud
api_format:
  - openai
capabilities:
  - OpenAI chat-completions wire format
  - tool use
limitations:
  - capabilities depend entirely on the backing model
---

Generic template for any engine that exposes an OpenAI-compatible
(`/v1/chat/completions`) endpoint. Copy this file, then set:

- `name`, `provider`, `model_id` to identify the engine.
- `base_url` to the endpoint host (include the `/v1` suffix if the provider
  expects it).
- `api_key_env` to the env var holding the bearer token (drop it for keyless
  local/gateway engines).
- `location` to `cloud`, `gateway`, or `local`.
- `tier` and `pricing` to match the backing model.

`api_format` stays `openai`. Add `anthropic` as a second list item only if the
same host also serves an Anthropic-compatible endpoint.
