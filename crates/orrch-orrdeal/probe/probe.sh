#!/bin/sh
# orrdeal probe agent — emits ONE JSON object describing the host.
# Must run on minimal images (alpine/busybox) and over SSH. POSIX sh only.
os="$(uname -s)"
if [ -r /etc/os-release ]; then
  # shellcheck disable=SC1091
  . /etc/os-release
  os="${ID:-$os} ${VERSION_ID:-}"
fi
arch="$(uname -m)"

cam=false
for d in /dev/video0 /dev/video1; do
  [ -e "$d" ] && cam=true && break
done
mic=false
[ -e /dev/snd ] && mic=true
gpu=false
{ [ -e /dev/dri ] || [ -e /dev/nvidia0 ]; } && gpu=true

printf '{"os":"%s","arch":"%s","camera":%s,"mic":%s,"gpu":%s,"filesystem":true}\n' \
  "$os" "$arch" "$cam" "$mic" "$gpu"
