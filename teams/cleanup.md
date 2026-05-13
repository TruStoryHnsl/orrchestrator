---
name: Cleanup
description: Reconciles open branches and PRs at the end of every workforce — analyzes, tests, merges, and writes a user-facing summary plus a detailed dev log entry.
operations:
  - CLEANUP RECONCILIATION
teams: []
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|---|---|---|---|
| pm | Project Manager | yes | - |
| rm | Repository Manager | no | - |
| bt | Beta Tester | no | - |
| ft | Feature Tester | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | rm | instructions |
| pm | bt | instructions |
| pm | ft | instructions |
| rm | pm | report |
| bt | pm | report |
| ft | pm | report |

## Steps

| Index | Agent | Tool/Skill | Operation |
|-------|-------|------------|-----------|
| 1 | Repository Manager | tool:list_open_branches | enumerate every non-main branch and open PR created during the workforce run; record sha, branch name, target PR number |
| 2 | Repository Manager | shell:cargo_build_test | run `cargo build` and `cargo test --workspace` on each branch; record build/test status per branch |
| 2 | Beta Tester | skill:go-nuts | exercise each branch's user-facing changes against the live binary; record any regressions |
| 2 | Feature Tester | skill:test-design | verify each branch's acceptance criteria against the original task list |
| 3 | Project Manager | skill:classify_pr | for each branch: classify as MERGE / REWORK / ESCALATE based on prior step reports; one verdict per branch |
| 4 | Repository Manager | tool:merge_to_main | for every MERGE-classified branch, run `~/projects/orrchestrator/library/tools/merge_to_main.sh` in dependency order; abort on tool exit 1 (escalation) |
| 5 | Project Manager | skill:cleanup_summary | write `.orrch/cleanup_summary.md` listing per-branch verdict, work stoppages, issues to fix, items requiring user attention |
| 6 | Project Manager | skill:log-dev | append a detailed dev-log entry to `DEVLOG.md` covering the entire workforce run (each team's contribution, files changed, branches merged, residual escalations) |

## Summary

The Cleanup team is the last team in every workforce. It is the user's interface
for fixing anything broken before the workforce closes. Its `cleanup_summary.md`
is short and actionable; its `DEVLOG.md` entry is comprehensive.

The cleanup session that runs this team writes the dev-log entry directly — it
is the team's persistent record of the workforce execution. Subsequent loop
iterations read `cleanup_summary.md` to decide whether the run was healthy
enough to start the next workforce.
