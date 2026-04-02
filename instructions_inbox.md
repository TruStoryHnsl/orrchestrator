# Orrchestrator — Instruction Inbox

## Package: UI Polish & Infrastructure Fixes
Source: plans/2026-04-16-00-47.md (ORRCHESTRATOR - FEEDBACK)
Processed: 2026-03-31
Instructions: 9

---

### INS-001: Responsive tab bar width
The 5 top-level tabs must compress gracefully when the terminal window is narrow. Implement: abbreviated labels when width < threshold (e.g., "Des" "Ove" "Hyp" "Ana" "Pub"), then icon-only or colored blank tabs at extreme widths. The tab bar should never extend beyond the window.

### INS-002: Fix Workforce sub-tab navigation
The second-level tab bar in Design > Workforce is not navigable. The Tab key must cycle through all 9 workforce sub-tabs (Workflows, Teams, Agents, Skills, Tools, MCP, Profiles, Training, Models). The cursor currently skips past them. Verify that Tab advances the workforce_tab index and the UI renders the selection highlight.

### INS-003: Left-justify Library sub-panel
Move the Library sub-panel back to left-justified alignment with Intentions and Workforce in the Design sub-bar. Remove the right-justification logic.

### INS-004: Add Harnesses editor as leftmost Workforce tab
Add a "Harnesses" tab as the first tab in Design > Workforce (before Workflows). This editor will eventually scrape source code from open-source harnesses (Claude Code, OpenCode, Crush, Codex, Gemini) to catalogue their built-in modules, tools, and features. For now: placeholder page that lists harness source directories and links to their repos. Long-term: visual harness aggregator that indexes features across harnesses for a custom fork.

### INS-005: Rich markdown preview renderer
Build a TUI rendering layer that displays .md files with rich formatting (headers, bold, lists, code blocks, links) instead of raw text. Research existing implementations: yazi's image protocol, mdcat, glow, bat. This renderer replaces the current plaintext preview pane in Library, Workforce, and Intentions panels. Must be scrollable and embeddable as a widget.

### INS-006: Fix orphaned tmux sessions on exit
Orrchestrator is not cleaning up tmux sessions when it exits. Implement: on quit, enumerate all orrchestrator-managed tmux windows and kill them. On startup, detect orphaned sessions from a previous run and offer to clean them up. Store managed session names in a state file for cross-run tracking.

### INS-007: Custom tmux status bar for managed sessions
Create a custom tmux status bar for the "orrch" tmux session that displays: window name, busy/waiting/idle status with color coding, sorted by urgency (waiting first). Add a custom hotkey that jumps to the most urgent window. Pair with tmux's built-in next-window for efficient navigation between Claude sessions.

### INS-008: Unified vim/nvim tmux window
All vim/nvim editing sessions should open in a single tmux window with the custom status bar. Users can split off individual vim sessions into their own windows; orrchestrator tracks the change and represents lone windows in the Intentions menu. The tmux+vim window should feel like a native orrchestrator editing interface.

### INS-009: Instruction audit trail with hash coordinates
When the COO splits user feedback into discrete instruction packages, each instruction must be indexed with a hash derived from the coordinate data (line range, character offsets) of the source text chunk. This creates a bidirectional audit trail: user can trace any instruction back to the exact text that spawned it, and can see how their words were interpreted/optimized. Store the audit log in each project's `.feedback/audit.jsonl`. Display the translation mapping in the Intentions panel when an idea is expanded.
