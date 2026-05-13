---
name: Develop Feature
description: Single-team feature development pipeline — PM plans, dev cluster implements in parallel waves, optional verifiers test, PM evaluates, RM commits. The standard "small team that ships one feature" unit.
operations:
  - DEVELOP FEATURE
teams: []
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|---|---|---|---|
| pm | Project Manager | yes | - |
| eng | Software Engineer | no | - |
| dev | Developer | no | - |
| res | Researcher | no | - |
| ui | UI Designer | no | - |
| ft | Feature Tester | no | - |
| pt | Penetration Tester | no | - |
| bt | Beta Tester | no | - |
| rm | Repository Manager | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
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

## Steps

| Index | Agent | Tool/Skill | Operation |
|-------|-------|------------|-----------|
| 1 | Project Manager | mcp:workflow_init | initialize codebase brief, read PLAN.md unchecked items, surface inbox stragglers |
| 2 | Project Manager | skill:plan_tasks | decompose goal into TASK blocks (id, agent, files, work, acceptance, depends) |
| 3 | Project Manager | mcp:workflow_cluster | cluster tasks by file overlap into parallel waves |
| 4 | Developer | * | execute coding tasks for assigned cluster (parallel across clusters within a wave) |
| 4 | Software Engineer | * | design architecture for cross-cutting changes |
| 4 | Researcher | * | investigate technologies / library APIs as needed |
| 4 | UI Designer | * | design TUI / web UI elements |
| 5 | Feature Tester | skill:test-design | design tests verifying acceptance criteria |
| 5 | Penetration Tester | skill:pen-test | attempt to exploit / break the implementation |
| 5 | Beta Tester | skill:go-nuts | aggressive usage to surface UX regressions |
| 6 | Project Manager | mcp:workflow_compress | compress each developer agent's output to structured summary |
| 7 | Project Manager | skill:evaluate_verdict | classify as PASS / REWORK / SHIP_WITH_ISSUES; spawn rework dev (max 3 cycles) if REWORK |
| 8 | Project Manager | skill:log-dev | write dev-log entry summarizing build, files changed, verdict |
| 9 | Repository Manager | skill:commit-review | review commit grouping; commit with conventional format; open PR |
| 10 | Project Manager | skill:reconcile_branches | PM-as-cleanup duty at team scale: review own branches/PRs from this team, surface escalations, prepare handoff to workforce-scale Cleanup team |

## Summary

This is the canonical single-team development unit. A workforce may run multiple
develop_feature teams sequentially (each ships one feature; each keeps its own
session open until the workforce-level Cleanup team merges everything).

Within this team, the PM performs the team-scale reconciliation duty in step 10:
it reviews only this team's branches and PRs, leaving cross-team reconciliation
to the workforce-level Cleanup team.
