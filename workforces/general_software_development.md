---
name: General Software Development
description: Full dev team with PM, engineers, testers, and DevOps. Suitable for most software projects.
operations:
  - INSTRUCTION INTAKE
  - DEVELOP FEATURE
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
