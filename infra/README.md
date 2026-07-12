# infra/ — Machine + Service Placement (single source of truth)

This directory answers one question authoritatively: **what is allowed to run where, and how.**

It exists because ad-hoc hard-blockers (`.no-concord` marker files, hardcoded
forbidden-host arrays in `install.sh`, prose scattered across docs) kept
multiplying — policy was duplicated in many places, drifted, and agents only
discovered a rule by tripping over it at deploy time. The 2026-05-17 orrapus
outage (a 32 GB orphan concord source checkout filling orrgate's root disk) is
the canonical example of what happens when placement policy isn't enforced.

Now there is **one manifest**, and everything else is generated from it.

```
topology.toml            ← EDIT HERE. hosts + services + allow/deny placements.
   │  gen.py  ▼           (deterministic generator)
   ├─ generated/<host>.placement.json   per-host guard data (committed)
   ├─ generated/index.json              ip/tailscale/wan/hostname → canonical host
   │
   ├─ deployed to servers:  /etc/orrch/placement.json   (+ /docker/stacks/.placement-policy note)
   │
   └─ placement-guard.py  ← PreToolUse hook. Reads the placement data, inspects
                            every Bash command (incl. `ssh <host> "..."` — it
                            resolves the TARGET host), and HARD-BLOCKS (exit 2)
                            a deploy that violates a rule. Fail-open if no data.

Query live from an agent:  MCP  mcp__orrchestrator__infra_placement
   { "service": "concord", "host": "orrgate", "mode": "docker-image" }
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

# 3. self-test the guard (9 cases; runner carries no deploy signature by design)
python3 test_guard.py

# 4. deploy a host's data to that server (defense-in-depth for on-box agents)
scp generated/orrgate.placement.json user@192.168.1.10:/tmp/p.json
ssh user@192.168.1.10 'sudo mkdir -p /etc/orrch && sudo mv /tmp/p.json /etc/orrch/placement.json'
```

## Wiring the hook (once, per workstation Claude Code runs on)

`~/.claude/settings.json`:
```json
{ "hooks": { "PreToolUse": [ { "matcher": "Bash",
  "hooks": [ { "type": "command",
    "command": "/home/user/projects/orrchestrator/infra/placement-guard.py",
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
