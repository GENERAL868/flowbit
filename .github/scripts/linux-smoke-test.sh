#!/usr/bin/env bash
# Starts Flowbit on a virtual screen with a fresh data folder and checks that it keeps
# running, that the torrent engine and streaming server are up, and that it made
# itself the handler for magnet links. Usage: linux-smoke-test.sh <path to flowbit>
set -euo pipefail
app="$(realpath "$1")"
work="$(mktemp -d)"
export FLOWBIT_DATA_DIR="$work/data"
export HOME="$work/home"
mkdir -p "$FLOWBIT_DATA_DIR" "$HOME"
echo "{\"downloadDir\": \"$work/downloads\", \"sounds\": false}" > "$FLOWBIT_DATA_DIR/prefs.json"

dbus-run-session -- bash -c '
  xvfb-run -a "$0" > "$1/app.log" 2>&1 &
  for i in $(seq 1 30); do
    sleep 1
    if ss -ltn | grep -q ":52525 "; then break; fi
  done
  status=0
  pgrep -x flowbit > /dev/null || { echo "Flowbit is not running"; status=1; }
  ss -ltn | grep -q ":52525 " || { echo "The streaming server is not listening"; status=1; }
  [ -s "$FLOWBIT_DATA_DIR/session/session.json" ] || [ -d "$FLOWBIT_DATA_DIR/session" ] \
    || { echo "The torrent engine did not start"; status=1; }
  handler="$(xdg-mime query default x-scheme-handler/magnet || true)"
  [ "$handler" = "flowbit-handler.desktop" ] \
    || { echo "Not the magnet link handler (got: $handler)"; status=1; }
  pkill -x flowbit || true
  sleep 1
  echo "--- app log"; cat "$1/app.log"
  [ "$status" = 0 ] && echo "Flowbit started fine"
  exit "$status"
' "$app" "$work"
