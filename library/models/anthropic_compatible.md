---
name: Anthropic-Compatible Endpoint (template)
provider: Custom
model_id: REPLACE-ME
tier: enterprise
pricing: per_token
base_url: https://REPLACE-ME.example.com
api_key_env: ANTHROPIC_COMPATIBLE_API_KEY
max_context: 200000
location: cloud
api_format:
  - anthropic
capabilities:
  - Anthropic messages wire format
  - tool use
limitations:
  - capabilities depend entirely on the backing model
---

Generic template for any engine that exposes an Anthropic-compatible
(`/v1/messages`) endpoint. Copy this file, then set:

- `name`, `provider`, `model_id` to identify the engine.
- `base_url` to the endpoint host (no `/v1/messages` suffix — that path is
  appended by the client).
- `api_key_env` to the env var holding the API key (drop it for keyless
  local/gateway engines).
- `location` to `cloud`, `gateway`, or `local`.
- `tier` and `pricing` to match the backing model.

`api_format` stays `anthropic`. Add `openai` as a second list item only if the
same host also serves an OpenAI-compatible endpoint.
