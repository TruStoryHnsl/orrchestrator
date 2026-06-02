---
name: Dependabot PR Processing
description: Dedicated preflight team for Dependabot and GitHub security autofix PRs. Resolves routine dependency warnings before any new feature work starts.
operations:
  - DEPENDABOT PR PROCESSING
teams: []
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|---|---|---|---|
| pm | Project Manager | yes | - |
| rm | Repository Manager | no | - |
| dev | Developer | no | - |
| ft | Feature Tester | no | - |
| pt | Penetration Tester | no | - |

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

## Steps

| Index | Agent | Tool/Skill | Operation |
|-------|-------|------------|-----------|
| 1 | Project Manager | skill:dependabot-pr-processing | inventory all open Dependabot/GitHub security autofix PRs before any new work; classify each as MERGE, FIX, CLOSE_SUPERSEDED, CLOSE_OBSOLETE, DEFER_MAJOR, or ESCALATE |
| 2 | Repository Manager | shell:gh_pr_dependency_audit | fetch/prune, inspect every Dependabot PR diff, verify target manifests still exist, and group compatible non-major updates by ecosystem where this reduces churn |
| 3 | Developer | * | for conflicted or failing Dependabot PRs with bounded dependency-resolution work, repair the manifest/lockfile branch without changing product scope |
| 4 | Feature Tester | skill:test-design | run the relevant ecosystem checks for each merge/fix candidate, including package-manager install, build, and project tests when present |
| 4 | Penetration Tester | skill:pen-test | prioritize security autofix PRs; verify that vulnerable package versions are removed from the resolved dependency graph |
| 5 | Project Manager | skill:evaluate_dependabot_prs | decide the terminal action for every Dependabot PR; no PR may remain unclassified |
| 6 | Repository Manager | shell:gh_pr_merge_close | merge verified PRs, close superseded/obsolete/stale PRs with one-line reasons, delete stale bot branches, and leave only escalated PRs open |
| 7 | Project Manager | skill:log-dev | write `.orrch/dependabot_summary.md` and a DEVLOG entry listing merged, fixed, closed, deferred-major, and escalated PRs |

## Summary

This team is a mandatory dependency-warning preflight. It handles routine
Dependabot and GitHub security autofix PRs before feature planning starts, so
human attention is reserved for PRs that agents cannot safely resolve.

The PM may skip or postpone this team only when the user explicitly demands
that Dependabot handling be bypassed for an urgent task. Otherwise, every open
Dependabot PR receives a terminal action: merged, fixed-and-merged, closed with
reason, deferred as a major upgrade requiring a fresh task, or escalated.
