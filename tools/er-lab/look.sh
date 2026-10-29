#!/bin/bash
# Turn the lab game's camera: look.sh DX moves the mouse DX pixels on the
# game's own X display (about 45 degrees per 400; negative turns left). The
# I/J/K/L camera keys do nothing there.
set -eu
GPID=$(ps -eo pid,args | grep -a 'ELDEN RING\\Game\\eldenring.exe' | grep -v grep | awk '{print $1}' | head -1)
[ -n "$GPID" ] || { echo "the lab game is not running"; exit 1; }
export DISPLAY=$(tr '\0' '\n' < /proc/$GPID/environ | sed -n 's/^DISPLAY=//p')
step=40; [ "${1:0:1}" = "-" ] && step=-40
for _ in $(seq $(( ${1#-} / 40 ))); do
  xdotool mousemove_relative -- $step 0; sleep 0.05
done
