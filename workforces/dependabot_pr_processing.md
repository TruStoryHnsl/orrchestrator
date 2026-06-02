---
name: Dependabot PR Processing
description: Focused workforce for GitHub Dependabot and security-autofix PR processing before feature work.
operations:
  - DEPENDABOT PR PROCESSING
---

## Agents

| ID | Agent Profile | User-Facing |
|----|---------------|-------------|
| pm | Project Manager | yes |
| rm | Repository Manager | no |
| dev | Developer | no |
| ft | Feature Tester | no |
| pt | Penetration Tester | no |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | rm | instructions |
| pm | dev | instructions |
| pm | ft | instructions |
| pm | pt | instructions |
| rm | pm | report |
| dev | pm | deliverable |
| ft | pm | report |
| pt | pm | report |
| pm | rm | verdict |

## Teams

| Order | Team | Description |
|-------|------|-------------|
| 1 | dependabot_pr_processing | mandatory Dependabot/security-autofix PR preflight before new feature work |
