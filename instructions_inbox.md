# Orrchestrator — Instruction Inbox

<!-- Source: plans/2026-04-30-07-05.md | Confirmed: 2026-04-16 -->

### OPT-001: Fix plan detection and progress ratio display
Audit and fix the plan detection protocol in the Oversee panel. For every project, scan its PLAN.md (and any other recognized plan files) to count total planned features vs completed features. Display this as a ratio (e.g. "12/34") on the project row. Ensure detection runs for ALL projects — the current implementation is inconsistent and misses most projects.

### OPT-002: Fix or remove the "Q: #" counter display
The gold "Q: #" value shown in the Oversee panel is not understood by the user. Investigate what this counter represents (queued prompts? sessions? inbox items?). If it maps to something meaningful and visible, make the label self-explanatory (e.g. "Queued: 3 tasks"). If it cannot be made clearly meaningful, remove it entirely.

### OPT-003: Make project details roadmap list scrollable
In the Oversee > project details panel, the roadmap list must scroll when content exceeds the visible area. Currently the cursor moves off-screen without the list scrolling. Fix scroll so the selected item is always visible.

### OPT-004: Add section-level focus control to project details panel
Restructure the project details panel navigation. Default focus must move between the three top-level sections (Roadmap, Sessions, Files) using Up/Down arrow keys. Pressing Right arrow drills into the focused section (enabling item-level navigation within it). Pressing Left arrow or Escape exits item-level navigation back to section-level. All sections and items must be reachable with arrow keys only — no mouse required.

### OPT-005: Add project logo management
Add logo management for projects. Allow the user to assign an image file as a project's logo. Store the logo path in orrchestrator's project config. Display the logo on the project details page in the Oversee panel. Provide a way to set/change the logo from within the UI.
