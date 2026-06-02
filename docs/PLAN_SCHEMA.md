# PLAN.md Schema

`PLAN.md` files may contain both prose and machine-readable roadmap data. The
parser only tracks roadmap data that follows this schema.

## Roadmap Region

Machine-readable phases and features live inside a roadmap region. A roadmap
region starts at a top-level `##` heading whose text matches one of these names,
case-insensitively:

- `Feature Roadmap`
- `Roadmap`
- `Development Phases`
- `Phases`
- `Plan`
- `Milestones`
- `Critical Path`

Everything outside a roadmap region is prose and is ignored by the parser,
including sections such as `Architecture`, `Open Conflicts`, `Design Decisions`,
`Vision`, and `Tech Stack`.

For backward compatibility with orrchestrator's root `PLAN.md`, a document may
also use implicit roadmap regions. A `## Phase N: ...` heading, or a `###`
heading containing `CRITICAL PATH` or `Cross-Cutting`, starts or continues an
implicit roadmap even when no wrapper heading such as `## Feature Roadmap`
exists. This compatibility path is only for phase-like headings; prose headings
still remain ignored.

## Phases

Inside a roadmap region, any `###` heading starts a phase. A nested `##` heading
also starts a phase when the current roadmap region was opened implicitly by a
phase-like heading.

Phase headings are generic. The parser captures an optional phase number when
the heading starts with `Phase N`; otherwise the heading text becomes the phase
name. These are all valid phase headings inside a roadmap region:

- `### Phase 1: Workspace Init`
- `### FR-001 - Fix de-dup merge 500`
- `### Done`
- `### Planned`
- `### Cross-Cutting: Dev Map`

## Features

A line is a feature only when it appears under a phase in a roadmap region and
matches one of these forms:

- A status-marker line, such as `- [ ] Title`, `1. [x] **Title** - detail`, or
  `CP-1. [v] **Title** - detail`
- A plain list item directly under a phase, such as `- Title` or `1. Title`

Numbered or bulleted lines outside a roadmap region are never features. Prose
sections may freely use lists without being tracked by the Design > Plans panel.

## Status Markers

The parser recognizes these status markers:

| Marker | Status |
|--------|--------|
| `[ ]` | planned |
| `[~]` | implementing |
| `[=]` | implemented |
| `[t]` | testing |
| `[v]` | verified |
| `[✓]` | user confirmed |
| `[x]` | done |

Strikethrough feature text is treated as deprecated. Existing text containing
`DEPRECATED` or `MOVED` may also be interpreted as deprecated for backward
compatibility.

## Canonical Example

```markdown
# Example Project Plan

## Architecture

- Prose bullets here are ignored.

## Feature Roadmap

### Phase 1: Foundation

1. [x] **Project scaffold** - create the workspace
2. [ ] **Config loader** - read settings from disk

### Planned

- **Search filters** - add saved filters to the project browser
- Export current view to CSV

## Open Conflicts

1. **Toolkit selection is unresolved.** This is prose, not a feature.
```

The parser is tolerant of the variants documented above so existing plans do not
need to be rewritten before they can be tracked.
