#!/usr/bin/env bash
set -euo pipefail

# window-poke.sh — apply clicks and keyboard input to a window on an ISOLATED display
#
# The "act" half of the agent's peek<->act loop (pairs with `spectacle-peek`).
# Drives a window on a private X display (e.g. one from `gui-sandbox`) via
# `DISPLAY=:N xdotool`, so input is confined to that display and NEVER touches
# the user's real cursor, focus, or screen.
#
# SCOPE POLICY: BANNED in `personal`-scope projects (exit 3). Encouraged elsewhere.
#
# A --display is REQUIRED (this tool is for isolated displays, not the live session,
# precisely so it can't interrupt the user). Coordinates are WINDOW-RELATIVE and
# converted to absolute via the target window's geometry.
#
# USAGE
#   window-poke --display :N [--window <id> | --title <substr>] <op> [<op> ...]
#
# TARGET
#   --display :N     required — the isolated X display to act on
#   --window  ID     act on this window id (else resolved from --title, else the
#                    single visible window on :N)
#   --title   S      resolve the window by title substring
#   --project DIR    project dir for the .scope gate (default: $PWD)
#
# OPS (executed left-to-right, in the order given)
#   --click   X,Y    left click at window-relative X,Y
#   --rclick  X,Y    right click
#   --mclick  X,Y    middle click
#   --dblclick X,Y   double click
#   --move    X,Y    move pointer (no click)
#   --text   "STR"   type a literal string
#   --key    CHORD   one key/chord, xdotool syntax (e.g. Return, ctrl+s, alt+F4)
#   --keys  "A B C"  several keys/chords separated by spaces
#   --scroll up|down[:N]   scroll wheel N notches (default 3)
#   --sleep  MS      pause MS milliseconds before the next op
#
# EXAMPLE
#   window-poke --display :99 --window 0x340 --click 640,400 --text "hello" --key Return

display="" win="" title="" project="$PWD"
ops_kind=(); ops_arg=()

die() { echo "window-poke: $*" >&2; exit 2; }
is_int()  { case "$1" in ''|*[!0-9]*) return 1;; *) return 0;; esac; }
add_op()  { ops_kind+=("$1"); ops_arg+=("$2"); }
xy_ok()   { case "$1" in *,*) is_int "${1%%,*}" && is_int "${1##*,}";; *) return 1;; esac; }

while [ $# -gt 0 ]; do
  case "$1" in
    --display) display="${2:-}"; shift 2;;
    --window)  win="${2:-}"; shift 2;;
    --title)   title="${2:-}"; shift 2;;
    --project) project="${2:-}"; shift 2;;
    --click|--rclick|--mclick|--dblclick|--move)
               xy_ok "${2:-}" || die "$1 needs X,Y integers (got '${2:-}')"; add_op "${1#--}" "$2"; shift 2;;
    --text)    add_op text "${2:-}"; shift 2;;
    --key)     add_op key "${2:-}"; shift 2;;
    --keys)    add_op keys "${2:-}"; shift 2;;
    --scroll)  case "${2:-}" in up|down|up:*|down:*) :;; *) die "--scroll up|down[:N]";; esac; add_op scroll "$2"; shift 2;;
    --sleep)   is_int "${2:-}" || die "--sleep needs integer ms"; add_op sleep "$2"; shift 2;;
    -h|--help) sed -n '3,45p' "$0"; exit 0;;
    *)         die "unknown argument: $1";;
  esac
done

# scope gate
if [ -f "$project/.scope" ]; then
  s="$(tr -d '[:space:]' < "$project/.scope" | tr '[:upper:]' '[:lower:]')"
  if [ "$s" = "personal" ]; then echo "window-poke: BANNED in personal-scope project ($project)." >&2; exit 3; fi
fi

[ -n "$display" ] || die "--display :N is required (this tool only drives isolated displays)"
case "$display" in :[0-9]*) :;; [0-9]*) display=":$display";; *) die "bad --display '$display'";; esac
command -v xdotool >/dev/null 2>&1 || die "xdotool not installed"
export DISPLAY="$display"
xdotool getdisplaygeometry >/dev/null 2>&1 || die "display $display not reachable (is the sandbox running?)"
[ "${#ops_kind[@]}" -gt 0 ] || die "no ops given (e.g. --click 10,20 --text hi --key Return)"

# resolve target window
if [ -z "$win" ]; then
  if [ -n "$title" ]; then
    win="$(xdotool search --name "$title" 2>/dev/null | head -n1 || true)"
    [ -n "$win" ] || die "no window matched --title '$title' on $display"
  else
    win="$(xdotool search --onlyvisible --name '.+' 2>/dev/null | head -n1 || true)"
    [ -n "$win" ] || die "no visible window on $display — pass --window or --title"
  fi
fi

# window geometry → absolute offset for coordinate ops
unset X Y WIDTH HEIGHT
eval "$(xdotool getwindowgeometry --shell "$win" 2>/dev/null || true)"
[ -n "${X:-}" ] && [ -n "${Y:-}" ] || die "could not read geometry for window $win"
OFFX="$X"; OFFY="$Y"

xdotool windowactivate --sync "$win" 2>/dev/null || xdotool windowfocus "$win" 2>/dev/null || true

abs() { # relative X,Y -> absolute screen coords on this display
  local rx="${1%%,*}" ry="${1##*,}"
  echo "$((OFFX + rx))" "$((OFFY + ry))"
}
click_at() { # button relx,rely
  read -r ax ay < <(abs "$2"); xdotool mousemove --sync "$ax" "$ay" click "$1"
}

for i in "${!ops_kind[@]}"; do
  k="${ops_kind[$i]}"; a="${ops_arg[$i]}"
  case "$k" in
    click)    click_at 1 "$a";;
    mclick)   click_at 2 "$a";;
    rclick)   click_at 3 "$a";;
    dblclick) read -r ax ay < <(abs "$a"); xdotool mousemove --sync "$ax" "$ay" click --repeat 2 1;;
    move)     read -r ax ay < <(abs "$a"); xdotool mousemove --sync "$ax" "$ay";;
    text)     xdotool type --window "$win" --clearmodifiers -- "$a";;
    key)      xdotool key --window "$win" --clearmodifiers "$a";;
    keys)     for kk in $a; do xdotool key --window "$win" --clearmodifiers "$kk"; done;;
    scroll)   dir="${a%%:*}"; n="${a##*:}"; [ "$dir" = "$n" ] && n=3; is_int "$n" || n=3
              btn=4; [ "$dir" = down ] && btn=5
              j=0; while [ "$j" -lt "$n" ]; do xdotool click "$btn"; j=$((j+1)); done;;
    sleep)    sleep "$(awk -v ms="$a" 'BEGIN{print ms/1000}')";;
  esac
done

echo "window-poke: applied ${#ops_kind[@]} op(s) to window $win on $display" >&2
