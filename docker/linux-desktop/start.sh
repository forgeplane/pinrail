#!/bin/bash
# Builds the app and the CLI from the checkout mounted at /src, then runs
# the app on a virtual display that http://localhost:6080 shows.
#
# The checkout is copied to /work rather than built in place, so Linux
# packages and build output never land in the host's working tree. The
# build output and the npm packages are kept in volumes between runs.
set -euo pipefail

if [ "$(id -u)" = 0 ]; then
  # the volumes are created for root; the build and the app run as pinrail
  mkdir -p /work /cache
  chown -R pinrail:pinrail /work /cache
  # the CLI, for `docker exec -u pinrail pinrail-linux pinrail …`
  ln -sf /cache/target/debug/pinrail /usr/local/bin/pinrail
  exec runuser -u pinrail -- "$0" "$@"
fi

# the toolchain rust-toolchain.toml pins is installed here on the first run
export CARGO_HOME=/cache/cargo
export RUSTUP_HOME=/cache/rustup
export CARGO_TARGET_DIR=/cache/target
export PATH="/usr/local/cargo/bin:$PATH"
export DISPLAY=:99
# WebKitGTK has no GPU in a container
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1

echo "pinrail: copying the checkout"
rsync -a --delete --exclude node_modules --exclude target --exclude dist \
  --exclude .git --exclude 'website/' /src/ /work/

echo "pinrail: building the UI"
(cd /work/desktop/app && npm ci --no-audit --no-fund --loglevel=error && npm run build)

echo "pinrail: building the app and the CLI (the first build takes a while)"
# custom-protocol serves the built UI, where a plain debug build would
# look for the Vite dev server
(cd /work/desktop && cargo build -p pinrail-desktop --features tauri/custom-protocol)
(cd /work/cli && cargo build)

echo "pinrail: starting the display"
Xvfb :99 -screen 0 1440x900x24 -nolisten tcp &
sleep 1
eval "$(dbus-launch --sh-syntax)"
export DBUS_SESSION_BUS_ADDRESS
openbox &
tint2 &
dunst &
x11vnc -display :99 -forever -shared -nopw -quiet -rfbport 5900 &
websockify --web /usr/share/novnc 6080 localhost:5900 >/dev/null 2>&1 &

echo "pinrail: the app is at http://localhost:6080/vnc.html?autoconnect=1&resize=scale"
echo "pinrail: in another terminal, docker exec -u pinrail pinrail-linux pinrail <command>"
exec "$CARGO_TARGET_DIR/debug/Pinrail"
