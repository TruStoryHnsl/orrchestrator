---
name: Mid Tier
description: Structured workforce for mid-tier models (Mistral Large API and similar). Explicit step boundaries, extra cross-checks, and a dedicated specialist reviewer. Mid-tier models target good capability at moderate cost but need clearer task framing than enterprise models.
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
| spec | Specialist | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | dev | instructions |
| pm | eng | instructions |
| pm | res | research |
| dev | ft | deliverable |
| ft | pm | report |
| dev | spec | deliverable |
| spec | pm | report |
