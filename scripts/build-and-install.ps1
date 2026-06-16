# One-shot installer for WebP Converter (Windows).
# Fetches the FFmpeg sidecar, builds the app + NSIS installer from this repo,
# installs it silently, and creates/refreshes a Desktop shortcut.
#
# Usage (from the repo root):  npm run install-app
#                         or:  powershell -ExecutionPolicy Bypass -File scripts/build-and-install.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "[1/5] Fetching FFmpeg sidecar..." -ForegroundColor Cyan
& "$PSScriptRoot\fetch-ffmpeg.ps1"

Write-Host "[2/5] Installing npm dependencies..." -ForegroundColor Cyan
npm install

Write-Host "[3/5] Closing any running WebP Converter..." -ForegroundColor Cyan
Stop-Process -Name "webp-converter", "WebP Converter" -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

Write-Host "[4/5] Building app + installer (this takes a few minutes)..." -ForegroundColor Cyan
npx tauri build

Write-Host "[5/5] Running the installer..." -ForegroundColor Cyan
$setup = Get-ChildItem "$root\src-tauri\target\release\bundle\nsis" -Filter "*-setup.exe" -ErrorAction Stop |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $setup) { throw "Installer not found in src-tauri\target\release\bundle\nsis" }
Start-Process -FilePath $setup.FullName -ArgumentList "/S" -Wait
Write-Host "Installed: $($setup.Name)" -ForegroundColor Green

# Create / refresh a Desktop shortcut pointing at the freshly installed app.
$startMenuLnk = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\WebP Converter.lnk"
if (Test-Path $startMenuLnk) {
    $ws = New-Object -ComObject WScript.Shell
    $target = $ws.CreateShortcut($startMenuLnk).TargetPath
    $desktop = [Environment]::GetFolderPath("Desktop")
    $deskLnk = $ws.CreateShortcut((Join-Path $desktop "WebP Converter.lnk"))
    $deskLnk.TargetPath = $target
    $deskLnk.Save()
    Write-Host "Desktop shortcut -> $target" -ForegroundColor Green
} else {
    Write-Host "Start Menu shortcut not found; skipped Desktop shortcut. Launch from the Start Menu." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Done. Launch 'WebP Converter' from your Desktop." -ForegroundColor Green
