#!/bin/bash
# orrch-spawn: systemd-run wrapper that constrains background tasks to orrch.slice
#
# Usage: orrch-spawn <command> [args...]
#
# Spawns <command> inside orrch.slice cgroup so its descendants inherit
# CPUWeight=20, IOWeight=20, memory and task limits. Detached children remain
# inside the slice via cgroup-v2 inheritance, not systemd-user (PID 1).

set -euo pipefail

if [[ $# -eq 0 ]]; then
    echo "orrch-spawn: no command given" >&2
    exit 1
fi

# Try systemd-run; fall back to direct exec with warning if missing.
if command -v systemd-run &>/dev/null; then
    exec systemd-run --user --scope --slice=orrch.slice --collect --quiet --same-dir -- "$@"
else
    echo "orrch-spawn: systemd-run not found, executing directly (cgroup isolation unavailable)" >&2
    exec "$@"
fi
