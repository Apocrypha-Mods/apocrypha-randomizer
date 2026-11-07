#!/usr/bin/env bash
# Phase 7's gate on this machine: the same seed, built by apoc-rando for Linux and by
# apoc-rando.exe (x86_64-pc-windows-gnu) under Wine, must write byte-identical files.
# macOS cannot be run here; CI's determinism test covers it without game files.
#
#   APOC_ER_STOCK=~/Drive/GameStock/elden-ring/2.7.2.0-build25827938 tools/cross-check.sh [case ...]
#
# Cases are named below; with none given, all run. Output goes to scratch/cross-check/
# (gitignored), one folder per system and case, and a manifest of SHA-256 hashes for each.
# Linux runs a second time on one thread, as rayon splits work by the cores it finds.
# SKIP_BUILD=1 uses binaries already built. Exits non-zero if any file differs.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
stock=${APOC_ER_STOCK:?set APOC_ER_STOCK to the stock game folder}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
export WINEDEBUG=${WINEDEBUG:--all}

declare -A cases=(
  [default]="--seed 1"
  [enemies]="--seed 2 --enemies bosses=100,ordinary=100,mix=10"
  [fog]="--seed 3 --fog-gates"
  [dlc-start]="--seed 4 --dlc-start --dlc-start-level 150 --dlc-start-flasks --dlc-start-smithing --dlc-start-shop"
  [onslaught]="--seed 5 --onslaught 3 --enemies bosses=100,ordinary=100"
  [cosmetic]="--seed 6 --boss-music --area-music --enemy-colours --random-gestures --whole-map"
  [tweaks]="--seed 7 --dlc separate --bias keys=60,weapons=30 --usable-start --light-start --all-recipes --cheap-upgrades --no-requirements --kindling-shards 3/5 --leyndell-runes 3 --gaol-keys --night-bosses-always --boss-runes-follow --no-malenia-heal --no-gargoyle-poison --torrent-woods"
  [oops]="--seed 8 --oops-all random"
)
order=(default enemies fog dlc-start onslaught cosmetic tweaks oops)
[ $# -gt 0 ] && order=("$@")

if [ -z "${SKIP_BUILD:-}" ]; then
  cargo build --release -p rando
  cargo build --release -p rando --target x86_64-pc-windows-gnu
fi
linux=target/release/apoc-rando
windows=target/x86_64-pc-windows-gnu/release/apoc-rando.exe

out=scratch/cross-check
# Hashes every file under a case's folder, the spoiler log and profile beside the mod
# folder included, by its path relative to that folder.
manifest() { (cd "$1" && find . -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum); }

failed=()
for c in "${order[@]}"; do
  args=${cases[$c]:?unknown case $c}
  for sys in linux linux-1 windows; do rm -rf "$out/$sys/$c"; mkdir -p "$out/$sys/$c"; done
  # The mod folder has one name on all, as the profile and file list name it.
  # shellcheck disable=SC2086
  "$linux" gen $args --out "$out/linux/$c/seed" "$stock" > "$out/linux/$c.log" 2>&1
  # One thread, as another machine may have any number of cores.
  # shellcheck disable=SC2086
  RAYON_NUM_THREADS=1 "$linux" gen $args --out "$out/linux-1/$c/seed" "$stock" > "$out/linux-1/$c.log" 2>&1
  # shellcheck disable=SC2086
  wine "$windows" gen $args --out "$(winepath -w "$out/windows/$c/seed")" "$(winepath -w "$stock")" \
    > "$out/windows/$c.log" 2>&1
  for sys in linux linux-1 windows; do manifest "$out/$sys/$c" > "$out/$sys/$c.sha256"; done
  n=$(wc -l < "$out/linux/$c.sha256")
  if cmp -s "$out/linux/$c.sha256" "$out/windows/$c.sha256" \
    && cmp -s "$out/linux/$c.sha256" "$out/linux-1/$c.sha256"; then
    echo "same     $c: $n files ($args)"
  else
    echo "DIFFERENT $c ($args):"
    diff "$out/linux/$c.sha256" "$out/windows/$c.sha256" | head -20 || true
    diff "$out/linux/$c.sha256" "$out/linux-1/$c.sha256" | head -20 || true
    failed+=("$c")
  fi
done

if [ ${#failed[@]} -gt 0 ]; then
  echo "differ: ${failed[*]}"
  exit 1
fi
echo "all ${#order[@]} cases byte-identical on Linux, on Linux with one thread and under Wine"
