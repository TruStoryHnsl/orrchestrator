#!/usr/bin/env bash
# ─── merge_to_main.sh ──────────────────────────────────────────────────
# Tiered merge of a session branch back to main.
#
# Strategy (cheapest → most expensive):
#   1. Plain git merge with -X patience (auto-resolves disjoint changes)
#   2. `.gitattributes` with `merge=union` auto-concatenates additive files
#      (PLAN.md, DEVLOG.md, CHANGELOG.md, instructions_inbox.md, etc.) —
#      no conflict markers, no stops.
#   3. For remaining conflicts, dispatch `claude -p` per file to decide:
#        COMBINE      — both changes kept, merged intelligently
#        PICK_OURS    — main's version supersedes (more complete/recent)
#        PICK_THEIRS  — session's version supersedes
#        ESCALATE     — genuine logic contradiction; stop + escalate
#   4. ANY ESCALATE aborts the merge entirely — the user decides.
#
# Safety net: tags main with `pre-merge-<branch>-<timestamp>` before
# touching it, so a bad auto-resolve is one `git reset` away.
#
# Usage:
#   merge_to_main.sh [--no-llm] [--dry-run] [--main-branch <name>]
#
# Exit codes:
#   0   merge complete, branch deleted
#   1   escalation required (user must resolve)
#   2   setup error (wrong state, no repo, dirty tree)
# ──────────────────────────────────────────────────────────────────────
set -uo pipefail

USE_LLM=1
DRY_RUN=0
MAIN_BRANCH=""
SB=""
CHECKPOINT=""
LOG_FILE=""
REPO_ROOT=""

log() {
    echo "[merge] $*"
    if [[ -n "$LOG_FILE" ]]; then
        echo "- $(date -Iseconds) | $*" >> "$LOG_FILE" 2>/dev/null || true
    fi
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --no-llm)       USE_LLM=0 ;;
            --dry-run)      DRY_RUN=1 ;;
            --main-branch)  MAIN_BRANCH="${2:-}"; shift ;;
            -h|--help)
                sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
                exit 0
                ;;
            *) echo "unknown option: $1" >&2; exit 2 ;;
        esac
        shift
    done
}

preflight() {
    REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
        echo "[merge] not in a git repo — skipping"
        exit 0
    }
    cd "$REPO_ROOT"

    SB="$(git branch --show-current)"
    case "$SB" in
        ""|main|master|develop|trunk)
            echo "[merge] on '$SB' (no session branch to merge) — skipping"
            exit 0
            ;;
    esac

    # Allow submodule pointer drift (cross-session state unrelated to merge).
    # Block only real uncommitted file changes.
    if ! git diff --quiet --ignore-submodules || ! git diff --cached --quiet --ignore-submodules; then
        echo "[merge] ERROR: uncommitted changes. Commit or stash first." >&2
        git status --short --ignore-submodules | head -10 >&2
        exit 2
    fi
    if ! git diff --quiet; then
        echo "[merge] note: submodule pointer drift present but ignored"
    fi

    if [[ -z "$MAIN_BRANCH" ]]; then
        for candidate in main master trunk; do
            if git rev-parse --verify "$candidate" >/dev/null 2>&1; then
                MAIN_BRANCH="$candidate"
                break
            fi
        done
    fi
    if [[ -z "$MAIN_BRANCH" ]]; then
        echo "[merge] ERROR: cannot find main/master/trunk branch" >&2
        exit 2
    fi

    local ts; ts="$(date +%Y%m%d-%H%M%S)"
    CHECKPOINT="pre-merge-${SB//\//-}-${ts}"
    LOG_FILE="${REPO_ROOT}/.orrch/merge_log.md"
    mkdir -p "$(dirname "$LOG_FILE")" 2>/dev/null || true

    log "session branch: $SB"
    log "main branch:    $MAIN_BRANCH"
    log "checkpoint tag: $CHECKPOINT"
    [[ $DRY_RUN -eq 1 ]] && log "DRY RUN — no commits or pushes"
}

finish_merge_and_exit() {
    local code="${1:-0}"

    if [[ $DRY_RUN -eq 1 ]]; then
        log "DRY RUN complete — aborting in-progress merge"
        git merge --abort 2>/dev/null || true
        exit "$code"
    fi

    # Stage any pending LLM resolutions
    if ! git diff --cached --quiet 2>/dev/null; then
        git commit --no-edit 2>&1 | sed 's/^/  /' || true
    elif git rev-parse MERGE_HEAD >/dev/null 2>&1 && [[ -z "$(git ls-files --unmerged)" ]]; then
        git commit --no-edit 2>&1 | sed 's/^/  /' || true
    fi

    if [[ -n "$(git ls-files --unmerged)" ]]; then
        log "ERROR: files still unmerged after resolution attempt"
        exit 2
    fi

    git push origin "$MAIN_BRANCH" 2>/dev/null || log "(push skipped — no remote or private scope)"
    git branch -d "$SB" 2>/dev/null || log "(could not fast-forward-delete local branch $SB)"
    git push origin --delete "$SB" 2>/dev/null || true

    log "merged $SB → $MAIN_BRANCH"
    log "undo: git reset --hard $CHECKPOINT"
    exit "$code"
}

abort_and_exit() {
    local code="${1:-1}"
    if git rev-parse MERGE_HEAD >/dev/null 2>&1; then
        git merge --abort 2>/dev/null || true
    fi
    log "merge aborted — main restored to checkpoint $CHECKPOINT"
    log "session branch $SB left intact for manual resolution"
    exit "$code"
}

resolve_with_llm() {
    local f="$1"
    local tmpdir; tmpdir="$(mktemp -d)"

    local size_lines
    size_lines="$(wc -l < "$f" 2>/dev/null || echo 0)"
    if [[ $size_lines -gt 3000 ]]; then
        log "  $f: too large ($size_lines lines) — escalating"
        rm -rf "$tmpdir"
        return 1
    fi

    git show ":1:$f" > "$tmpdir/base"   2>/dev/null || : > "$tmpdir/base"
    git show ":2:$f" > "$tmpdir/ours"   2>/dev/null || : > "$tmpdir/ours"
    git show ":3:$f" > "$tmpdir/theirs" 2>/dev/null || : > "$tmpdir/theirs"
    cp "$f" "$tmpdir/conflicted"

    local prompt_file="$tmpdir/prompt"
    {
        echo "You are a git merge conflict resolver."
        echo ""
        echo "CONTEXT: Two parallel development sessions modified the same file. Git already tried its normal merge (with -X patience and merge=union for additive files). The remaining conflicts need judgment."
        echo ""
        echo "YOUR JOB: for this file, decide one of four verdicts and output the resolved file content (unless ESCALATE)."
        echo ""
        echo "VERDICTS:"
        echo "  COMBINE      — both changes are compatible and independent; both should survive. Merge intelligently, preserve intent of both sides."
        echo "  PICK_OURS    — main's version clearly supersedes (more complete, more recent approach, or theirs is a subset)."
        echo "  PICK_THEIRS  — session branch's version clearly supersedes."
        echo "  ESCALATE     — genuine contradiction: mutually exclusive implementations, conflicting values, or any case you are unsure about. BIAS TOWARD ESCALATE when uncertain — the human can resolve in seconds; a silent wrong merge is expensive."
        echo ""
        echo "WHAT COUNTS AS 'COMBINE':"
        echo "  - Different imports → keep both"
        echo "  - Different functions added to the same file → keep both"
        echo "  - Different list entries / enum variants / match arms → keep both"
        echo "  - Different documentation sections → keep both"
        echo ""
        echo "WHAT COUNTS AS 'ESCALATE':"
        echo "  - Same function implemented two different ways"
        echo "  - Same config value set to two different values"
        echo "  - Same prose rewritten incompatibly"
        echo "  - Schema/API changes that break callers if both kept"
        echo "  - ANY case you are unsure about"
        echo ""
        echo "OUTPUT FORMAT (REQUIRED):"
        echo "  Line 1: VERDICT: <COMBINE|PICK_OURS|PICK_THEIRS|ESCALATE>"
        echo "  Line 2: REASON: <one short sentence>"
        echo "  Line 3+: if verdict is not ESCALATE, the full resolved file content. No code fences, no commentary, no markdown wrapping — just raw file bytes. Content from line 3 onward is written verbatim to disk."
        echo ""
        echo "FILE: $f"
        echo ""
        echo "=== BASE (common ancestor) ==="
        cat "$tmpdir/base"
        echo ""
        echo "=== OURS (main) ==="
        cat "$tmpdir/ours"
        echo ""
        echo "=== THEIRS (session branch $SB) ==="
        cat "$tmpdir/theirs"
        echo ""
        echo "=== CONFLICTED (file as git left it, with <<<<<<< markers) ==="
        cat "$tmpdir/conflicted"
    } > "$prompt_file"

    local output_file="$tmpdir/resolved"
    if ! claude -p \
        --model sonnet \
        --dangerously-skip-permissions \
        --allowed-tools "" \
        < "$prompt_file" > "$output_file" 2>/dev/null
    then
        log "  $f: LLM call failed — escalating"
        rm -rf "$tmpdir"
        return 1
    fi

    local verdict reason
    verdict="$(sed -n '1s/^VERDICT: *//p' "$output_file" | tr -d '[:space:]')"
    reason="$(sed -n '2s/^REASON: *//p' "$output_file")"

    case "$verdict" in
        COMBINE|PICK_OURS|PICK_THEIRS)
            if [[ $DRY_RUN -eq 0 ]]; then
                tail -n +3 "$output_file" > "$f"
            fi
            log "  $f: $verdict — $reason"
            rm -rf "$tmpdir"
            return 0
            ;;
        ESCALATE)
            log "  $f: ESCALATE — $reason"
            rm -rf "$tmpdir"
            return 1
            ;;
        *)
            log "  $f: unparseable LLM verdict ('$verdict') — escalating"
            rm -rf "$tmpdir"
            return 1
            ;;
    esac
}

main() {
    parse_args "$@"
    preflight

    if [[ $DRY_RUN -eq 0 ]]; then
        git checkout "$MAIN_BRANCH" || { log "checkout $MAIN_BRANCH failed"; exit 2; }
        git pull --ff-only origin "$MAIN_BRANCH" 2>/dev/null || true
        git tag "$CHECKPOINT" HEAD || true
    fi

    log "attempt 1: git merge --no-ff -X patience"
    local merge_output
    merge_output="$(git merge --no-ff -X patience "$SB" -m "merge: $SB" 2>&1 || true)"
    echo "$merge_output" | sed 's/^/  /'

    # Clean merge?
    if [[ -z "$(git ls-files --unmerged)" ]] && ! echo "$merge_output" | grep -q "CONFLICT"; then
        log "clean merge (patience + union attributes resolved everything)"
        finish_merge_and_exit 0
    fi

    local -a UNMERGED
    mapfile -t UNMERGED < <(git ls-files --unmerged | awk '{print $4}' | sort -u)

    if [[ ${#UNMERGED[@]} -eq 0 ]]; then
        log "no files still unmerged after patience — proceeding to commit"
        finish_merge_and_exit 0
    fi

    log "${#UNMERGED[@]} file(s) still conflicted after patience + union:"
    for f in "${UNMERGED[@]}"; do log "  $f"; done

    if [[ $USE_LLM -eq 0 ]]; then
        log "--no-llm set; aborting"
        abort_and_exit 1
    fi

    if ! command -v claude >/dev/null 2>&1; then
        log "WARNING: 'claude' CLI not in PATH — cannot LLM-resolve"
        abort_and_exit 1
    fi

    local -a ESCALATED=() RESOLVED=()
    for f in "${UNMERGED[@]}"; do
        if resolve_with_llm "$f"; then
            RESOLVED+=("$f")
        else
            ESCALATED+=("$f")
        fi
    done

    if [[ ${#ESCALATED[@]} -gt 0 ]]; then
        log ""
        log "!!! ESCALATION REQUIRED !!!"
        log "Files with genuine logic conflicts (another session made incompatible changes):"
        for f in "${ESCALATED[@]}"; do log "  $f"; done
        log ""
        log "Resolve manually, then commit. Or abandon this merge:"
        log "  git merge --abort && git reset --hard $CHECKPOINT"
        log ""
        abort_and_exit 1
    fi

    log "resolved ${#RESOLVED[@]} file(s) via LLM"
    if [[ $DRY_RUN -eq 0 ]]; then
        for f in "${RESOLVED[@]}"; do
            git add "$f"
        done
    fi
    finish_merge_and_exit 0
}

main "$@"
