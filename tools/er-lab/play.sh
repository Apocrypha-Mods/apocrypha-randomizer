#!/bin/bash
# Play a randomizer mod folder on the desktop through me3, in the game's own
# Steam prefix, on a save of its own so the real one is not touched. Not the
# lab: this is a normal window with sound and no limits, for a person to play.
#
#   tools/er-lab/play.sh [mod folder]    (default scratch/patch-test)
#
# Steam must be running. The module's log is scratch/play/native.log.
set -eu
source "$(dirname "$0")/env.sh"
MOD="$(realpath "${1:-$REPO/scratch/patch-test}")"
[ -f "$MOD/regulation.bin" ] || { echo "$MOD has no regulation.bin: build it with apoc-rando gen"; exit 1; }
pgrep -x steam >/dev/null || { echo "Steam is not running: start it first"; exit 1; }

L="$REPO/scratch/play"
mkdir -p "$L"
rm -f "$L/me3.log" "$L/native.log"; : > "$L/console.txt"
export WINEPREFIX="$STEAM_PFX"
export STEAM_COMPAT_DATA_PATH="$(dirname "$STEAM_PFX")"
unset PULSE_SINK PROTON_DISABLE_HIDRAW

NATIVES=""
if [ -f "$MOD/apoc_native.dll" ]; then
  NATIVES="{ path = 'Z:$(printf '%s' "$MOD/apoc_native.dll" | sed 's#/#\\#g')', optional = false, load_before = [], load_after = [], load_early = false }"
fi
cat > "$L/attach.toml" <<TOML
game = "elden-ring"
natives = [$NATIVES]
early_natives = []
savefile = "apoc-rando-play.sl2"
suspend = false
boot_boost = false
skip_logos = true
start_online = false
disable_arxan = false
mem_patch = false
skip_steam_init = false

[[packages]]
id = "apoc-rando"
source = 'Z:$(printf '%s' "$MOD" | sed 's#/#\\#g')'
load_before = []
load_after = []

[property_overrides]
TOML
cd "$GAME"
exec env ME3_GAME_LAUNCHED='"elden-ring"' ME3_LAUNCHER_ARGV='[]' \
  ME3_LAUNCHER_EXE="$(win "$GAME/eldenring.exe")" \
  ME3_LAUNCHER_HOST_DLL="$(win "$RT/me3_mod_host.dll")" \
  ME3_LAUNCHER_HOST_CONFIG_PATH="$(win "$L/attach.toml")" \
  ME3_TELEMETRY_ENABLED=false ME3_FILE_LOG_LEVEL='"info"' \
  ME3_TELEMETRY_LOG_FILE_PATH="$(win "$L/me3.log")" \
  ME3_TELEMETRY_MONITOR_PIPE_PATH="$(win "$L/console.txt")" \
  APOC_NATIVE_LOG="Z:$(printf '%s' "$L/native.log" | sed 's#/#\\#g')" \
  "$PROTON" waitforexitandrun "$RT/me3-launcher.exe"
