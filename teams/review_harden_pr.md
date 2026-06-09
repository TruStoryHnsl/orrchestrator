---
name: Review Harden PR
description: Layered code-review + architecture-design-analysis pipeline for an EXISTING codebase. Independent reviewers fan out across dimensions, every finding is adversarially verified to kill false positives, confirmed issues are investigated, fixed with regression tests, and re-verified by observation. Delivers a PR with a full findings/fix report — and explicitly NEVER auto-merges. Runs to completion: does not stop until the PR is delivered.
operations:
  - REVIEW HARDEN PR
teams: []
---

## Agents

| ID | Agent Profile | User-Facing | Nested Workforce |
|---|---|---|---|
| pm | Project Manager | yes | - |
| eng | Software Engineer | no | - |
| res | Researcher | no | - |
| ft | Feature Tester | no | - |
| pt | Penetration Tester | no | - |
| bt | Beta Tester | no | - |
| dev | Developer | no | - |
| rm | Repository Manager | no | - |

## Connections

| From | To | Data Type |
|------|----|-----------|
| pm | eng | instructions |
| pm | res | instructions |
| pm | ft | instructions |
| pm | pt | instructions |
| pm | bt | instructions |
| eng | pm | findings |
| res | pm | findings |
| ft | pm | findings |
| pt | pm | findings |
| bt | pm | findings |
| pm | dev | instructions |
| dev | ft | deliverable |
| dev | pt | deliverable |
| ft | pm | report |
| pt | pm | report |
| pm | rm | deliverable |

## Steps

| Index | Agent | Tool/Skill | Operation |
|-------|-------|------------|-----------|
| 0 | Project Manager | mcp:workflow_init | RECON: build codebase brief; read PLAN.md, CLAUDE.md (ALL of them — global, workspace, project), and every durable project invariant. Define explicit review scope and the project rules that any fix MUST NOT violate (e.g. no-auto-tag, public-template invariant, dev-first/no-auto-deploy posture). Establish a clean baseline branch. |
| 1 | Software Engineer | * | LAYER A — ARCHITECTURE & DESIGN ANALYSIS (independent, context-isolated). Map module boundaries, coupling/cohesion, layering violations, data-model/schema integrity, transaction & error-handling patterns, scalability ceilings, dead code, and tech-debt. Check adherence to the project's own stated invariants. Emit STRUCTURED findings: {id, dimension:'architecture', title, severity, location(file:line), evidence, why_it_matters, suggested_fix}. |
| 1 | Researcher | mcp:context7 | LAYER B — DEPENDENCY & PRACTICE AUDIT (independent). Inventory dependencies; check for known-vulnerable / EOL / outdated libs and current best-practice deltas vs how the code uses them. Verify claims against live docs. Emit structured findings with sources + dates. |
| 1 | Feature Tester | skill:test-design | LAYER C — RUNTIME / BEHAVIOR REVIEW (independent). Build and LAUNCH the app locally (dev-first). Exercise the real user-facing surfaces and key flows. Record runtime defects with reproduction steps + evidence (screenshots/logs/HTTP codes). Emit structured findings. |
| 1 | Penetration Tester | skill:pen-test | LAYER D — SECURITY REVIEW (independent). OWASP sweep + domain-specific threats (authn/authz on admin surfaces, injection, SSRF/path traversal in file handling, secret handling, mass-assignment). Attempt PoC exploits. Emit structured findings with severity + reproduction. |
| 1 | Beta Tester | skill:go-nuts | LAYER E — CHAOS / EDGE-CASE REVIEW (independent). Aggressive, out-of-order, malformed-input usage to surface breakage the happy-path reviewers miss. Emit structured findings with repro steps. |
| 2 | Software Engineer | * | LAYER F — ADVERSARIAL VERIFICATION. For EACH finding from steps 1A–1E, an independent skeptic (who did NOT author it) attempts to REFUTE it: reproduce it, read the source, and default to 'rejected' unless it can be concretely confirmed. Output per finding: {confirmed:bool, reproduction, corrected_severity, notes}. Drop everything not confirmed. This is the false-positive gate. |
| 3 | Project Manager | mcp:workflow_compress | TRIAGE & SYNTHESIS. Dedupe confirmed findings; assign priority P0..P3; split into IN-SCOPE (fix this PR) vs DEFERRED (backlog). Every in-scope item gets explicit acceptance criteria. Verify no proposed fix violates a project invariant captured in step 0. Produce the fix plan. |
| 4 | Developer | * | SOLVE (isolated branch / worktree). Implement the in-scope fixes, smallest correct change first, matching existing conventions. EVERY fix ships with a regression test that fails before and passes after. No fix touches a forbidden invariant. Report files changed + test status. Parallel across non-overlapping fix clusters. |
| 5 | Feature Tester | * | VERIFY FIXES BY OBSERVATION. Re-run the full test suite. Re-launch the app and OBSERVE that each confirmed issue is actually resolved and nothing regressed (screenshots/traces/logs as evidence). Status language must be 'observed', never 'should work'. |
| 5 | Penetration Tester | skill:pen-test | RE-VERIFY any security fixes with the original PoC; confirm the exploit no longer works. |
| 6 | Project Manager | skill:evaluate_verdict | EVALUATE: PASS / REWORK / SHIP-PARTIAL. On REWORK, loop back to step 4 (max 3 cycles). Do not advance until every in-scope fix is observation-verified or explicitly demoted to DEFERRED with a reason. |
| 7 | Project Manager | skill:log-dev | RECORD: write the architecture-analysis report + dev-log entry (findings, verdicts, fixes, evidence, deferred backlog) to the project's docs and .orrch/. |
| 8 | Repository Manager | skill:commit-review | DELIVER PR — NO MERGE. Group fixes into conventional commits, push the branch, and open a PR whose body contains the full layered report: every confirmed finding, its fix + evidence, and the deferred backlog. EXPLICITLY DO NOT MERGE, do not enable auto-merge, do not delete the branch. The team is DONE only when the PR URL exists; if PR creation fails, retry/repair until a PR is delivered. |

## Summary

A layered hardening pass for an existing codebase. Five independent review
layers (architecture, dependencies, runtime, security, chaos) fan out with
context isolation so they cannot anchor on each other. A mandatory adversarial
verification layer then tries to REFUTE every finding — only confirmed,
reproducible issues survive, which is what keeps the fix phase from chasing
hallucinated problems. Surviving issues are triaged, fixed with paired
regression tests, and re-verified by actually observing the running app (not by
asserting the tests pass).

Two hard rules distinguish this team:

1. **It never merges.** The terminal deliverable is a PR plus a complete
   findings/fix report. Merge is always a separate, human-gated decision.
2. **It does not stop short.** The team runs every layer through to a delivered
   PR; a failed PR creation is retried/repaired rather than abandoned. Findings
   that are out of scope for the PR are recorded as a DEFERRED backlog, never
   silently dropped.

All fixes must respect the target project's durable invariants gathered in
step 0 — the review hardens the code without breaking the rules the project was
built around.
