#!/usr/bin/env bash
# orrchestrator install — one-shot best-practices installer.
#
# What it does:
#   1. Builds target/release/orrchestrator if missing or older than source
#   2. Installs orrch-mcp-server to ~/.local/bin (a COPY, refreshed every run, so
#      the MCP server binary survives `cargo clean` wiping target/ and is always
#      as fresh as the last build)
#   3. Creates ~/.config/orrchestrator/ and seeds launch.env from the example
#      (never clobbers an existing launch.env)
#   4. Symlinks packaging/bin/orrchestrator → ~/.local/bin/orrchestrator
#      Same for orrchestrator-dev
#   5. Verifies ~/.local/bin is in $PATH and warns if not
#
# Re-running is safe — every step is idempotent.

set -euo pipefail

APP_NAME="orrchestrator"
REPO_ROOT="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
PKG_BIN_DIR="$REPO_ROOT/packaging/bin"
PKG_CFG_DIR="$REPO_ROOT/packaging/config"

LOCAL_BIN="$HOME/.local/bin"
CFG_DIR="$HOME/.config/$APP_NAME"
CFG_FILE="$CFG_DIR/launch.env"
CMD_DIR="$HOME/.claude/commands"
SKILLS_DIR="$REPO_ROOT/library/skills"
# Dispatch-loop skills that must resolve as Claude Code slash commands in every
# spawned session (any project). Without these, `/run-workforce` / `/develop-*`
# silently do nothing and spawned sessions ignore the guided workflow.
DISPATCH_COMMANDS=(run-workforce develop-feature develop-aio)

cyan() { printf '\033[36m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }

cyan "[1/6] Build check"
BINARY="$REPO_ROOT/target/release/$APP_NAME"
NEED_BUILD=0
if [[ ! -x "$BINARY" ]]; then
    NEED_BUILD=1
else
    NEWEST_SRC=$(find "$REPO_ROOT/src" "$REPO_ROOT/crates" "$REPO_ROOT"/Cargo.toml -name '*.rs' -o -name 'Cargo.toml' 2>/dev/null \
        | xargs -I{} stat -c '%Y' {} 2>/dev/null | sort -rn | head -1)
    BIN_MTIME=$(stat -c '%Y' "$BINARY")
    if [[ "${NEWEST_SRC:-0}" -gt "$BIN_MTIME" ]]; then
        NEED_BUILD=1
    fi
fi
if [[ "$NEED_BUILD" == "1" ]]; then
    yellow "      source newer than binary — building (cargo build --release)"
    (cd "$REPO_ROOT" && cargo build --release)
    green "      built"
else
    green "      binary is current"
fi

cyan "[2/6] MCP server"
# The MCP server is a separate workspace binary the Claude Code MCP config launches
# by absolute path. Pointing that config at target/release/ is fragile — `cargo
# clean` (or a linker-config change) wipes target/ and the server vanishes (ENOENT).
# So install a COPY at a stable path and refresh it on every install run.
MCP_BIN="$REPO_ROOT/target/release/orrch-mcp-server"
MCP_DST="$LOCAL_BIN/orrch-mcp-server"
mkdir -p "$LOCAL_BIN"
if [[ ! -x "$MCP_BIN" ]]; then
    yellow "      orrch-mcp-server missing from target/release — building"
    (cd "$REPO_ROOT" && cargo build --release -p orrch-mcp --bin orrch-mcp-server)
fi
# COPY, not symlink: a symlink into target/release would break on cargo clean.
install -m 0755 "$MCP_BIN" "$MCP_DST"
green "      installed $MCP_DST (copy — survives cargo clean, refreshed each run)"
yellow "      MCP config command should be: $MCP_DST"

cyan "[3/6] Slash commands"
# Register the workflow dispatch skills as global Claude Code slash commands so
# orrchestrator-spawned sessions are HANDED the compiled workflow and execute it
# mechanically, instead of a prose request the model can ignore. Symlink (not
# copy) so the repo stays the single source of truth.
mkdir -p "$CMD_DIR"
for cmd in "${DISPATCH_COMMANDS[@]}"; do
    src="$SKILLS_DIR/$cmd.md"
    dst="$CMD_DIR/$cmd.md"
    if [[ ! -f "$src" ]]; then
        yellow "      skill $src missing — skipping /$cmd"
        continue
    fi
    if [[ -L "$dst" && "$(readlink -f "$dst")" == "$(readlink -f "$src")" ]]; then
        green "      /$cmd → already linked"
    else
        # Anything already at $dst that isn't the correct symlink (excluded above)
        # gets backed up before we clobber it: plain files/dirs, AND symlinks
        # pointing at a different (or now-missing) target. -e is false for broken
        # symlinks, so test -L too.
        if [[ -e "$dst" || -L "$dst" ]]; then
            if [[ -L "$dst" ]]; then
                yellow "      $dst is a symlink to a different target ($(readlink "$dst")) — backing up to $dst.bak"
            else
                yellow "      $dst exists and is NOT a symlink — backing up to $dst.bak"
            fi
            mv "$dst" "$dst.bak"
        fi
        ln -sfn "$src" "$dst"
        green "      /$cmd → $src"
    fi
done

cyan "[4/6] Config"
mkdir -p "$CFG_DIR"
if [[ -f "$CFG_FILE" ]]; then
    green "      $CFG_FILE already exists (left untouched)"
else
    cp "$PKG_CFG_DIR/launch.env.example" "$CFG_FILE"
    green "      seeded $CFG_FILE from template"
fi

cyan "[5/6] Symlink"
mkdir -p "$LOCAL_BIN"
for name in "$APP_NAME" "$APP_NAME-dev"; do
    src="$PKG_BIN_DIR/$name"
    dst="$LOCAL_BIN/$name"
    if [[ ! -x "$src" ]]; then
        chmod +x "$src"
    fi
    if [[ -L "$dst" && "$(readlink -f "$dst")" == "$src" ]]; then
        green "      $dst → already linked"
    else
        if [[ -e "$dst" && ! -L "$dst" ]]; then
            yellow "      $dst exists and is NOT a symlink — backing up to $dst.bak"
            mv "$dst" "$dst.bak"
        fi
        ln -sfn "$src" "$dst"
        green "      $dst → $src"
    fi
done

cyan "[6/6] PATH check"
case ":$PATH:" in
    *":$LOCAL_BIN:"*)
        green "      $LOCAL_BIN is in PATH"
        ;;
    *)
        yellow "      $LOCAL_BIN is NOT in PATH"
        yellow "      Add to your shell rc:  export PATH=\"\$HOME/.local/bin:\$PATH\""
        ;;
esac

echo
green "✔ orrchestrator installed"
echo "  Run:                $APP_NAME"
echo "  Live-reload dev:    $APP_NAME-dev"
echo "  Config:             $CFG_FILE"
echo "  MCP server:         $LOCAL_BIN/orrch-mcp-server"
