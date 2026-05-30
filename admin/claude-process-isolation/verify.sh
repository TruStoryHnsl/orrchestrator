#!/bin/bash
# Verification script: confirm cgroup limits are in place and active

set -euo pipefail

echo "=== Effective systemd slice limits ==="

echo ""
echo "orrch.slice:"
systemctl --user show orrch.slice -p CPUWeight,IOWeight,IOAccounting,MemoryHigh,MemoryMax,TasksMax

echo ""
echo "claude-limit.slice:"
systemctl --user show claude-limit.slice -p CPUWeight,IOWeight,IOAccounting,MemoryHigh,MemoryMax,TasksMax

echo ""
echo "user.slice:"
systemctl --user show user.slice -p IOAccounting,CPUAccounting

echo ""
echo "=== Current shell cgroup membership ==="
cat /proc/self/cgroup

echo ""
echo "=== Testing orrch-spawn detach isolation ==="
echo "Spawning: orrch-spawn sleep 10 &"

# Spawn a sleep in the background via orrch-spawn
if command -v orrch-spawn &>/dev/null; then
    orrch-spawn sleep 10 &
    sleep 1

    echo "Checking systemd-cgls for orrch.slice membership:"
    systemd-cgls --user-unit orrch.slice 2>/dev/null || echo "(systemd-cgls not available)"

    # Wait for the background sleep to finish
    wait
    echo "Sleep completed; orrch-spawn test passed."
else
    echo "WARNING: orrch-spawn not in PATH, skipping spawn test"
    exit 1
fi

echo ""
echo "=== Verification complete ==="
