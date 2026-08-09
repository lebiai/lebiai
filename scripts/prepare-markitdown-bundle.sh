#!/usr/bin/env bash
# Build a relocatable MarkItDown tree for Tauri app Resources.
#
# Output (gitignored venv — regenerate on each machine/CI):
#   crates/hermes-gui/resources/markitdown-sidecar/
#     markitdown          # wrapper (relative paths)
#     venv/               # uv/python venv with markitdown[docx,pdf,xlsx]
#     VERSION             # pin record
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

if [ -x "$OUT/markitdown" ] && [ -d "$OUT/venv" ] && [ "$FORCE" -eq 0 ]; then
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

# Relocatable wrapper — never rely on absolute shebang of venv/bin/markitdown.
cat > "$OUT/markitdown" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
PY="$ROOT/venv/bin/python"
if [ ! -x "$PY" ]; then
  echo "markitdown-sidecar: missing $PY" >&2
  exit 127
fi
exec "$PY" -m markitdown "$@"
EOF
chmod +x "$OUT/markitdown"

printf '%s\n' "$MARKITDOWN_VERSION" > "$OUT/VERSION"
printf '%s\n' "markitdown[docx,pdf,xlsx]==${MARKITDOWN_VERSION}" > "$OUT/REQUIREMENTS.txt"

echo "==> Verifying"
"$OUT/markitdown" --version
echo "==> Done. Size: $(du -sh "$OUT" | cut -f1)"
echo "    Tauri will pack this under app Resources as markitdown-sidecar/"
