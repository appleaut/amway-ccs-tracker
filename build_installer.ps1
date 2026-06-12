# Build the Amway CCS Tracker Windows installer: release exe -> Inno Setup -> dist.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

Write-Host "==> cargo build --release"
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
    foreach ($p in @(
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    )) { if (Test-Path $p) { $iscc = $p; break } }
}
if (-not $iscc) {
    throw "Inno Setup compiler (iscc) not found. Install it with: winget install JRSoftware.InnoSetup"
}

Write-Host "==> compiling installer with $iscc"
& $iscc "installer\amway_ccs_tracker.iss"
if ($LASTEXITCODE -ne 0) { throw "iscc failed" }

Write-Host "==> done: dist\AmwayCCSTracker-Setup.exe"
