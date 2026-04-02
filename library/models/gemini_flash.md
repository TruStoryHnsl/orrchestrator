---
name: Gemini 2.5 Flash
provider: Google
model_id: gemini-2.5-flash
tier: mid-tier
pricing: free
max_context: 1000000
api_key_env: GOOGLE_API_KEY
capabilities:
  - fast
  - massive context
  - good for research and bulk processing
limitations:
  - free tier rate limited
  - less reliable for complex tasks
---

Best value model in the library — free tier with 1M context.
Use for bulk operations: scanning repos, generating summaries, processing documentation.
Rate limits on free tier may throttle sustained use — stagger requests or mix with paid models.
Not reliable enough for agentic coding loops — use as a read-heavy assistant, not a primary coder.
