---
name: Develop AIO
description: All-in-one legacy develop-feature pipeline preserved verbatim from the pre-overhaul `/develop-feature` skill. Lightweight single-session team that drives the full plan → cluster → implement → verify → commit → merge loop in one harness conversation. Useful for quick fixes that don't justify the multi-team workforce overhead.
operations:
  - DEVELOP AIO
teams: []
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|---|---|---|---|
| pm | Project Manager | yes | - |
| dev | Developer | no | - |
| ft | Feature Tester | no | - |
| pt | Penetration Tester | no | - |
| bt | Beta Tester | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | dev | instructions |
| dev | ft | deliverable |
| dev | pt | deliverable |
| dev | bt | deliverable |
| ft | pm | report |
| pt | pm | report |
| bt | pm | report |

## Steps

| Index | Agent | Tool/Skill | Operation |
|-------|-------|------------|-----------|
| 1 | Project Manager | skill:develop-aio | run the full legacy develop-feature pipeline (workflow_init → PM plan → cluster → context-bundle → wave-parallel implementation → conditional verify → evaluate → log/commit → tiered merge) in a single session |

## Summary

The AIO team is the original `/develop-feature` skill preserved as-is. It
encodes a lot of accumulated optimization (bundle-files-per-cluster to avoid
duplicate reads, light-vs-full verification heuristics, tiered-merge tool
invocation). Use this when:
- The work is small enough to fit in one session's token budget.
- You want the proven optimization heuristics rather than the structured
  multi-team workforce expansion.
- You are running orrchestrator headlessly without TUI orchestration.

The new compiled `develop_feature` team supersedes this for the standard path,
but the AIO variant remains the harness-side fallback and the upgrade target
for future heuristic improvements.
