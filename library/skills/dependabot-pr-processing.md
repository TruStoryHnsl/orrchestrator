---
name: Dependabot PR Processing
description: >
  Resolve Dependabot and GitHub security autofix PRs before new development
  work. Used by PM agents as a dependency-warning preflight.
type: skill
domain: repository-maintenance
usage: >
  Run at the start of develop_feature and commercial workflows unless the user
  explicitly demands that Dependabot handling be skipped.
---

# Dependabot PR Processing

## Purpose

Resolve routine GitHub dependency warnings without involving the user. The PM
escalates only when available agents cannot safely merge, fix, or close a PR.

## Protocol

1. Inventory open PRs with `gh pr list --state open --json number,title,author,headRefName,baseRefName,mergeStateStatus,isDraft,updatedAt`.
2. Select PRs authored by `app/dependabot`, `dependabot[bot]`, or GitHub security autofix automation.
3. Prioritize security autofix PRs before routine version PRs.
4. For each PR, inspect changed manifests and lockfiles. Confirm every targeted file still exists in the current public/private repo shape.
5. Classify every PR:
   - `MERGE`: clean, bounded dependency update; install/build/tests pass.
   - `FIX`: useful update, but branch is stale/conflicted or lockfile is wrong; assign a Developer or Repository Manager to repair, then verify.
   - `CLOSE_SUPERSEDED`: the dependency version is already present on the base branch or included in a better rollup.
   - `CLOSE_OBSOLETE`: the dependency/file no longer exists in the repo.
   - `DEFER_MAJOR`: major upgrade needs a product-aware task or migration plan.
   - `ESCALATE`: agents cannot safely resolve it, tests fail for unclear reasons, or the update implies architectural risk.
6. Verify merge/fix candidates with the package-manager install command, build command, and project tests appropriate to the touched manifests.
7. Merge verified PRs with squash commits. Close superseded, obsolete, stale, or deferred-major PRs with one concise comment explaining why.
8. Leave open only `ESCALATE` PRs, and report the exact blocker.
9. Write `.orrch/dependabot_summary.md` listing merged, fixed, closed, deferred, and escalated PRs.

## Hard Rules

- Do not start feature planning while unresolved Dependabot/security PRs exist unless the user explicitly demanded a bypass.
- Do not wake the user for routine clean merges, superseded PRs, obsolete PRs, or bounded conflict repairs.
- Do not merge a dependency PR without verification.
- Do not treat major upgrades as routine. Defer them into explicit planned work unless they are security-critical and verified.
- Do not leave stale bot PRs dangling after a rollup or repo-shape cleanup.
