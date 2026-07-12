#!/usr/bin/env python3
"""Self-test for placement-guard.py. Cases live in cases.json so this runner's
own command line carries no deploy signature (else the live hook blocks it)."""
import json
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
GUARD = HERE / "placement-guard.py"
CASES = json.loads((HERE / "cases.json").read_text())

fails = 0
for c in CASES:
    payload = json.dumps({"tool_name": "Bash", "tool_input": {"command": c["command"]}})
    env = dict(os.environ, ORRCH_HOST_OVERRIDE=c["host"])
    r = subprocess.run([sys.executable, str(GUARD)], input=payload,
                       capture_output=True, text=True, env=env)
    got = r.returncode
    ok = got == c["expect"]
    fails += not ok
    print(f"{'PASS' if ok else 'FAIL'} [got {got} want {c['expect']}] {c['name']}")
    if not ok and r.stderr:
        print("   ", r.stderr.strip().splitlines()[0])

print(f"\n{len(CASES)-fails}/{len(CASES)} passed")
sys.exit(1 if fails else 0)
