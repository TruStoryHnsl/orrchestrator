---
name: Cleanup
description: Reconciles open branches and PRs at the end of a workforce.
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|---|---|---|---|
| pm | Project Manager | yes | - |
| rm | Repository Manager | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | rm | instructions |
| rm | pm | report |

## Steps

| Index | Agent | Tool/Skill | Operation |
|-------|-------|------------|-----------|
| 1 | Repository Manager | tool:list_open_branches | enumerate branches |
| 2 | Project Manager | skill:classify_pr | classify each PR |
| 3 | Repository Manager | tool:merge_to_main | merge accepted PRs |
| 4 | Project Manager | skill:cleanup_summary | write cleanup_summary.md |
