#!/bin/bash
# Run Elden Ring through me3 with $MOD in headless gamescope (640x360, 10 fps),
# boxed in so it cannot crowd the desktop: its own prefix, nice 19, idle I/O,
# at most four cores' worth of CPU, and a hard memory cap (killed rather than
# swapped into a freeze). Audio goes to a null sink. Stop it with
# `systemctl --user stop apoc-er-lab.scope`.
#
# Steam must be running (`steam -silent`): without it SteamAPI_Init fails
# with "create pipe failed" and the game quits. The CPU is limited by quota,
# not taskset: Wwise pins its threads to cores and fails to start otherwise.
# me3's own Steam check fails inside gamescope, so the attach config skips it.
set -eu
source "$(dirname "$0")/env.sh"
pgrep -x steam >/dev/null || { echo "Steam is not running: start it with 'steam -silent'"; exit 1; }
rm -f "$L/me3.log" "$L/native.log"; : > "$L/console.txt"
# A native module of ours for me3 to load too ($ER_NATIVE, a .dll).
NATIVES=""
if [ -n "${ER_NATIVE:-}" ]; then
  NATIVES="{ path = 'Z:$(printf '%s' "$ER_NATIVE" | sed 's#/#\\#g')', optional = false, load_before = [], load_after = [], load_early = false }"
fi
pactl list short sinks | grep -q apoc_mute || pactl load-module module-null-sink sink_name=apoc_mute >/dev/null
cat > "$L/attach.toml" <<TOML
game = "elden-ring"
natives = [$NATIVES]
early_natives = []
savefile = "$SAVE"
suspend = false
boot_boost = false
skip_logos = true
start_online = false
disable_arxan = false
mem_patch = false
skip_steam_init = true

[[packages]]
id = "apoc-rando"
source = 'Z:$(printf '%s' "$MOD" | sed 's#/#\\#g')'
load_before = []
load_after = []

[property_overrides]
TOML
cd "$GAME"
exec systemd-run --user --scope --unit=apoc-er-lab \
  -p MemoryMax=9G -p MemorySwapMax=1G -p CPUWeight=10 -p CPUQuota=400% -p IOWeight=10 \
  nice -n 19 ionice -c 3 \
  gamescope --backend headless -W 640 -H 360 -r 10 -- \
  env ME3_GAME_LAUNCHED='"elden-ring"' ME3_LAUNCHER_ARGV='[]' \
    ME3_LAUNCHER_EXE="$(win "$GAME/eldenring.exe")" \
    ME3_LAUNCHER_HOST_DLL="$(win "$RT/me3_mod_host.dll")" \
    ME3_LAUNCHER_HOST_CONFIG_PATH="$(win "$L/attach.toml")" \
    ME3_TELEMETRY_ENABLED=false ME3_FILE_LOG_LEVEL='"info"' \
    ME3_TELEMETRY_LOG_FILE_PATH="$(win "$L/me3.log")" \
    ME3_TELEMETRY_MONITOR_PIPE_PATH="$(win "$L/console.txt")" \
    ${ER_PROBE:+APOC_NATIVE_PROBE=1} \
    APOC_NATIVE_LOG="Z:$(printf '%s' "$L/native.log" | sed 's#/#\\#g')" \
  "$PROTON" waitforexitandrun "$RT/me3-launcher.exe"
