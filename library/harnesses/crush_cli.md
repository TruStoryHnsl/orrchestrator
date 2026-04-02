---
name: Crush CLI
command: crush
description: Ollama-native agentic coding CLI
capabilities:
  - file editing
  - shell execution
  - local model support
  - offline operation
supported_models:
  - mistral
  - codestral
  - llama3
  - any Ollama model
flags: []
limitations:
  - no cloud API
  - depends on Ollama running
---

Only harness that works fully offline — critical for air-gapped or no-internet scenarios.
Provides tool use scaffolding for local models that lack native tool support.
Performance depends entirely on local hardware — GPU required for reasonable speed.
Verify Ollama is running before assigning tasks. Falls back poorly if the service is down.
