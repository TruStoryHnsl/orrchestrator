# Orrchestrator WebUI — Design Spec

**Date:** 2026-04-17
**Branch:** feat/intentions-dynamic-progress
**Scope:** private

---

## Overview

Add an always-on WebUI to orrchestrator with feature parity to the TUI. Accessible via browser on local network; Tailscale provides authenticated remote access. Ships in two phases within a single new crate: Phase 1 (xterm.js PTY bridge for immediate parity) and Phase 2 (native Vanilla JS UI with master/detail desktop layout and mobile control pad).

---

## Architecture

### New crate: `orrch-webui`

Sits alongside `orrch-webedit` in the workspace. Single public interface:

```rust
WebUiServer::start(port: Option<u16>, app_state: Arc<Mutex<AppState>>) -> WebUiServer
// Returns handle with .port() and .stop()
```

Called from `main.rs` at startup. Binds to `0.0.0.0` (Tailscale is the auth boundary). Port displayed in TUI status bar bottom-right: `⬡ :8742`.

**HTTP framework:** axum — built-in WebSocket upgrade, tokio-native. Server runs in a background `tokio::runtime::Runtime` (same pattern as `orrch-webedit`).

**Static files:** embedded with `include_str!` at compile time. No runtime file I/O, no separate asset server.

### Routes

```
GET  /            → index.html (mode selector: Terminal | UI)
GET  /terminal    → xterm.js PTY view (Phase 1)
GET  /ui          → native Vanilla JS UI (Phase 2)
WS   /pty         → PTY bridge (Phase 1)
WS   /state       → JSON state stream (Phase 2)
POST /action      → keypress / action dispatch (Phase 2)
GET  /static/*    → embedded assets
```

---

## Phase 1 — xterm.js PTY Bridge

### PTY bridge (`/pty` WebSocket)

On connect: spawn `tmux attach-session -t orrchestrator` inside a PTY using the `portable-pty` crate. PTY stdout streams as binary WebSocket frames to xterm.js. Keystrokes from xterm.js send back as binary frames to PTY stdin.

Rationale for `tmux attach` over spawning a new process: orrchestrator already runs in tmux. Attaching gives the browser a shared view of the real running session. Multiple browser tabs = multiple tmux clients, all sharing the same session. TUI keyboard user and web keyboard user coexist.

**Resize protocol:** browser sends text frame `{"type":"resize","cols":N,"rows":N}`. Server calls `tmux resize-window -t orrchestrator -x N -y N`.

**Connection lifecycle:**
- Connect → spawn PTY child with tmux attach → stream
- Disconnect → kill PTY child (tmux detaches cleanly, session stays alive)
- Multiple simultaneous connections: each gets its own PTY child

### Frontend (`/terminal`)

Single HTML file. xterm.js loaded from CDN (or bundled as embedded asset). `FitAddon` for resize. No frameworks, no build step.

---

## Phase 2 — Native Vanilla JS UI

### State stream (`/state` WebSocket)

Server serializes `WebAppState` — a slimmed subset of `App` — as JSON on connect and on every state change. Change detection via hash comparison each tick; only broadcasts on diff. The existing 10s sync loop triggers re-broadcasts when pipeline state updates.

`WebAppState` fields:
```
active_panel
active_sub_panel
ideas:        [{filename, progress, targets, submitted_at}]
sessions:     [{name, category, started, goal}]
projects:     [{name, path, status}]
session_logs: [{name, path, started, goal, attach_cmd}]
```

### Action dispatch (`POST /action`)

Browser posts `{"action":"key","key":"n"}` or named actions like `{"action":"retract","filename":"..."}`. Server dispatches into the same handler functions used by the TUI keyboard handler — no parallel code paths, same Rust functions.

### Frontend structure

```
/ui → ui.html
  js/
    ws.js          — WebSocket subscription + POST dispatch
    layout.js      — panel switching, top/sub nav
    intentions.js  — master/detail intentions panel
    sessions.js    — Hypervise session list
    mobile.js      — mobile control pad
```

State arrives → `ws.js` calls `render(state)` on active panel module → panel updates DOM via targeted `textContent`/`innerHTML` on named elements. No virtual DOM, no framework.

### Desktop layout — Master/Detail

Left pane: scrollable list of items with progress bars and status badges. Right pane: detail view with visible action buttons (Submit, Edit, Retract, etc.). Top nav mirrors TUI top bar (Design / Oversee / Hypervise / Analyze). Sub-nav mirrors TUI sub-bar (Intentions / Workforce / Library / Plans).

Web additions over TUI:
- Click navigation (click any item to select)
- Hover tooltips showing key hints
- Gradient progress bars
- Clickable session log attach commands

### Mobile layout — Overlay control pad

Responsive breakpoint: `≤768px` → mobile layout; `>768px` → desktop master/detail. One HTML file, CSS media query switches layout.

**Control pad:** transparent backdrop-blur panel floating at screen bottom. `⊟` toggle in hint strip slides it into a fixed-height split (TUI area gets remaining height). Split/overlay state persists in `localStorage`.

**Button layout:**

```
[hint strip: context-sensitive action buttons]    [⊟]

[Tab] [Esc]               ↑
[    Enter    ]        ←  ·  →
                          ↓
```

Action strip mirrors the TUI hint bar, updated from `state.active_panel` + `state.active_sub_panel`. Each button dispatches `POST /action`.

**Input handling:** `touchstart` + `touchend` (eliminates 300ms mobile tap delay). Hold behavior: 150ms initial delay, 80ms repeat. Hardware keyboards work alongside the control pad — it is additive, not a replacement.

---

## File Structure

```
crates/orrch-webui/
  Cargo.toml
  src/
    lib.rs        — WebUiServer::start(), WebUiServer::stop()
    pty.rs        — PTY bridge handler
    state.rs      — WebAppState, serialization, change detection
    routes.rs     — axum router, static file embedding
  static/
    index.html
    terminal.html
    ui.html
    js/
      ws.js
      layout.js
      intentions.js
      sessions.js
      mobile.js
    css/
      main.css
```

---

## Integration Points

- `main.rs`: call `WebUiServer::start()` after App init, store handle, display port in status bar
- `crates/orrch-tui/src/ui.rs`: render port in status bar bottom-right
- `App` state: expose `App::web_snapshot() -> WebAppState` — called from the state broadcaster on each tick. Avoids wrapping TUI state in `Arc<Mutex<>>`; the broadcaster holds the snapshot, not a reference to `App`
- `Cargo.toml` (workspace): add `orrch-webui` to members

---

## Out of Scope

- Authentication (Tailscale handles it)
- HTTPS (Tailscale handles it)
- WebRTC / audio
- Drag-and-drop (Phase 2 v1)
- Analyze / Publish panels (placeholders only, same as TUI)
