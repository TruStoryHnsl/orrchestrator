#!/usr/bin/env bash
# compress_output.sh — deterministically extract structured data from raw agent output
# Replaces LLM-based "reasoning about what to keep" with pattern-matched extraction.
# Usage:
#   compress_output.sh [input_file]   # read from file
#   ... | compress_output.sh          # read from stdin
# Output: compact ~20-40 line summary on stdout

set -euo pipefail

# ── Input ─────────────────────────────────────────────────────────────────────

if [[ $# -ge 1 ]]; then
    if [[ ! -f "$1" ]]; then
        echo "ERROR: file not found: $1" >&2
        exit 1
    fi
    INPUT="$(cat "$1")"
else
    INPUT="$(cat)"
fi

if [[ -z "$INPUT" ]]; then
    echo "## Agent Output Summary"
    echo ""
    echo "(empty input)"
    exit 0
fi

# ── Helpers ───────────────────────────────────────────────────────────────────

# Print lines matching a pattern, stripping ANSI codes, deduplicating
extract_lines() {
    local pattern="$1"
    printf '%s\n' "$INPUT" \
        | sed 's/\x1b\[[0-9;]*m//g' \
        | grep -E "$pattern" \
        | sort -u \
        || true
}

# ── FILE EXTRACTION ───────────────────────────────────────────────────────────
# Patterns captured:
#   - Explicit labels: Modified:, Created:, File:, Changed:, Updated:, Wrote:, Read:
#   - Bare paths: lines where a token contains crates/, src/, library/, or
#     ends with .rs/.sh/.toml/.md and looks like a path (contains /)
#   - Bullet items: "- path/to/file"

# Pass 1: lines with explicit file-operation labels
LABELED_LINES="$(printf '%s\n' "$INPUT" \
    | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -Ei '(^|[[:space:]])(modified|created|file|changed|updated|wrote|wrote to|wrote:|editing|edited|read):[[:space:]]' \
    || true)"

# Pass 2: lines that contain bare file paths (crates/… src/… library/…)
PATH_LINES="$(printf '%s\n' "$INPUT" \
    | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -Eo '(crates|src|library)/[A-Za-z0-9_./-]+\.(rs|sh|toml|md|json)' \
    | sort -u \
    || true)"

# Pass 3: bullet items that look like paths
BULLET_PATHS="$(printf '%s\n' "$INPUT" \
    | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -E '^[[:space:]]*[-*] +(crates|src|library)/[A-Za-z0-9_./-]+' \
    | grep -Eo '(crates|src|library)/[A-Za-z0-9_./-]+' \
    | sort -u \
    || true)"

# Combine all found paths into one sorted-unique list
ALL_PATHS="$(printf '%s\n%s\n%s\n' "$PATH_LINES" "$BULLET_PATHS" \
    "$(printf '%s\n' "$LABELED_LINES" \
        | grep -Eo '(crates|src|library)/[A-Za-z0-9_./-]+' \
        || true)" \
    | grep -v '^$' | sort -u || true)"

# Classify each path as Created or Modified using nearby context
classify_paths() {
    while IFS= read -r path; do
        [[ -z "$path" ]] && continue
        # Look for "creat" keyword on any line that also contains this path
        local created_hit
        created_hit="$(printf '%s\n' "$INPUT" \
            | sed 's/\x1b\[[0-9;]*m//g' \
            | grep -i "creat" \
            | grep -F "$path" \
            || true)"
        if [[ -n "$created_hit" ]]; then
            printf 'Created: %s\n' "$path"
        else
            printf 'Modified: %s\n' "$path"
        fi
    done <<< "$ALL_PATHS"
}

FILES_SECTION=""
if [[ -n "$ALL_PATHS" ]]; then
    FILES_SECTION="$(classify_paths)"
fi

# ── CHANGES EXTRACTION ────────────────────────────────────────────────────────
# Look for lines that describe additions/implementations.
# Patterns:
#   - Lines with a leading + (diff-style or summary bullet)
#   - Lines containing "added", "implemented", "introduced", "new function", "new struct"
#   - Symbol names: words followed by () or starting with uppercase (PascalCase)

CHANGES_LINES="$(printf '%s\n' "$INPUT" \
    | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -E '(^[[:space:]]*\+[^+]|added|implemented|introduced|new function|new struct|new enum|new trait|new module)' \
    | grep -v '^+++' \
    | grep -v '^---' \
    | sed 's/^[[:space:]]*//' \
    | sort -u \
    | head -20 \
    || true)"

# Also extract lines describing per-file changes (common agent summary format):
# "- filename.rs: +Foo, +bar()"
FILE_CHANGE_LINES="$(printf '%s\n' "$INPUT" \
    | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -E '^[[:space:]]*[-*] +[A-Za-z0-9_.-]+\.(rs|sh|toml|md):[[:space:]]*\+' \
    | sed 's/^[[:space:]]*//' \
    | head -15 \
    || true)"

# ── STATUS EXTRACTION ─────────────────────────────────────────────────────────

# Build result
BUILD_LINE="$(printf '%s\n' "$INPUT" \
    | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -Ei '(Finished|Compiling|error\[|^error:|cargo build|cargo check|build (passed|failed|succeeded|ok))' \
    | tail -5 \
    || true)"

# Test result
TEST_LINE="$(printf '%s\n' "$INPUT" \
    | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -Ei '(test result:|tests? (passed|failed)|[0-9]+ passed|[0-9]+ failed|running [0-9]+ test)' \
    | tail -3 \
    || true)"

# Concerns / warnings / issues
ISSUES_LINES="$(printf '%s\n' "$INPUT" \
    | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -Ei '(concern|warning:|issue:|bug:|blocker|TODO:|FIXME:|note:|error:)' \
    | grep -v '^[[:space:]]*//' \
    | grep -v 'extern\|use \|mod \|#\[' \
    | sed 's/^[[:space:]]*//' \
    | sort -u \
    | head -10 \
    || true)"

# ── FALLBACK ──────────────────────────────────────────────────────────────────

# Determine if we found anything useful
FOUND_SOMETHING=false
[[ -n "$FILES_SECTION" ]] && FOUND_SOMETHING=true
[[ -n "$CHANGES_LINES" || -n "$FILE_CHANGE_LINES" ]] && FOUND_SOMETHING=true
[[ -n "$BUILD_LINE" || -n "$TEST_LINE" || -n "$ISSUES_LINES" ]] && FOUND_SOMETHING=true

# ── OUTPUT ────────────────────────────────────────────────────────────────────

echo "## Agent Output Summary"
echo ""

if [[ "$FOUND_SOMETHING" == false ]]; then
    echo "### Fallback (no structured patterns found — last 20 lines)"
    echo ""
    printf '%s\n' "$INPUT" | tail -20
    exit 0
fi

# Files section
echo "### Files"
if [[ -n "$FILES_SECTION" ]]; then
    printf '%s\n' "$FILES_SECTION"
else
    echo "(none detected)"
fi
echo ""

# Changes section
echo "### Changes"
if [[ -n "$FILE_CHANGE_LINES" ]]; then
    printf '%s\n' "$FILE_CHANGE_LINES"
elif [[ -n "$CHANGES_LINES" ]]; then
    printf '%s\n' "$CHANGES_LINES"
else
    echo "(none detected)"
fi
echo ""

# Status section
echo "### Status"

if [[ -n "$BUILD_LINE" ]]; then
    # Summarise: look for pass/fail keywords in the raw cargo output
    if printf '%s\n' "$BUILD_LINE" | grep -qi 'error\['; then
        # Extract first error line as the summary
        first_err="$(printf '%s\n' "$BUILD_LINE" | grep -i 'error\[' | head -1)"
        echo "Build: FAIL — $first_err"
    elif printf '%s\n' "$BUILD_LINE" | grep -qiE 'Finished|build (passed|succeeded|ok)'; then
        echo "Build: pass"
    else
        echo "Build: $(printf '%s\n' "$BUILD_LINE" | tail -1)"
    fi
else
    echo "Build: (no cargo output detected)"
fi

if [[ -n "$TEST_LINE" ]]; then
    # Summarise test result to one line
    summary="$(printf '%s\n' "$TEST_LINE" \
        | grep -Ei 'test result:|[0-9]+ passed' \
        | tail -1)"
    if [[ -n "$summary" ]]; then
        echo "Tests: $summary"
    else
        echo "Tests: $(printf '%s\n' "$TEST_LINE" | tail -1)"
    fi
else
    echo "Tests: (no test output detected)"
fi

if [[ -n "$ISSUES_LINES" ]]; then
    echo "Issues:"
    printf '%s\n' "$ISSUES_LINES" | sed 's/^/  /'
else
    echo "Issues: none"
fi
