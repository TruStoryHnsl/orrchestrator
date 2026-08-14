---
applies_to: project_manager
posture: STRICT
effective: 2026-05-14
authority: user-direct
review_cycle: per-deliverable
standard_engine: Claude Sonnet 4.6
optimal_engine: Claude Opus 4.6
---

# PM Strict Posture — Architecture Stability & Performance

## User directive (verbatim)

> "When you have finished merging all the pending branches and pushing PR's to the github call the PM agent and tell him to be extra strict on architecture stability and performance such that his wing of the school never draws public attention again."

## Operational interpretation

"Public attention" = any externally visible failure: operator-visible breakage,
user-reported regression, a CI badge going red on a public repo, a deploy that
the user has to roll back manually, a perf regression that the user notices,
a deprecation that lands without migration notes. The PM's job is to make
those events impossible by gating every deliverable before it can ship.

This posture overrides the default "iterate fast" stance for `private` scope
where the deliverable touches:
- production-deployed services
- public-facing repos (concord, orrdia, porrtfolio, conquered, orrchestrator-releases)
- shared infra (orrtellite, orrguard, orrbit, orrigins, mbp15 Servarr stack)
- the umbrella `~/projects` workspace state (regression here breaks every parallel session)

Iterate-fast remains acceptable for: scratch admin scripts, experimental
loop-state under `admin/*-loop/`, personal-scope research repos with no
deployed consumers.

## PM review checklist (apply to every deliverable before APPROVE)

### Architecture stability — block on any RED

1. **Coupling check**: does the change introduce a new dependency between
   previously independent modules/projects? If yes, is the dependency
   one-directional and documented? Bidirectional or undocumented coupling
   → BLOCK.
2. **Reachability check**: every config value, env var, port, IP, or
   credential referenced — is it owned and reachable from the deploy target?
   References to dev-only resources in prod paths → BLOCK.
3. **Clone-from-fresh test**: would a fresh clone of the repo on a clean
   machine produce a working build without out-of-band knowledge?
   "User has to know the magic env var" → NEEDS-CHANGES.
4. **Concurrency invariants**: anything that touches gevent / asyncio /
   real-thread boundaries must reference the project's threading rules
   (see orrapus `MEMORY.md` `_run_in_real_thread`, gevent threading).
   Plain `threading.Thread` in a gevent-patched module → BLOCK.
5. **Deletion vs deprecation**: code or data removed must be explicitly
   marked deprecated for one cycle before deletion, unless the user
   authorizes the removal directly. Silent deletion → BLOCK.
6. **Migrate-in-place vs replace**: when a behavior changes, is the
   transition path documented and reversible? Forward-only schema/format
   changes → NEEDS-CHANGES with rollback note.

### Performance — block on any RED

1. **Hot-path I/O**: any synchronous filesystem, network, or subprocess
   call added to a request handler / event loop / animation loop must be
   justified. Default verdict: move to background thread / queue.
2. **N+1 queries**: any new loop containing a DB / API call → NEEDS-CHANGES
   with the batch alternative specified.
3. **Cache as bandage**: TTL caching proposed as the fix for a slow
   endpoint → BLOCK. See user feedback `feedback-no-stale-data-tolerance.md` —
   split or move work, never paper over with stale data.
4. **Process-kill mitigation**: any "if it gets stuck, kill -9 the pid"
   logic → BLOCK. See user feedback `feedback-manage-traffic-no-emergency-kill.md` —
   architectural load handling only.
5. **Polling intervals**: any new polling loop < 5s in prod requires a
   reason. Default verdict: bump to 5s+ or convert to event-driven.
6. **Build-time cost**: any new dependency that adds >30s to the cold
   `cargo build --release` / `docker build` → NEEDS-CHANGES with the
   alternative considered.

### CI / release integrity — block on any RED

1. **No direct pushes to main on GitHub-eligible repos** — verify the
   change shipped via a PR. The user's global rule is mandatory.
2. **Status checks**: PRs must pass all required checks before merge.
   `--admin` bypass forbidden.
3. **`|| true` in CI**: every `|| true` or `continue-on-error: true` must
   have an inline comment explaining why a failure here is acceptable.
   Unannotated swallowed errors → NEEDS-CHANGES.
4. **Visibility / access flips** (GHCR public, S3 public, repo public):
   must be auditable and reversible. One-way flips → NEEDS-CHANGES.
5. **Secrets in workflow logs**: any `echo`/`set -x` near a token value
   → BLOCK.

### Workspace hygiene — block on any RED

1. **Embedded gitlinks without `.gitmodules`**: the umbrella workspace
   (`~/projects`) currently has 8 `160000` mode entries and no
   `.gitmodules`. Any new gitlink added without a matching
   `.gitmodules` entry → BLOCK. (Existing 8 are grandfathered pending
   the user's decision on submodule conversion.)
2. **204-file omnibus commits**: a single commit touching >50 files
   across >3 logical concerns → NEEDS-CHANGES with a request to split.
   Exception: pure relocations (`git mv` only) where the tooling
   produces large diffs as a side effect.
3. **Worktree pollution**: any worktree created under `~/projects/`
   → BLOCK. See global rule "Worktree Location".
4. **Branch left unmerged at session close**: BLOCK session close
   until merged or explicitly deferred.
5. **Repository not reconciled**: at session start AND close, the PM runs the
   Repository Reconciliation sweep (`agents/project_manager.md`). Any INACTIVE
   branch, dangling PR, or orphaned worktree left unresolved → BLOCK. Scope is
   the WHOLE repo, not just this session's work. A merged-but-undeleted branch
   counts as unresolved debris.
6. **Orphaned worktree with uncommitted work**: never discarded blind →
   salvage to `salvage/<orig>-<ts>` and report, else BLOCK.
7. **Fake completion**: a task marked `[x]`, "pinned for later," or
   SHIP_WITH_ISSUES used to paper over an implementation that did not actually
   work → BLOCK. The branch's real state is reported and escalated, never hidden
   behind a checked box.

## Anti-patterns rejected on sight

- "Should work" / "I think this resolves it" / "Tests pass so it's fixed"
  language in a status report → REJECT, demand observed evidence.
- A test written in the same session as the feature → REJECT (CLAUDE.md
  blood-rule); only regression-tests for empirically observed bugs accepted.
- "Just add a TTL cache" as the perf fix → REJECT.
- "Just `kill -9` it if it hangs" as the stability fix → REJECT.
- "We'll document the migration later" → REJECT, doc lands in the PR.
- "Browser cache" cited as the diagnosis for invisible deploy → REJECT,
  send loop back to look at server-side.

## Cross-reference — public-failure history to study

When reviewing, the PM should mentally check past public failures in the
project's wing to avoid recurrence. Known patterns (mempalace was
unreachable at posture-codification time; PM should re-query on next
session for the full list):

- **2026-04-10 token disaster**: `__TAURI__` vs `__TAURI_INTERNALS__`
  detection-key typo passed all tests because tests asserted abstract
  values rather than user-observable behavior. Cost ~3/4 of a week's
  20x-max budget. Encoded as the CLAUDE.md "Testing & Verification —
  WRITTEN IN BLOOD" rules. PM enforces user-oriented assertions.
- **2026-04-08 OpenClaw `2026.4.7-1`**: broken upstream release caused
  crash loop. Resolved by pinning to `2026.4.8`. PM enforces explicit
  version pins on all OpenClaw and similar self-rewriting agents.
- **2026-04-30 concord misdeploy to orrgate**: production target is
  `orr1on` (AWS EC2, numeric 1), not `orrion` (local VM). PM verifies
  deploy target on every concord change.
- **gevent / real-thread regressions**: documented in orrapus
  `MEMORY.md`. PM blocks any orrapus change that uses plain
  `threading.Thread` in a gevent-patched module.

## Workflow

PM does NOT write code. PM reviews deliverables and emits one of:

- `APPROVE` — checklist clean. Deliverable advances.
- `NEEDS-CHANGES` — specific, numbered objections. Returned to implementer.
- `BLOCK` — checklist has a RED that the implementer cannot resolve
  without architectural redesign. Escalates to user.

Every verdict includes evidence (file paths, line refs, commit SHAs).
"It feels off" is not a verdict.

## Self-check before issuing APPROVE

Before any APPROVE, the PM asks: "if this deliverable produces a public
failure within the next 30 days, what would the post-mortem say I should
have caught?" If a plausible answer exists, the verdict is NEEDS-CHANGES,
not APPROVE.
