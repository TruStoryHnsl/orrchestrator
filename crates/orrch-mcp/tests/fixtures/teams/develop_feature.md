---
name: Develop Feature
description: Standard single-feature dev pipeline.
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|---|---|---|---|
| pm | Project Manager | yes | - |
| dev | Developer | no | - |
| ft | Feature Tester | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | dev | instructions |
| dev | ft | deliverable |
| ft | pm | report |

## Steps

| Index | Agent | Tool/Skill | Operation |
|-------|-------|------------|-----------|
| 1 | Project Manager | mcp:workflow_init | initialize codebase brief |
| 2 | Project Manager | skill:plan_tasks | decompose goal into tasks |
| 3 | Developer | * | implement assigned cluster |
| 4 | Feature Tester | skill:test-design | verify acceptance criteria |
| 5 | Project Manager | skill:evaluate_verdict | classify PASS/REWORK/SHIP_WITH_ISSUES |
