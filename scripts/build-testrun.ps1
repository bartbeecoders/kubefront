#!/usr/bin/env pwsh
#
# KubeFront — build everything and stage it into testrun\ for local testing.
#
# Produces a correct, self-contained pair of executables you can run against your
# hosted k3s:
#   - kube-front.exe        (desktop app, with the frontend EMBEDDED)
#   - kubefront-backend.exe (headless REST server)
# and copies both into testrun\.
#
# Usage:
#   .\scripts\build-testrun.ps1            # full clean build of both exes
#   .\scripts\build-testrun.ps1 -NoBackend # desktop only
#   .\scripts\build-testrun.ps1 -KeepDist  # skip wiping dist\ first (faster, riskier)
#   .\scripts\build-testrun.ps1 -CertPath C:\NetData\CodeCertificates\CodeSign.pfx -CertPassword ''  # sign the staged .exe files
#
# Why this exists / why a plain `cargo build` can give "page not found":
#   The desktop frontend is embedded into kube-front.exe at COMPILE time (Tauri
#   reads frontendDist = ..\dist). A bare `cargo build -p kube-front` does not
#   reliably re-embed when only dist\ changed — cargo can reuse cached build-script
#   output, leaving stale/empty assets baked in -> the WebView shows "page not
#   found". `tauri build` always rebuilds the frontend and re-embeds, so we use it
#   here. We wipe dist\ first to guarantee a clean embed.

[CmdletBinding()]
param(
    [switch]$NoBackend,
    [switch]$KeepDist,
    [switch]$NoOpenSslWorkaround,
    [string]$CertPath,
    [string]$CertPassword
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

# Reuse the prebuilt vendored OpenSSL locally (no Perl/NASM needed).
. (Join-Path $PSScriptRoot "_cargo-env.ps1")
Set-KubefrontCargoEnv -ProjectRoot $ProjectRoot -NoWorkaround:$NoOpenSslWorkaround

function Invoke-Step {
    param([string]$Name, [scriptblock]$Action)
    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Action
    if ($LASTEXITCODE -ne 0) { throw "$Name failed (exit $LASTEXITCODE)" }
}

$TestrunDir = Join-Path $ProjectRoot "testrun"
$DistDir    = Join-Path $ProjectRoot "dist"

# 0. Make sure the desktop exe isn't running (a locked file would fail the copy).
$running = Get-Process -Name "kube-front", "kubefront-backend" -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "[stop] Closing running KubeFront processes so the exes can be replaced..." -ForegroundColor Yellow
    $running | Stop-Process -Force
    Start-Sleep -Milliseconds 500
}

# 1. Frontend deps (first run only).
if (-not (Test-Path "node_modules")) {
    Invoke-Step "npm install" { npm install }
}

# 2. Wipe dist\ so the embed can't pick up stale assets (the "page not found" fix).
if (-not $KeepDist -and (Test-Path $DistDir)) {
    Write-Host "[clean] Removing dist\ for a fresh frontend embed..."
    Remove-Item -Recurse -Force $DistDir
}

# 3. Desktop app: tauri build re-runs the frontend build (beforeBuildCommand) AND
#    re-embeds it. --no-bundle skips installer generation (NSIS/MSI) — we only want
#    the raw exe for testing.
Invoke-Step "tauri build (frontend + desktop, no installers)" {
    # Call the CLI directly (not `npm run tauri ... -- --no-bundle`) so the flag is
    # forwarded reliably — npm's `--` passthrough was silently dropping it.
    npx tauri build --no-bundle
}

# 4. Backend server.
if (-not $NoBackend) {
    Invoke-Step "cargo build --release -p kubefront-backend" {
        cargo build --release -p kubefront-backend
    }
}

# 5. Locate the build output. The OpenSSL workaround redirects the target dir to
#    src-tauri\target; otherwise it's the workspace root target\.
$TargetDir  = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $ProjectRoot "target" }
$ReleaseDir = Join-Path $TargetDir "release"

# 6. Sanity check: the frontend really did embed (dist must have index.html).
if (-not (Test-Path (Join-Path $DistDir "index.html"))) {
    throw "dist\index.html missing after build — the frontend did not build, the desktop exe would show 'page not found'."
}

# 7. Stage executables into testrun\.
New-Item -ItemType Directory -Force $TestrunDir | Out-Null

$exes = @("kube-front.exe")
if (-not $NoBackend) { $exes += "kubefront-backend.exe" }

foreach ($exe in $exes) {
    $src = Join-Path $ReleaseDir $exe
    if (-not (Test-Path $src)) { throw "expected build output not found: $src" }
    Copy-Item $src (Join-Path $TestrunDir $exe) -Force
}

# 7b. Code-sign the staged exes (optional).
if ($CertPath) {
    Invoke-Step "Code-signing" {
        $secPass = ConvertTo-SecureString $CertPassword -AsPlainText -Force
        $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($CertPath, $secPass)
        foreach ($exe in $exes) {
            $path = Join-Path $TestrunDir $exe
            if (Test-Path $path) {
                Write-Host "  Signing $exe ..." -ForegroundColor Yellow
                $sig = Set-AuthenticodeSignature -FilePath $path -Certificate $cert -TimestampServer "http://timestamp.digicert.com" -HashAlgorithm SHA256
                if ($sig.Status -eq "Valid") {
                    Write-Host "  $exe — signed OK" -ForegroundColor Green
                } elseif ($sig.Status -eq "UnknownError") {
                    # Chain not trusted locally — signature is still embedded.
                    Write-Host "  $exe — signed (chain not trusted locally, safe to distribute)" -ForegroundColor DarkYellow
                } else {
                    throw "Signing $exe failed: $($sig.Status) — $($sig.StatusMessage)"
                }
            }
        }
    }
}

# 8. Seed a backend.toml in testrun\ if the user doesn't have one yet.
$BackendToml = Join-Path $TestrunDir "backend.toml"
if (-not (Test-Path $BackendToml)) {
    $example = Join-Path $ProjectRoot "crates\kubefront-backend\backend.toml.example"
    if (Test-Path $example) {
        Copy-Item $example $BackendToml
        Write-Host "[seed] Created testrun\backend.toml from the example — edit it before running the backend." -ForegroundColor Yellow
    }
}

# 9. Report.
Write-Host ""
Write-Host "Staged into testrun\:" -ForegroundColor Green
foreach ($exe in $exes) {
    $path = Join-Path $TestrunDir $exe
    $item = Get-Item $path
    $size = "{0:N1} MB" -f ($item.Length / 1MB)
    Write-Host ("  {0,-22} {1}  ({2})" -f $exe, $item.LastWriteTime, $size)
}
Write-Host ""
Write-Host "Run the desktop app:   .\testrun\kube-front.exe" -ForegroundColor Green
if (-not $NoBackend) {
    Write-Host "Run the backend:       cd testrun; .\kubefront-backend.exe --config backend.toml" -ForegroundColor Green
}
