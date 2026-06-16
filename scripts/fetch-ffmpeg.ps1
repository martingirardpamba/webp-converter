# Downloads a static GPL FFmpeg build (with libx264 + libvpx-vp9 + aac/opus)
# and places it as the Windows Tauri sidecar binary.
$ErrorActionPreference = "Stop"

$version = "7.1"
$url = "https://github.com/GyanD/codexffmpeg/releases/download/$version/ffmpeg-$version-essentials_build.zip"
$root = Split-Path -Parent $PSScriptRoot
$binDir = Join-Path $root "src-tauri/binaries"
$target = Join-Path $binDir "ffmpeg-x86_64-pc-windows-msvc.exe"

if (Test-Path $target) {
    Write-Host "FFmpeg already present: $target"
    exit 0
}

New-Item -ItemType Directory -Force -Path $binDir | Out-Null
$tmpZip = Join-Path $env:TEMP "ffmpeg-$version.zip"
$tmpDir = Join-Path $env:TEMP "ffmpeg-$version-extract"

Write-Host "Downloading FFmpeg $version (GPL essentials build)..."
Invoke-WebRequest -Uri $url -OutFile $tmpZip

Write-Host "Extracting..."
if (Test-Path $tmpDir) { Remove-Item -Recurse -Force $tmpDir }
Expand-Archive -Path $tmpZip -DestinationPath $tmpDir

$exe = Get-ChildItem -Path $tmpDir -Recurse -Filter "ffmpeg.exe" | Select-Object -First 1
if (-not $exe) { throw "ffmpeg.exe not found in archive" }
Copy-Item $exe.FullName $target -Force

Write-Host "FFmpeg ready: $target"
& $target -version | Select-Object -First 1
