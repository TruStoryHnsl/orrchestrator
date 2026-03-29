"""Process manager for Claude Code sessions.

Spawns Claude Code instances in PTYs, tracks their state,
and discovers external (unmanaged) Claude processes.
"""

from __future__ import annotations

import asyncio
import enum
import fcntl
import os
import pty
import signal
import struct
import termios
import time
from dataclasses import dataclass, field
from pathlib import Path


class SessionState(enum.Enum):
    WORKING = "working"
    WAITING = "waiting"
    IDLE = "idle"
    DEAD = "dead"


@dataclass
class Session:
    sid: str
    project_dir: str
    pid: int
    fd: int  # PTY master fd
    state: SessionState = SessionState.IDLE
    managed: bool = True
    started_at: float = field(default_factory=time.time)
    output_buffer: bytearray = field(default_factory=bytearray)
    _last_output_time: float = 0

    @property
    def display_name(self) -> str:
        return Path(self.project_dir).name

    @property
    def uptime(self) -> str:
        elapsed = int(time.time() - self.started_at)
        if elapsed < 60:
            return f"{elapsed}s"
        if elapsed < 3600:
            return f"{elapsed // 60}m{elapsed % 60:02d}s"
        return f"{elapsed // 3600}h{(elapsed % 3600) // 60:02d}m"

    @property
    def state_icon(self) -> str:
        return {
            SessionState.WORKING: "⚙",
            SessionState.WAITING: "❓",
            SessionState.IDLE: "💤",
            SessionState.DEAD: "💀",
        }[self.state]


@dataclass
class ExternalSession:
    pid: int
    project_dir: str
    cmdline: str

    @property
    def display_name(self) -> str:
        return Path(self.project_dir).name if self.project_dir else f"pid:{self.pid}"


class ProcessManager:
    """Manages Claude Code sessions in PTYs."""

    def __init__(self) -> None:
        self._sessions: dict[str, Session] = {}
        self._external: list[ExternalSession] = []
        self._next_id = 1
        self._readers: dict[str, asyncio.Task] = {}

    @property
    def sessions(self) -> list[Session]:
        return list(self._sessions.values())

    @property
    def external_sessions(self) -> list[ExternalSession]:
        return list(self._external)

    @property
    def all_managed_pids(self) -> set[int]:
        return {s.pid for s in self._sessions.values()}

    def get_session(self, sid: str) -> Session | None:
        return self._sessions.get(sid)

    def spawn(
        self,
        project_dir: str,
        prompt: str | None = None,
        on_output=None,
        rows: int = 24,
        cols: int = 80,
    ) -> Session:
        """Spawn a new Claude Code session in a PTY."""
        sid = f"s{self._next_id}"
        self._next_id += 1

        master_fd, slave_fd = pty.openpty()

        # Set terminal size
        winsize = struct.pack("HHHH", rows, cols, 0, 0)
        fcntl.ioctl(slave_fd, termios.TIOCSWINSZ, winsize)

        cmd = ["claude", "--dangerously-skip-permissions"]
        if prompt:
            cmd.extend(["-p", prompt])

        pid = os.fork()
        if pid == 0:
            # Child — becomes the Claude Code process
            os.close(master_fd)
            os.setsid()
            fcntl.ioctl(slave_fd, termios.TIOCSCTTY, 0)
            os.dup2(slave_fd, 0)
            os.dup2(slave_fd, 1)
            os.dup2(slave_fd, 2)
            if slave_fd > 2:
                os.close(slave_fd)
            os.chdir(project_dir)
            # Direct exec — no shell involved, safe from injection
            os.execvp(cmd[0], cmd)  # noqa: S606
        else:
            # Parent
            os.close(slave_fd)
            # Make master fd non-blocking
            flags = fcntl.fcntl(master_fd, fcntl.F_GETFL)
            fcntl.fcntl(master_fd, fcntl.F_SETFL, flags | os.O_NONBLOCK)

            session = Session(
                sid=sid,
                project_dir=project_dir,
                pid=pid,
                fd=master_fd,
                state=SessionState.WORKING,
            )
            self._sessions[sid] = session

            # Start async reader
            self._readers[sid] = asyncio.ensure_future(
                self._read_loop(session, on_output)
            )

            return session

    async def _read_loop(self, session: Session, on_output=None) -> None:
        """Read PTY output asynchronously."""
        loop = asyncio.get_event_loop()
        fd = session.fd

        while session.state != SessionState.DEAD:
            try:
                data = await loop.run_in_executor(None, self._blocking_read, fd)
                if data is None:
                    session.state = SessionState.DEAD
                    break
                if data:
                    session.output_buffer.extend(data)
                    session._last_output_time = time.time()
                    session.state = SessionState.WORKING
                    if on_output:
                        await on_output(session, data)
            except OSError:
                session.state = SessionState.DEAD
                break

    @staticmethod
    def _blocking_read(fd: int) -> bytes | None:
        """Blocking read with a short timeout. Returns None on EOF/error."""
        import select

        r, _, _ = select.select([fd], [], [], 0.25)
        if r:
            try:
                data = os.read(fd, 4096)
                return data if data else None  # empty read = EOF
            except OSError:
                return None
        return b""  # timeout, not EOF

    def write_to_session(self, sid: str, data: bytes) -> None:
        """Send input to a session's PTY."""
        session = self._sessions.get(sid)
        if session and session.state != SessionState.DEAD:
            os.write(session.fd, data)

    def resize_session(self, sid: str, rows: int, cols: int) -> None:
        """Resize a session's PTY."""
        session = self._sessions.get(sid)
        if session and session.state != SessionState.DEAD:
            winsize = struct.pack("HHHH", rows, cols, 0, 0)
            fcntl.ioctl(session.fd, termios.TIOCSWINSZ, winsize)

    def kill_session(self, sid: str) -> None:
        """Kill a managed session."""
        session = self._sessions.get(sid)
        if not session or not session.managed:
            return

        # Cancel reader
        reader = self._readers.pop(sid, None)
        if reader:
            reader.cancel()

        # Kill process group
        try:
            os.killpg(os.getpgid(session.pid), signal.SIGTERM)
        except (ProcessLookupError, PermissionError):
            pass

        # Close fd
        try:
            os.close(session.fd)
        except OSError:
            pass

        session.state = SessionState.DEAD

    def remove_dead(self) -> list[str]:
        """Remove dead sessions, return their sids."""
        dead = [sid for sid, s in self._sessions.items() if s.state == SessionState.DEAD]
        for sid in dead:
            reader = self._readers.pop(sid, None)
            if reader:
                reader.cancel()
            try:
                os.close(self._sessions[sid].fd)
            except OSError:
                pass
            del self._sessions[sid]
        return dead

    async def discover_external(self) -> list[ExternalSession]:
        """Find Claude Code processes not managed by us."""
        try:
            proc = await asyncio.create_subprocess_exec(
                "pgrep", "-af", "claude",
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.DEVNULL,
            )
            stdout, _ = await proc.communicate()
        except FileNotFoundError:
            return []

        managed_pids = self.all_managed_pids
        external = []

        for line in stdout.decode(errors="replace").strip().splitlines():
            parts = line.split(None, 1)
            if len(parts) < 2:
                continue
            try:
                pid = int(parts[0])
            except ValueError:
                continue
            cmdline = parts[1]

            if pid in managed_pids:
                continue
            if "claude" not in cmdline.lower():
                continue

            # Try to find working directory from /proc
            cwd = ""
            try:
                cwd = os.readlink(f"/proc/{pid}/cwd")
            except (OSError, PermissionError):
                pass

            external.append(ExternalSession(pid=pid, project_dir=cwd, cmdline=cmdline))

        self._external = external
        return external

    def cleanup(self) -> None:
        """Kill all managed sessions and clean up."""
        for sid in list(self._sessions):
            self.kill_session(sid)
        self._sessions.clear()
        self._readers.clear()
