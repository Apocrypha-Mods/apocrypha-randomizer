#!/bin/bash
# Press keys in the lab game, then save a screenshot: keys.sh OUT.png KEY...
# A key is an xdotool name (Return, Escape, Down, w, e); `waitN` pauses N
# seconds. Keys go to the game's own X display inside gamescope, never the
# desktop's.
set -eu
OUT=$1; shift
GPID=$(ps -eo pid,args | grep -a 'ELDEN RING\\Game\\eldenring.exe' | grep -v grep | awk '{print $1}' | head -1)
[ -n "$GPID" ] || { echo "the lab game is not running"; exit 1; }
envof() { tr '\0' '\n' < /proc/$GPID/environ | sed -n "s/^$1=//p"; }
export DISPLAY=$(envof DISPLAY) GAMESCOPE_WAYLAND_DISPLAY=$(envof GAMESCOPE_WAYLAND_DISPLAY)
[ -n "$GAMESCOPE_WAYLAND_DISPLAY" ] || { echo "the game is not inside gamescope"; exit 1; }
for k in "$@"; do
  case $k in wait*) sleep "${k#wait}"; continue;; esac
  xdotool keydown "$k"; sleep 0.25; xdotool keyup "$k"; sleep 0.8
done
rm -f "$OUT"; gamescopectl screenshot "$OUT"
for _ in $(seq 20); do [ -s "$OUT" ] && break; sleep 0.5; done
