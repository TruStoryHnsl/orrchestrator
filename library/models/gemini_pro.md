---
name: Gemini 2.5 Pro
provider: Google
model_id: gemini-2.5-pro
tier: enterprise
pricing: per_token
max_context: 1000000
api_key_env: GOOGLE_API_KEY
capabilities:
  - massive context
  - research
  - documentation analysis
  - code generation
limitations:
  - less reliable tool use than Claude
---

Strong for research-heavy tasks: reading large docs, analyzing entire repos, summarizing.
1M context makes it ideal for ingesting full codebases or documentation sets in one pass.
Tool use is functional but less predictable than Claude — avoid for complex multi-tool workflows.
Good complement to Claude: use Gemini for research/analysis, Claude for implementation.
