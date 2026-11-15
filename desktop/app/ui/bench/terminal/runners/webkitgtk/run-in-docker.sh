#!/bin/sh
# Runs the benchmark in WebKitGTK on Linux, inside the Docker image of
# `mise run linux-desktop` (Debian bookworm, webkit2gtk-4.1, Xvfb). The
# display is virtual and WebGL is drawn by Mesa's software renderer, so the
# numbers show that the path works and how WebKitGTK compares, not the
# reference machine's results.
#
#   runners/webkitgtk/run-in-docker.sh [run.mjs options, such as --cases burst]
#
# On a Linux machine with webkit2gtk-4.1 installed, run the same steps
# without Docker: build runner.c with gcc, then
#   WEBKITGTK_RUNNER=./wkgtk-runner node run.mjs --runners webkitgtk
set -e
bench=$(cd "$(dirname "$0")/../.." && pwd)
repo=$(cd "$bench/../../../../.." && pwd)
# The page bundle, the corpus and the host's snapshots are made here, as
# the image has no Rust; node_modules hold no native code the run needs.
cd "$bench"
[ -f corpus/synthetic/tui.bin ] || node corpus/tools/gen.mjs
node build.mjs >/dev/null
node --input-type=module -e '
  const { startCore } = await import("./core/server.mjs");
  await (await startCore()).close();
'
docker build -q -t pinrail-linux-desktop "$repo/docker/linux-desktop" >/dev/null
docker run --rm --shm-size=1g -v "$repo:/repo" --entrypoint bash pinrail-linux-desktop -c '
  set -e
  gcc -O2 -o /tmp/wkgtk-runner /repo/desktop/app/ui/bench/terminal/runners/webkitgtk/runner.c $(pkg-config --cflags --libs webkit2gtk-4.1)
  cd /repo/desktop/app/ui/bench/terminal
  Xvfb :99 -screen 0 2000x800x24 -nolisten tcp >/dev/null 2>&1 &
  sleep 1
  DISPLAY=:99 WEBKITGTK_RUNNER=/tmp/wkgtk-runner \
    node run.mjs --runners webkitgtk --no-build --tag linux-docker "$@"
' bash "$@"
