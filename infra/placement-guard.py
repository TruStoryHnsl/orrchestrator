#!/usr/bin/env python3
"""PreToolUse hook — HARD-BLOCK deploys that violate the infra topology.

Reads the local host's generated placement.json (single source of truth:
orrchestrator/infra/topology.toml -> gen.py -> generated/<host>.placement.json,
also deployable to /etc/orrch/placement.json on servers). Inspects Bash
commands; if one would deploy/build a service in a mode forbidden on THIS host,
it exits 2 (Claude Code blocks the call and shows the reason on stderr).

Fail-OPEN: no placement file, bad JSON, or non-Bash tool -> allow. This guard
never wedges a machine that hasn't been onboarded to the manifest.

Wire in ~/.claude/settings.json:
  "hooks": { "PreToolUse": [ { "matcher": "Bash",
    "hooks": [ { "type": "command",
      "command": "/path/to/orrchestrator/infra/placement-guard.py" } ] } ] }

Test override env: ORRCH_HOST_OVERRIDE, ORRCH_PLACEMENT_FILE.
"""
import json
import os
import pathlib
import re
import socket
import sys


def allow():  # fail-open
    sys.exit(0)


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        allow()

    if payload.get("tool_name") != "Bash":
        allow()
    cmd = (payload.get("tool_input") or {}).get("command", "")
    if not cmd:
        allow()

    local_host = os.environ.get("ORRCH_HOST_OVERRIDE") or socket.gethostname().split(".")[0].lower()
    gen_dir = pathlib.Path(os.path.expanduser("~/projects/orrchestrator/infra/generated"))

    # If the command deploys to a REMOTE host via ssh, evaluate against THAT
    # host's ruleset (ops run from one workstation, ssh into servers).
    target_host = local_host
    m = re.search(r"\bssh\s+(?:-\S+\s+)*(?:[\w.-]+@)?([\w.-]+)", cmd)
    if m:
        tok = m.group(1).lower()
        try:
            idx = json.loads((gen_dir / "index.json").read_text())
            if tok in idx:
                target_host = idx[tok]
        except Exception:
            pass

    forced = os.environ.get("ORRCH_PLACEMENT_FILE")
    candidates = ([pathlib.Path(forced)] if forced else []) + [
        gen_dir / f"{target_host}.placement.json",
        pathlib.Path("/etc/orrch/placement.json"),
        pathlib.Path(os.path.expanduser("~/.config/orrch/placement.json")),
        pathlib.Path(os.path.expanduser(f"~/projects/orrchestrator/infra/generated/{local_host}.placement.json")),
    ]
    host = target_host
    data = None
    for p in candidates:
        try:
            if p.is_file():
                data = json.loads(p.read_text())
                break
        except Exception:
            pass
    if not data:
        allow()

    low = cmd.lower()

    # source build/checkout signatures
    BUILD = re.compile(
        r"\bgit\s+clone\b|\bcargo\s+(build|watch|run|install)\b|\bnpm\s+(run\s+build|ci|install)\b"
        r"|\bpnpm\b.*\bbuild\b|/target/|\bdocker\s+build\b|\bnpm\s+run\s+tauri\b"
    )
    # any deploy/mutate action (for fully-forbidden services)
    DEPLOY = re.compile(
        r"\bgit\s+clone\b|\bdocker\s+compose\b.*\bup\b|\bdocker\s+run\b|\bdocker\s+load\b"
        r"|\bmkdir\b.*/docker/stacks/|\bcargo\s+(build|run)\b|\bnpm\s+(run|ci|install)\b"
    )

    def mentions(svc):
        # A service is only "targeted" when its name appears in a deploy-relevant
        # position — a path component, a git repo ref, or a docker image/tag —
        # NOT in incidental prose (comments, echo text, heredocs). This avoids
        # blocking commands that merely talk about a service.
        s = re.escape(svc)
        patterns = [
            r"[/~][\w.\-/]*" + s + r"\b",   # /docker/stacks/<svc>, ~/projects/<svc>
            r"[:/]" + s + r"(?:\.git)?(?:[\s\"']|$)",  # git URL ...:owner/<svc>[.git]
            r"\b" + s + r"[:@][\w.\-]",     # docker image/tag  <svc>:tag  <svc>@sha
            r"container_name[^\n]*\b" + s + r"\b",
            r"-t\s+" + s + r"\b",           # docker build -t <svc>
        ]
        return any(re.search(p, low) for p in patterns)

    for rule in data.get("deny_rules", []):
        svc = rule["service"]
        if not mentions(svc):
            continue
        # command targeting an explicitly-allowed dir (and not a build) passes
        if any(d and d.lower() in low for d in rule.get("allowed_dirs", [])) and not BUILD.search(low):
            continue
        fmodes = rule.get("forbidden_modes", [])
        if "any" in fmodes:
            blocked = bool(DEPLOY.search(low))
        else:
            blocked = any(m.startswith("source") for m in fmodes) and bool(BUILD.search(low))
        if blocked:
            print(
                "⛔ BLOCKED by infra topology guard.\n"
                f"Host '{host}' does not permit this deploy of '{svc}'.\n"
                f"Reason: {rule.get('reason')}\n"
                f"Allowed here: {rule.get('allow_hint')}\n"
                "Authority: orrchestrator/infra/topology.toml (query: MCP infra_placement).\n"
                "If the manifest is wrong, fix topology.toml and regenerate — do not bypass the guard.",
                file=sys.stderr,
            )
            sys.exit(2)

    allow()


if __name__ == "__main__":
    main()
