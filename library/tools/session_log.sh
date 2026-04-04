#!/usr/bin/env bash
set -euo pipefail

# session_log.sh — Append a session development log entry
# Usage: session_log.sh <project_dir> <version_tag> <summary>

usage() {
    echo "Usage: session_log.sh <project_dir> <version_tag> <summary>" >&2
    echo "  project_dir  — absolute path to the target project directory" >&2
    echo "  version_tag  — version or tag string (e.g., v0.3.0, WIP)" >&2
    echo "  summary      — brief summary of work done" >&2
    exit 1
}

if [[ $# -lt 3 ]]; then
    usage
fi

project_dir="$1"
version_tag="$2"
summary="$3"

if [[ ! -d "$project_dir" ]]; then
    echo "Error: project directory does not exist: $project_dir" >&2
    exit 1
fi

if [[ -z "$version_tag" ]]; then
    echo "Error: version_tag cannot be empty" >&2
    exit 1
fi

if [[ -z "$summary" ]]; then
    echo "Error: summary cannot be empty" >&2
    exit 1
fi

log_file="$project_dir/.dev_log.md"
timestamp="$(date '+%Y-%m-%d %H:%M:%S')"

# Create file with header if it doesn't exist
if [[ ! -f "$log_file" ]]; then
    printf '# Development Log\n\n' > "$log_file"
fi

# Append log entry
{
    printf '## %s — %s\n\n' "$timestamp" "$version_tag"
    printf '%s\n\n' "$summary"
} >> "$log_file"

echo "Log entry appended to $log_file ($version_tag at $timestamp)"
