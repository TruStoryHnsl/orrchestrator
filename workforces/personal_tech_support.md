---
name: Personal Tech Support
description: Lightweight team for system configuration, troubleshooting, quick fixes, and bug intake. No heavy QA or DevOps.
operations:
  - INSTRUCTION INTAKE
  - INTAKE BUGREPORT
  - ASSESS DEVELOPMENT
---

## Agents

| ID | Agent Profile | User-Facing |
|----|---------------|-------------|
| ea | Executive Assistant | yes |
| coo | Chief Operations Officer | no |
| pm | Project Manager | yes |
| dev | Developer | no |
| res | Researcher | no |
| ft | Feature Tester | no |
| bt | Beta Tester | no |
| rm | Repository Manager | no |

## Connections

| From | To | Data Type |
|------|----|-----------|
| ea | coo | instructions |
| coo | pm | instructions |
| pm | dev | instructions |
| pm | res | instructions |
| res | dev | research |
| dev | ft | deliverable |
| dev | bt | deliverable |
| ft | pm | report |
| bt | pm | report |
| pm | rm | deliverable |
