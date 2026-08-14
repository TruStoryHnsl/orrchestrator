# infra/ — Machine + Service Placement (single source of truth)

This directory answers one question authoritatively: **what is allowed to run
where, and how.** It is an optional subsystem — orrchestrator runs fine without
it — but if you drive deploys across a fleet from agent sessions, it stops an
agent from putting the wrong workload on the wrong host.

It exists because ad-hoc hard-blockers (`.no-<service>` marker files, hardcoded
forbidden-host arrays in deploy scripts, prose scattered across docs) tend to
multiply — the policy gets duplicated in many places, drifts, and agents only
discover a rule by tripping over it at deploy time. A classic failure is a
multi-gigabyte source checkout compiling in place and filling a production
host's root volume. This subsystem replaces all of that with one manifest.

```
topology.toml            ← EDIT HERE. hosts + services + allow/deny placements.
   │  gen.py  ▼           (deterministic generator)
   ├─ generated/<host>.placement.json   per-host guard data
   ├─ generated/index.json              ip/hostname → canonical host
   │
   ├─ deployed to servers:  /etc/orrch/placement.json
   │
   └─ placement-guard.py  ← PreToolUse hook. Reads the placement data, inspects
                            every Bash command (incl. `ssh <host> "..."` — it
                            resolves the TARGET host), and HARD-BLOCKS (exit 2)
                            a deploy that violates a rule. Fail-open if no data.

Query live from an agent:  MCP  mcp__orrchestrator__infra_placement
   { "service": "webapp", "host": "app-1", "mode": "docker-image" }
```

## Getting started

`topology.toml` is gitignored — it describes your real hosts. Start from the
committed example:

```bash
cp topology.example.toml topology.toml
$EDITOR topology.toml          # describe your fleet
python3 gen.py --all           # regenerate per-host guard data + index
```

## Modes

| mode          | meaning                                                        |
|---------------|----------------------------------------------------------------|
| `docker-image`| load a prebuilt image + data. No compile. Small footprint.     |
| `source-build`| git checkout + compile in place. Heavy (`target/` trees).      |
| `source-dev`  | dev checkout, run from source.                                 |

A service on a host is **denied by default** unless topology.toml lists an
`[[services.<svc>.allow]]` for that (host, mode). `[[services.<svc>.deny]]`
entries exist only to carry a specific human reason for common wrong moves.

## Workflow

```bash
# 1. edit the manifest
$EDITOR topology.toml

# 2. regenerate all per-host guard data + index
python3 gen.py --all

# 3. self-test the guard (runner carries no deploy signature by design)
python3 test_guard.py

# 4. deploy a host's data to that server (defense-in-depth for on-box agents)
scp generated/app-1.placement.json you@app-1:/tmp/p.json
ssh you@app-1 'sudo mkdir -p /etc/orrch && sudo mv /tmp/p.json /etc/orrch/placement.json'
```

## Wiring the hook (once, per workstation Claude Code runs on)

`~/.claude/settings.json`:
```json
{ "hooks": { "PreToolUse": [ { "matcher": "Bash",
  "hooks": [ { "type": "command",
    "command": "/path/to/orrchestrator/infra/placement-guard.py",
    "timeout": 8 } ] } ] } }
```

The guard resolves `ssh <ip|host>` targets via `generated/index.json`, so it
enforces the **target** host's rules even though ops are driven from one
workstation. Test overrides: `ORRCH_HOST_OVERRIDE`, `ORRCH_PLACEMENT_FILE`.

## The rule for future agents

If you want to run something on a host and the guard blocks you, **the guard is
not the problem** — either you're deploying to the wrong host/mode, or the
manifest is out of date. Fix `topology.toml` and regenerate. Never bypass the
guard, and never re-introduce a one-off `.no-<service>` blocker.
