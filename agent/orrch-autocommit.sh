#!/bin/bash
# orrch-autocommit.sh — Daily auto-commit for all projects with dirty git repos.
# Called by systemd timer at 5am. Spawns Claude sessions for each dirty project.
#
# Each project gets its own Claude tmux session that:
# 1. Analyzes the git diff
# 2. Writes a meaningful commit message
# 3. Stages and commits
# 4. Pushes to remote

set -e

PROJECTS_DIR="${HOME}/projects"
FEEDBACK_DIR="${PROJECTS_DIR}/.feedback"
LOG="/tmp/orrch-autocommit.log"

mkdir -p "$FEEDBACK_DIR"

echo "$(date): orrch-autocommit starting" >> "$LOG"

count=0
for project_dir in "$PROJECTS_DIR"/*/; do
    name=$(basename "$project_dir")

    # Skip non-git, dotfiles, deprecated
    [ -d "$project_dir/.git" ] || continue
    [[ "$name" == .* ]] && continue
    [ "$name" = "deprecated" ] && continue

    # Check for remote
    remote=$(git -C "$project_dir" remote get-url origin 2>/dev/null) || continue
    [ -z "$remote" ] && continue

    # Check for changes
    dirty=$(git -C "$project_dir" status --porcelain 2>/dev/null | wc -l)
    [ "$dirty" -eq 0 ] && continue

    echo "$(date): committing $name ($dirty changes)" >> "$LOG"

    # Write prompt
    prompt_file="$FEEDBACK_DIR/.autocommit-${name}.md"
    cat > "$prompt_file" <<PROMPT
You are performing an automated daily commit for the ${name} project.

Review the current git status and diff, then:
1. Stage all changes (git add -A), excluding any .env or credential files
2. Write a concise commit message: "daily sync: <summary of changes>"
3. Commit
4. Push to origin

If there are no meaningful changes, skip the commit.
This is an automated backup commit — keep the message brief.
PROMPT

    runner_file="$FEEDBACK_DIR/.autocommit-${name}.sh"
    cat > "$runner_file" <<RUNNER
#!/bin/bash
cd "$project_dir"
prompt=\$(cat "$prompt_file")
claude --dangerously-skip-permissions "\$prompt"
rm -f "$prompt_file" "$runner_file"
RUNNER

    session_name="orrch-auto-${name}"

    # Kill any existing session with this name
    tmux kill-session -t "$session_name" 2>/dev/null || true

    # Spawn
    tmux new-session -d -s "$session_name" bash "$runner_file" 2>/dev/null && {
        echo "$(date): spawned $session_name" >> "$LOG"
        count=$((count + 1))
    }

    # Stagger spawns to avoid overwhelming the system
    sleep 2
done

echo "$(date): orrch-autocommit done, spawned $count sessions" >> "$LOG"
