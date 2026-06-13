#!/usr/bin/env pwsh
#
# KubeFront — complete release build (Windows)
#
# Builds the whole stack:
#   - frontend (React/Vite -> dist\, embedded by the desktop app)
#   - kubefront-core   (shared library crate)
#   - kube-front       (desktop / Tauri app  -> kube-front.exe)
#   - kubefront-backend (headless REST server -> kubefront-backend.exe)
#
# Usage:
#   .\scripts\build.ps1                 # full build: frontend + all crates (release)
#   .\scripts\build.ps1 -Backend        # backend crate only
#   .\scripts\build.ps1 -Desktop        # desktop crate only (still builds frontend)
#   .\scripts\build.ps1 -Bundle         # produce installers via `tauri build` (NSIS/MSI)
#   .\scripts\build.ps1 -SkipFrontend   # skip the npm frontend build
#   .\scripts\build.ps1 -NoOpenSslWorkaround   # CI: build vendored OpenSSL normally
#
# OpenSSL note: locally this reuses the prebuilt vendored OpenSSL under
# src-tauri\target (no Perl/NASM needed). See scripts\_cargo-env.ps1.

[CmdletBinding()]
param(
    [switch]$Backend,
    [switch]$Desktop,
    [switch]$Bundle,
    [switch]$SkipFrontend,
    [switch]$NoOpenSslWorkaround
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

. (Join-Path $PSScriptRoot "_cargo-env.ps1")
Set-KubefrontCargoEnv -ProjectRoot $ProjectRoot -NoWorkaround:$NoOpenSslWorkaround

function Invoke-Step {
    param([string]$Name, [scriptblock]$Action)
    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Action
    if ($LASTEXITCODE -ne 0) { throw "$Name failed (exit $LASTEXITCODE)" }
}

# 1. Frontend deps (first run only).
if (-not (Test-Path "node_modules")) {
    Invoke-Step "npm install" { npm install }
}

# 2. Frontend build (dist\). The desktop app embeds this; `-Bundle` runs it via
#    tauri's beforeBuildCommand, so skip the standalone build in that case.
if (-not $SkipFrontend -and -not $Bundle) {
    Invoke-Step "Frontend build (tsc + vite)" { npm run build }
}

# 3. Rust build.
$buildAll = -not ($Backend -or $Desktop)

if ($Bundle) {
    # Full desktop bundle (installers). Runs npm run build itself, then cargo.
    Invoke-Step "Tauri bundle (frontend + desktop + installers)" { npm run tauri build }
}
elseif ($buildAll) {
    Invoke-Step "cargo build --release --workspace" {
        cargo build --release --workspace
    }
}
else {
    if ($Desktop) {
        Invoke-Step "cargo build --release -p kube-front" {
            cargo build --release -p kube-front
        }
    }
    if ($Backend) {
        Invoke-Step "cargo build --release -p kubefront-backend" {
            cargo build --release -p kubefront-backend
        }
    }
}

# 4. Report artifacts.
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $ProjectRoot "target" }
$ReleaseDir = Join-Path $TargetDir "release"

Write-Host ""
Write-Host "Build complete." -ForegroundColor Green
foreach ($exe in @("kube-front.exe", "kubefront-backend.exe")) {
    $path = Join-Path $ReleaseDir $exe
    if (Test-Path $path) {
        $size = "{0:N1} MB" -f ((Get-Item $path).Length / 1MB)
        Write-Host ("  {0,-26} {1}  ({2})" -f $exe, $path, $size)
    }
}
if ($Bundle) {
    Write-Host "  installers                 $(Join-Path $ReleaseDir 'bundle')"
}
