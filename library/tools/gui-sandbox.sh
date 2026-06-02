#!/usr/bin/env bash
set -euo pipefail

# gui-sandbox.sh — run a GUI app in an isolated headless X display (Xvfb)
#
# Lets an agent drive its OWN application without ever touching the user's real
# desktop: the app renders into a private Xvfb display (:N), and `window-poke`
# (input) + `spectacle-peek --display :N` (capture) act ONLY on that display.
# Truly non-interrupting — your cursor/focus/screen are never used.
#
# SCOPE POLICY: BANNED in `personal`-scope projects (exit 3). Encouraged elsewhere.
#
# Xvfb is X11, so the app must support X11/XWayland (egui/winit, Electron/Chromium,
# GTK with GDK_BACKEND=x11, Qt with QT_QPA_PLATFORM=xcb — all set automatically).
# Pure-Wayland-only apps will not run here.
#
# USAGE
#   gui-sandbox start [--size WxH] [--name NAME] [--project DIR] -- <app cmd...>
#       Start Xvfb on a free display, launch the app, print DISPLAY + window id.
#   gui-sandbox list                 # running sandboxes
#   gui-sandbox win   <:N|N>         # list window ids+titles in that display
#   gui-sandbox stop  <:N|N>         # kill app + Xvfb, clean up
#
# OUTPUT (start): machine-readable lines —
#   DISPLAY=:N
#   WINDOW=<id>
#   SIZE=WxH
# then a human summary on stderr. Feed DISPLAY/WINDOW to window-poke + spectacle-peek.

ROOT=/tmp/gui-sandbox
SIZE=1280x800
NAME=""
PROJECT="$PWD"

die() { echo "gui-sandbox: $*" >&2; exit 2; }
have() { command -v "$1" >/dev/null 2>&1; }

scope_gate() {
  local sf="$PROJECT/.scope"
  if [ -f "$sf" ]; then
    local s; s="$(tr -d '[:space:]' < "$sf" | tr '[:upper:]' '[:lower:]')"
    if [ "$s" = "personal" ]; then echo "gui-sandbox: BANNED in personal-scope project ($PROJECT)." >&2; exit 3; fi
  fi
}

norm_disp() { # ":99" or "99" -> "99"
  local d="$1"; d="${d#:}"; case "$d" in ''|*[!0-9]*) die "bad display '$1'";; esac; echo "$d"
}

cmd="${1:-}"; [ -n "$cmd" ] || die "need a subcommand: start|list|win|stop"
shift || true

case "$cmd" in
  start)
    APPCMD=()
    while [ $# -gt 0 ]; do
      case "$1" in
        --size)    SIZE="${2:-}"; shift 2;;
        --name)    NAME="${2:-}"; shift 2;;
        --project) PROJECT="${2:-}"; shift 2;;
        --)        shift; APPCMD=("$@"); break;;
        *)         die "unknown start arg: $1 (did you forget '--' before the app command?)";;
      esac
    done
    [ "${#APPCMD[@]}" -gt 0 ] || die "no app command. Usage: gui-sandbox start [--size WxH] -- <app cmd...>"
    case "$SIZE" in *x*) :;; *) die "--size must be WxH, e.g. 1280x800";; esac
    have Xvfb   || die "Xvfb not installed"
    have xdotool|| die "xdotool not installed"
    scope_gate

    # pick a free display number (high range to avoid the real :0)
    N=""
    for n in $(seq 99 -1 50); do
      [ -e "/tmp/.X11-unix/X$n" ] && continue
      [ -d "$ROOT/$n" ] && continue
      N="$n"; break
    done
    [ -n "$N" ] || die "no free display in :50..:99"
    mkdir -p "$ROOT/$N"

    W="${SIZE%x*}"; H="${SIZE#*x}"
    Xvfb ":$N" -screen 0 "${W}x${H}x24" -nolisten tcp >"$ROOT/$N/xvfb.log" 2>&1 &
    XVFB_PID=$!
    echo "$XVFB_PID" > "$ROOT/$N/xvfb.pid"

    # wait for the display to accept connections
    ready=0
    for _ in $(seq 1 50); do
      if DISPLAY=":$N" xdotool getdisplaygeometry >/dev/null 2>&1; then ready=1; break; fi
      kill -0 "$XVFB_PID" 2>/dev/null || break
      sleep 0.1
    done
    [ "$ready" = 1 ] || { kill "$XVFB_PID" 2>/dev/null || true; rm -rf "$ROOT/$N"; die "Xvfb :$N failed to start (see was $ROOT/$N/xvfb.log)"; }

    # launch the app onto the isolated display with X11 toolkit hints
    DISPLAY=":$N" GDK_BACKEND=x11 QT_QPA_PLATFORM=xcb MOZ_ENABLE_WAYLAND=0 \
      SDL_VIDEODRIVER=x11 WINIT_UNIX_BACKEND=x11 \
      nohup "${APPCMD[@]}" >"$ROOT/$N/app.log" 2>&1 &
    APP_PID=$!
    echo "$APP_PID" > "$ROOT/$N/app.pid"

    # wait for the app's window to map
    WIN=""
    for _ in $(seq 1 120); do
      kill -0 "$APP_PID" 2>/dev/null || break
      WIN="$(DISPLAY=":$N" xdotool search --onlyvisible --pid "$APP_PID" 2>/dev/null | head -n1 || true)"
      [ -z "$WIN" ] && WIN="$(DISPLAY=":$N" xdotool search --onlyvisible --name '.+' 2>/dev/null | head -n1 || true)"
      [ -n "$WIN" ] && break
      sleep 0.1
    done

    if [ -n "$WIN" ]; then
      # fill the virtual screen so coordinates are predictable
      DISPLAY=":$N" xdotool windowmove "$WIN" 0 0 >/dev/null 2>&1 || true
      DISPLAY=":$N" xdotool windowsize "$WIN" "$W" "$H" >/dev/null 2>&1 || true
      DISPLAY=":$N" xdotool windowactivate "$WIN" >/dev/null 2>&1 || true
      echo "$WIN" > "$ROOT/$N/window"
    fi
    printf '%s\n' "$SIZE" > "$ROOT/$N/size"
    printf '%s\n' "${NAME:-${APPCMD[0]}}" > "$ROOT/$N/name"
    printf '%q ' "${APPCMD[@]}" > "$ROOT/$N/cmd"; echo >> "$ROOT/$N/cmd"

    echo "DISPLAY=:$N"
    echo "WINDOW=${WIN:-none}"
    echo "SIZE=$SIZE"
    if [ -n "$WIN" ]; then
      echo "gui-sandbox: '${NAME:-${APPCMD[0]}}' running on :$N (window $WIN, app pid $APP_PID)" >&2
    else
      echo "gui-sandbox: app launched on :$N but no window mapped yet (app pid $APP_PID; see $ROOT/$N/app.log)" >&2
    fi
    ;;

  list)
    [ -d "$ROOT" ] || { echo "(no sandboxes)" >&2; exit 0; }
    found=0
    for d in "$ROOT"/*/; do
      [ -d "$d" ] || continue
      n="$(basename "$d")"; found=1
      xp="$(cat "$d/xvfb.pid" 2>/dev/null || echo '?')"; ap="$(cat "$d/app.pid" 2>/dev/null || echo '?')"
      st="dead"; kill -0 "$ap" 2>/dev/null && st="running"
      printf ':%s\t%s\tapp=%s xvfb=%s win=%s\t%s\n' "$n" "$st" "$ap" "$xp" "$(cat "$d/window" 2>/dev/null || echo none)" "$(cat "$d/name" 2>/dev/null)"
    done
    [ "$found" = 1 ] || echo "(no sandboxes)" >&2
    ;;

  win)
    N="$(norm_disp "${1:-}")"
    [ -d "$ROOT/$N" ] || die "no sandbox on :$N"
    ids="$(DISPLAY=":$N" xdotool search --onlyvisible --name '.+' 2>/dev/null || true)"
    [ -n "$ids" ] || { echo "(no visible windows on :$N)" >&2; exit 0; }
    while IFS= read -r id; do
      [ -n "$id" ] || continue
      printf '%s\t%s\n' "$id" "$(DISPLAY=":$N" xdotool getwindowname "$id" 2>/dev/null || echo '?')"
    done <<< "$ids"
    ;;

  stop)
    N="$(norm_disp "${1:-}")"
    [ -d "$ROOT/$N" ] || die "no sandbox on :$N"
    ap="$(cat "$ROOT/$N/app.pid" 2>/dev/null || true)"
    xp="$(cat "$ROOT/$N/xvfb.pid" 2>/dev/null || true)"
    [ -n "$ap" ] && kill "$ap" 2>/dev/null || true
    sleep 0.2
    [ -n "$ap" ] && kill -9 "$ap" 2>/dev/null || true
    [ -n "$xp" ] && kill "$xp" 2>/dev/null || true
    rm -f "/tmp/.X11-unix/X$N" 2>/dev/null || true
    rm -rf "$ROOT/$N"
    echo "gui-sandbox: stopped :$N" >&2
    ;;

  -h|--help) sed -n '3,40p' "$0";;
  *) die "unknown subcommand: $cmd";;
esac
