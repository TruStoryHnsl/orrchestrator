#!/usr/bin/env python3
"""Generate per-host placement guards from the authoritative topology.toml.

Usage:
    gen.py --all                 # regenerate infra/generated/<host>.placement.json for every host
    gen.py --host orrgate        # just one host
    gen.py --host orrgate --print # print to stdout instead of writing

The generated placement.json is the ONLY thing the PreToolUse hook
(placement-guard.sh) reads. Edit topology.toml, never the generated files.
"""
import argparse
import json
import pathlib
import sys

try:
    import tomllib  # py3.11+
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore

HERE = pathlib.Path(__file__).resolve().parent
TOPO = HERE / "topology.toml"
OUT = HERE / "generated"


def load():
    with open(TOPO, "rb") as f:
        return tomllib.load(f)


def host_placement(topo: dict, host: str) -> dict:
    """Fold the manifest into the flat structure the hook consumes for one host."""
    services = {}
    deny_rules = []
    for svc, spec in topo.get("services", {}).items():
        allows = [a for a in spec.get("allow", []) if a.get("host") in (host, "*")]
        denies = [d for d in spec.get("deny", []) if d.get("host") in (host, "*")]
        allowed_modes = sorted({a["mode"] for a in allows})
        allowed_dirs = sorted({a["dir"] for a in allows if a.get("dir")})

        # A service with NO allow entry for this host is fully forbidden here.
        fully_forbidden = len(allows) == 0
        forbidden_modes = sorted({d["mode"] for d in denies})
        # explicit "any" deny, or no allowed placement at all → forbid every mode
        if fully_forbidden or "any" in forbidden_modes:
            forbidden_modes = ["any"]

        # Build a hint describing what IS allowed here (if anything).
        if allows:
            hint = "; ".join(f"{a['mode']} at {a.get('dir', '?')}" for a in allows)
        else:
            hint = f"{svc} is not sanctioned on {host} at all — see topology.toml"

        reason = next((d.get("reason") for d in denies if d.get("reason")), None)
        if not reason and fully_forbidden:
            reason = f"{svc} has no sanctioned placement on {host}. Add one to topology.toml and regenerate before deploying here."

        services[svc] = {
            "allowed_modes": allowed_modes,
            "allowed_dirs": allowed_dirs,
            "forbidden_modes": forbidden_modes,
            "reason": reason,
            "allow_hint": hint,
        }
        if forbidden_modes:
            deny_rules.append({
                "service": svc,
                "forbidden_modes": forbidden_modes,
                "reason": reason or f"{svc}: mode not sanctioned on {host}.",
                "allow_hint": hint,
                "allowed_dirs": allowed_dirs,
            })

    hostmeta = topo.get("hosts", {}).get(host, {})
    return {
        "host": host,
        "generated_from": "orrchestrator/infra/topology.toml",
        "hostname": hostmeta.get("hostname", host),
        "role": hostmeta.get("role", ""),
        "services": services,
        "deny_rules": deny_rules,
        "warning": "GENERATED FILE — do not edit. Edit topology.toml and run infra/gen.py.",
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host")
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--print", dest="to_stdout", action="store_true")
    args = ap.parse_args()

    topo = load()
    hosts = list(topo.get("hosts", {}))
    targets = hosts if args.all else ([args.host] if args.host else [])
    if not targets:
        ap.error("specify --host <name> or --all")

    OUT.mkdir(exist_ok=True)

    # host resolution index: every ip / tailscale / wan / hostname -> canonical host.
    # Lets the hook map an `ssh <ip> '...'` deploy to the TARGET host's ruleset
    # (ops are driven from one workstation via ssh, so local hostname != target).
    index = {}
    for h, meta in topo.get("hosts", {}).items():
        for key in ("lan_ip", "tailscale_ip", "wan_ip", "hostname"):
            v = meta.get(key)
            if v:
                index[str(v).lower()] = h
        index[h.lower()] = h
    (OUT / "index.json").write_text(json.dumps(index, indent=2) + "\n")

    for h in targets:
        if h not in topo.get("hosts", {}):
            print(f"unknown host: {h}", file=sys.stderr)
            sys.exit(1)
        data = host_placement(topo, h)
        blob = json.dumps(data, indent=2)
        if args.to_stdout:
            print(blob)
        else:
            (OUT / f"{h}.placement.json").write_text(blob + "\n")
            print(f"wrote infra/generated/{h}.placement.json")


if __name__ == "__main__":
    main()
