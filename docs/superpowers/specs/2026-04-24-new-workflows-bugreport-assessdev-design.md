# Design — `intake_bugreport` + `assess_development` Workflows

**Date:** 2026-04-24
**Scope:** Add two new operations to the orrchestrator workforce system and integrate them into all three existing workforces.

## Goals

1. **`intake_bugreport`** — Process user-submitted text/file as a bug report explicitly (not a feature/idea). Route to a per-project `bugs_inbox.md`, append a pending entry to `.bugfix-ledger.md`, and create a fix task in `PLAN.md`.
2. **`assess_development`** — Audit claimed-complete features against actual code. Reconcile `PLAN.md` to match reality. Available as a spawnable session from the Oversee panel.

## Non-goals

- No automatic fix/commit of audited bugs (that's `develop_feature`'s job).
- No changes to the `develop_feature` operation.
- No full Oversee-panel UI rewiring — provide the dispatcher script; the TUI wiring is a follow-up.

## Architecture

### `intake_bugreport` (9 steps)

```
1 | Executive Assistant | skill:triage      | classify input as a bug report (vs idea/feature/query); pass raw text to COO
2 | Beta Tester         | skill:reproduce   | attempt reproduction in sandbox; record observed vs expected, env, severity
3 | Chief Operations Officer | skill:bug-parse | structure report (symptoms, repro, affected components, severity, env)
4 | Chief Operations Officer | tool:write-review | write raw + structured to review.json (pending)
5 | Hypervisor          | *                 | BLOCKING — user reviews in Design > Intentions; sets review.json status
6 | Chief Operations Officer | skill:parse   | determine target project from symptoms/components (reads project CLAUDE.md)
7 | Chief Operations Officer | tool:route-bug | append to project bugs_inbox.md as BUG-NNN entry
8 | Repository Manager  | tool:ledger-append | append pending entry to project .bugfix-ledger.md
9 | Project Manager     | skill:synthesize_instructions | incorporate fix task into PLAN.md referencing BUG-NNN
```

**Separate from `INSTRUCTION INTAKE`** — bugs live in `bugs_inbox.md`, not `instructions_inbox.md`. Queryable separately; different lifecycle (bugs stay open until verified-fixed).

**Severity is inferred by COO at parse time** from user text. The user-audit gate (step 5) lets the user correct severity before routing.

### `assess_development` (7 steps; step 2 runs 3-way parallel)

```
1 | Project Manager    | skill:read-plan       | enumerate [x]-complete and in-progress plan items
2 | Feature Tester     | skill:verify-claim    | per claimed-complete item: file/symbol exists, behavior matches desc
2 | Developer          | skill:codebase-audit  | scan code for undeclared features (present in code, absent in plan)
2 | Researcher         | skill:gap-audit       | flag plan items referencing missing files/modules/functions
3 | Beta Tester        | skill:exercise        | live-exercise top-N user-facing claimed-complete features
4 | Project Manager    | skill:reconcile-plan  | merge findings: downgrade false-complete → partial, promote undocumented → tracked
5 | Project Manager    | skill:log-audit       | write .orrch/assess_report_<timestamp>.md
6 | Repository Manager | mcp:github            | commit updated PLAN.md with `docs(plan): reconcile state`
7 | Repository Manager | skill:merge-to-main   | MANDATORY merge
```

Three audit angles run parallel at step 2 so each sees the plan cold without peer influence (context-isolation principle). Step 3 exercises the top-N user-facing features — keeps token cost bounded on large plans.

### New skill files (library/skills/)

| File | Purpose |
|---|---|
| `bug-parse.md` | Structure raw bug text into report fields |
| `reproduce.md` | Attempt reproduction; record observed vs expected |
| `read-plan.md` | Enumerate claimed/in-progress plan items |
| `verify-claim.md` | Verify a single plan item's claim against code/tests |
| `codebase-audit.md` | Find features in code that aren't tracked in plan |
| `gap-audit.md` | Find plan items referencing missing artifacts |
| `exercise.md` | Live-exercise a feature, observe user-facing behavior |
| `reconcile-plan.md` | Merge audit findings into PLAN.md |
| `log-audit.md` | Write an assessment report to disk |

### New tool files (library/tools/)

| File | Purpose |
|---|---|
| `route_bug.sh` | Append BUG-NNN entry to `<project>/bugs_inbox.md`; auto-increment |
| `ledger_append.sh` | Append pending entry to `<project>/.bugfix-ledger.md` in existing format |
| `spawn_operation.sh` | Dispatcher to spawn any operation as a session (used by Oversee panel) |

### Workforce wiring

All three workforces gain both operations:

- `general_software_development.md` — add to `operations:` list (all required agents already present)
- `commercial_software_development.md` — add to `operations:` list (all required agents already present)
- `personal_tech_support.md` — add to `operations:` list AND add missing agents (`coo`, `pm`, `rm`, `bt`). Pre-existing bug: its current `INSTRUCTION INTAKE` operation references agents missing from its table.

### Oversee panel integration

Minimum viable: `spawn_operation.sh <operation-name> <project-dir>` — reads the operation file, sets up `${ORRCH_DIR}`, launches a dispatcher session.

Full TUI wiring (adding an Oversee menu entry) is deferred. Documented as a follow-up item — no blocker for the workflow itself.

## Data model additions

### `bugs_inbox.md` (per project)

```markdown
# Bugs Inbox

---

### BUG-2026-04-24-001 — <short title>

**Severity:** high
**Reported:** 2026-04-24
**Source idea:** <link>
**Status:** pending

#### Symptoms
...

#### Reproduction
...

#### Affected
...
```

### `.bugfix-ledger.md` (existing format, unchanged)

Intake appends a "pending" entry with no Solution field populated. `develop_feature` later updates it when the bug is fixed.

## Testing

- Operation markdown parses cleanly via `parse_operation_markdown` (unit tests exist in `crates/orrch-workforce/src/parser.rs`).
- Workforce files parse cleanly via `parse_workforce_markdown`.
- `route_bug.sh` and `ledger_append.sh` covered by shell-level smoke tests (create inbox if missing, auto-increment BUG-NNN, format matches existing ledger template).

## Risks / Trade-offs

- **Severity inference at intake:** COO may misjudge; mitigated by the user audit gate (step 5).
- **Step 3 of assess_development runs Beta Tester unconditionally:** costs extra tokens per audit. Scoped to top-N user-facing items to bound cost.
- **`personal_tech_support` bloat:** adding coo+pm+rm+bt changes its lightweight character. Acceptable because its existing `INSTRUCTION INTAKE` already requires them; we're making the declared truth match what the operations demand.

## Out of scope (future)

- Automatic BUG-NNN → PLAN.md linking bidirectional sync
- Assess cadence automation (every-N-cycles trigger)
- Full Oversee panel menu entry for spawnable ops
