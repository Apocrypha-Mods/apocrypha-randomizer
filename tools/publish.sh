#!/usr/bin/env bash
# Builds a release of the add-on, then stops: the package and its signed release.json
# are uploaded by hand in the creator editor (apocrypha-web, docs/addons.md).
#
#   tools/publish.sh --check                      build and judge the package; no key, no signature
#   tools/publish.sh --sign --notes "text"        also sign, with the offline key addons-1
#   tools/publish.sh --sign --dev --notes "text"  sign as addons-dev, for a local website only
#
# The key is $ADDONS_SIGNING_KEY. A real signature needs a clean tree whose commit is on
# origin/dev, as the licence (GPL-3.0-or-later) wants each release's source public, and it
# tags that commit. A version number can never be used twice: bump ui/addon.json for each.
set -euo pipefail
usage() { echo 'usage: tools/publish.sh --check | --sign [--dev] (--notes "text" | --notes-file <file>)' >&2; exit 2; }
die() { echo "publish: $*" >&2; exit 1; }

mode="" dev=0 notes="" notes_set=0
while [ $# -gt 0 ]; do
  case "$1" in
    --check) mode=check ;;
    --sign) mode=sign ;;
    --dev) dev=1 ;;
    --notes) [ $# -ge 2 ] || usage; notes=$2; notes_set=1; shift ;;
    --notes-file) [ $# -ge 2 ] && [ -f "$2" ] || usage; notes=$(cat -- "$2"); notes_set=1; shift ;;
    *) usage ;;
  esac
  shift
done
[ -n "$mode" ] || usage
cd "$(dirname "$0")/.."
for t in jq openssl unzip git sha256sum base64; do
  command -v "$t" >/dev/null || die "$t is not installed"
done

key_id=addons-1
expected_pub="i0PVdz0mpjBbK2pWYZp+g4ljLnqpr2PoAnyjzILmK20="
[ "$dev" = 0 ] || key_id=addons-dev
key=${ADDONS_SIGNING_KEY:-}
if [ "$mode" = sign ]; then
  [ "$notes_set" = 1 ] || die "release notes are required to sign"
  notes_len=$(printf '%s' "$notes" | LC_ALL=C.UTF-8 wc -m)
  [ "$notes_len" -ge 1 ] && [ "$notes_len" -le 400 ] || die "release notes are $notes_len characters, from 1 to 400"
  [ -r "$key" ] || die "ADDONS_SIGNING_KEY does not name a readable key"
  if [ "$dev" = 0 ]; then
    [ -z "$(git status --porcelain)" ] || die "the working tree has changes"
    git fetch -q origin dev
    git merge-base --is-ancestor HEAD origin/dev || die "this commit is not on origin/dev, so its source is not public"
  fi
fi

tools/pack.sh >/dev/null
id=$(jq -er .id ui/addon.json)
version=$(jq -er .version ui/addon.json)
pkg="dist/$id-$version.apocaddon"
[ -f "$pkg" ] || die "pack did not produce $pkg"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version $version is not major.minor.patch"
manifest=$(unzip -p "$pkg" addon.json) || die "the package has no addon.json"
[ "$(jq -cS . <<<"$manifest")" = "$(jq -cS . ui/addon.json)" ] || die "the package's manifest is not ui/addon.json"
size=$(stat -c %s "$pkg")
[ "$size" -le $((64 * 1024 * 1024)) ] || die "the package is $size bytes, at most 64 MiB"
[ "$(unzip -Z1 "$pkg" | wc -l)" -le 400 ] || die "the package holds more than 400 entries"
unpacked=$(unzip -Zt "$pkg" | sed -n 's/.* \([0-9]*\) bytes uncompressed.*/\1/p')
[ "$unpacked" -le $((64 * 1024 * 1024)) ] || die "the package unpacks to $unpacked bytes, at most 64 MiB"
tools/check-package.sh "$pkg"
sha=$(sha256sum "$pkg" | cut -d' ' -f1)
echo "$id $version  $size bytes  sha256 $sha"
if [ "$mode" = check ]; then
  echo "the package's files are ones the website takes; nothing signed"
  exit 0
fi
[ "$dev" = 1 ] || ! git rev-parse -q --verify "refs/tags/v$version" >/dev/null || die "tag v$version already exists; bump the version"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
jq -cj --argjson size "$size" --arg sha "$sha" --arg notes "$notes" --arg published "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '{
    schema: "addon-release/1", id, name, about, version,
    games: (.games // []), needs: (.needs // []),
    size: $size, sha256: $sha, notes: $notes, published: $published
  }' <<<"$manifest" >"$work/release"
openssl pkeyutl -sign -rawin -inkey "$key" -in "$work/release" -out "$work/sig" 2>/dev/null || die "signing failed; is it an Ed25519 key?"
openssl pkey -in "$key" -pubout -out "$work/pub.pem" 2>/dev/null || die "cannot derive the public key"
pub=$(openssl pkey -pubin -in "$work/pub.pem" -outform DER 2>/dev/null | tail -c 32 | base64 -w0)
[ "$dev" = 1 ] || [ "$pub" = "$expected_pub" ] || die "the signing key is not $key_id"
openssl pkeyutl -verify -rawin -pubin -inkey "$work/pub.pem" -in "$work/release" -sigfile "$work/sig" >/dev/null 2>&1 \
  || die "the signature does not verify"
out="dist/$id-$version.release.json"
jq -cnj --arg keyId "$key_id" --arg release "$(base64 -w0 "$work/release")" --arg signature "$(base64 -w0 "$work/sig")" \
  '{keyId: $keyId, release: $release, signature: $signature}' >"$out"
[ "$(stat -c %s "$out")" -le $((16 * 1024)) ] || die "release.json is over 16 KiB"
jq -r .release "$out" | base64 -d | jq .
echo "signed as $key_id: $pkg and $out, for the creator editor"
if [ "$dev" = 0 ]; then
  git tag -a "v$version" -m "$id $version" -m "$notes"
  echo "tagged v$version; push it with: git push origin v$version"
fi
