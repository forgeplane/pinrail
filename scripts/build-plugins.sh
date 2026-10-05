#!/bin/sh
# Builds each plugin in plugins/ that has a build of its own, a package.json
# beside its manifest, so that its view/ exists before the app embeds it.
# CI runs it before anything compiles the core; locally,
# `mise run plugins:build` does, and `mise run setup` includes it.
set -e
cd "$(dirname "$0")/../plugins"
for dir in */; do
  [ -f "$dir/package.json" ] && [ -f "$dir/manifest.json" ] || continue
  echo "building plugins/$dir"
  (cd "$dir" && npm ci && npm run build)
done
