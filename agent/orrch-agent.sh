#!/bin/sh
# orrch-agent.sh — Cross-platform remote agent for orrchestrator.
#
# Deployed to remote nodes via SSH. Provides a uniform interface for
# session discovery, spawning, and management across Linux and macOS.
#
# Usage:
#   orrch-agent.sh discover          — list Claude sessions as JSON lines
#   orrch-agent.sh spawn <project> <backend> <goal> [flags...]
#   orrch-agent.sh kill <session_name>
#   orrch-agent.sh list              — list orrchestrator-managed sessions
#   orrch-agent.sh check             — health check (prints capabilities)

set -e

ORRCH_PREFIX="orrch-"
PROJECTS_DIR="$HOME/projects"

# ─── Platform detection ──────────────────────────────────────────────

detect_os() {
    case "$(uname -s)" in
        Darwin*) echo "macos" ;;
        Linux*)  echo "linux" ;;
        *)       echo "unknown" ;;
    esac
}

OS="$(detect_os)"

# ─── Session multiplexer detection ───────────────────────────────────

detect_mux() {
    if command -v tmux >/dev/null 2>&1; then
        echo "tmux"
    elif command -v screen >/dev/null 2>&1; then
        echo "screen"
    else
        echo "nohup"
    fi
}

MUX="$(detect_mux)"

# ─── Process discovery (cross-platform) ─────────────────────────────

# Get CWD for a PID. Linux uses /proc, macOS uses lsof.
get_cwd() {
    pid="$1"
    if [ "$OS" = "linux" ]; then
        readlink "/proc/$pid/cwd" 2>/dev/null || echo ""
    else
        # macOS: lsof -p PID -a -d cwd -F n → lines starting with 'n' = path
        lsof -p "$pid" -a -d cwd -F n 2>/dev/null | grep '^n' | cut -c2- || echo ""
    fi
}

# Discover all Claude CLI sessions. Output: one JSON object per line.
cmd_discover() {
    # Use ps (POSIX) instead of pgrep for cross-platform compatibility
    # ps -eo pid,command works on both Linux and macOS
    ps -eo pid,command 2>/dev/null | while IFS= read -r line; do
        # Skip header
        case "$line" in *PID*) continue ;; esac

        # Extract PID and command
        pid="$(echo "$line" | awk '{print $1}')"
        cmd="$(echo "$line" | awk '{$1=""; print $0}' | sed 's/^ //')"

        # Must be a claude process
        case "$cmd" in
            *claude*) ;;
            *) continue ;;
        esac

        # Skip shell wrappers, grep itself, this script
        case "$cmd" in
            *grep*|*pgrep*|*orrch-agent*|*"bash -c"*|*"sh -c"*|*"zsh -c"*) continue ;;
        esac

        # Skip non-claude binaries that happen to match (e.g. claude-related config tools)
        case "$cmd" in
            claude*|*/claude*) ;;
            *) continue ;;
        esac

        cwd="$(get_cwd "$pid")"

        # Output as JSON line
        printf '{"pid":%s,"cmdline":"%s","cwd":"%s"}\n' \
            "$pid" \
            "$(echo "$cmd" | sed 's/"/\\"/g')" \
            "$(echo "$cwd" | sed 's/"/\\"/g')"
    done
}

# ─── Session spawning ────────────────────────────────────────────────

cmd_spawn() {
    project="$1"
    backend="$2"
    goal="$3"
    shift 3
    flags="$*"

    session_name="${ORRCH_PREFIX}${project}"
    project_dir="${PROJECTS_DIR}/${project}"

    if [ ! -d "$project_dir" ]; then
        echo "ERROR: project directory not found: $project_dir" >&2
        exit 1
    fi

    # Build the full command
    full_cmd="cd '$project_dir' && $backend $flags '$goal'"

    case "$MUX" in
        tmux)
            # Try to create new session, or send keys to existing one
            tmux new-session -d -s "$session_name" -c "$project_dir" \
                "$backend $flags '$goal'" 2>/dev/null || \
            tmux send-keys -t "$session_name" "$backend $flags '$goal'" Enter
            ;;
        screen)
            screen -dmS "$session_name" sh -c "$full_cmd; exec sh"
            ;;
        nohup)
            # Last resort: nohup with a marker file for tracking
            marker_dir="$HOME/.orrchestrator/sessions"
            mkdir -p "$marker_dir"
            nohup sh -c "$full_cmd" > "$marker_dir/${session_name}.log" 2>&1 &
            echo "$!" > "$marker_dir/${session_name}.pid"
            ;;
    esac

    echo "OK:${session_name}:${MUX}"
}

# ─── Session management ──────────────────────────────────────────────

cmd_kill() {
    session_name="$1"

    case "$MUX" in
        tmux)
            tmux kill-session -t "$session_name" 2>/dev/null
            ;;
        screen)
            screen -S "$session_name" -X quit 2>/dev/null
            ;;
        nohup)
            pid_file="$HOME/.orrchestrator/sessions/${session_name}.pid"
            if [ -f "$pid_file" ]; then
                kill "$(cat "$pid_file")" 2>/dev/null
                rm -f "$pid_file"
            fi
            ;;
    esac
}

cmd_list() {
    case "$MUX" in
        tmux)
            tmux list-sessions -F '#{session_name}' 2>/dev/null | grep "^${ORRCH_PREFIX}" || true
            ;;
        screen)
            screen -ls 2>/dev/null | grep "${ORRCH_PREFIX}" | awk '{print $1}' | sed 's/.*\.//' || true
            ;;
        nohup)
            marker_dir="$HOME/.orrchestrator/sessions"
            if [ -d "$marker_dir" ]; then
                for f in "$marker_dir"/*.pid; do
                    [ -f "$f" ] || continue
                    name="$(basename "$f" .pid)"
                    pid="$(cat "$f")"
                    if kill -0 "$pid" 2>/dev/null; then
                        echo "$name"
                    else
                        rm -f "$f"
                    fi
                done
            fi
            ;;
    esac
}

# ─── Health check ────────────────────────────────────────────────────

cmd_check() {
    has_claude="false"
    has_gemini="false"
    command -v claude >/dev/null 2>&1 && has_claude="true"
    command -v gemini >/dev/null 2>&1 && has_gemini="true"

    printf '{"os":"%s","mux":"%s","claude":%s,"gemini":%s,"projects_dir":"%s","hostname":"%s"}\n' \
        "$OS" "$MUX" "$has_claude" "$has_gemini" \
        "$(echo "$PROJECTS_DIR" | sed 's/"/\\"/g')" \
        "$(hostname)"
}

# ─── Main dispatch ───────────────────────────────────────────────────

case "${1:-help}" in
    discover) cmd_discover ;;
    spawn)
        shift
        if [ $# -lt 3 ]; then
            echo "Usage: orrch-agent.sh spawn <project> <backend> <goal> [flags...]" >&2
            exit 1
        fi
        cmd_spawn "$@"
        ;;
    kill)
        shift
        cmd_kill "$1"
        ;;
    list) cmd_list ;;
    check) cmd_check ;;
    *)
        echo "orrch-agent.sh — orrchestrator remote agent"
        echo "Commands: discover | spawn | kill | list | check"
        exit 0
        ;;
esac
