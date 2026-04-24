#!/usr/bin/env bash
set -euo pipefail

# ledger_append.sh — Append a pending entry to a project's .bugfix-ledger.md
# Usage: ledger_append.sh <project_dir> <bug_id> <bug_text>
#
# The structured bug text is expected to come from the bug-parse skill and
# already include Severity / Symptoms / Reproduction sections. We extract those
# and emit a ledger entry matching the existing ledger schema with the
# Solution / Verification / Prevention sections left blank for develop_feature
# to fill in when the bug is fixed.

usage() {
    echo "Usage: ledger_append.sh <project_dir> <bug_id> <bug_text>" >&2
    echo "  project_dir — absolute path to the target project directory" >&2
    echo "  bug_id      — BUG-YYYY-MM-DD-NNN identifier (from route_bug.sh)" >&2
    echo "  bug_text    — structured bug report (from bug-parse skill)" >&2
    exit 1
}

if [[ $# -lt 3 ]]; then
    usage
fi

project_dir="$1"
bug_id="$2"
bug_text="$3"

if [[ ! -d "$project_dir" ]]; then
    echo "Error: project directory does not exist: $project_dir" >&2
    exit 1
fi

ledger="$project_dir/.bugfix-ledger.md"
today="$(date '+%Y-%m-%d')"

# Best-effort field extraction from bug_text. If a field is absent, leave it
# blank — the user-audit gate at intake catches material omissions.
title=$(printf '%s' "$bug_text" | grep -m1 -E '^### ' | sed 's/^### //' || echo "untitled")
severity=$(printf '%s' "$bug_text" | grep -m1 -iE '^\*\*severity:\*\*' | sed -E 's/.*\*\*[Ss]everity:\*\*[[:space:]]*//' || echo "unknown")
affected=$(printf '%s' "$bug_text" | grep -m1 -iE '^\*\*affected:\*\*' | sed -E 's/.*\*\*[Aa]ffected:\*\*[[:space:]]*//' || echo "unknown")
trigger=$(printf '%s' "$bug_text" | awk '/^#### Reproduction/{flag=1; next} /^#### /{flag=0} flag && /^[0-9]+\./{print; exit}' || true)
symptoms=$(printf '%s' "$bug_text" | awk '/^#### Symptoms/{flag=1; next} /^#### /{flag=0} flag' || true)

if [[ ! -f "$ledger" ]]; then
    printf '# Bugfix Ledger\n\nRecorded bugs and the working solutions that fixed them. New entries are appended in pending state by `intake_bugreport`; the Solution / Verification / Prevention sections are filled by `develop_feature` when a fix lands.\n' > "$ledger"
fi

{
    printf '\n---\n\n'
    printf '### %s — %s\n\n' "$bug_id" "$title"
    printf '**Date:** %s\n' "$today"
    printf '**Severity:** %s\n' "$severity"
    printf '**Affected:** %s\n' "$affected"
    if [[ -n "$trigger" ]]; then
        printf '**Trigger:** %s\n' "$trigger"
    else
        printf '**Trigger:** unknown\n'
    fi
    printf '**Status:** pending\n'

    printf '\n#### Symptoms\n'
    if [[ -n "$symptoms" ]]; then
        printf '%s\n' "$symptoms"
    else
        printf '_see %s in bugs_inbox.md_\n' "$bug_id"
    fi

    printf '\n#### Root cause\n_pending — to be filled by develop_feature_\n'
    printf '\n#### Solution\n_pending — to be filled by develop_feature_\n'
    printf '\n#### Verification\n_pending — to be filled by develop_feature_\n'
    printf '\n#### Prevention\n_pending — to be filled by develop_feature_\n'
} >> "$ledger"

echo "$bug_id appended to $ledger (status: pending)"
