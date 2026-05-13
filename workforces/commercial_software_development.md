---
name: Commercial Software Development
description: Full corporate-style team with all departments. Includes legal, marketing, and comprehensive QA. For production-grade distributable software.
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
| irm | Intelligence Resources Manager | no |
| mentor | Mentor | no |
| pm | Project Manager | yes |
| ts | Talent Scout | no |
| eng | Software Engineer | no |
| dev | Developer | no |
| res | Researcher | no |
| ui | UI Designer | no |
| ft | Feature Tester | no |
| pt | Penetration Tester | no |
| bt | Beta Tester | no |
| rm | Repository Manager | no |
| ux | UX Specialist | no |
| mr | Market Researcher | no |
| la | Licensing Auditor | no |
| ci | Copyright Investigator | no |

## Connections

| From | To | Data Type |
|------|----|-----------|
| ea | coo | instructions |
| coo | pm | instructions |
| pm | eng | instructions |
| pm | dev | instructions |
| pm | res | instructions |
| pm | ui | instructions |
| pm | ts | instructions |
| dev | ft | deliverable |
| dev | pt | deliverable |
| dev | bt | deliverable |
| ft | pm | report |
| pt | pm | report |
| bt | pm | report |
| eng | dev | instructions |
| res | eng | research |
| pm | rm | deliverable |
| pm | ux | deliverable |
| ux | pm | report |
| mr | pm | research |
| la | pm | report |
| ci | pm | report |
| mentor | pm | instructions |
| irm | pm | instructions |

## Teams

| Order | Team | Description |
|-------|------|-------------|
| 1 | develop_feature | primary feature implementation cycle |
| 2 | develop_feature | secondary feature cycle (parallel-track features) |
| 3 | develop_feature | tertiary feature cycle (refinements, follow-ups) |
| 4 | cleanup | MANDATORY workforce-scale reconciliation: review all branches/PRs from teams 1-3, run build+test, classify merge/rework/escalate, run merge_to_main, write cleanup_summary.md + DEVLOG entry |
