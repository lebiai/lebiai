#!/usr/bin/env bash
# Build a macOS .dmg of the lebi-AI desktop GUI.
#
# Usage:    scripts/build-dmg.sh
# Output:   target/release/bundle/dmg/lebi-AI_<version>_<arch>.dmg
# Requires: macOS, Node + npm, `cargo install tauri-cli --version "^2.0" --locked`
#
# Universal (Intel + Apple Silicon) build:
#   rustup target add x86_64-apple-darwin
#   TAURI_TARGET=universal-apple-darwin scripts/build-dmg.sh
#
# Notes:
# - Never bundle scripts/license-issuer.html or scripts/issue-license.py
#   (developer-only signing tools; they are not under frontendDist or
#   bundle.resources and must stay out of the DMG).
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

echo "==> Preparing markitdown-sidecar for app Resources"
"$ROOT/scripts/prepare-markitdown-bundle.sh"
if [ ! -x "$GUI_DIR/resources/markitdown-sidecar/markitdown" ]; then
  echo "error: markitdown-sidecar missing after prepare" >&2
  exit 1
fi

# Updater artifacts need the signing key (not Apple codesign).
# shellcheck disable=SC1091
source "$ROOT/scripts/load-updater-key.sh"

echo "==> Building DMG + updater archive (this can take several minutes on first run)"
cd "$GUI_DIR"
if [ -n "${TAURI_TARGET:-}" ]; then
  cargo tauri build --bundles app,dmg --target "$TAURI_TARGET"
  BUNDLE_ROOT="$ROOT/target/$TAURI_TARGET/release/bundle"
else
  cargo tauri build --bundles app,dmg
  BUNDLE_ROOT="$ROOT/target/release/bundle"
fi

DMG_DIR="$BUNDLE_ROOT/dmg"
MACOS_DIR="$BUNDLE_ROOT/macos"
DMG="$(ls -1t "$DMG_DIR"/*.dmg 2>/dev/null | head -n 1 || true)"
if [ -z "$DMG" ]; then
  echo "No .dmg produced at $DMG_DIR — check tauri output above." >&2
  exit 1
fi

TGZ="$(ls -1t "$MACOS_DIR"/*.app.tar.gz 2>/dev/null | head -n 1 || true)"
SIG="$(ls -1t "$MACOS_DIR"/*.app.tar.gz.sig 2>/dev/null | head -n 1 || true)"
if [ -z "$TGZ" ] || [ -z "$SIG" ]; then
  echo "error: updater artifacts missing under $MACOS_DIR (need .app.tar.gz and .sig)." >&2
  echo "Confirm TAURI_SIGNING_PRIVATE_KEY is set and createUpdaterArtifacts is true." >&2
  exit 1
fi

echo
echo "Built: $DMG"
echo "Size:  $(du -h "$DMG" | cut -f1)"
echo "Updater: $TGZ"
