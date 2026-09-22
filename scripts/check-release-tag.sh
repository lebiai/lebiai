#!/usr/bin/env bash
# Fail fast when a release tag does not match the shipped app version.
#
# Usage: scripts/check-release-tag.sh <tag>
#   <tag>  v1.4.0  or  1.4.0
#
# Why: the tag decides which GitHub Release receives the installers, while
# `tauri.conf.json` decides what the updater advertises to installed clients.
# If they drift, clients download a release whose `latest.json` version is
# lower than the release itself — the updater then either skips it silently or
# loops. One check here beats shipping a mis-tagged build.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONF="$ROOT/crates/hermes-gui/tauri.conf.json"
TAG_RAW="${1:?usage: scripts/check-release-tag.sh <tag>}"

if [ ! -f "$CONF" ]; then
  echo "error: $CONF not found" >&2
  exit 1
fi

TAG="${TAG_RAW#v}"
APP_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$CONF")"

if [ "$TAG" != "$APP_VERSION" ]; then
  cat >&2 <<MSG
error: release tag does not match the app version.

  tag:                $TAG_RAW   (parsed as $TAG)
  tauri.conf.json:    $APP_VERSION   ($CONF)

Fix one of them, then re-run:
  - bump "version" in $CONF, or
  - re-tag: git tag -f v$APP_VERSION && git push -f origin v$APP_VERSION
MSG
  exit 1
fi

echo "tag $TAG_RAW matches tauri.conf.json version $APP_VERSION"
