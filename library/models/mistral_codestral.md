---
name: Codestral
provider: Mistral
model_id: codestral-latest
tier: mid-tier
pricing: per_token
max_context: 256000
api_key_env: MISTRAL_API_KEY
capabilities:
  - code-specialized
  - fast
  - good for routine implementation
limitations:
  - weaker at architecture decisions
---

Code-specialized model — faster and cheaper than general-purpose models for pure implementation.
Best for well-defined tasks: "implement this function", "write tests for this module", "add error handling".
Do not use for design decisions, architecture planning, or ambiguous requirements.
256K context gives it an edge over Mistral Large for larger file sets.
