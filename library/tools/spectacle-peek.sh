#!/usr/bin/env bash
set -euo pipefail

# spectacle-peek.sh — fast, lightweight window screenshots for an agent testing an app
#
# Captures a SPECIFIC application window (never the whole screen) and prints the
# saved PNG path(s) so the agent can Read them and SEE what its app looks like.
# Headless: no GUI, no notification, no clicking. KDE-native via `spectacle`,
# with backend fallbacks for wlroots (grim) and X11 (import).
#
# SCOPE POLICY: BANNED in `personal`-scope projects (exits 3). Encouraged in
# private/public/commercial. The script self-enforces by reading the target
# project's `.scope` file.
#
# WINDOW TARGETING IS MANDATORY — one of --title / --class / --pid / --active.
# There is intentionally no whole-screen / fullscreen mode.
#
# USAGE
#   spectacle-peek --title <substr>            # find window by title substring, capture it
#   spectacle-peek --class <substr>            # find window by app/class substring
#   spectacle-peek --pid <pid>                 # capture the window owned by <pid>
#   spectacle-peek --active                    # capture the currently-focused window
#   spectacle-peek --title foo --burst 5 --interval 600   # 5 shots, 600ms apart
#   spectacle-peek --title foo --watch --duration 8 --threshold 0.01  # shoot on change
#   spectacle-peek --list                      # list candidate windows (titles), then exit
#
# OPTIONS
#   --title S        match window by title substring (regex ok)
#   --class S        match window by class/app-id substring
#   --pid N          match window owned by process N
#   --active         capture the focused window
#   --list           print candidate windows and exit
#   --burst N        take N shots (default 1)
#   --interval MS    ms between burst shots (default 500)
#   --watch          capture only when the window's pixels change
#   --duration S     --watch run length in seconds (default 6)
#   --threshold T    --watch change threshold, 0..1 RMSE fraction (default 0.01)
#   --max-width PX   downscale long edge to PX for token-light reads (default 1280; 0=off)
#   --project DIR    project dir to read .scope from (default: $PWD)
#   --outdir DIR     output dir (default: /tmp/spectacle-peek/<timestamp>-<pid>)
#   --settle MS      ms to wait after activating a window before capture (default 250)
#
# OUTPUT
#   One absolute PNG path per line on stdout (in capture order). Diagnostics on stderr.
#   Exit 0 = at least one image captured; 2 = usage/target error; 3 = scope-banned;
#   4 = no usable capture backend.

# ---------------------------------------------------------------------------- args
title="" class="" pid="" active=0 do_list=0
burst=1 interval=500 watch=0 duration=6 threshold=0.01
max_width=1280 project="$PWD" outdir="" settle=250 display="" winid=""

die() { echo "spectacle-peek: $*" >&2; exit 2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --title)     title="${2:-}"; shift 2;;
    --class)     class="${2:-}"; shift 2;;
    --pid)       pid="${2:-}"; shift 2;;
    --active)    active=1; shift;;
    --list)      do_list=1; shift;;
    --burst)     burst="${2:-}"; shift 2;;
    --interval)  interval="${2:-}"; shift 2;;
    --watch)     watch=1; shift;;
    --duration)  duration="${2:-}"; shift 2;;
    --threshold) threshold="${2:-}"; shift 2;;
    --max-width) max_width="${2:-}"; shift 2;;
    --project)   project="${2:-}"; shift 2;;
    --outdir)    outdir="${2:-}"; shift 2;;
    --settle)    settle="${2:-}"; shift 2;;
    --display)   display="${2:-}"; shift 2;;
    --window)    winid="${2:-}"; shift 2;;
    -h|--help)   sed -n '3,55p' "$0"; exit 0;;
    *)           die "unknown argument: $1";;
  esac
done

# ---------------------------------------------------------------- validate input
# Every numeric option flows into awk and $(( )) arithmetic; validate strictly so
# nothing reaches an eval/arithmetic context as code. (Strings like --title are
# passed as argv to kdotool, never evaluated, so they need no shell escaping.)
is_uint()   { case "$1" in ''|*[!0-9]*)       return 1;; *) return 0;; esac; }
is_ufloat() { case "$1" in ''|*[!0-9.]*|*.*.*) return 1;; *) return 0;; esac; }

is_uint "$burst"     && [ "$burst" -ge 1 ] || die "--burst must be a positive integer"
is_uint "$interval"  || die "--interval must be a non-negative integer (ms)"
is_uint "$duration"  && [ "$duration" -ge 1 ] || die "--duration must be a positive integer (seconds)"
is_uint "$max_width" || die "--max-width must be a non-negative integer (px; 0=off)"
is_uint "$settle"    || die "--settle must be a non-negative integer (ms)"
is_ufloat "$threshold" || die "--threshold must be a number in 0..1"
[ -z "$pid" ] || is_uint "$pid" || die "--pid must be a positive integer"

# --------------------------------------------------------------------- scope gate
scope_file="$project/.scope"
if [ -f "$scope_file" ]; then
  s="$(tr -d '[:space:]' < "$scope_file" | tr '[:upper:]' '[:lower:]')"
  if [ "$s" = "personal" ]; then
    echo "spectacle-peek: BANNED in personal-scope project ($project). Refusing to capture." >&2
    exit 3
  fi
fi
# Missing .scope defaults to private (allowed) per workspace policy.

# --------------------------------------------------------------- backend detect
have() { command -v "$1" >/dev/null 2>&1; }
IM=""; have magick && IM="magick"; { [ -z "$IM" ] && have convert; } && IM="convert"

# --display :N forces the isolated-X11 path (e.g. a gui-sandbox Xvfb display):
# capture via `import` and target windows via `xdotool`, all pinned to that
# DISPLAY so nothing touches the user's real session.
if [ -n "$display" ]; then
  case "$display" in :[0-9]*) :;; [0-9]*) display=":$display";; *) die "bad --display '$display'";; esac
  have import  || die "--display needs ImageMagick 'import'"
  have xdotool || die "--display needs xdotool"
  export DISPLAY="$display"
  xdotool getdisplaygeometry >/dev/null 2>&1 || die "display $display not reachable (is the sandbox running?)"
  backend="x11"; WQ="xdotool"
else
  backend=""
  if have spectacle; then backend="spectacle"
  elif have grim && { have swaymsg || have hyprctl; }; then backend="grim"
  elif have import && { have kdotool || have xdotool; }; then backend="x11"
  else
    echo "spectacle-peek: no usable capture backend (need spectacle, or grim+swaymsg, or import+xdotool)" >&2
    exit 4
  fi
  # window query helper (KWin via kdotool; X11 via xdotool)
  WQ=""; have kdotool && WQ="kdotool"; { [ -z "$WQ" ] && have xdotool; } && WQ="xdotool"
fi

# --------------------------------------------------------------------- --list
if [ "$do_list" -eq 1 ]; then
  [ -n "$WQ" ] || die "--list needs kdotool or xdotool"
  ids="$($WQ search --name '.*' 2>/dev/null || $WQ search '' 2>/dev/null || true)"
  if [ -z "$ids" ]; then echo "(no windows found)" >&2; exit 0; fi
  while IFS= read -r id; do
    [ -n "$id" ] || continue
    name="$($WQ getwindowname "$id" 2>/dev/null || echo '?')"
    printf '%s\t%s\n' "$id" "$name"
  done <<< "$ids"
  exit 0
fi

# ------------------------------------------------------------- resolve a target
[ "$active" -eq 1 ] || [ -n "$winid" ] || [ -n "$title" ] || [ -n "$class" ] || [ -n "$pid" ] || \
  die "no target. Pass one of --window / --title / --class / --pid / --active (whole-screen capture is not supported)."

win_id=""
resolve_window() {
  # Sets win_id when targeting a non-active window. Empty win_id == use active.
  [ -n "$winid" ] && { win_id="$winid"; return; }
  [ "$active" -eq 1 ] && { win_id=""; return; }
  [ -n "$WQ" ] || die "targeting by --title/--class/--pid needs kdotool or xdotool"
  local out=""
  if [ -n "$title" ]; then out="$($WQ search --name "$title" 2>/dev/null || true)"; fi
  if [ -z "$out" ] && [ -n "$class" ]; then
    out="$($WQ search --class "$class" 2>/dev/null || $WQ search --classname "$class" 2>/dev/null || true)"
  fi
  if [ -z "$out" ] && [ -n "$pid" ]; then out="$($WQ search --pid "$pid" 2>/dev/null || true)"; fi
  win_id="$(printf '%s\n' "$out" | grep -v '^$' | head -n1 || true)"
  [ -n "$win_id" ] || die "no window matched (title='$title' class='$class' pid='$pid'). Try --list."
}
resolve_window

activate_target() {
  [ -n "$win_id" ] || return 0
  $WQ windowactivate "$win_id" >/dev/null 2>&1 || $WQ windowraise "$win_id" >/dev/null 2>&1 || true
}

# geometry for grim (wlroots) — "X,Y WxH"
win_geometry() {
  [ -n "$WQ" ] || return 1
  local id="${win_id:-$($WQ getactivewindow 2>/dev/null)}"
  [ -n "$id" ] || return 1
  # xdotool/kdotool getwindowgeometry --shell → X,Y,WIDTH,HEIGHT vars
  eval "$($WQ getwindowgeometry --shell "$id" 2>/dev/null)" || return 1
  [ -n "${WIDTH:-}" ] && [ -n "${HEIGHT:-}" ] || return 1
  printf '%s,%s %sx%s' "${X:-0}" "${Y:-0}" "$WIDTH" "$HEIGHT"
}

# ----------------------------------------------------------------- capture one
# capture_one <outfile> : capture the resolved (or active) WINDOW into outfile.
capture_one() {
  local out="$1"
  case "$backend" in
    spectacle)
      activate_target
      # -b background (no GUI), -n no notify, -a active window, -d settle delay
      spectacle -b -n -a -d "$settle" -o "$out" >/dev/null 2>&1
      ;;
    grim)
      local geo; geo="$(win_geometry || true)"
      [ -n "$geo" ] || { echo "spectacle-peek: could not get window geometry for grim" >&2; return 1; }
      grim -g "$geo" "$out" >/dev/null 2>&1
      ;;
    x11)
      local id="${win_id:-$($WQ getactivewindow 2>/dev/null)}"
      activate_target
      [ -n "$id" ] || { echo "spectacle-peek: no X11 window id" >&2; return 1; }
      import -window "$id" "$out" >/dev/null 2>&1
      ;;
  esac
  [ -s "$out" ]
}

shrink() {
  local f="$1"
  [ "$max_width" -gt 0 ] || return 0
  [ -n "$IM" ] || return 0
  "$IM" "$f" -resize "${max_width}x${max_width}>" "$f" >/dev/null 2>&1 || true
}

# ----------------------------------------------------------------- run dir
if [ -z "$outdir" ]; then
  outdir="/tmp/spectacle-peek/$(date +%Y%m%dT%H%M%S)-$$"
fi
mkdir -p "$outdir"

emit() { shrink "$1"; printf '%s\n' "$1"; }

# RMSE change fraction between two images (echo 0..1; 1 on error/size mismatch)
change_frac() {
  local a="$1" b="$2" m
  m="$(compare -metric RMSE "$a" "$b" null: 2>&1 || true)"
  # e.g. "3445.27 (0.0526)" → 0.0526
  local frac; frac="$(printf '%s' "$m" | sed -n 's/.*(\([0-9.][0-9.]*\)).*/\1/p')"
  [ -n "$frac" ] && printf '%s' "$frac" || printf '1'
}

count=0

if [ "$watch" -eq 1 ]; then
  # capture-on-change: poll the window, save a frame whenever it differs enough
  have compare || die "--watch needs ImageMagick 'compare'"
  prev="$outdir/.prev.png"
  end=$(( $(date +%s) + duration ))
  idx=0
  # seed with first frame (always emitted)
  if capture_one "$prev"; then
    idx=$((idx+1)); f="$(printf '%s/%02d.png' "$outdir" "$idx")"
    cp "$prev" "$f"; emit "$f"; count=$((count+1))
  fi
  while [ "$(date +%s)" -lt "$end" ]; do
    sleep "$(awk -v ms="$interval" 'BEGIN{print ms/1000}')"
    cur="$outdir/.cur.png"
    capture_one "$cur" || continue
    frac="$(change_frac "$prev" "$cur")"
    if awk -v f="$frac" -v t="$threshold" 'BEGIN{exit !(f>=t)}'; then
      idx=$((idx+1)); f="$(printf '%s/%02d.png' "$outdir" "$idx")"
      mv "$cur" "$f"; emit "$f"; count=$((count+1))
      cp "$f" "$prev"
    fi
  done
  rm -f "$outdir/.prev.png" "$outdir/.cur.png"
else
  # single shot or burst
  i=0
  while [ "$i" -lt "$burst" ]; do
    i=$((i+1))
    f="$(printf '%s/%02d.png' "$outdir" "$i")"
    if capture_one "$f"; then emit "$f"; count=$((count+1)); fi
    if [ "$i" -lt "$burst" ]; then sleep "$(awk -v ms="$interval" 'BEGIN{print ms/1000}')"; fi
  done
fi

if [ "$count" -eq 0 ]; then
  echo "spectacle-peek: captured 0 images (backend=$backend, target title='$title' class='$class' pid='$pid' active=$active)" >&2
  exit 2
fi
echo "spectacle-peek: $count image(s) in $outdir (backend=$backend)" >&2
