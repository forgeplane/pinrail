# The Pinrail desktop app on Linux, in a container with a virtual display
# that a browser shows through noVNC. `mise run linux-desktop` builds and
# starts it; see CONTRIBUTING.md.
FROM rust:1-bookworm

# what the app needs to build and run on Linux (the same as CI), a display
# with a window manager, a panel with a tray, a notification daemon, and
# noVNC to show the display in a browser
RUN apt-get update && apt-get install -y --no-install-recommends \
      libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
      librsvg2-dev libxdo-dev libssl-dev \
      xvfb openbox tint2 dunst dbus-x11 libnotify-bin xdg-utils desktop-file-utils \
      x11vnc novnc websockify \
      fonts-dejavu fonts-noto-color-emoji rsync git ca-certificates curl xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Node, the major that mise.toml pins
RUN arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) node=x64 ;; arm64) node=arm64 ;; *) echo "unsupported: $arch"; exit 1 ;; esac; \
    base=https://nodejs.org/dist/latest-v24.x; \
    file="$(curl -fsSL "$base/SHASUMS256.txt" | grep -o "node-v24[^ ]*-linux-${node}.tar.xz" | head -1)"; \
    curl -fsSL "$base/$file" | tar -xJ -C /usr/local --strip-components=1

# the app runs as a person would, not as root
RUN useradd --create-home --shell /bin/bash pinrail

COPY dunstrc /etc/xdg/dunst/dunstrc
COPY start.sh /usr/local/bin/start-pinrail
RUN chmod +x /usr/local/bin/start-pinrail

EXPOSE 6080
ENTRYPOINT ["/usr/local/bin/start-pinrail"]
