#!/usr/bin/env bash
set -euo pipefail

# ─── run_workflow.sh ─────────────────────────────────────────────────
# Shell dispatcher for the develop-feature workflow.
# The Hypervisor is THIS SCRIPT — not an LLM. Each step spawns a
# claude -p subprocess with full agentic tool use, captures output,
# pipes through deterministic compression tools, feeds to next step.
# ─────────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="${1:-.}"
# Resolve to absolute path
PROJECT_DIR="$(cd "${PROJECT_DIR}" && pwd)"
GOAL="${2:-continue development}"
ORRCH_DIR="${PROJECT_DIR}/.orrch"
TOOLS_DIR="${SCRIPT_DIR}"
AGENTS_DIR="${SCRIPT_DIR}/../../agents"
LOG_FILE="${ORRCH_DIR}/workflow.log"

# Claude CLI flags for agent subprocess calls
CLAUDE_BASE=(claude -p --dangerously-skip-permissions --model sonnet)

# ─── Helpers ─────────────────────────────────────────────────────────

log() {
    local msg="[workflow] $(date +%H:%M:%S) $*"
    echo "${msg}"
    echo "${msg}" >> "${LOG_FILE}" 2>/dev/null
}
update_status() {
    echo "{\"workflow\":\"develop-feature\",\"step\":$1,\"status\":\"$2\"}" > "${ORRCH_DIR}/workflow.json"
}

run_agent() {
    local name="$1"
    local tools="$2"
    local prompt="$3"
    local output_file="$4"

    log "spawning agent: ${name}"
    echo "────────────────── ${name} ──────────────────"

    # Write prompt to a temp file — passing large prompts as shell args
    # exceeds argument limits and causes claude -p to hang.
    local prompt_file="${ORRCH_DIR}/.prompt_${RANDOM}.txt"
    echo "${prompt}" > "${prompt_file}"

    # claude -p runs the full agentic loop (tool use, file edits, reasoning)
    # and exits when done. Pipe prompt via stdin.
    # Use script(1) to capture output while preserving TTY behavior —
    # tee breaks because pipes cause block buffering, hiding all output.
    script -qfc "${CLAUDE_BASE[*]} \
        --allowed-tools '${tools}' \
        --append-system-prompt 'You are the ${name}. Work in ${PROJECT_DIR}.' \
        < '${prompt_file}'" \
        "${output_file}"

    rm -f "${prompt_file}"
    echo ""
    echo "────────────────── /${name} ──────────────────"
    log "agent done: ${name} ($(wc -l < "${output_file}" 2>/dev/null || echo 0) lines)"
}

run_agent_bg() {
    # Same as run_agent but backgrounded. Output goes to file only.
    local name="$1"
    local tools="$2"
    local prompt="$3"
    local output_file="$4"

    local prompt_file="${ORRCH_DIR}/.prompt_bg_${RANDOM}.txt"
    echo "${prompt}" > "${prompt_file}"

    log "spawning agent (background): ${name}"
    "${CLAUDE_BASE[@]}" \
        --allowed-tools "${tools}" \
        --append-system-prompt "You are the ${name}. Work in ${PROJECT_DIR}." \
        < "${prompt_file}" \
        > "${output_file}" 2>/dev/null &
}

# ─── INIT ────────────────────────────────────────────────────────────

mkdir -p "${ORRCH_DIR}"
: > "${LOG_FILE}"  # truncate log

# Trap errors so we see what went wrong
trap 'log "ERROR: script failed at line $LINENO (exit $?)"; update_status -1 "error"; echo "--- Workflow failed. See ${LOG_FILE} ---"; read -r -p "Press Enter to close..."' ERR

log "=== develop-feature workflow ==="
log "project: ${PROJECT_DIR}"
log "goal: ${GOAL}"

update_status 0 "init"

SCOPE=$(cat "${PROJECT_DIR}/.scope" 2>/dev/null || echo "private")
log "scope: ${SCOPE}"

# ─── STEP 1: Parse instructions ─────────────────────────────────────

update_status 1 "parsing"

if [[ "${GOAL}" == "continue development" || "${GOAL}" == "continue" ]]; then
    log "reading dev map for unchecked items..."

    # Check inbox for unintegrated stragglers
    INBOX_STRAGGLERS=""
    if [[ -f "${PROJECT_DIR}/instructions_inbox.md" ]]; then
        # Lines that look like instruction entries but aren't marked implemented
        INBOX_STRAGGLERS=$(grep -E '^###\s' "${PROJECT_DIR}/instructions_inbox.md" \
            | grep -v '~~' | grep -v 'IMPLEMENTED' | grep -v 'No pending' || true)
    fi

    # Read PLAN.md for uncompleted items from lowest incomplete phase
    if [[ ! -f "${PROJECT_DIR}/PLAN.md" ]]; then
        log "ERROR: No PLAN.md found. Nothing to do."
        exit 0
    fi

    # Detect plan format and extract uncompleted items:
    #   Format A (orrchestrator): "N. [ ] **Feature**" — checkbox style
    #   Format B (visualizorr etc): "### Task N: Title" — task-header style
    #   Format C: "- [ ] item" — markdown task list
    INSTRUCTIONS=""
    if grep -q '\[ \]' "${PROJECT_DIR}/PLAN.md"; then
        # Format A/C: checkbox items
        INSTRUCTIONS=$(grep -n '\[ \]' "${PROJECT_DIR}/PLAN.md" || true)
        log "plan format: checkbox ($(echo "${INSTRUCTIONS}" | wc -l) unchecked items)"
    elif grep -qE '^### Task [0-9]+:' "${PROJECT_DIR}/PLAN.md"; then
        # Format B: task headers — treat ALL tasks as uncompleted unless marked DONE/COMPLETE
        INSTRUCTIONS=$(grep -nE '^### Task [0-9]+:' "${PROJECT_DIR}/PLAN.md" | grep -iv 'DONE\|COMPLETE\|✓' || true)
        log "plan format: task-header ($(echo "${INSTRUCTIONS}" | wc -l) tasks)"
    else
        # Fallback: pass the whole PLAN.md to the PM and let it figure out what to do
        INSTRUCTIONS=$(cat "${PROJECT_DIR}/PLAN.md")
        log "plan format: unstructured (passing full plan to PM)"
    fi

    if [[ -z "${INSTRUCTIONS}" ]]; then
        log "Dev map is complete — no uncompleted items in PLAN.md."
        exit 0
    fi

    # Write instructions to disk
    echo "${INSTRUCTIONS}" > "${ORRCH_DIR}/instructions.md"
    if [[ -n "${INBOX_STRAGGLERS}" ]]; then
        echo -e "\n## Inbox stragglers (not yet in dev map)\n${INBOX_STRAGGLERS}" >> "${ORRCH_DIR}/instructions.md"
    fi
else
    # Explicit goal — use as-is
    echo "${GOAL}" > "${ORRCH_DIR}/instructions.md"
fi

INSTRUCTION_COUNT=$(wc -l < "${ORRCH_DIR}/instructions.md")
log "instructions: ${INSTRUCTION_COUNT} lines"

# ─── STEP 2: Codebase brief ─────────────────────────────────────────

update_status 2 "brief"
log "generating codebase brief..."

"${TOOLS_DIR}/codebase_brief.sh" "${PROJECT_DIR}" > "${ORRCH_DIR}/codebase_brief.txt" 2>/dev/null
log "brief: $(wc -l < "${ORRCH_DIR}/codebase_brief.txt") lines"

# ─── STEP 3: PM plans ───────────────────────────────────────────────

update_status 3 "planning"

PM_PROMPT="You are the Project Manager. Plan and delegate — never write code.

## Inbox check
Verify these inbox items are already in the dev map. If any are missing, add them to your task list:
$(grep 'Inbox stragglers' "${ORRCH_DIR}/instructions.md" 2>/dev/null || echo 'none')

## Dev map items to implement
$(cat "${ORRCH_DIR}/instructions.md")

## Codebase
$(cat "${ORRCH_DIR}/codebase_brief.txt")

## MANDATORY output format (downstream tools parse this exactly)
For each task, output EXACTLY this block:

TASK <id>: <one-line description>
Agent: <Developer or Software Engineer or UI Designer or Researcher or Feature Tester>
Files: <comma-separated paths of files this task will read or modify>
Work: <what to do, 2-3 sentences max>
Acceptance: <one-line pass/fail criteria>
Depends: <comma-separated task ids, or none>

After all tasks: REUSE: <cross-project notes or none>"

run_agent "Project Manager" "Read,Glob,Grep" "${PM_PROMPT}" "${ORRCH_DIR}/plan.md"

# ─── STEP 4: Compress + cluster ─────────────────────────────────────

update_status 4 "clustering"
log "compressing plan..."
cat "${ORRCH_DIR}/plan.md" | "${TOOLS_DIR}/compress_output.sh" > "${ORRCH_DIR}/plan_compressed.md"

log "clustering tasks by file overlap..."
cat "${ORRCH_DIR}/plan.md" | "${TOOLS_DIR}/cluster_tasks.sh" > "${ORRCH_DIR}/clusters.txt"
log "clusters:"
cat "${ORRCH_DIR}/clusters.txt"

# ─── STEP 5: Implement (parallel per cluster, sequential per wave) ──

update_status 5 "implementing"

# Parse clusters.txt to extract waves and cluster assignments
# Format: "### Wave N" sections with "CLUSTER K:" lines and "  Tasks:" lines
CURRENT_WAVE=0
MAX_WAVE=$(grep -c '### Wave' "${ORRCH_DIR}/clusters.txt" || echo 0)

: > "${ORRCH_DIR}/workspace_state.md"  # initialize empty

for wave_num in $(seq 1 "${MAX_WAVE}"); do
    log "=== Wave ${wave_num} ==="

    # Extract cluster blocks for this wave
    # Each cluster has: CLUSTER N: [files]\n  Tasks: t1, t2\n  Suggested agent: Role
    WAVE_SECTION=$(sed -n "/### Wave ${wave_num}/,/### Wave/p" "${ORRCH_DIR}/clusters.txt" | head -n -1)
    if [[ -z "${WAVE_SECTION}" ]]; then
        # Last wave — no trailing "### Wave" delimiter
        WAVE_SECTION=$(sed -n "/### Wave ${wave_num}/,\$p" "${ORRCH_DIR}/clusters.txt")
    fi

    CLUSTER_COUNT=$(echo "${WAVE_SECTION}" | grep -c 'CLUSTER' || echo 0)
    PIDS=()
    CLUSTER_FILES=()

    for cluster_idx in $(seq 1 "${CLUSTER_COUNT}"); do
        # Extract this cluster's info
        CLUSTER_LINE=$(echo "${WAVE_SECTION}" | grep 'CLUSTER' | sed -n "${cluster_idx}p")
        TASKS_LINE=$(echo "${WAVE_SECTION}" | grep 'Tasks:' | sed -n "${cluster_idx}p")
        AGENT_LINE=$(echo "${WAVE_SECTION}" | grep 'Suggested agent:' | sed -n "${cluster_idx}p")

        AGENT_ROLE=$(echo "${AGENT_LINE}" | sed 's/.*Suggested agent: //' | sed 's/ (.*//')
        TASK_IDS=$(echo "${TASKS_LINE}" | sed 's/.*Tasks: //')

        # Extract the actual task blocks from plan.md for these task IDs
        TASK_BLOCKS=""
        for tid in $(echo "${TASK_IDS}" | tr ',' ' '); do
            tid=$(echo "${tid}" | xargs)  # trim whitespace
            BLOCK=$(sed -n "/^TASK ${tid}:/,/^$/p" "${ORRCH_DIR}/plan.md")
            TASK_BLOCKS="${TASK_BLOCKS}${BLOCK}\n\n"
        done

        WORKSPACE_CTX=""
        if [[ ${wave_num} -gt 1 && -s "${ORRCH_DIR}/workspace_state.md" ]]; then
            WORKSPACE_CTX="## Prior changes this session
$(cat "${ORRCH_DIR}/workspace_state.md")"
        fi

        IMPL_PROMPT="Scope: ${SCOPE} — iterate fast, no over-engineering for private.

## Codebase (do NOT read files for orientation — only read files you will EDIT)
$(cat "${ORRCH_DIR}/codebase_brief.txt")

${WORKSPACE_CTX}

## Your tasks
$(echo -e "${TASK_BLOCKS}")

## Rules
- Read each target file before editing
- Follow existing code conventions
- Only read files listed in your tasks' Files: field
- Report: files modified/created, one line per file describing the change"

        OUTPUT_FILE="${ORRCH_DIR}/impl_w${wave_num}_c${cluster_idx}.md"
        CLUSTER_FILES+=("${OUTPUT_FILE}")

        run_agent_bg "${AGENT_ROLE} (wave ${wave_num}, cluster ${cluster_idx})" \
            "Read,Edit,Write,Bash,Glob,Grep" \
            "${IMPL_PROMPT}" \
            "${OUTPUT_FILE}"
        PIDS+=($!)
    done

    # Wait for all agents in this wave
    log "waiting for ${#PIDS[@]} agents in wave ${wave_num}..."
    for pid in "${PIDS[@]}"; do
        wait "${pid}" || log "WARNING: agent PID ${pid} exited non-zero"
    done

    # Compress outputs and append to workspace state
    echo "--- Wave ${wave_num} results ---" >> "${ORRCH_DIR}/workspace_state.md"
    for f in "${CLUSTER_FILES[@]}"; do
        if [[ -s "${f}" ]]; then
            cat "${f}" | "${TOOLS_DIR}/compress_output.sh" >> "${ORRCH_DIR}/workspace_state.md"
        fi
    done

    # Build check
    log "verifying build..."
    if ! cargo build 2>&1 | tail -5; then
        log "ERROR: build failed after wave ${wave_num}"
        update_status 5 "build-failed"
        exit 1
    fi
done

# ─── STEP 6: Verify (parallel, context-isolated) ────────────────────

update_status 6 "verifying"

# Extract file list from workspace state
FILE_LIST=$(grep -oE 'crates/[^ ]+\.rs|src/[^ ]+\.rs|library/[^ ]+\.(sh|md)' \
    "${ORRCH_DIR}/workspace_state.md" | sort -u | tr '\n' ', ')
log "files to verify: ${FILE_LIST}"

INSTRUCTIONS_TEXT=$(cat "${ORRCH_DIR}/instructions.md")

SECURITY_PROMPT="You are a security tester. Find vulnerabilities in recently changed code.

What was built:
${INSTRUCTIONS_TEXT}

Files to review (ONLY these — do not read other files): ${FILE_LIST}

Report each finding as one line: SEVERITY | description | file:line | remediation
No prose. No headers. Just findings, one per line."

DESTRUCTIVE_PROMPT="You are a destructive tester. Break the recently implemented features.

What was built:
${INSTRUCTIONS_TEXT}

Files to review (ONLY these — do not read other files): ${FILE_LIST}

Also run: cargo build && cargo test --workspace

Report each failure as one line: SEVERITY | description | file:line | fix
No prose. No headers. Just failures, one per line."

run_agent_bg "Security Tester" "Read,Bash,Glob,Grep" \
    "${SECURITY_PROMPT}" "${ORRCH_DIR}/security_findings.md"
SEC_PID=$!

run_agent_bg "Destructive Tester" "Read,Bash,Glob,Grep" \
    "${DESTRUCTIVE_PROMPT}" "${ORRCH_DIR}/destructive_findings.md"
DEST_PID=$!

log "waiting for testers..."
wait "${SEC_PID}" || log "WARNING: security tester exited non-zero"
wait "${DEST_PID}" || log "WARNING: destructive tester exited non-zero"

# Compress findings
cat "${ORRCH_DIR}/security_findings.md" | "${TOOLS_DIR}/compress_output.sh" > "${ORRCH_DIR}/security_compressed.md"
cat "${ORRCH_DIR}/destructive_findings.md" | "${TOOLS_DIR}/compress_output.sh" > "${ORRCH_DIR}/destructive_compressed.md"

# ─── STEP 7: Evaluate ───────────────────────────────────────────────

update_status 7 "evaluating"

REWORK_CYCLE=0
MAX_REWORK=3

while true; do
    EVAL_PROMPT="Evaluate these verification findings and decide.

Instructions:
$(cat "${ORRCH_DIR}/instructions.md")

Changes made:
$(cat "${ORRCH_DIR}/workspace_state.md")

Security findings:
$(cat "${ORRCH_DIR}/security_compressed.md")

Destructive findings:
$(cat "${ORRCH_DIR}/destructive_compressed.md")

Output EXACTLY one of:
VERDICT: PASS
VERDICT: SHIP_WITH_ISSUES
Known issues: <one per line>
VERDICT: REWORK
<FIX | file:line | what to fix | severity — one per line>"

    run_agent "PM Evaluator" "Read,Glob,Grep" "${EVAL_PROMPT}" "${ORRCH_DIR}/verdict.md"

    VERDICT=$(grep '^VERDICT:' "${ORRCH_DIR}/verdict.md" | head -1 | sed 's/VERDICT: //')
    log "verdict: ${VERDICT}"

    if [[ "${VERDICT}" == "REWORK" && ${REWORK_CYCLE} -lt ${MAX_REWORK} ]]; then
        REWORK_CYCLE=$((REWORK_CYCLE + 1))
        log "=== Rework cycle ${REWORK_CYCLE}/${MAX_REWORK} ==="

        FIX_LINES=$(grep '^FIX |' "${ORRCH_DIR}/verdict.md" || true)
        REWORK_PROMPT="Fix these issues. Each line is: FIX | file:line | what to fix | severity

${FIX_LINES}

## Codebase
$(cat "${ORRCH_DIR}/codebase_brief.txt")

Read each file before editing. Report: files modified, one line per file."

        run_agent "Developer (rework ${REWORK_CYCLE})" "Read,Edit,Write,Bash,Glob,Grep" \
            "${REWORK_PROMPT}" "${ORRCH_DIR}/rework_${REWORK_CYCLE}.md"

        echo "--- Rework ${REWORK_CYCLE} ---" >> "${ORRCH_DIR}/workspace_state.md"
        cat "${ORRCH_DIR}/rework_${REWORK_CYCLE}.md" | "${TOOLS_DIR}/compress_output.sh" >> "${ORRCH_DIR}/workspace_state.md"

        # Re-verify
        run_agent_bg "Security Tester" "Read,Bash,Glob,Grep" \
            "${SECURITY_PROMPT}" "${ORRCH_DIR}/security_findings.md"
        SEC_PID=$!
        run_agent_bg "Destructive Tester" "Read,Bash,Glob,Grep" \
            "${DESTRUCTIVE_PROMPT}" "${ORRCH_DIR}/destructive_findings.md"
        DEST_PID=$!
        wait "${SEC_PID}" || true
        wait "${DEST_PID}" || true

        cat "${ORRCH_DIR}/security_findings.md" | "${TOOLS_DIR}/compress_output.sh" > "${ORRCH_DIR}/security_compressed.md"
        cat "${ORRCH_DIR}/destructive_findings.md" | "${TOOLS_DIR}/compress_output.sh" > "${ORRCH_DIR}/destructive_compressed.md"

        continue  # re-evaluate
    fi

    break  # PASS or SHIP_WITH_ISSUES or max rework reached
done

# ─── STEP 8: Update dev map ─────────────────────────────────────────

update_status 8 "finishing"
log "updating dev map..."

# Mark completed items in PLAN.md
# Extract feature names from instructions and find matching lines
while IFS= read -r line; do
    # Extract the feature text after "**" markers
    feature=$(echo "${line}" | grep -oP '\*\*.*?\*\*' | head -1 | tr -d '*')
    if [[ -n "${feature}" ]]; then
        # Escape for sed and replace [ ] with [x]
        escaped=$(printf '%s\n' "${feature}" | sed 's/[[\.*^$()+?{|]/\\&/g')
        sed -i "s/\[ \] \*\*${escaped}/[x] **${escaped}/" "${PROJECT_DIR}/PLAN.md" 2>/dev/null || true
    fi
done < "${ORRCH_DIR}/instructions.md"

# ─── STEP 9: Dev log ────────────────────────────────────────────────

DATE=$(date +"%Y-%m-%d %H:%M")
KNOWN_ISSUES=$(grep -A 100 '^VERDICT:' "${ORRCH_DIR}/verdict.md" | grep -v '^VERDICT:' | grep -v '^$' || echo "none")

cat >> "${PROJECT_DIR}/DEVLOG.md" << DEVLOG

## Dev Session: ${DATE}
### Completed
$(cat "${ORRCH_DIR}/instructions.md" | head -20)
### Verdict
${VERDICT} (rework cycles: ${REWORK_CYCLE})
### Known Issues
${KNOWN_ISSUES}
### Files Changed
$(grep -oE 'crates/[^ ]+|src/[^ ]+|library/[^ ]+' "${ORRCH_DIR}/workspace_state.md" | sort -u)
DEVLOG

# ─── STEP 10: Commit ────────────────────────────────────────────────

update_status 9 "committing"

# Stage changed files
CHANGED_FILES=$(grep -oE 'crates/[^ ]+\.rs|src/[^ ]+\.rs|library/[^ ]+\.(sh|md)' \
    "${ORRCH_DIR}/workspace_state.md" | sort -u)

git add PLAN.md DEVLOG.md ${CHANGED_FILES} 2>/dev/null || true
git commit -m "feat: implement dev map items (automated workflow)

Verdict: ${VERDICT}
Rework cycles: ${REWORK_CYCLE}

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>" 2>/dev/null || log "WARNING: commit failed (maybe no changes)"

COMMIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || echo "none")

# Push the branch so it exists on the remote before merge
git push -u origin HEAD 2>/dev/null || log "note: push skipped (no remote or private scope)"

# ─── STEP 11: Merge session branch to main ──────────────────────────

update_status 11 "merging"

SB=$(git branch --show-current 2>/dev/null || echo "")
MERGE_STATUS="skipped"

case "${SB}" in
    main|master|develop|"")
        log "already on ${SB:-no-branch} — no merge needed"
        MERGE_STATUS="not-needed"
        ;;
    *)
        log "merging ${SB} → main..."
        if ! git diff --quiet 2>/dev/null; then
            log "ERROR: uncommitted changes — cannot merge"
            MERGE_STATUS="failed-dirty-tree"
        else
            git checkout main 2>&1 | tail -3 || { log "ERROR: checkout main failed"; MERGE_STATUS="failed-checkout"; }
            git pull --ff-only origin main 2>/dev/null || true

            if git merge --no-ff "${SB}" -m "merge: ${SB}" 2>&1 | tail -5; then
                if [[ -n "$(git ls-files --unmerged 2>/dev/null)" ]]; then
                    log "ERROR: merge conflict with main — escalating to user"
                    git merge --abort 2>/dev/null || true
                    MERGE_STATUS="conflict"
                    update_status 11 "merge-conflict"
                else
                    git push origin main 2>/dev/null || true
                    git branch -d "${SB}" 2>/dev/null || true
                    git push origin --delete "${SB}" 2>/dev/null || true
                    log "merged ${SB} → main"
                    MERGE_STATUS="merged"
                fi
            else
                log "ERROR: merge command failed"
                git merge --abort 2>/dev/null || true
                MERGE_STATUS="failed"
            fi
        fi
        ;;
esac

# ─── DONE ────────────────────────────────────────────────────────────

update_status 12 "complete"

log "=== Workflow complete ==="
log "verdict: ${VERDICT}"
log "rework cycles: ${REWORK_CYCLE}"
log "commit: ${COMMIT_HASH}"
log "merge: ${MERGE_STATUS}"
log "see ${ORRCH_DIR}/ for all step outputs"

if [[ "${MERGE_STATUS}" == "conflict" || "${MERGE_STATUS}" == "failed"* ]]; then
    echo ""
    echo "!!! MERGE TO MAIN DID NOT COMPLETE — session is NOT finished !!!"
    echo "!!! Resolve the merge manually before starting new parallel work !!!"
fi

echo ""
echo "--- Workflow finished. Full log: ${LOG_FILE} ---"
read -r -p "Press Enter to close..."
