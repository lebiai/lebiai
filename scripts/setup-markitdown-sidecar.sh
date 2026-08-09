#!/usr/bin/env bash
# Install MarkItDown into lebi-AI data dir (default converter path).
# Compliant path: ~/.lebi-ai/bin/markitdown — not system PATH luck.
#
# Usage (repo root):
#   scripts/setup-markitdown-sidecar.sh
# Override data root:
#   LEBI_DATA_DIR=/path scripts/setup-markitdown-sidecar.sh

set -euo pipefail

DATA_ROOT="${LEBI_DATA_DIR:-${HERMES_DATA_DIR:-${HOME}/.lebi-ai}}"
BIN_DIR="${DATA_ROOT}/bin"
VENV_DIR="${DATA_ROOT}/.markitdown-venv"
MARKITDOWN_VERSION="${HERMES_MARKITDOWN_VERSION:-0.1.6}"

echo "==> lebi-AI markitdown sidecar"
echo "    data root: ${DATA_ROOT}"
echo "    version:   markitdown==${MARKITDOWN_VERSION} [docx,pdf,xlsx]"

mkdir -p "${BIN_DIR}"

if command -v uv >/dev/null 2>&1; then
  echo "==> Creating venv with uv (re-run safe)"
  uv venv --clear "${VENV_DIR}"
  # shellcheck disable=SC1091
  source "${VENV_DIR}/bin/activate"
  uv pip install "markitdown[docx,pdf,xlsx]==${MARKITDOWN_VERSION}"
elif command -v python3 >/dev/null 2>&1; then
  echo "==> Creating venv with python3 -m venv (re-run safe)"
  python3 -m venv --clear "${VENV_DIR}"
  # shellcheck disable=SC1091
  source "${VENV_DIR}/bin/activate"
  pip install -U pip
  pip install "markitdown[docx,pdf,xlsx]==${MARKITDOWN_VERSION}"
else
  echo "error: need uv or python3 to install the sidecar" >&2
  exit 1
fi

SRC="$(command -v markitdown)"
if [[ -z "${SRC}" || ! -x "${SRC}" ]]; then
  echo "error: markitdown not found after install" >&2
  exit 1
fi

# Wrapper so upgrades only need re-run of this script (activate venv).
WRAPPER="${BIN_DIR}/markitdown"
cat > "${WRAPPER}" <<EOF
#!/usr/bin/env bash
set -euo pipefail
# shellcheck disable=SC1091
source "${VENV_DIR}/bin/activate"
exec markitdown "\$@"
EOF
chmod +x "${WRAPPER}"

echo "==> Installed: ${WRAPPER}"
"${WRAPPER}" --version || true
echo "==> Done. lebi-AI resolves converter at: ${BIN_DIR}/markitdown"
echo "    (override with HERMES_MARKITDOWN for dev)"
