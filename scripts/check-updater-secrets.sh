#!/usr/bin/env bash
# Fail fast if updater signing secrets are empty. Prints lengths only — never values.
set -euo pipefail

key_len=${#TAURI_SIGNING_PRIVATE_KEY}
pass_len=${#TAURI_SIGNING_PRIVATE_KEY_PASSWORD}

echo "updater secret key_len=${key_len} pass_len=${pass_len}"

if [ "${key_len}" -eq 0 ]; then
  echo "error: TAURI_SIGNING_PRIVATE_KEY is empty on this runner." >&2
  echo "It must be a Repository Actions secret (not a Variable, not an Environment secret)." >&2
  echo "Paste the full contents of ~/.tauri/lebi-ai.key" >&2
  exit 1
fi

if [ "${pass_len}" -eq 0 ]; then
  echo "error: TAURI_SIGNING_PRIVATE_KEY_PASSWORD is empty on this runner." >&2
  echo "Paste the full contents of ~/.tauri/lebi-ai.key.pass" >&2
  exit 1
fi
