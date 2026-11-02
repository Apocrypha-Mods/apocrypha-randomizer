#!/bin/sh
# Builds the add-on as the one file the mod manager installs:
# dist/randomizer-<version>.apocaddon, a zip of the interface, the module and auto-upgrade's native module.
# GPL-3.0-or-later (PLAN.md §6b): the package carries the licence, and each release must have its source public.
set -eu
cd "$(dirname "$0")/.."
export CARGO_PROFILE_RELEASE_DEBUG=false CARGO_PROFILE_RELEASE_STRIP=true CARGO_PROFILE_RELEASE_LTO=thin CARGO_TARGET_DIR=target/addon
cargo build --release -p addon --target wasm32-unknown-unknown
version=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' ui/addon.json)
rm -rf dist/pack && mkdir -p dist/pack
cp LICENSE ui/addon.json ui/index.html ui/app.css ui/app.js ui/core.js ui/host.js ui/logic.js ui/pieces.js ui/worker.js dist/pack/
cp target/addon/wasm32-unknown-unknown/release/addon.wasm dist/pack/randomizer.wasm
# Auto-upgrade's module, which the page writes into the mod when the option is on.
cargo build --release -p native --target x86_64-pc-windows-gnu
cp target/addon/x86_64-pc-windows-gnu/release/apoc_native.dll dist/pack/
out="dist/randomizer-$version.apocaddon"
rm -f "$out"
python3 -I -c '
import sys, zipfile, pathlib
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for f in sorted(pathlib.Path("dist/pack").iterdir()):
        z.write(f, f.name)
' "$out"
rm -rf dist/pack
ls -l "$out"
sha256sum "$out"
