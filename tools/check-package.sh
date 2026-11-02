#!/usr/bin/env bash
# Whether the website's scan would take this package's files: tools/check-package.sh <package>
# A copy of the file rules in apocrypha-web's AddonPackageRules, run before anything is signed,
# as an add-on version number can never be used twice.
set -euo pipefail
pkg=$1
die() { echo "check-package: $*" >&2; exit 1; }
allowed=" html htm js mjs css json wasm woff woff2 ttf otf png jpg jpeg gif webp avif svg ico txt md "
while IFS= read -r entry; do
  [[ "$entry" == */ ]] && continue
  # unzip -p reads its argument as a pattern, so names stay plain.
  [[ "$entry" =~ ^[A-Za-z0-9._/-]+$ ]] || die "a file has an unusual name: $entry"
  magic=$({ unzip -p "$pkg" "$entry" || true; } | head -c 4 | od -An -tx1 | tr -d ' \n')
  # Auto-upgrade's module, the one program the website takes, by this name at the root only.
  if [ "$entry" = apoc_native.dll ]; then
    [[ "$magic" == 4d5a* ]] || die "$entry is not a Windows module"
    continue
  fi
  base=${entry##*/}
  case "${base^^}" in
    LICENSE|README|NOTICE|COPYING) ;;
    .*|*.) die "the website refuses $entry (no extension)" ;;
    *.*) ext=${base##*.}; [[ "$allowed" == *" ${ext,,} "* ]] || die "the website refuses $entry (.$ext files are not accepted)" ;;
    *) die "the website refuses $entry (no extension)" ;;
  esac
  case "$magic" in
    7f454c46|4d5a*|feedface|feedfacf|cefaedfe|cffaedfe|cafebabe|bebafeca) die "the website refuses $entry (it starts like a program)" ;;
    504b*|1f8b*|377abcaf|52617221|fd377a58|28b52ffd|425a68*) die "the website refuses $entry (it starts like an archive)" ;;
  esac
done < <(unzip -Z1 "$pkg")
