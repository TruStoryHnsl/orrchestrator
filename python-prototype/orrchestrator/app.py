"""Orrchestrator — AI development pipeline hypervisor."""

from __future__ import annotations

import asyncio
import os
from pathlib import Path

from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Container, Horizontal, Vertical, VerticalScroll
from textual.reactive import reactive
from textual.screen import ModalScreen
from textual.widgets import (
    DataTable,
    Footer,
    Header,
    Input,
    Label,
    ListItem,
    ListView,
    Static,
)

from .process_manager import ExternalSession, ProcessManager, Session, SessionState
from .retrospect import ErrorStore, SessionAnalyzer, SolutionTracker
from .terminal_widget import TerminalView

PROJECTS_DIR = Path.home() / "projects"


# ─── Project Picker Modal ────────────────────────────────────────────


class ProjectPicker(ModalScreen[str | None]):
    """Modal to select a project directory for a new session."""

    DEFAULT_CSS = """
    ProjectPicker {
        align: center middle;
    }
    #picker-dialog {
        width: 60;
        height: auto;
        max-height: 80%;
        background: $surface;
        border: tall $accent;
        padding: 1 2;
    }
    #picker-title {
        text-align: center;
        text-style: bold;
        margin-bottom: 1;
    }
    #picker-list {
        height: auto;
        max-height: 20;
    }
    #picker-input {
        margin-top: 1;
    }
    """

    BINDINGS = [
        Binding("escape", "cancel", "Cancel"),
    ]

    def __init__(self) -> None:
        super().__init__()
        self._projects: list[Path] = []
        if PROJECTS_DIR.is_dir():
            self._projects = sorted(
                p for p in PROJECTS_DIR.iterdir()
                if p.is_dir() and not p.name.startswith(".") and p.name != "deprecated"
            )

    def compose(self) -> ComposeResult:
        with Vertical(id="picker-dialog"):
            yield Label("Select Project", id="picker-title")
            yield Input(placeholder="Filter or enter path...", id="picker-input")
            yield ListView(
                *[ListItem(Label(p.name), name=str(p)) for p in self._projects],
                id="picker-list",
            )

    @on(ListView.Selected, "#picker-list")
    def on_project_selected(self, event: ListView.Selected) -> None:
        path = event.item.name
        if path:
            self.dismiss(path)

    @on(Input.Submitted, "#picker-input")
    def on_input_submit(self, event: Input.Submitted) -> None:
        value = event.value.strip()
        if not value:
            return
        candidate = PROJECTS_DIR / value
        if candidate.is_dir():
            self.dismiss(str(candidate))
        elif Path(value).is_dir():
            self.dismiss(value)

    def action_cancel(self) -> None:
        self.dismiss(None)


# ─── Focus Screen (maximized session) ────────────────────────────────


class FocusScreen(ModalScreen):
    """Full-screen view of a single Claude session's terminal."""

    DEFAULT_CSS = """
    FocusScreen {
        background: #0a0a1a;
    }
    #focus-terminal {
        width: 100%;
        height: 100%;
    }
    #focus-bar {
        dock: bottom;
        height: 1;
        background: $accent;
        color: $text;
        text-align: center;
    }
    """

    BINDINGS = [
        Binding("escape", "unfocus", "Return to dashboard"),
    ]

    def __init__(self, session: Session, pm: ProcessManager, **kwargs) -> None:
        super().__init__(**kwargs)
        self.session = session
        self.pm = pm

    def compose(self) -> ComposeResult:
        yield TerminalView(rows=40, cols=120, id="focus-terminal")
        yield Static(
            f" {self.session.display_name} [{self.session.sid}] — Esc to return ",
            id="focus-bar",
        )

    def on_mount(self) -> None:
        terminal = self.query_one("#focus-terminal", TerminalView)
        # Feed existing buffer
        if self.session.output_buffer:
            terminal.feed(bytes(self.session.output_buffer))
        # Start reading new output
        self._read_task = asyncio.ensure_future(self._follow_output(terminal))

    async def _follow_output(self, terminal: TerminalView) -> None:
        last_len = len(self.session.output_buffer)
        while self.session.state != SessionState.DEAD:
            await asyncio.sleep(0.1)
            current_len = len(self.session.output_buffer)
            if current_len > last_len:
                new_data = bytes(self.session.output_buffer[last_len:current_len])
                terminal.feed(new_data)
                last_len = current_len

    def on_key(self, event) -> None:
        if event.key == "escape":
            return  # handled by binding
        # Forward keystrokes to the session
        if event.character:
            self.pm.write_to_session(self.session.sid, event.character.encode())
        elif event.key == "enter":
            self.pm.write_to_session(self.session.sid, b"\r")
        elif event.key == "backspace":
            self.pm.write_to_session(self.session.sid, b"\x7f")
        elif event.key == "tab":
            self.pm.write_to_session(self.session.sid, b"\t")

    def action_unfocus(self) -> None:
        if hasattr(self, "_read_task"):
            self._read_task.cancel()
        self.dismiss()


# ─── Session Table ────────────────────────────────────────────────────


class SessionTable(DataTable):
    """Table displaying all sessions (managed + external)."""

    DEFAULT_CSS = """
    SessionTable {
        height: auto;
        max-height: 50%;
    }
    """


# ─── Main App ────────────────────────────────────────────────────────


class OrrchApp(App):
    """Orrchestrator — AI development pipeline hypervisor."""

    TITLE = "orrchestrator"
    SUB_TITLE = "AI pipeline hypervisor"

    CSS = """
    #dashboard {
        height: 100%;
    }
    #header-bar {
        dock: top;
        height: 3;
        background: #16213e;
        padding: 0 2;
    }
    #header-title {
        text-style: bold;
        color: #e94560;
    }
    #header-stats {
        color: #a8a8b8;
        text-align: right;
    }
    #session-panel {
        height: 1fr;
        border: solid #333;
        padding: 0 1;
    }
    #session-title {
        text-style: bold;
        margin-bottom: 1;
    }
    #info-panel {
        height: auto;
        max-height: 40%;
        border: solid #333;
        padding: 1;
    }
    #info-content {
        color: #a8a8b8;
    }
    #no-sessions {
        text-align: center;
        color: #666;
        margin: 2;
    }
    """

    BINDINGS = [
        Binding("n", "new_session", "New session"),
        Binding("k", "kill_session", "Kill session"),
        Binding("enter", "focus_session", "Focus session"),
        Binding("r", "refresh_external", "Refresh"),
        Binding("d", "remove_dead", "Remove dead"),
        Binding("q", "quit", "Quit"),
    ]

    session_count = reactive(0)

    def __init__(self) -> None:
        super().__init__()
        self.pm = ProcessManager()
        # Retrospect: per-project error stores and per-session analyzers
        self._error_stores: dict[str, ErrorStore] = {}
        self._solution_trackers: dict[str, SolutionTracker] = {}
        self._analyzers: dict[str, SessionAnalyzer] = {}  # session_id → analyzer
        self._error_count = 0

    def compose(self) -> ComposeResult:
        yield Header()
        with Vertical(id="dashboard"):
            with Horizontal(id="header-bar"):
                yield Label("⚡ orrchestrator", id="header-title")
                yield Label("0 sessions", id="header-stats")
            with Vertical(id="session-panel"):
                yield Label("Sessions", id="session-title")
                table = SessionTable(id="session-table")
                table.cursor_type = "row"
                yield table
                yield Label(
                    "No active sessions. Press [bold]n[/bold] to spawn one.",
                    id="no-sessions",
                )
            with Vertical(id="info-panel"):
                yield Label("", id="info-content")
        yield Footer()

    def on_mount(self) -> None:
        table = self.query_one("#session-table", SessionTable)
        table.add_columns("", "ID", "Project", "State", "Uptime", "Type")
        self.refresh_table()
        # Start periodic refresh
        self.set_interval(2.0, self._periodic_refresh)
        # Initial external scan
        self.discover_external()

    @work(exclusive=True, group="discover")
    async def discover_external(self) -> None:
        await self.pm.discover_external()
        self.refresh_table()

    async def _periodic_refresh(self) -> None:
        # Check for dead processes
        for session in self.pm.sessions:
            if session.state != SessionState.DEAD:
                try:
                    os.waitpid(session.pid, os.WNOHANG)
                except ChildProcessError:
                    session.state = SessionState.DEAD
        self.refresh_table()

    def refresh_table(self) -> None:
        table = self.query_one("#session-table", SessionTable)
        no_sessions = self.query_one("#no-sessions", Label)
        stats = self.query_one("#header-stats", Label)

        table.clear()

        managed = self.pm.sessions
        external = self.pm.external_sessions

        total = len(managed) + len(external)
        self.session_count = total

        if total == 0:
            no_sessions.display = True
            table.display = False
            stats.update("0 sessions")
            return

        no_sessions.display = False
        table.display = True

        for s in managed:
            table.add_row(
                s.state_icon,
                s.sid,
                s.display_name,
                s.state.value,
                s.uptime,
                "managed",
                key=s.sid,
            )

        for ext in external:
            table.add_row(
                "👁",
                str(ext.pid),
                ext.display_name,
                "running",
                "",
                "[external]",
                key=f"ext-{ext.pid}",
            )

        working = sum(1 for s in managed if s.state == SessionState.WORKING)
        waiting = sum(1 for s in managed if s.state == SessionState.WAITING)

        parts = [f"{total} session{'s' if total != 1 else ''}"]
        if working:
            parts.append(f"{working} working")
        if waiting:
            parts.append(f"{waiting} waiting")
        if external:
            parts.append(f"{len(external)} external")
        stats.update(" | ".join(parts))

        # Update info panel
        self._update_info()

    def _update_info(self) -> None:
        info = self.query_one("#info-content", Label)
        table = self.query_one("#session-table", SessionTable)

        if not table.row_count:
            info.update("")
            return

        row_key, _ = table.coordinate_to_cell_key(table.cursor_coordinate)
        sid = str(row_key)

        if sid.startswith("ext-"):
            pid = int(sid[4:])
            for ext in self.pm.external_sessions:
                if ext.pid == pid:
                    info.update(
                        f"[bold]External session[/bold]\n"
                        f"PID: {ext.pid}\n"
                        f"Dir: {ext.project_dir}\n"
                        f"Cmd: {ext.cmdline[:100]}"
                    )
                    return
        else:
            session = self.pm.get_session(sid)
            if session:
                buf_size = len(session.output_buffer)
                lines = [
                    f"[bold]{session.display_name}[/bold] [{session.sid}]",
                    f"PID: {session.pid} | State: {session.state.value}",
                    f"Dir: {session.project_dir}",
                    f"Buffer: {buf_size:,} bytes | Uptime: {session.uptime}",
                ]
                # Show Retrospect stats for this project
                store = self._error_stores.get(session.project_dir)
                if store:
                    st = store.stats()
                    if st["total_occurrences"] > 0:
                        lines.append(
                            f"Retrospect: {st['unique_errors']} unique errors "
                            f"({st['resolved']} resolved, {st['unresolved']} open)"
                        )
                info.update("\n".join(lines))
                return

        info.update("")

    @on(DataTable.RowHighlighted)
    def on_row_highlighted(self, event) -> None:
        self._update_info()

    # ─── Actions ──────────────────────────────────────────────────

    def action_new_session(self) -> None:
        self.push_screen(ProjectPicker(), self._on_project_picked)

    def _get_retrospect(self, project_dir: str) -> tuple[ErrorStore, SolutionTracker]:
        """Get or create the error store and solution tracker for a project."""
        if project_dir not in self._error_stores:
            store = ErrorStore(project_dir)
            tracker = SolutionTracker(store)
            self._error_stores[project_dir] = store
            self._solution_trackers[project_dir] = tracker
        return self._error_stores[project_dir], self._solution_trackers[project_dir]

    def _on_project_picked(self, project_dir: str | None) -> None:
        if not project_dir:
            return
        session = self.pm.spawn(
            project_dir=project_dir,
            on_output=self._on_session_output,
        )
        # Create a Retrospect analyzer for this session
        store, tracker = self._get_retrospect(project_dir)
        self._analyzers[session.sid] = SessionAnalyzer(
            session_id=session.sid,
            project_dir=project_dir,
            store=store,
            tracker=tracker,
        )
        self.refresh_table()
        self.notify(f"Spawned session in {Path(project_dir).name}")

    async def _on_session_output(self, session: Session, data: bytes) -> None:
        """Route session output through Retrospect error analysis."""
        analyzer = self._analyzers.get(session.sid)
        if analyzer:
            new_errors = analyzer.feed(data)
            for err in new_errors:
                self._error_count += 1
                known_fix = analyzer.get_known_fix(err.fingerprint)
                if known_fix:
                    self.notify(
                        f"Known error in {session.display_name}: {err.category} "
                        f"(fix available)",
                        severity="warning",
                    )
                else:
                    self.notify(
                        f"New error in {session.display_name}: {err.category}",
                        severity="error",
                    )

    def action_kill_session(self) -> None:
        table = self.query_one("#session-table", SessionTable)
        if not table.row_count:
            return
        row_key, _ = table.coordinate_to_cell_key(table.cursor_coordinate)
        sid = str(row_key)
        if sid.startswith("ext-"):
            self.notify("Cannot kill external sessions", severity="warning")
            return
        session = self.pm.get_session(sid)
        if session:
            # Notify solution tracker that session ended
            analyzer = self._analyzers.pop(sid, None)
            if analyzer:
                tracker = self._solution_trackers.get(session.project_dir)
                if tracker:
                    tracker.on_session_end(sid)
            self.pm.kill_session(sid)
            self.notify(f"Killed {session.display_name} [{sid}]")
            self.refresh_table()

    def action_focus_session(self) -> None:
        table = self.query_one("#session-table", SessionTable)
        if not table.row_count:
            return
        row_key, _ = table.coordinate_to_cell_key(table.cursor_coordinate)
        sid = str(row_key)
        if sid.startswith("ext-"):
            self.notify("External sessions can't be focused here", severity="warning")
            return
        session = self.pm.get_session(sid)
        if session and session.state != SessionState.DEAD:
            self.push_screen(FocusScreen(session, self.pm))

    def action_refresh_external(self) -> None:
        self.discover_external()
        self.notify("Scanning for external sessions...")

    def action_remove_dead(self) -> None:
        removed = self.pm.remove_dead()
        if removed:
            self.notify(f"Removed {len(removed)} dead session(s)")
            self.refresh_table()
        else:
            self.notify("No dead sessions to remove")

    def on_unmount(self) -> None:
        self.pm.cleanup()


def main() -> None:
    app = OrrchApp()
    app.run()


if __name__ == "__main__":
    main()
