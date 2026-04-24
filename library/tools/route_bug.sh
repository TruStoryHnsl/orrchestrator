#!/usr/bin/env bash
set -euo pipefail

# route_bug.sh — Append a structured bug report to a project's bugs_inbox.md
# Usage: route_bug.sh <project_dir> <bug_text> [source_idea_filename]
#
# Auto-increments BUG-YYYY-MM-DD-NNN by counting today's existing entries.

usage() {
    echo "Usage: route_bug.sh <project_dir> <bug_text> [source_idea_filename]" >&2
    echo "  project_dir            — absolute path to the target project directory" >&2
    echo "  bug_text               — structured bug report content (from bug-parse skill)" >&2
    echo "  source_idea_filename   — optional filename of the originating idea" >&2
    exit 1
}

if [[ $# -lt 2 ]]; then
    usage
fi

project_dir="$1"
bug_text="$2"
source_idea="${3:-}"

if [[ ! -d "$project_dir" ]]; then
    echo "Error: project directory does not exist: $project_dir" >&2
    exit 1
fi

if [[ -z "$bug_text" ]]; then
    echo "Error: bug_text cannot be empty" >&2
    exit 1
fi

inbox="$project_dir/bugs_inbox.md"
today="$(date '+%Y-%m-%d')"
timestamp="$(date '+%Y-%m-%d %H:%M:%S')"

if [[ ! -f "$inbox" ]]; then
    printf '# Bugs Inbox\n\nReports of confirmed and reproducible bugs awaiting fix. New entries are appended by `intake_bugreport`. Cleared by `develop_feature` when fixes verify.\n' > "$inbox"
fi

# Auto-increment NNN: count BUG-<today>-NNN entries already in the file.
# `grep -c` always prints a count; `|| true` swallows its exit-1-on-no-matches
# without doubling the output.
existing_count=$(grep -cE "^### BUG-${today}-[0-9]{3}" "$inbox" 2>/dev/null || true)
existing_count=${existing_count:-0}
next_seq=$(printf '%03d' $((existing_count + 1)))
bug_id="BUG-${today}-${next_seq}"

{
    printf '\n---\n\n'
    printf '### %s\n\n' "$bug_id"
    printf '**Routed:** %s\n' "$timestamp"
    printf '**Status:** pending\n'
    if [[ -n "$source_idea" ]]; then
        printf '**Source idea:** %s\n' "$source_idea"
    fi
    printf '\n%s\n' "$bug_text"
} >> "$inbox"

echo "$bug_id appended to $inbox"
