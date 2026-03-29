"""Retrospect — automated error learning engine.

Captures errors from Claude session output, fingerprints them for dedup,
tracks resolutions, and builds per-project troubleshooting protocols.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import time
from dataclasses import dataclass, field, asdict
from pathlib import Path


# ─── Error Fingerprinting ────────────────────────────────────────────

# Patterns that indicate an error in session output
ERROR_PATTERNS = [
    # Python tracebacks
    re.compile(r"^Traceback \(most recent call last\):", re.MULTILINE),
    # Generic error/exception lines
    re.compile(r"^(\w+Error|\w+Exception):\s+.+", re.MULTILINE),
    # Node.js errors
    re.compile(r"^(TypeError|ReferenceError|SyntaxError|RangeError):\s+.+", re.MULTILINE),
    # Claude Code tool errors
    re.compile(r"^Error:?\s+.+", re.MULTILINE),
    # Test failures
    re.compile(r"^(FAILED|FAIL|ERROR)\s+.+", re.MULTILINE),
    # Bash errors
    re.compile(r"^.+: command not found$", re.MULTILINE),
    re.compile(r"^.+: No such file or directory$", re.MULTILINE),
    re.compile(r"^.+: Permission denied$", re.MULTILINE),
    # Docker errors
    re.compile(r"^ERROR \[.+\]", re.MULTILINE),
    # pip/package errors
    re.compile(r"^(ERROR|error):\s+(Could not|Failed to|No matching).+", re.MULTILINE),
    # Import errors
    re.compile(r"^(ImportError|ModuleNotFoundError):\s+.+", re.MULTILINE),
    # Attribute errors (common with API drift)
    re.compile(r"^AttributeError:\s+.+", re.MULTILINE),
]

# Patterns to strip when fingerprinting (variable parts that change between occurrences)
STRIP_PATTERNS = [
    # Line numbers
    (re.compile(r", line \d+"), ", line <N>"),
    # File paths with varying prefixes
    (re.compile(r'File "/.+?/([^/]+\.py)"'), r'File "<...>/\1"'),
    # Hex addresses
    (re.compile(r"0x[0-9a-fA-F]+"), "0x<ADDR>"),
    # Timestamps
    (re.compile(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}"), "<TIMESTAMP>"),
    # UUIDs
    (re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"), "<UUID>"),
    # Port numbers (when part of connection strings)
    (re.compile(r":\d{4,5}(?=[\s/])"), ":<PORT>"),
    # Numeric IDs
    (re.compile(r"(?<=\s)\d{3,}(?=\s)"), "<NUM>"),
    # Quoted string values (preserve structure, strip value)
    (re.compile(r"'[^']{20,}'"), "'<...>'"),
]


def extract_errors(text: str) -> list[str]:
    """Extract error blocks from session output text.

    Returns a list of error context strings — each is the error line
    plus surrounding context (up to 10 lines of traceback above).
    """
    lines = text.splitlines()
    errors = []

    for pattern in ERROR_PATTERNS:
        for match in pattern.finditer(text):
            # Find which line the match is on
            match_start = match.start()
            line_num = text[:match_start].count("\n")

            # Grab context: up to 15 lines before (for traceback) and 3 after
            start = max(0, line_num - 15)
            end = min(len(lines), line_num + 4)
            context = "\n".join(lines[start:end])

            # Dedup: if new context overlaps with existing, keep the longer one
            replaced = False
            for i, existing in enumerate(errors):
                if existing in context:
                    # New context is a superset — replace the shorter one
                    errors[i] = context
                    replaced = True
                    break
                if context in existing:
                    # Existing is already a superset — skip
                    replaced = True
                    break
            if not replaced:
                errors.append(context)

    return errors


def fingerprint(error_text: str) -> str:
    """Create a stable fingerprint for an error by normalizing variable parts.

    The fingerprint is a hex digest that groups "same class" errors together —
    e.g., two KeyErrors with different key names map to the same fingerprint.
    """
    normalized = error_text
    for pattern, replacement in STRIP_PATTERNS:
        normalized = pattern.sub(replacement, normalized)

    # Collapse whitespace
    normalized = re.sub(r"\s+", " ", normalized).strip()

    return hashlib.sha256(normalized.encode()).hexdigest()[:16]


def classify_error(error_text: str) -> str:
    """Classify an error into a broad category.

    Checks specific exception types first, then falls back to generic patterns.
    Order matters — a traceback containing "KeyError" should be "lookup",
    not "runtime".
    """
    text_lower = error_text.lower()

    # Specific exception types first (most to least specific)
    if "importerror" in text_lower or "modulenotfounderror" in text_lower:
        return "import"
    if "attributeerror" in text_lower:
        return "api-drift"
    if "keyerror" in text_lower or "indexerror" in text_lower:
        return "lookup"
    if "typeerror" in text_lower:
        return "type"
    if "syntaxerror" in text_lower:
        return "syntax"
    if "valueerror" in text_lower:
        return "value"
    if "filenotfounderror" in text_lower or "no such file" in text_lower:
        return "missing-file"
    if "permissionerror" in text_lower or "permission denied" in text_lower:
        return "permission"
    if "command not found" in text_lower:
        return "missing-command"
    if "connectionerror" in text_lower or "timeout" in text_lower:
        return "network"
    # Generic patterns last
    if "fail" in text_lower or "FAILED" in error_text:
        return "test-failure"
    if "traceback" in text_lower or "error" in text_lower:
        return "runtime"
    return "unknown"


# ─── Error Store (JSONL) ─────────────────────────────────────────────


@dataclass
class ErrorRecord:
    fingerprint: str
    category: str
    raw_context: str
    session_id: str
    project_dir: str
    timestamp: float = field(default_factory=time.time)
    resolved: bool = False
    resolution: str | None = None
    resolution_timestamp: float | None = None

    def to_json(self) -> str:
        return json.dumps(asdict(self), ensure_ascii=False)

    @classmethod
    def from_json(cls, line: str) -> ErrorRecord:
        data = json.loads(line)
        return cls(**data)


class ErrorStore:
    """Append-only JSONL error store for a project."""

    def __init__(self, project_dir: str) -> None:
        self.project_dir = project_dir
        self.store_dir = Path(project_dir) / ".retrospect"
        self.store_path = self.store_dir / "errors.jsonl"
        # In-memory index: fingerprint → list of records
        self._index: dict[str, list[ErrorRecord]] = {}
        self._loaded = False

    def _ensure_dir(self) -> None:
        self.store_dir.mkdir(parents=True, exist_ok=True)

    def _load(self) -> None:
        """Load existing records into memory index."""
        if self._loaded:
            return
        self._loaded = True
        if not self.store_path.exists():
            return
        with open(self.store_path) as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    record = ErrorRecord.from_json(line)
                    self._index.setdefault(record.fingerprint, []).append(record)
                except (json.JSONDecodeError, TypeError):
                    continue

    def append(self, record: ErrorRecord) -> None:
        """Append an error record to the store."""
        self._ensure_dir()
        self._load()
        with open(self.store_path, "a") as f:
            f.write(record.to_json() + "\n")
        self._index.setdefault(record.fingerprint, []).append(record)

    def has_fingerprint(self, fp: str) -> bool:
        """Check if we've seen this error before."""
        self._load()
        return fp in self._index

    def get_records(self, fp: str) -> list[ErrorRecord]:
        """Get all records for a fingerprint."""
        self._load()
        return self._index.get(fp, [])

    def get_resolution(self, fp: str) -> str | None:
        """Get the most recent resolution for a fingerprint, if any."""
        self._load()
        records = self._index.get(fp, [])
        for record in reversed(records):
            if record.resolved and record.resolution:
                return record.resolution
        return None

    def mark_resolved(self, fp: str, resolution: str) -> None:
        """Mark all unresolved records for a fingerprint as resolved.

        Also appends a resolution record to the store file.
        """
        self._load()
        records = self._index.get(fp, [])
        for record in records:
            if not record.resolved:
                record.resolved = True
                record.resolution = resolution
                record.resolution_timestamp = time.time()

        # Append a resolution marker
        if records:
            marker = ErrorRecord(
                fingerprint=fp,
                category=records[0].category,
                raw_context=f"[RESOLVED] {resolution}",
                session_id="retrospect",
                project_dir=self.project_dir,
                resolved=True,
                resolution=resolution,
                resolution_timestamp=time.time(),
            )
            self._ensure_dir()
            with open(self.store_path, "a") as f:
                f.write(marker.to_json() + "\n")

    def unresolved_fingerprints(self) -> list[str]:
        """Get fingerprints that have never been resolved."""
        self._load()
        unresolved = []
        for fp, records in self._index.items():
            if not any(r.resolved for r in records):
                unresolved.append(fp)
        return unresolved

    def stats(self) -> dict:
        """Summary statistics for this store."""
        self._load()
        total = sum(len(recs) for recs in self._index.values())
        unique = len(self._index)
        resolved = sum(
            1 for recs in self._index.values() if any(r.resolved for r in recs)
        )
        return {
            "total_occurrences": total,
            "unique_errors": unique,
            "resolved": resolved,
            "unresolved": unique - resolved,
        }


# ─── Solution Tracker ────────────────────────────────────────────────


class SolutionTracker:
    """Tracks error→resolution pairs within sessions.

    When a session produces errors and then continues working without
    further errors for a sustained period, we consider the error "resolved"
    and capture the resolution window.
    """

    # Seconds of clean output after an error before we consider it resolved
    RESOLUTION_COOLDOWN = 30.0

    def __init__(self, store: ErrorStore) -> None:
        self.store = store
        # session_id → list of (fingerprint, timestamp, raw_context)
        self._pending: dict[str, list[tuple[str, float, str]]] = {}
        # session_id → timestamp of last error
        self._last_error_time: dict[str, float] = {}
        # session_id → accumulated output since last error
        self._output_since_error: dict[str, list[str]] = {}

    def on_error(self, session_id: str, fp: str, raw_context: str) -> None:
        """Record that an error was seen in a session."""
        self._pending.setdefault(session_id, []).append((fp, time.time(), raw_context))
        self._last_error_time[session_id] = time.time()
        self._output_since_error[session_id] = []

    def on_output(self, session_id: str, text: str) -> list[str]:
        """Feed non-error output. Returns list of fingerprints that were just resolved."""
        if session_id not in self._pending:
            return []

        self._output_since_error.setdefault(session_id, []).append(text)

        now = time.time()
        last_err = self._last_error_time.get(session_id, 0)

        if now - last_err < self.RESOLUTION_COOLDOWN:
            return []

        # Cooldown passed — resolve all pending errors for this session
        resolved_fps = []
        output_summary = "\n".join(self._output_since_error.get(session_id, []))
        # Truncate to reasonable size
        if len(output_summary) > 2000:
            output_summary = output_summary[-2000:]

        for fp, _ts, _ctx in self._pending.pop(session_id, []):
            resolution = f"Auto-resolved after continued output. Post-error context:\n{output_summary[:500]}"
            self.store.mark_resolved(fp, resolution)
            resolved_fps.append(fp)

        self._last_error_time.pop(session_id, None)
        self._output_since_error.pop(session_id, None)

        return resolved_fps

    def on_session_end(self, session_id: str) -> None:
        """Session ended — any pending errors remain unresolved."""
        self._pending.pop(session_id, None)
        self._last_error_time.pop(session_id, None)
        self._output_since_error.pop(session_id, None)


# ─── Session Output Analyzer (hooks into ProcessManager) ─────────────


class SessionAnalyzer:
    """Analyzes a single session's output stream for errors.

    One instance per managed session. Feeds errors to the store and
    solution tracker.
    """

    def __init__(
        self,
        session_id: str,
        project_dir: str,
        store: ErrorStore,
        tracker: SolutionTracker,
    ) -> None:
        self.session_id = session_id
        self.project_dir = project_dir
        self.store = store
        self.tracker = tracker
        self._text_buffer = ""
        # Debounce: don't re-report the same fingerprint within 60s
        self._recent_fps: dict[str, float] = {}

    def feed(self, data: bytes) -> list[ErrorRecord]:
        """Feed raw PTY output. Returns any new error records created."""
        text = data.decode("utf-8", errors="replace")
        self._text_buffer += text

        # Only analyze complete lines
        if "\n" not in self._text_buffer:
            return []

        # Split on last newline — keep incomplete line in buffer
        complete, self._text_buffer = self._text_buffer.rsplit("\n", 1)

        errors = extract_errors(complete)
        new_records = []

        if errors:
            for error_text in errors:
                fp = fingerprint(error_text)

                # Debounce
                now = time.time()
                if fp in self._recent_fps and now - self._recent_fps[fp] < 60:
                    continue
                self._recent_fps[fp] = now

                category = classify_error(error_text)
                record = ErrorRecord(
                    fingerprint=fp,
                    category=category,
                    raw_context=error_text,
                    session_id=self.session_id,
                    project_dir=self.project_dir,
                )
                self.store.append(record)
                self.tracker.on_error(self.session_id, fp, error_text)
                new_records.append(record)
        else:
            # No errors in this chunk — feed to solution tracker
            self.tracker.on_output(self.session_id, complete)

        return new_records

    def get_known_fix(self, fp: str) -> str | None:
        """Check if there's a known fix for a fingerprint."""
        return self.store.get_resolution(fp)
