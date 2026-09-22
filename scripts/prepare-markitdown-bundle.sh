#!/usr/bin/env bash
# Build a relocatable MarkItDown tree for Tauri app Resources.
#
# Output (gitignored — regenerate on each machine/CI):
#   crates/hermes-gui/resources/markitdown-sidecar/
#     markitdown          # wrapper (relative paths)
#     python/             # vendored interpreter (relocatable)
#     site-packages/      # markitdown[docx,pdf,xlsx] + deps
#     VERSION             # pin record
#
# Self-contained on purpose: a venv's `bin/python` is an absolute symlink into
# the builder's uv cache and `pyvenv.cfg` records that same home, so a packaged
# .app would look for /Users/<builder>/... on the customer's Mac and fail.
# We vendor the interpreter instead and drive it with PYTHONPATH.
#
# Usage (repo root):
#   scripts/prepare-markitdown-bundle.sh
#   scripts/prepare-markitdown-bundle.sh --force   # recreate even if present
#
# Consumed by:
#   - tauri.conf.json bundle.resources
#   - scripts/build-dmg.sh (always ensure before package)
#   - GUI ConverterPathConfig.bundled_binary

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/crates/hermes-gui/resources/markitdown-sidecar"
MARKITDOWN_VERSION="${HERMES_MARKITDOWN_VERSION:-0.1.6}"
FORCE=0

for arg in "$@"; do
  case "$arg" in
    --force|-f) FORCE=1 ;;
    -h|--help)
      sed -n '2,25p' "$0"
      exit 0
      ;;
  esac
done

if [ -x "$OUT/markitdown" ] && [ -d "$OUT/site-packages" ] && [ "$FORCE" -eq 0 ]; then
  if "$OUT/markitdown" --version >/dev/null 2>&1; then
    echo "==> markitdown-sidecar already present (use --force to rebuild)"
    echo "    $OUT"
    "$OUT/markitdown" --version 2>/dev/null | head -1 || true
    exit 0
  fi
fi

echo "==> Preparing markitdown-sidecar (markitdown==${MARKITDOWN_VERSION})"
echo "    → $OUT"
rm -rf "$OUT"
mkdir -p "$OUT"

if command -v uv >/dev/null 2>&1; then
  uv venv "$OUT/venv"
  # shellcheck disable=SC1091
  source "$OUT/venv/bin/activate"
  uv pip install "markitdown[docx,pdf,xlsx]==${MARKITDOWN_VERSION}"
elif command -v python3 >/dev/null 2>&1; then
  python3 -m venv "$OUT/venv"
  # shellcheck disable=SC1091
  source "$OUT/venv/bin/activate"
  pip install -U pip
  pip install "markitdown[docx,pdf,xlsx]==${MARKITDOWN_VERSION}"
else
  echo "error: need uv or python3 to build the bundle" >&2
  exit 1
fi

# ── Relocate: lift the packages out of the venv and vendor the interpreter ──
echo "==> Relocating (self-contained; no builder paths in the bundle)"
REAL_PY="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$OUT/venv/bin/python")"
if [ ! -x "$REAL_PY" ]; then
  echo "error: cannot resolve the venv interpreter (looked for $REAL_PY)" >&2
  exit 1
fi
BASE_PREFIX="$(dirname "$(dirname "$REAL_PY")")"
SITE_DIR="$(ls -d "$OUT"/venv/lib/python3.*/site-packages)"

# python-build-standalone is relocatable (its rpath is @executable_path/../lib),
# so a plain copy keeps working from inside the .app.
cp -R "$BASE_PREFIX" "$OUT/python"
mv "$SITE_DIR" "$OUT/site-packages"
# `venv/bin` held only builder-absolute symlinks, scripts with absolute
# shebangs, and the `magika` console script — a 27 MB per-arch CLI binary we
# never invoke. (markitdown *does* `import magika`, but that is the Python
# package in site-packages, which stays.) We run `python -m markitdown`, so
# none of `venv/bin` is needed; dropping it keeps dangling links and the
# builder's home directory out of the bundle.
rm -rf "$OUT/venv"

# Relocatable wrapper — never rely on an absolute shebang.
cat > "$OUT/markitdown" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
PY="$ROOT/python/bin/python3.12"
SITE="$ROOT/site-packages"
if [ ! -x "$PY" ] || [ ! -d "$SITE" ]; then
  echo "markitdown-sidecar: incomplete bundle (need $PY and $SITE)" >&2
  exit 127
fi
exec env PYTHONPATH="$SITE${PYTHONPATH:+:$PYTHONPATH}" "$PY" -m markitdown "$@"
EOF
chmod +x "$OUT/markitdown"

printf '%s\n' "$MARKITDOWN_VERSION" > "$OUT/VERSION"
printf '%s\n' "markitdown[docx,pdf,xlsx]==${MARKITDOWN_VERSION}" > "$OUT/REQUIREMENTS.txt"

echo "==> Verifying"
"$OUT/markitdown" --version
# Guard the whole point of the relocation: nothing may hard-code the builder's
# home directory in a load-bearing file (site-packages metadata may mention
# paths in RECORD files — those are inert).
STRAY="$(grep -Rl -E '/Users/|/home/' "$OUT" \
  --include='*.cfg' --include='*.json' \
  --exclude-dir='*.dist-info' --exclude-dir='*.egg-info' \
  2>/dev/null | head -n 5 || true)"
if [ -n "$STRAY" ]; then
  echo "warn: absolute build paths remain in:" >&2
  printf '  %s\n' $STRAY >&2
fi
echo "==> Done. Size: $(du -sh "$OUT" | cut -f1)"
echo "    Tauri will pack this under app Resources as markitdown-sidecar/"
