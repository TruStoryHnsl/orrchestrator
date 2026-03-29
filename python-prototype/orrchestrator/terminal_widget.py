"""Terminal emulator widget using pyte for ANSI rendering inside Textual."""

from __future__ import annotations

import pyte
from rich.text import Text
from textual.widget import Widget
from textual.reactive import reactive
from textual.strip import Strip

# Map pyte colors to Rich color names
PYTE_COLORS = {
    "black": "black",
    "red": "red",
    "green": "green",
    "brown": "yellow",
    "blue": "blue",
    "magenta": "magenta",
    "cyan": "cyan",
    "white": "white",
    "default": "default",
}


class TerminalEmulator:
    """Wraps a pyte screen + stream for terminal emulation."""

    def __init__(self, rows: int = 24, cols: int = 80) -> None:
        self.screen = pyte.Screen(cols, rows)
        self.stream = pyte.Stream(self.screen)
        self.rows = rows
        self.cols = cols

    def feed(self, data: bytes) -> None:
        self.stream.feed(data.decode("utf-8", errors="replace"))

    def resize(self, rows: int, cols: int) -> None:
        self.rows = rows
        self.cols = cols
        self.screen.resize(rows, cols)

    def render_line(self, row: int) -> Text:
        """Render a single screen line as a Rich Text object."""
        line = self.screen.buffer[row]
        text = Text()
        for col in range(self.cols):
            char = line[col]
            ch = char.data or " "
            fg = PYTE_COLORS.get(char.fg, "default") if char.fg != "default" else "default"
            bg = PYTE_COLORS.get(char.bg, "default") if char.bg != "default" else "default"
            style_parts = []
            if fg != "default":
                style_parts.append(fg)
            if bg != "default":
                style_parts.append(f"on {bg}")
            if char.bold:
                style_parts.append("bold")
            if char.underscore:
                style_parts.append("underline")
            if char.reverse:
                style_parts.append("reverse")
            text.append(ch, " ".join(style_parts) if style_parts else None)
        return text


class TerminalView(Widget):
    """Textual widget that displays a pyte-emulated terminal."""

    DEFAULT_CSS = """
    TerminalView {
        background: #1a1a2e;
        color: #c8c8d4;
    }
    """

    version = reactive(0)  # bump to trigger re-render

    def __init__(self, rows: int = 24, cols: int = 80, **kwargs) -> None:
        super().__init__(**kwargs)
        self.emulator = TerminalEmulator(rows, cols)

    def feed(self, data: bytes) -> None:
        self.emulator.feed(data)
        self.version += 1

    def render_line(self, y: int) -> Strip:
        if y < self.emulator.rows:
            text = self.emulator.render_line(y)
            return Strip([text])
        return Strip.blank(self.size.width)

    def render_lines(self, crop) -> list[Strip]:
        y, _, height, _ = crop
        strips = []
        for row in range(y, y + height):
            strips.append(self.render_line(row))
        return strips
