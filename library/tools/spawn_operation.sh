#!/usr/bin/env bash
set -euo pipefail

# spawn_operation.sh — Dispatcher to spawn any operation as a workflow session.
# Usage: spawn_operation.sh <operation_name> <project_dir> [extra_args...]
#
# Reads the operation markdown file from operations/<name>.md, sets up the
# .orrch workspace, and launches a Claude session with the operation as the
# entry skill. The Oversee panel uses this to spawn ad-hoc operations
# (audits, bug intakes, etc.) without baking each into the TUI.

usage() {
    echo "Usage: spawn_operation.sh <operation_name> <project_dir> [extra_args...]" >&2
    echo "  operation_name  — operation file basename in operations/ (e.g. assess_development)" >&2
    echo "  project_dir     — target project absolute path" >&2
    echo "  extra_args      — optional, passed through to the operation context" >&2
    echo "" >&2
    echo "Available operations:" >&2
    local script_dir
    script_dir="$(cd "$(dirname "$0")" && pwd)"
    local ops_dir="${script_dir}/../../operations"
    if [[ -d "$ops_dir" ]]; then
        for f in "$ops_dir"/*.md; do
            [[ -f "$f" ]] || continue
            echo "  - $(basename "$f" .md)" >&2
        done
    fi
    exit 1
}

if [[ $# -lt 2 ]]; then
    usage
fi

operation_name="$1"
project_dir="$2"
shift 2
extra_args=("$@")

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OPS_DIR="${SCRIPT_DIR}/../../operations"
operation_file="${OPS_DIR}/${operation_name}.md"

if [[ ! -f "$operation_file" ]]; then
    echo "Error: operation not found: $operation_file" >&2
    echo "" >&2
    usage
fi

if [[ ! -d "$project_dir" ]]; then
    echo "Error: project directory does not exist: $project_dir" >&2
    exit 1
fi

project_dir="$(cd "$project_dir" && pwd)"
ORRCH_DIR="${project_dir}/.orrch"
mkdir -p "$ORRCH_DIR"

timestamp="$(date '+%Y-%m-%d-%H%M%S')"
LOG_FILE="${ORRCH_DIR}/operation_${operation_name}_${timestamp}.log"

echo "[$(date '+%H:%M:%S')] spawning operation: ${operation_name}" | tee -a "$LOG_FILE"
echo "[$(date '+%H:%M:%S')] project: ${project_dir}" | tee -a "$LOG_FILE"
echo "[$(date '+%H:%M:%S')] operation file: ${operation_file}" | tee -a "$LOG_FILE"
if [[ ${#extra_args[@]} -gt 0 ]]; then
    echo "[$(date '+%H:%M:%S')] extra args: ${extra_args[*]}" | tee -a "$LOG_FILE"
fi

# The dispatcher hands the operation markdown to a Claude session as the
# entry context. The session reads the step table and dispatches agents
# via the Agent tool, identical to the develop-feature pattern but
# parameterized on the operation file rather than hardcoded.
cmd=(claude -p --dangerously-skip-permissions --model sonnet
    --append-system-prompt "You are the Hypervisor for a workforce operation. Read operations/${operation_name}.md, follow the step table mechanically, and dispatch agents via the Agent tool. Work in ${project_dir}.")

prompt_file="${ORRCH_DIR}/.spawn_prompt_${RANDOM}.txt"
{
    printf 'Execute the operation defined in operations/%s.md.\n\n' "$operation_name"
    printf 'Project directory: %s\n' "$project_dir"
    if [[ ${#extra_args[@]} -gt 0 ]]; then
        printf 'Extra context: %s\n' "${extra_args[*]}"
    fi
    printf '\nOperation definition:\n\n'
    cat "$operation_file"
} > "$prompt_file"

if command -v script >/dev/null 2>&1; then
    script -qfc "${cmd[*]} < '${prompt_file}'" "$LOG_FILE"
else
    "${cmd[@]}" < "$prompt_file" 2>&1 | tee -a "$LOG_FILE"
fi

rm -f "$prompt_file"
echo "[$(date '+%H:%M:%S')] operation complete; log at $LOG_FILE"
