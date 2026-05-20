#!/usr/bin/env bash
# Build a macOS .dmg of Hermes GUI.
#
# Usage:    scripts/build-dmg.sh
# Output:   target/release/bundle/dmg/Hermes_<version>_<arch>.dmg
# Requires: macOS, Node + npm, `cargo install tauri-cli --version "^2.0" --locked`
#
# Universal (Intel + Apple Silicon) build:
#   rustup target add x86_64-apple-darwin
#   TAURI_TARGET=universal-apple-darwin scripts/build-dmg.sh
#
# Notes:
# - The resulting DMG is unsigned. macOS Gatekeeper will warn on first launch.
#   For distribution, add codesigning + notarization separately.
# - First run can take 5-10 minutes (Tauri pulls in webkit bindings, etc.).

set -euo pipefail

if [ "$(uname)" != "Darwin" ]; then
  echo "DMG can only be built on macOS." >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UI_DIR="$ROOT/crates/hermes-gui/ui"
GUI_DIR="$ROOT/crates/hermes-gui"

if ! cargo tauri --version >/dev/null 2>&1; then
  cat >&2 <<'EOF'
cargo tauri not installed.

Install with:
  cargo install tauri-cli --version "^2.0" --locked
EOF
  exit 1
fi

if [ ! -d "$UI_DIR/node_modules" ]; then
  echo "==> npm install (one-time, in $UI_DIR)"
  (cd "$UI_DIR" && npm install)
fi

echo "==> Building frontend (vite)"
(cd "$UI_DIR" && npm run build)

echo "==> Building DMG (this can take several minutes on first run)"
cd "$GUI_DIR"
if [ -n "${TAURI_TARGET:-}" ]; then
  cargo tauri build --bundles dmg --target "$TAURI_TARGET"
  DMG_DIR="$ROOT/target/$TAURI_TARGET/release/bundle/dmg"
else
  cargo tauri build --bundles dmg
  DMG_DIR="$ROOT/target/release/bundle/dmg"
fi

DMG="$(ls -1t "$DMG_DIR"/*.dmg 2>/dev/null | head -n 1 || true)"
if [ -z "$DMG" ]; then
  echo "No .dmg produced at $DMG_DIR — check tauri output above." >&2
  exit 1
fi

echo
echo "Built: $DMG"
echo "Size:  $(du -h "$DMG" | cut -f1)"
