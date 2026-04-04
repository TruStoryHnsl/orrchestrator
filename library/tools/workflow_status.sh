#!/usr/bin/env bash
set -euo pipefail

# workflow_status.sh — Update workflow status file for TUI visibility
# Usage: workflow_status.sh <project_dir> <workflow_name> <step> <total_steps> <status> [agent_json]

usage() {
    echo "Usage: workflow_status.sh <project_dir> <workflow_name> <step> <total_steps> <status> [agent_json]" >&2
    echo "  project_dir    — absolute path to the target project directory" >&2
    echo "  workflow_name  — name of the workflow" >&2
    echo "  step           — current step number (integer)" >&2
    echo "  total_steps    — total number of steps (integer)" >&2
    echo "  status         — one of: running, complete, failed, paused" >&2
    echo "  agent_json     — optional JSON array of agent statuses" >&2
    exit 1
}

if [[ $# -lt 5 ]]; then
    usage
fi

project_dir="$1"
workflow_name="$2"
step="$3"
total_steps="$4"
status="$5"
agent_json="${6:-[]}"

if [[ ! -d "$project_dir" ]]; then
    echo "Error: project directory does not exist: $project_dir" >&2
    exit 1
fi

# Validate step and total_steps are integers
if ! [[ "$step" =~ ^[0-9]+$ ]]; then
    echo "Error: step must be a non-negative integer, got: $step" >&2
    exit 1
fi

if ! [[ "$total_steps" =~ ^[0-9]+$ ]]; then
    echo "Error: total_steps must be a non-negative integer, got: $total_steps" >&2
    exit 1
fi

# Validate status
case "$status" in
    running|complete|failed|paused) ;;
    *)
        echo "Error: status must be one of: running, complete, failed, paused — got: $status" >&2
        exit 1
        ;;
esac

# Validate agent_json is valid JSON if provided
if [[ "$agent_json" != "[]" ]]; then
    if ! printf '%s' "$agent_json" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        echo "Error: agent_json is not valid JSON: $agent_json" >&2
        exit 1
    fi
fi

orrch_dir="$project_dir/.orrch"
status_file="$orrch_dir/workflow.json"
timestamp="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

mkdir -p "$orrch_dir"

# Write status as JSON using python for safe serialization
python3 -c "
import json, sys

data = {
    'workflow': sys.argv[1],
    'step': int(sys.argv[2]),
    'total_steps': int(sys.argv[3]),
    'status': sys.argv[4],
    'agents': json.loads(sys.argv[5]),
    'updated_at': sys.argv[6]
}

print(json.dumps(data, indent=2))
" "$workflow_name" "$step" "$total_steps" "$status" "$agent_json" "$timestamp" > "$status_file"

echo "Workflow status updated: $workflow_name step $step/$total_steps ($status)"
