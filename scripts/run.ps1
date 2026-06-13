#!/usr/bin/env pwsh
#
# KubeFront — build & run (Windows)
#
# Default: dev mode (Vite HMR + Rust app, fast iteration).
#   .\scripts\run.ps1
#
# Release: build optimized bundles, then launch the compiled binary.
#   .\scripts\run.ps1 -Release
#
# Pass RUST_LOG for verbose backend logs, e.g.:
#   $env:RUST_LOG = "debug,kube=info"; .\scripts\run.ps1

[CmdletBinding()]
param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

# Reuse the prebuilt vendored OpenSSL locally (no Perl/NASM needed).
. (Join-Path $PSScriptRoot "_cargo-env.ps1")
Set-KubefrontCargoEnv -ProjectRoot $ProjectRoot

# First-run: install frontend deps if missing.
if (-not (Test-Path "node_modules")) {
    Write-Host "[setup] Installing frontend dependencies..."
    npm install
    if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
}

if (-not $Release) {
    Write-Host "[run] Starting KubeFront in dev mode (Vite HMR + Rust app)..."
    npm run tauri dev
    exit $LASTEXITCODE
}

Write-Host "[run] Building optimized release (frontend + Rust app + bundles)..."
npm run tauri build
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

# Locate and launch the compiled binary. The workspace puts the target dir at the
# repo root by default, but the OpenSSL workaround redirects it to src-tauri\target.
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $ProjectRoot "target" }
$Bin = Join-Path $TargetDir "release\kube-front.exe"

if (Test-Path $Bin) {
    Write-Host "[run] Launching $Bin ..."
    & $Bin
    exit $LASTEXITCODE
}

Write-Host "Build finished, but the binary was not found at $Bin."
Write-Host "Check the bundle output under target\release\bundle\."
exit 1
