# Routing Table — Intake 2026-04-23-20-34

All 7 optimized instructions target **orrchestrator** itself. Each item names orrchestrator-internal concepts (Design panel sub-tabs, Workforce tabs, `orrch-tui` panel framework, `orrch-workforce` parser, `library/skills/` loader). No cross-project split needed.

| Instruction | Target Project | Reasoning |
|-------------|---------------|-----------|
| OPT-001 | orrchestrator | Adds a new Design > Plans sub-tab — internal TUI panel + PM integration |
| OPT-002 | orrchestrator | Fixes the skill-name loader in `orrch-library` / Workforce > Skills tab |
| OPT-003 | orrchestrator | Wires Design > Workforce > MCP tab to live MCP server enumeration |
| OPT-004 | orrchestrator | Audits Library and Workforce panels for filesystem-backed rendering |
| OPT-005 | orrchestrator | Adds automatic scroll architecture to `orrch-tui` panel framework |
| OPT-006 | orrchestrator | Removes dead keybind hints across TUI panels |
| OPT-007 | orrchestrator | Adds roundtrip expansion/compression to workflow edit mode in Design > Workforce > Workflows |

**Target inbox**: `/home/corr/projects/orrchestrator/instructions_inbox.md`
