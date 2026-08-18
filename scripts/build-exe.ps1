# Build a Windows .exe installer (NSIS) of the lebi-AI desktop GUI.
#
# Usage:    .\scripts\build-exe.ps1
# Output:   target\release\bundle\nsis\lebi-AI_<version>_<arch>-setup.exe
# Requires: Windows 10/11, Node + npm, Rust toolchain, and
#           `cargo install tauri-cli --version "^2" --locked`
#
# Notes:
# - Never bundle scripts/license-issuer.html or scripts/issue-license.py
#   (developer-only signing tools; they are not under frontendDist or
#   bundle.resources and must stay out of the EXE/NSIS installer).
# - The resulting installer is unsigned. Windows SmartScreen will warn on
#   first run — see docs/install.md for the "More info → Run anyway" path.
# - The markitdown document-converter sidecar is bundled on both macOS and
#   Windows (tauri.macos.conf.json / tauri.windows.conf.json resources).
#   On Windows it ships a self-contained embeddable Python + wheels
#   (scripts/prepare-markitdown-bundle.ps1), so document import works
#   out of the box. HERMES_MARKITDOWN still overrides as a dev escape hatch.
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

# 3. markitdown sidecar (self-contained embeddable Python + wheels)
Write-Host "==> Preparing markitdown-sidecar"
& (Join-Path $Root "scripts\prepare-markitdown-bundle.ps1")
if ($LASTEXITCODE -ne 0) { Write-Error "prepare-markitdown-bundle.ps1 failed" }

# 4. Updater signing key (not Authenticode). CI injects the env vars.
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
  $keyFile = if ($env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
    $env:TAURI_SIGNING_PRIVATE_KEY_PATH
  } else {
    Join-Path $env:USERPROFILE ".tauri\lebi-ai.key"
  }
  if (Test-Path $keyFile) {
    $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -LiteralPath $keyFile -Raw
    $passFile = "$keyFile.pass"
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -and (Test-Path $passFile)) {
      $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content -LiteralPath $passFile -Raw).Trim()
    }
  } else {
    Write-Error "updater signing key missing. Set TAURI_SIGNING_PRIVATE_KEY or put the key at $keyFile (see docs/dev/updater-signing.md)."
  }
}

# 5. NSIS installer (bundles the sidecar via tauri.windows.conf.json resources)
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
$SetupSig = Get-Item -LiteralPath ($Setup.FullName + ".sig") -ErrorAction SilentlyContinue
if (-not $SetupSig) {
  Write-Error "updater signature missing next to $($Setup.FullName). Confirm TAURI_SIGNING_PRIVATE_KEY and createUpdaterArtifacts."
}

Write-Host ""
Write-Host "Built: $($Setup.FullName)"
Write-Host ("Size:  {0:N1} MB" -f ($Setup.Length / 1MB))
Write-Host "Updater sig: $($SetupSig.FullName)"
