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

# Stop any KubeFront instance/dev server left over from a previous run, so we don't
# hit "Port 1420 already in use" or a locked kube-front.exe. Scoped to THIS project:
# the compiled app, build/dev helpers whose command line references this repo (vite,
# tauri-cli, rustc), and whatever currently holds the Vite ports.
function Stop-KubefrontProcesses {
    param([Parameter(Mandatory)][string]$ProjectRoot)

    $rootEsc = [regex]::Escape($ProjectRoot)
    $stopped = [System.Collections.Generic.List[string]]::new()

    function Stop-One([int]$ProcId, [string]$Label) {
        $p = Get-Process -Id $ProcId -ErrorAction SilentlyContinue
        if ($p) {
            Stop-Process -Id $ProcId -Force -ErrorAction SilentlyContinue
            [void]$stopped.Add("$Label (pid $ProcId)")
        }
    }

    # 1) The compiled desktop app.
    Get-Process kube-front -ErrorAction SilentlyContinue | ForEach-Object { Stop-One $_.Id "kube-front.exe" }

    # 2) Dev/build helpers for THIS project (vite, tauri-cli, cargo, rustc), matched by
    #    a command line referencing the repo root or the `tauri dev` task.
    Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            $_.Name -in @('node.exe', 'cargo.exe', 'rustc.exe', 'cargo-tauri.exe') -and
            $_.CommandLine -and ($_.CommandLine -match $rootEsc -or $_.CommandLine -match 'tauri\s+dev')
        } |
        ForEach-Object { Stop-One $_.ProcessId $_.Name }

    # 3) Anything still listening on the Vite dev/HMR ports.
    foreach ($port in 1420, 1421) {
        Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue |
            Select-Object -ExpandProperty OwningProcess -Unique |
            ForEach-Object { Stop-One $_ "listener on port $port" }
    }

    if ($stopped.Count -gt 0) {
        Write-Host "[cleanup] Stopped existing KubeFront processes:"
        $stopped | Sort-Object -Unique | ForEach-Object { Write-Host "  - $_" }
        Start-Sleep -Milliseconds 500
    }
    else {
        Write-Host "[cleanup] No existing KubeFront processes to stop."
    }
}

Stop-KubefrontProcesses -ProjectRoot $ProjectRoot

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
