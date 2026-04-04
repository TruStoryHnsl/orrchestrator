#!/usr/bin/env bash
set -euo pipefail

SESSION="${1:-}"

if [[ -z "$SESSION" ]]; then
    echo "W:? I:? A:?"
    exit 0
fi

# Check if session exists
if ! tmux has-session -t "$SESSION" 2>/dev/null; then
    echo "[gone]"
    exit 0
fi

# Get window list: index<TAB>name
window_list=$(tmux list-windows -t "$SESSION" -F '#{window_index}	#{window_name}' 2>/dev/null) || {
    echo "[err]"
    exit 0
}

waiting=0
idle=0
active=0

while IFS=$'\t' read -r idx _name; do
    [[ -z "$idx" ]] && continue
    # Capture visible pane content and take the last 5 lines.
    # Avoid -l flag: it is not reliable on tmux 3.6a and causes exit code 1.
    pane_text=$(tmux capture-pane -t "${SESSION}:${idx}" -p 2>/dev/null | tail -5)

    text_lower="${pane_text,,}"

    if echo "$text_lower" | grep -qE 'y/n|\[y/n\]|proceed\?|approve or deny|waiting for|do you want'; then
        (( waiting++ ))
    elif echo "$pane_text" | grep -qE '❯|\?[[:space:]]*$'; then
        (( waiting++ ))
    elif echo "$pane_text" | grep -qE 'bypass permissions|esc to interrupt'; then
        (( idle++ ))
    else
        (( active++ ))
    fi
done <<< "$window_list"

echo "W:${waiting} I:${idle} A:${active}"
