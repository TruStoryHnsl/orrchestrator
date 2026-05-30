# claude-process-isolation

Hardens systemd user cgroup constraints to prevent background tasks from escaping resource limits and rendering the terminal unresponsive.

## Problem

On 2026-05-14, git 2.54's automatic maintenance (`git gc --auto`) detached and reparented itself to systemd-user (PID 1) via `--detach`, escaping the `claude.slice` cgroup. pack-objects then consumed 19 GB/s IO, making the terminal unresponsive across three restarts. Root cause: user.slice has `IOAccounting=no`, silently disabling IO weighting for all children, including orrch.slice.

## Solution

Install three systemd slice units and drop-ins that enforce CPU/IO/memory/task limits on background work:

- **orrch.slice** — new cgroup for background tasks (CPUWeight=20, IOWeight=20, MemoryHigh=4G, MemoryMax=8G)
- **claude-limit.slice.d/override.conf** — hardened constraints on existing slice
- **user.slice.d/io-accounting.conf** — enables IO accounting inheritance for all children
- **orrch-spawn** — wrapper script that runs detached commands inside orrch.slice

## Install

```bash
./deploy.sh
```

Idempotent; backs up existing files to `.bak` before overwriting.

## Verify

```bash
./verify.sh
```

Confirms:
1. orrch.slice has non-default CPUWeight, IOWeight, IOAccounting
2. claude-limit.slice has matching limits
3. user.slice has IOAccounting=yes
4. A test `orrch-spawn sleep 10` lands inside orrch.slice (not user.slice)

## Daily Use

```bash
orrch-spawn <heavy command>
```

Runs `<heavy command>` and its detached children inside orrch.slice. Descendants cannot exceed 4G soft memory, 8G hard memory, 2048 tasks; IO and CPU are weighted at 1/5 relative to foreground.

## Rollback

```bash
rm ~/.local/bin/orrch-spawn \
   ~/.config/systemd/user/orrch.slice \
   ~/.config/systemd/user/claude-limit.slice.d/override.conf \
   ~/.config/systemd/user/user.slice.d/io-accounting.conf
systemctl --user daemon-reload
```
