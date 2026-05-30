#!/usr/bin/env bash
# orrchestrator install — one-shot best-practices installer.
#
# What it does:
#   1. Builds target/release/orrchestrator if missing or older than source
#   2. Creates ~/.config/orrchestrator/ and seeds launch.env from the example
#      (never clobbers an existing launch.env)
#   3. Symlinks packaging/bin/orrchestrator → ~/.local/bin/orrchestrator
#      Same for orrchestrator-dev
#   4. Verifies ~/.local/bin is in $PATH and warns if not
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

cyan() { printf '\033[36m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }

cyan "[1/4] Build check"
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

cyan "[2/4] Config"
mkdir -p "$CFG_DIR"
if [[ -f "$CFG_FILE" ]]; then
    green "      $CFG_FILE already exists (left untouched)"
else
    cp "$PKG_CFG_DIR/launch.env.example" "$CFG_FILE"
    green "      seeded $CFG_FILE from template"
fi

cyan "[3/4] Symlink"
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

cyan "[4/4] PATH check"
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
