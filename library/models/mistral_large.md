---
name: Mistral Large
provider: Mistral
model_id: mistral-large-latest
tier: mid-tier
pricing: per_token
max_context: 128000
api_key_env: MISTRAL_API_KEY
capabilities:
  - strong coding
  - instruction following
  - multilingual
limitations:
  - needs more structured instructions than Claude
  - smaller context
---

Solid mid-tier option when Claude/Gemini quotas are exhausted or cost is a concern.
Instruction following is good but benefits from explicit, structured prompts — avoid vague tasks.
Multilingual strength is useful for projects with i18n or non-English codebases.
128K context is adequate for most single-file and small-module work.
