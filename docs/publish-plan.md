# Publish Panel — Phase 9 Design Plan

## Overview

The **Publish** panel (`Panel::Publish`) is the fifth top-level panel in orrchestrator, providing a complete release pipeline UI. It is split into five sub-tabs: Packaging, Distribution, Compliance, Marketing, and History.

This document records architecture decisions, data flow, and extension points for Phase 9 of the orrchestrator roadmap.

---

## Sub-Tab Responsibilities

### Packaging
- Shows release notes generated from conventional commits since the last git tag
- Runs a pre-release checklist (CHANGELOG exists, no .env files, Cargo version present, working tree clean)
- Detects build targets from project structure (Cargo.toml → cargo build --release, pyproject.toml → python build, package.json → npm run build, Dockerfile → docker build)
- Displays build results per target with ✓/✗/⏳ status
- **Keybinds**: `v` = preview next version changelog (patch bump, does NOT tag), `b` = run all build targets, `r` = refresh, Left/Right = switch tabs

### Distribution
- Will show platform-specific distribution channels: GitHub Releases, crates.io, PyPI, npm, Docker Hub
- Each platform shows connection status, last publish date, publish action
- Stub: `publish_to_platform(project_dir, platform)` in release.rs

### Compliance
- License scanning via `compliance::scan_licenses(project_dir)` — parses Cargo.lock, maps crate names to SPDX IDs via embedded table (no network)
- Copyright header scanning via `compliance::check_copyright(project_dir)` — walks .rs/.py/.ts files, checks first 10 lines for `SPDX-License-Identifier:` or `Copyright`
- Two tables: top = dependency licenses (name, SPDX, status: OK/COPYLEFT/?), bottom = copyright coverage (% covered, list of missing files)

### Marketing
- Generates `MarketingDraft { description, highlights, badges, social_post }` from project metadata and release notes
- Planned: LLM-assisted draft generation from commits + PLAN.md
- Stub: marketing.rs or extend release.rs

### History
- Lists all git tags with their annotations and dates
- Shows download counts from crates.io / PyPI APIs (planned)
- Planned: `poll_release_stats(project_dir) -> ReleaseStats`
- Planned: `rollback_release(project_dir, version)` with confirmation modal

---

## Data Flow

```
project_dir (orrchestrator/)
    │
    ├─► release.rs
    │       generate_release_notes()     → app.release_notes_preview
    │       run_checklist()              → app.checklist_results
    │       detect_build_targets()       → app.build_targets
    │       build_artifact()             → app.build_results
    │       next_version_string()        → version string for preview
    │       generate_changelog_entry()   → formatted changelog section
    │       bump_version()               → git annotated tag (writes to repo)
    │
    └─► compliance.rs
            scan_licenses()             → app.license_report
            check_copyright()           → app.copyright_report
```

### Rendering pipeline

```
draw_publish(frame, app, area)
    ├── tab bar (Left/Right to switch)
    ├── auto-refresh on first render per tab
    │       Packaging  → refresh_packaging_data()
    │       Compliance → refresh_compliance_data()
    └── dispatch:
            Packaging   → draw_packaging_tab()
            Compliance  → draw_compliance_tab()
            others      → draw_placeholder()
```

---

## Key Source Files

| File | Role |
|------|------|
| `crates/orrch-core/src/release.rs` | Version tagging, changelog generation, build target detection and execution |
| `crates/orrch-core/src/compliance.rs` | License scanning (Cargo.lock), copyright header scanning |
| `crates/orrch-tui/src/app.rs` | App state: `publish_tab`, `release_notes_preview`, `checklist_results`, `build_targets`, `build_results`, `license_report`, `copyright_report` |
| `crates/orrch-tui/src/ui.rs` | `draw_publish`, `draw_packaging_tab`, `draw_compliance_tab` |

---

## Architecture Decisions

### No network in compliance scanning
License data comes from an embedded `HashMap<&str, &str>` in `compliance.rs`. This avoids network dependency and keeps the TUI snappy. Unknown crates show `Unknown` / `?` status. The table can be extended as new crates are added to the codebase.

### Synchronous builds
`build_artifact()` runs synchronously when `b` is pressed. For a TUI with a 60Hz render loop this causes a short freeze but avoids the complexity of background threads + channel wiring. Future improvement: spawn a thread, send results over `mpsc`, poll in the event loop.

### bump_version does NOT auto-tag
The `v` keybind only previews the changelog; it does NOT call `bump_version()`. Actual tagging is a destructive git operation and requires explicit user intent. Future: add a confirmation modal before tagging.

### Compliance tab refreshes lazily
`refresh_compliance_data()` is called only when the Compliance tab is first displayed (or on `r`). For large codebases the Cargo.lock walk can take ~100ms; lazy loading keeps the initial render fast.

### BumpKind is patch-only from TUI
The `v` key always previews a patch bump. Future: sub-menu or secondary keybind (`V` for minor, `Ctrl+V` for major) to choose bump kind before previewing/tagging.

---

## Extension Points

1. **Platform distribution** (PLAN item 101): add `publish_to_platform(project_dir, platform: Platform)` in `release.rs` returning a `PlatformResult`. Wire `p` key in Distribution tab to trigger.

2. **Marketing draft** (PLAN item 105): add `marketing.rs` with `generate_marketing_draft(project_dir, version) -> MarketingDraft`. Call Claude API if available, fall back to template. Display in Marketing tab.

3. **Release stats polling** (PLAN item 107): add `poll_release_stats(project_dir)` that hits crates.io JSON API (`https://crates.io/api/v1/crates/<name>`). Display in History tab alongside git tag list.

4. **Rollback** (PLAN item 108): add `rollback_release(project_dir, version)` — soft-delete tag with `git tag -d`, push `git push --delete origin <tag>`, write advisory to CHANGELOG. Requires confirmation modal (reuse the existing `SubView` pattern).

5. **Async builds**: replace synchronous `build_artifact()` call with `std::thread::spawn` + `mpsc::channel`. Store `build_rx: Option<Receiver<BuildResult>>` on App, drain in `App::tick()`.

6. **Multi-project support**: currently hardcoded to `projects_dir/orrchestrator`. Add a project selector (Left/Right in Packaging, similar to Plans panel) to switch the active release subject.

---

## Status

| Item | Status |
|------|--------|
| 99 — Build artifacts | ✓ complete |
| 100 — Version tagging + changelog | ✓ complete (preview; tagging needs confirmation modal) |
| 101 — Platform distribution | stub (placeholder tab) |
| 102 — License compliance | ✓ complete |
| 103 — Copyright verification | ✓ complete (merged into compliance.rs) |
| 105 — Marketing generation | stub (placeholder tab) |
| 107 — Post-release monitoring | stub (placeholder tab) |
| 108 — Rollback capability | stub (planned) |
| 95 — This document | ✓ complete |
