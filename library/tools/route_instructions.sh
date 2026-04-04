#!/usr/bin/env bash
set -euo pipefail

# route_instructions.sh — Append optimized instructions to a project's instructions_inbox.md
# Usage: route_instructions.sh <project_dir> <instructions_text>

usage() {
    echo "Usage: route_instructions.sh <project_dir> <instructions_text>" >&2
    echo "  project_dir      — absolute path to the target project directory" >&2
    echo "  instructions_text — the instruction content to append" >&2
    exit 1
}

if [[ $# -lt 2 ]]; then
    usage
fi

project_dir="$1"
instructions_text="$2"

if [[ ! -d "$project_dir" ]]; then
    echo "Error: project directory does not exist: $project_dir" >&2
    exit 1
fi

if [[ -z "$instructions_text" ]]; then
    echo "Error: instructions_text cannot be empty" >&2
    exit 1
fi

inbox="$project_dir/instructions_inbox.md"
timestamp="$(date '+%Y-%m-%d %H:%M:%S')"

# Create file with header if it doesn't exist
if [[ ! -f "$inbox" ]]; then
    printf '# Instruction Inbox\n\n' > "$inbox"
fi

# Append instruction with timestamp separator
{
    printf '\n---\n\n'
    printf '### Routed: %s\n\n' "$timestamp"
    printf '%s\n' "$instructions_text"
} >> "$inbox"

echo "Instruction appended to $inbox at $timestamp"
