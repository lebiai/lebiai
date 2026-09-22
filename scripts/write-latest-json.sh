#!/usr/bin/env bash
# Assemble Tauri updater latest.json from built artifacts.
#
# Usage: scripts/write-latest-json.sh <version> <artifacts-dir> <out-json> [darwin-arches]
#   version:        1.2.0  or  v1.2.0
#   artifacts-dir:  directory containing .app.tar.gz[.sig] and *-setup.exe[.sig]
#   out-json:       path to write latest.json
#   darwin-arches:  comma list, default "aarch64,x86_64"
#
# The Tauri updater keys on `${os}-${runtime arch}` (its own source: target() =
# `darwin-<arch>`, then get_urls() tries `darwin-<arch>-<installer>` and
# `darwin-<arch>`). There is no `darwin-universal` key. We ship one universal
# .app.tar.gz, so both `darwin-aarch64` and `darwin-x86_64` must point at it -
# otherwise Intel Macs see no update at all. Pass a narrower list only if the
# macOS artifact stops being universal.
#
# URLs point at GitHub Releases for lebiai/lebiai. The publish job must upload
# those same files to the same tag.

set -euo pipefail

VERSION_RAW="${1:?version}"
DIR="${2:?artifacts-dir}"
OUT="${3:?out-json}"
DARWIN_ARCHES="${4:-aarch64,x86_64}"

VERSION="${VERSION_RAW#v}"
TAG="v${VERSION}"
BASE="https://github.com/lebiai/lebiai/releases/download/${TAG}"

if [ ! -d "$DIR" ]; then
  echo "error: artifacts dir not found: $DIR" >&2
  exit 1
fi

APP_TGZ="$(find "$DIR" -type f -name '*.app.tar.gz' ! -name '*.sig' | head -n 1 || true)"
APP_SIG="$(find "$DIR" -type f -name '*.app.tar.gz.sig' | head -n 1 || true)"
WIN_EXE="$(find "$DIR" -type f -name '*-setup.exe' ! -name '*.sig' | head -n 1 || true)"
WIN_SIG="$(find "$DIR" -type f -name '*-setup.exe.sig' | head -n 1 || true)"

if [ -z "$APP_TGZ" ] || [ -z "$APP_SIG" ]; then
  echo "error: missing macOS updater artifacts (.app.tar.gz + .sig) in $DIR" >&2
  find "$DIR" -type f -print >&2 || true
  exit 1
fi
if [ -z "$WIN_EXE" ] || [ -z "$WIN_SIG" ]; then
  echo "error: missing Windows updater artifacts (*-setup.exe + .sig) in $DIR" >&2
  find "$DIR" -type f -print >&2 || true
  exit 1
fi

PUB_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
DARWIN_NAME="$(basename "$APP_TGZ")"
WIN_NAME="$(basename "$WIN_EXE")"

python3 - "$OUT" "$VERSION" "$PUB_DATE" "$BASE/$DARWIN_NAME" "$APP_SIG" "$BASE/$WIN_NAME" "$WIN_SIG" "$DARWIN_ARCHES" <<'PY'
import json
import pathlib
import sys

out, version, pub_date, darwin_url, darwin_sig, win_url, win_sig, darwin_arches = sys.argv[1:]

def sig(path: str) -> str:
    text = pathlib.Path(path).read_text(encoding="utf-8").strip()
    if not text:
        raise SystemExit(f"empty signature: {path}")
    return text

arches = [a.strip() for a in darwin_arches.split(",") if a.strip()]
if not arches:
    raise SystemExit(f"no darwin arches given: {darwin_arches!r}")

# Every arch key gets the same universal tarball + signature. Duplicating the
# payload is what the updater expects; it never looks for "darwin-universal".
platforms = {
    f"darwin-{arch}": {"signature": sig(darwin_sig), "url": darwin_url}
    for arch in arches
}
platforms["windows-x86_64"] = {"signature": sig(win_sig), "url": win_url}

payload = {
    "version": version,
    "notes": "版本更新",
    "pub_date": pub_date,
    "platforms": platforms,
}
path = pathlib.Path(out)
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(f"wrote {path} with darwin arches: {', '.join(arches)}")
PY
