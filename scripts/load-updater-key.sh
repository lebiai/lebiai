#!/usr/bin/env bash
# Export TAURI_SIGNING_PRIVATE_KEY(+PASSWORD) for local bundling.
# Sourced by build-dmg.sh. CI should already have the env vars from secrets.
#
# Does not print the key. Safe to source.

if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  return 0 2>/dev/null || exit 0
fi

KEY_FILE="${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.tauri/lebi-ai.key}"
if [ -f "$KEY_FILE" ]; then
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY_FILE")"
  export TAURI_SIGNING_PRIVATE_KEY
  if [ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ] && [ -f "${KEY_FILE}.pass" ]; then
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat "${KEY_FILE}.pass")"
    export TAURI_SIGNING_PRIVATE_KEY_PASSWORD
  fi
  return 0 2>/dev/null || exit 0
fi

echo "error: updater signing key missing." >&2
echo "Set TAURI_SIGNING_PRIVATE_KEY, or put the private key at $KEY_FILE" >&2
echo "See docs/dev/updater-signing.md" >&2
return 1 2>/dev/null || exit 1
