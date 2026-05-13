---
name: Test Workforce
description: Fixture workforce for the workflow compiler integration test.
operations:
  - DEVELOP FEATURE
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|---|---|---|---|
| pm | Project Manager | yes | - |
| dev | Developer | no | - |
| ft | Feature Tester | no | - |
| rm | Repository Manager | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | dev | instructions |
| dev | ft | deliverable |
| ft | pm | report |
| pm | rm | deliverable |

## Teams

| Order | Team | Description |
|-------|------|-------------|
| 1 | develop_feature | primary feature cycle |
| 2 | develop_feature | secondary feature cycle |
| 3 | cleanup | MANDATORY workforce-scale reconciliation |
