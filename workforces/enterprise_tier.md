---
name: Enterprise Tier
description: High-trust workforce for enterprise-tier models (Claude Opus/Sonnet, GPT-4o). Loose guidance, broad scope, agents handle ambiguity well. Best for complex multi-step features requiring judgment.
operations:
  - DEVELOP FEATURE
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|----|---------------|-------------|------------------|
| pm | Project Manager | yes | - |
| dev | Developer | no | - |
| eng | Software Engineer | no | - |
| res | Researcher | no | - |
| ft | Feature Tester | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | dev | instructions |
| pm | eng | instructions |
| pm | res | research |
| dev | ft | deliverable |
| ft | pm | report |
