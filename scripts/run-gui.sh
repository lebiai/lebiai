#!/usr/bin/env bash
# Start the lebi-AI desktop GUI on the **stable** path: ui/dist + cargo.
#
# Usage (from repo root):
#   scripts/run-gui.sh
#   scripts/run-gui.sh --release
#
# This is the default way for collaborators / agents to open the GUI.
# Do NOT rely on Vite :5173 unless you intentionally opt into HMR (see docs).
#
# White-screen root cause we avoid here:
#   Debug builds with tauri `devUrl: http://localhost:5173` load the dev server.
#   If Vite is not running → blank webview. We removed devUrl; always load dist.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UI_DIR="$ROOT/crates/hermes-gui/ui"

RELEASE=0
for arg in "$@"; do
  case "$arg" in
    --release|-r) RELEASE=1 ;;
    -h|--help)
      sed -n '2,20p' "$0"
      exit 0
      ;;
  esac
done

if [ ! -d "$UI_DIR/node_modules" ]; then
  echo "==> npm install (one-time, in crates/hermes-gui/ui)"
  (cd "$UI_DIR" && npm install)
fi

echo "==> Building frontend → ui/dist"
(cd "$UI_DIR" && npm run build)

# Dev builds resolve bundled converter from crates/hermes-gui/resources/…
# (same tree as release Resources). Skip if user already has data-dir bin only.
if [ ! -x "$ROOT/crates/hermes-gui/resources/markitdown-sidecar/markitdown" ]; then
  if [ -x "${LEBI_DATA_DIR:-${HERMES_DATA_DIR:-$HOME/.lebi-ai}}/bin/markitdown" ]; then
    echo "==> markitdown-sidecar not in resources; using data-dir bin (ok for dev)"
  else
    echo "==> Preparing markitdown-sidecar (first time may take a minute)"
    "$ROOT/scripts/prepare-markitdown-bundle.sh" || {
      echo "warn: prepare-markitdown-bundle failed; document import may be unavailable" >&2
      echo "      try: scripts/setup-markitdown-sidecar.sh" >&2
    }
  fi
fi

if [ ! -f "$UI_DIR/dist/index.html" ]; then
  echo "error: ui/dist/index.html missing after build" >&2
  exit 1
fi

echo "==> Starting lebi-AI GUI (loads ui/dist, not :5173)"
cd "$ROOT"
if [ "$RELEASE" -eq 1 ]; then
  exec cargo run -p hermes-gui --release
else
  exec cargo run -p hermes-gui
fi
