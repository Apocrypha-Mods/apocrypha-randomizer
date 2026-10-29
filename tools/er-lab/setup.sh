#!/bin/bash
# Make the lab's own prefix: low graphics in a 640x360 window, and a copy of
# the save to play on from the Steam prefix. Nothing in the Steam prefix is
# changed.
set -eu
source "$(dirname "$0")/env.sh"
mkdir -p "$L"
if [ ! -d "$WINEPREFIX/drive_c/users/steamuser" ]; then
  nice -n 19 "$PROTON" run cmd /c exit > "$L/setup.log" 2>&1 || true
fi
mkdir -p "$ROAMING/$STEAMID"
SRC="$STEAM_PFX/drive_c/users/steamuser/AppData/Roaming/EldenRing"
[ -f "$SRC/$STEAMID/$SAVE" ] && cp -f "$SRC/$STEAMID/$SAVE" "$ROAMING/$STEAMID/"
python3 - "$SRC/GraphicsConfig.xml" "$ROAMING/GraphicsConfig.xml" <<'PY'
import re, sys
s = open(sys.argv[1], encoding='utf-16-le').read()
s = s.replace('<ScreenMode>FULLSCREEN<', '<ScreenMode>WINDOW<')
s = re.sub(r'(Resolution-\w+Width>)\d+', r'\g<1>640', s)
s = re.sub(r'(Resolution-\w+Height>)\d+', r'\g<1>360', s)
s = s.replace('BestRenderingSettings>ON<', 'BestRenderingSettings>OFF<')
s = s.replace('>HIGH<', '>LOW<')
open(sys.argv[2], 'w', encoding='utf-16-le').write(s)
PY
echo "prefix ready: $WINEPREFIX"
