# Settings for the headless Elden Ring lab, sourced by the other scripts.
# Machine paths come first; each can be overridden from the environment.
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
L="${ER_LAB_DIR:-$REPO/scratch/lab}"
GAME="${ER_GAME:-$HOME/Drive/SteamLibrary/steamapps/common/ELDEN RING/Game}"
STEAM_PFX="${ER_STEAM_PFX:-$HOME/Drive/SteamLibrary/steamapps/compatdata/1245620/pfx}"
RT="${ER_ME3:-$HOME/.local/share/apocrypha/me3-runtime/v0.13.0}"
STEAMID="${ER_STEAMID:-76561198147790693}"
# The mod folder to load, and the save (in the lab prefix) to play on.
MOD="${ER_MOD:-$REPO/scratch/boot-test-mod}"
SAVE="${ER_SAVE:-apoc-rando-boot.sl2}"

# Steam's own Proton build, with a compatdata folder of the lab's own: it
# copies the Steam client files into the prefix, which me3's launcher needs.
PROTON="${ER_PROTON:-/usr/share/steam/compatibilitytools.d/proton-cachyos-slr/proton}"
export WINEPREFIX=$L/pfx
export STEAM_COMPAT_DATA_PATH=$L
export STEAM_COMPAT_CLIENT_INSTALL_PATH=$HOME/.local/share/Steam
export SteamAppId=1245620 SteamGameId=1245620
export PULSE_SINK=apoc_mute PROTON_DISABLE_HIDRAW=1
ROAMING="$WINEPREFIX/drive_c/users/steamuser/AppData/Roaming/EldenRing"

# A Linux path as a JSON string of its Windows path under Wine's Z: drive.
win() { printf '"Z:%s"' "$(printf '%s' "$1" | sed 's#/#\\\\#g')"; }
