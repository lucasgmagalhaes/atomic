#!/usr/bin/env bash
# Starts opencode's server mode bound to all network interfaces, so other
# devices on the same LAN/Wi-Fi can connect to it (e.g. from a phone or
# another computer's browser, or from a remote opencode/TUI client pointed
# at this machine's IP).
#
# Requires opencode to be installed (https://opencode.ai). This script does
# NOT install it — run it once first, e.g.:
#   curl -fsSL https://opencode.ai/install | bash
# or via your package manager (see opencode's own docs, since install
# methods change over time).
#
# Usage:
#   ./start-opencode-remote.sh [port]
#
# Env vars:
#   OPENCODE_PORT   Port to listen on (default: 4096)
#   OPENCODE_ARGS   Extra arguments passed through to `opencode serve`
#
# Security note: this exposes opencode (and therefore shell/file access on
# this machine, depending on what it's doing) to every device on your
# local network. Only run this on a network you trust, and stop it
# (Ctrl+C) when you're done.

set -euo pipefail

PORT="${1:-${OPENCODE_PORT:-4096}}"
HOST="0.0.0.0"

if ! command -v opencode >/dev/null 2>&1; then
  echo "error: 'opencode' was not found on PATH." >&2
  echo "Install it first — see https://opencode.ai — then re-run this script." >&2
  exit 1
fi

# Best-effort LAN IP detection, for printing a connectable URL. Tries the
# common macOS interfaces first (Wi-Fi/Ethernet), then falls back to
# whatever `ipconfig`/`ifconfig` can find. If detection fails, the script
# still starts the server — you can find your IP manually with
# `ipconfig getifaddr en0` (Wi-Fi) or `ipconfig getifaddr en1` (Ethernet).
detect_lan_ip() {
  local ip=""
  if command -v ipconfig >/dev/null 2>&1; then
    for iface in en0 en1 en2; do
      ip="$(ipconfig getifaddr "$iface" 2>/dev/null || true)"
      [ -n "$ip" ] && break
    done
  fi
  if [ -z "$ip" ] && command -v ifconfig >/dev/null 2>&1; then
    ip="$(ifconfig | awk '/inet /{print $2}' | grep -v '^127\.' | head -n1)"
  fi
  echo "$ip"
}

LAN_IP="$(detect_lan_ip)"

echo "Starting opencode server on ${HOST}:${PORT} ..."
if [ -n "$LAN_IP" ]; then
  echo "Reachable from other devices on this network at: http://${LAN_IP}:${PORT}"
else
  echo "Could not auto-detect this machine's LAN IP — check it manually"
  echo "(e.g. 'ipconfig getifaddr en0') and connect to http://<that-ip>:${PORT}"
fi
echo "Press Ctrl+C to stop."
echo

# shellcheck disable=SC2086
exec opencode serve --hostname "$HOST" --port "$PORT" ${OPENCODE_ARGS:-}
