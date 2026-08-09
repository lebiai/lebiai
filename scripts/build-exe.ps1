# Build a Windows .exe installer (NSIS) of the lebi-AI desktop GUI.
#
# Usage:    .\scripts\build-exe.ps1
# Output:   target\release\bundle\nsis\lebi-AI_<version>_<arch>-setup.exe
# Requires: Windows 10/11, Node + npm, Rust toolchain, and
#           `cargo install tauri-cli --version "^2" --locked`
#
# Notes:
# - The resulting installer is unsigned. Windows SmartScreen will warn on
#   first run — see docs/install.md for the "More info → Run anyway" path.
# - The markitdown document-converter sidecar is bundled on macOS only
#   (see tauri.macos.conf.json). On Windows, document import falls back to
#   the data-dir binary (%USERPROFILE%\.lebi-ai\bin\markitdown.exe) or the
#   HERMES_MARKITDOWN env var.
# - First run takes 5–10 minutes (Tauri pulls in WebView2 bindings and the
#   release profile compiles the whole workspace).

$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
  Write-Error "The Windows EXE can only be built on Windows (Tauri cannot cross-compile installers). Use the release.yml CI workflow or a Windows machine."
  exit 1
}

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$UiDir = Join-Path $Root "crates\hermes-gui\ui"
$GuiDir = Join-Path $Root "crates\hermes-gui"

# 1. tauri-cli
if (-not (Get-Command cargo-tauri -ErrorAction SilentlyContinue)) {
  Write-Host "cargo tauri not installed. Install with:"
  Write-Host "  cargo install tauri-cli --version '^2' --locked"
  exit 1
}

# 2. Frontend (same path as build-dmg.sh / run-gui.sh)
if (-not (Test-Path (Join-Path $UiDir "node_modules"))) {
  Write-Host "==> npm install (one-time, in $UiDir)"
  Push-Location $UiDir
  npm install
  Pop-Location
}
Write-Host "==> Building frontend (vite)"
Push-Location $UiDir
npm run build
Pop-Location

# 3. NSIS installer (no markitdown sidecar on Windows — see header note)
Write-Host "==> Building NSIS installer (first run can take several minutes)"
Push-Location $GuiDir
cargo tauri build --bundles nsis
Pop-Location

$NsisDir = Join-Path $Root "target\release\bundle\nsis"
$Setup = Get-ChildItem -Path $NsisDir -Filter "*-setup.exe" -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $Setup) {
  Write-Error "No NSIS installer produced at $NsisDir — check tauri output above."
}

Write-Host ""
Write-Host "Built: $($Setup.FullName)"
Write-Host ("Size:  {0:N1} MB" -f ($Setup.Length / 1MB))
