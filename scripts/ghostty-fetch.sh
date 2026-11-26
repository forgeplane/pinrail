#!/bin/sh
# Downloads the libghostty-vt tarballs desktop/screen/ghostty.lock pins and
# checks their SHA-256 sums, so that a later build needs no network.
#
#   sh scripts/ghostty-fetch.sh [folder]
#
# Without a folder, they go where desktop/screen's build looks first,
# desktop/target/ghostty/<version>. With one, point the build at it with
# PINRAIL_GHOSTTY_SRC=<folder>.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
lock="$root/desktop/screen/ghostty.lock"
# the SHA-256 of a file: sha256sum where there is one, as on Linux and in
# Git for Windows, else shasum, as on macOS
sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d ' ' -f 1
}
value() { sed -n "/^\[$1\]/,/^\[/s/^$2 = \"\(.*\)\"$/\1/p" "$lock" | head -n 1; }
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$lock" | head -n 1)
dir=${1:-"$root/desktop/target/ghostty/$version"}
mkdir -p "$dir"

for part in source deps; do
  url=$(value "$part" url)
  sum=$(value "$part" sha256)
  file="$dir/$(basename "$url")"
  if [ ! -f "$file" ]; then
    curl --fail --silent --show-error --location --output "$file.download" "$url"
    mv "$file.download" "$file"
  fi
  actual=$(sha256 "$file")
  if [ "$actual" != "$sum" ]; then
    echo "ghostty:fetch: $file has SHA-256 $actual, but ghostty.lock expects $sum" >&2
    exit 1
  fi
  echo "ghostty:fetch: $(basename "$file") checked"
done
echo "ghostty:fetch: libghostty-vt $version is in $dir"
