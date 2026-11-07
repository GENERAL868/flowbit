#!/usr/bin/env bash
# What building Flowbit on Ubuntu/Debian needs (Tauri's WebKitGTK window, tray icon),
# plus what the smoke test uses (a virtual screen, D-Bus, xdg-utils)
set -euo pipefail
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential curl wget file patchelf pkg-config \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev \
  xvfb dbus dbus-x11 xdg-utils desktop-file-utils
