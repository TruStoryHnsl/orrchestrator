---
name: Local Tier
description: Narrow rigid scope workforce for local-tier models (Ollama Mistral, Gemini free tier). Small agent set because local models choke on too many concurrent agents. Every task has explicit verification via feature tester and penetration tester. Best for bounded fixes and well-defined patches.
operations:
  - DEVELOP FEATURE
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|----|---------------|-------------|------------------|
| pm | Project Manager | yes | - |
| dev | Developer | no | - |
| ft | Feature Tester | no | - |
| pt | Penetration Tester | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | dev | instructions |
| dev | ft | deliverable |
| dev | pt | deliverable |
| ft | pm | report |
| pt | pm | report |
