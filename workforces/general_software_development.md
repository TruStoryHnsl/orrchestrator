---
name: General Software Development
description: Full dev team with PM, engineers, testers, and DevOps. Suitable for most software projects.
operations:
  - INSTRUCTION INTAKE
  - INTAKE BUGREPORT
  - DEVELOP FEATURE
  - ASSESS DEVELOPMENT
---

## Agents

| ID | Agent Profile | User-Facing |
|----|---------------|-------------|
| ea | Executive Assistant | yes |
| coo | Chief Operations Officer | no |
| pm | Project Manager | yes |
| eng | Software Engineer | no |
| dev | Developer | no |
| res | Researcher | no |
| ui | UI Designer | no |
| ft | Feature Tester | no |
| pt | Penetration Tester | no |
| bt | Beta Tester | no |
| rm | Repository Manager | no |

## Connections

| From | To | Data Type |
|------|----|-----------|
| ea | coo | instructions |
| coo | pm | instructions |
| pm | eng | instructions |
| pm | dev | instructions |
| pm | res | instructions |
| pm | ui | instructions |
| dev | ft | deliverable |
| dev | pt | deliverable |
| dev | bt | deliverable |
| ft | pm | report |
| pt | pm | report |
| bt | pm | report |
| eng | dev | instructions |
| res | eng | research |
| pm | rm | deliverable |

## Teams

| Order | Team | Description |
|-------|------|-------------|
| 1 | develop_feature | primary feature implementation cycle |
| 2 | develop_feature | secondary feature cycle (follow-up work) |
| 3 | cleanup | MANDATORY workforce-scale reconciliation: review all branches/PRs, run build+test, classify, run merge_to_main, write cleanup_summary.md + DEVLOG entry |
