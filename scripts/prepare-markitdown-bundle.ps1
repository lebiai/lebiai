# Build a relocatable MarkItDown tree for Windows Tauri app Resources.
#
# Windows venvs depend on the build machine's Python base install, so they
# break when shipped to a user's machine. Instead this bundles a
# self-contained Python embeddable distribution + MarkItDown wheels in
# `crates/hermes-gui/resources/markitdown-sidecar/`:
#
#   markitdown.cmd      # wrapper (relative paths)
#   python/             # python embeddable + site-packages with markitdown
#   VERSION             # pin record
#
# Usage (repo root, PowerShell):
#   .\scripts\prepare-markitdown-bundle.ps1
#   .\scripts\prepare-markitdown-bundle.ps1 -Force
#
# Consumed by:
#   - crates/hermes-gui/tauri.windows.conf.json bundle.resources
#   - scripts/build-exe.ps1
#   - GUI ConverterPathConfig.bundled_binary (markitdown.cmd)

param([switch]$Force)

$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Out = Join-Path $Root "crates\hermes-gui\resources\markitdown-sidecar"
$Version = $env:HERMES_MARKITDOWN_VERSION
if (-not $Version) { $Version = "0.1.6" }

$PythonVersion = "3.12.7"
$EmbedUrl = "https://www.python.org/ftp/python/$PythonVersion/python-$PythonVersion-embed-amd64.zip"
$GetPipUrl = "https://bootstrap.pypa.io/get-pip.py"

# Already present and healthy?
$Wrapper = Join-Path $Out "markitdown.cmd"
if (-not $Force -and (Test-Path $Wrapper)) {
  & $Wrapper --version 2>$null
  if ($LASTEXITCODE -eq 0) { Write-Host "==> markitdown-sidecar already present (use -Force to rebuild)"; exit 0 }
}

Write-Host "==> Preparing markitdown-sidecar (markitdown==$Version, embed python $PythonVersion)"
Write-Host "    -> $Out"
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Path $Out | Out-Null
New-Item -ItemType Directory -Path (Join-Path $Out "python") | Out-Null

$tmp = Join-Path $env:TEMP "lebi-markitdown-embed-$PID"
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $zip = Join-Path $tmp "python-embed.zip"
  Write-Host "==> Downloading embeddable Python $PythonVersion"
  Invoke-WebRequest -Uri $EmbedUrl -OutFile $zip
  Write-Host "==> Extracting embeddable Python"
  Expand-Archive -Path $zip -DestinationPath (Join-Path $Out "python")

  # Enable site + our site-packages dir so pip can install into it.
  $pth = Join-Path $Out "python\python312._pth"
  if (Test-Path $pth) {
    $lines = Get-Content $pth
    $lines = $lines | ForEach-Object {
      if ($_ -match "^#?\s*import site") { "import site" } else { $_ }
    }
    $lines = @($lines | Where-Object { $_ -ne "Lib\site-packages" }) + @("Lib\site-packages")
    Set-Content -Path $pth -Value $lines
  } else {
    Set-Content -Path $pth -Value @("python312.zip", ".", "Lib\site-packages", "import site")
  }

  $EmbedPython = Join-Path $Out "python\python.exe"
  Write-Host "==> Bootstrapping pip"
  Invoke-WebRequest -Uri $GetPipUrl -OutFile (Join-Path $tmp "get-pip.py")
  & $EmbedPython (Join-Path $tmp "get-pip.py")
  if ($LASTEXITCODE -ne 0) { throw "get-pip failed" }

  Write-Host "==> Installing markitdown[$Version] (+docx, pdf, xlsx)"
  & $EmbedPython -m pip install --no-warn-script-location --target (Join-Path $Out "python\Lib\site-packages") "markitdown[docx,pdf,xlsx]==$Version"
  if ($LASTEXITCODE -ne 0) { throw "pip install markitdown failed" }
}
finally {
  if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
}

# Relocatable wrapper — resolves paths relative to its own directory.
@"
@echo off
setlocal
set "ROOT=%~dp0"
"%ROOT%python\python.exe" -m markitdown %*
exit /b %ERRORLEVEL%
"@ | Set-Content -Path $Wrapper -Encoding ASCII

Set-Content -Path (Join-Path $Out "VERSION") -Value $Version -Encoding ASCII
Set-Content -Path (Join-Path $Out "REQUIREMENTS.txt") -Value "markitdown[docx,pdf,xlsx]==$Version" -Encoding ASCII

Write-Host "==> Verifying"
& $Wrapper --version
if ($LASTEXITCODE -ne 0) { throw "wrapper verification failed" }

Write-Host "==> Done. Size: $([math]::Round((Get-ChildItem $Out -Recurse | Measure-Object Length -Sum).Sum / 1MB, 1)) MB"
Write-Host "    Tauri will pack this under app Resources as markitdown-sidecar/"
