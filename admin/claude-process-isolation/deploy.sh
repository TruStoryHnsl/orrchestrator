#!/bin/bash
# Idempotent installer for claude process isolation (systemd cgroup hardening)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Color codes for output
YELLOW='\033[33m'
GREEN='\033[32m'
NC='\033[0m' # No Color

echo "Installing claude process isolation..."

# 1. Create drop-in directories
mkdir -p ~/.config/systemd/user/claude-limit.slice.d
mkdir -p ~/.config/systemd/user/user.slice.d

# 2. Install orrch.slice
ORRCH_DEST=~/.config/systemd/user/orrch.slice
if [[ -f "$ORRCH_DEST" ]] && ! cmp -s "$SCRIPT_DIR/orrch.slice" "$ORRCH_DEST"; then
    echo "Backing up existing $ORRCH_DEST to ${ORRCH_DEST}.bak"
    cp "$ORRCH_DEST" "${ORRCH_DEST}.bak"
fi
cp "$SCRIPT_DIR/orrch.slice" "$ORRCH_DEST"
echo "Installed $ORRCH_DEST"

# 3. Install claude-limit.slice.d/override.conf
LIMIT_OVERRIDE_DEST=~/.config/systemd/user/claude-limit.slice.d/override.conf
if [[ -f "$LIMIT_OVERRIDE_DEST" ]] && ! cmp -s "$SCRIPT_DIR/claude-limit.slice.d_override.conf" "$LIMIT_OVERRIDE_DEST"; then
    echo "Backing up existing $LIMIT_OVERRIDE_DEST to ${LIMIT_OVERRIDE_DEST}.bak"
    cp "$LIMIT_OVERRIDE_DEST" "${LIMIT_OVERRIDE_DEST}.bak"
fi
cp "$SCRIPT_DIR/claude-limit.slice.d_override.conf" "$LIMIT_OVERRIDE_DEST"
echo "Installed $LIMIT_OVERRIDE_DEST"

# 4. Install user.slice.d/io-accounting.conf
IO_ACCOUNTING_DEST=~/.config/systemd/user/user.slice.d/io-accounting.conf
if [[ -f "$IO_ACCOUNTING_DEST" ]] && ! cmp -s "$SCRIPT_DIR/user.slice.d_io-accounting.conf" "$IO_ACCOUNTING_DEST"; then
    echo "Backing up existing $IO_ACCOUNTING_DEST to ${IO_ACCOUNTING_DEST}.bak"
    cp "$IO_ACCOUNTING_DEST" "${IO_ACCOUNTING_DEST}.bak"
fi
cp "$SCRIPT_DIR/user.slice.d_io-accounting.conf" "$IO_ACCOUNTING_DEST"
echo "Installed $IO_ACCOUNTING_DEST"

# 5. Symlink orrch-spawn.sh into ~/.local/bin
mkdir -p ~/.local/bin
SPAWN_DEST=~/.local/bin/orrch-spawn
if [[ -e "$SPAWN_DEST" ]] && [[ ! -L "$SPAWN_DEST" ]]; then
    echo "Backing up existing $SPAWN_DEST to ${SPAWN_DEST}.bak"
    mv "$SPAWN_DEST" "${SPAWN_DEST}.bak"
fi
ln -sf "$SCRIPT_DIR/orrch-spawn.sh" "$SPAWN_DEST"
echo "Installed symlink $SPAWN_DEST → $SCRIPT_DIR/orrch-spawn.sh"

# 6. Reload systemd user daemons
systemctl --user daemon-reload
echo "Reloaded systemd user daemons"

# 7. Materialize the slice
systemctl --user start orrch.slice
echo "Started orrch.slice"

# 8. Check if ~/.local/bin is in PATH
if ! echo "$PATH" | grep -q "$(eval echo ~/.local/bin)"; then
    echo -e "${YELLOW}WARNING: ~/.local/bin is not in your PATH${NC}"
    echo "Add this line to your shell rc (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish, etc):"
    echo "  export PATH=\$HOME/.local/bin:\$PATH"
fi

echo -e "${GREEN}Installation complete.${NC}"
echo "Run: ./verify.sh"
