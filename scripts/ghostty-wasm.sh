#!/bin/sh
# Builds libghostty-vt as WebAssembly for the window's terminal renderer,
# from the release desktop/screen/ghostty.lock pins, and puts it with its
# BUILD_INFO where the app serves it, desktop/app/ui/public/ghostty/. The
# build is kept in desktop/target/ghostty/<version>/wasm/ and made again
# only when the pin changes; the same tag and Zig give the same file.
#
#   sh scripts/ghostty-wasm.sh
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
lock="$root/desktop/screen/ghostty.lock"
field() { sed -n "s/^$1 = \"\(.*\)\"$/\1/p" "$lock" | head -n 1; }
version=$(field version)
tag=$(field tag)
commit=$(field commit)
want_zig=$(field zig)

# builds into the cache, with Zig at the version the pin names
build() {
  zig=${ZIG:-zig}
  # outside `mise run`, the Zig mise installs
  if ! command -v "$zig" >/dev/null 2>&1; then
    zig=$(mise which zig 2>/dev/null) || {
      echo "ghostty:wasm: building libghostty-vt needs Zig $want_zig; install it with \`mise install\`" >&2
      exit 1
    }
  fi
  zig_version=$("$zig" version)
  case "$zig_version" in
    "$want_zig".*) ;;
    *)
      echo "ghostty:wasm: libghostty-vt needs Zig $want_zig, but $zig is $zig_version" >&2
      exit 1
      ;;
  esac
  # the tarballs, checked against ghostty.lock
  dir="$root/desktop/target/ghostty/$version"
  sh "$root/scripts/ghostty-fetch.sh" "$dir" >/dev/null
  # resolved, as Zig's relative paths break across /var and /private/var
  work=$(cd "$(mktemp -d)" && pwd -P)
  trap 'rm -rf "$work"' EXIT
  tar -xzf "$dir/libghostty-vt-$version.tar.gz" -C "$work"
  tar -xzf "$dir/libghostty-vt-$version-deps.tar.gz" -C "$work"
  (
    cd "$work/libghostty-vt-$version"
    ZIG_GLOBAL_CACHE_DIR="$work/cache" "$zig" build \
      --prefix "$work/out" \
      --system "$work/libghostty-vt-$version-deps" \
      -Doptimize=ReleaseSmall \
      -Dtarget=wasm32-freestanding \
      -Demit-xcframework=false \
      -Dstrip=true
  )
  mkdir -p "$cache"
  cp "$work/out/bin/ghostty-vt.wasm" "$cache/ghostty-vt.wasm"
  sum=$(shasum -a 256 "$cache/ghostty-vt.wasm" | cut -d ' ' -f 1)
  cat > "$cache/BUILD_INFO" <<INFO
# libghostty-vt as WebAssembly, written by scripts/ghostty-wasm.sh
tag = "$tag"
commit = "$commit"
zig = "$zig_version"
target = "wasm32-freestanding"
optimize = "ReleaseSmall"
sha256 = "$sum"
INFO
}

out="$root/desktop/app/ui/public/ghostty"
cache="$root/desktop/target/ghostty/$version/wasm"
info_of() { sed -n "s/^$2 = \"\(.*\)\"$/\1/p" "$1/BUILD_INFO" 2>/dev/null | head -n 1; }

# a build of the pinned release, kept until the pin changes
if [ "$(info_of "$cache" tag)" != "$tag" ] || [ "$(info_of "$cache" commit)" != "$commit" ] ||
  [ "$(shasum -a 256 "$cache/ghostty-vt.wasm" 2>/dev/null | cut -d ' ' -f 1)" != "$(info_of "$cache" sha256)" ]; then
  echo "ghostty:wasm: building libghostty-vt $version as WebAssembly, once for this release"
  build
fi
mkdir -p "$out"
cp "$cache/ghostty-vt.wasm" "$cache/BUILD_INFO" "$out/"
