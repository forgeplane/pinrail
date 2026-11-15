#!/bin/sh
# Opens one renderer on your login shell in a WKWebView window, for the hand
# checks in checks.md. Close the window to stop. macOS only.
#
#   ui/bench/terminal/live.sh ghostty          # screen reader mirror off
#   ui/bench/terminal/live.sh ghostty --sr     # screen reader mirror on
#   ui/bench/terminal/live.sh xterm --item synthetic/unicode.bin
set -e
cd "$(dirname "$0")"
adapter=${1:?usage: live.sh <adapter> [--sr] [--item <corpus item>]}
shift
query="adapter=$adapter&case=live&pty=shell"
while [ $# -gt 0 ]; do
  case $1 in
    --sr) query="$query&sr=1" ;;
    --item) query="$query&item=$2"; shift ;;
  esac
  shift
done
[ -f corpus/synthetic/tui.bin ] || node corpus/tools/gen.mjs
node build.mjs >/dev/null
mkdir -p .cache
[ .cache/wkcheck -nt runners/wkcheck.swift ] || swiftc -O runners/wkcheck.swift -o .cache/wkcheck
node core/server.mjs --port 7358 >/dev/null &
core=$!
trap 'kill $core 2>/dev/null' EXIT
sleep 2
.cache/wkcheck "http://127.0.0.1:7358/page.html?$query" 1400 760 --hold
