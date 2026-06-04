# KubeFront Release Builder + Self-Signed Code Signing (Windows, native)
#
# Builds the frontend + Rust app + Windows bundles with Tauri, then (optionally)
# signs the standalone executable with a self-signed certificate.
#
# Usage:
#   .\scripts\build-release.ps1
#   .\scripts\build-release.ps1 -Clean
#   .\scripts\build-release.ps1 -Version 0.2.3        # stamp all manifests before building
#   .\scripts\build-release.ps1 -Sign -CertName "My Company Code Signing"
#   .\scripts\build-release.ps1 -SkipToolchain        # don't auto-provision NASM/Perl
#
# Requirements:
#   - Node.js 18+ and npm
#   - Rust toolchain (MSVC) + the VS "Desktop development with C++" workload
#   - WebView2 runtime (preinstalled on Windows 10/11)
#   - NASM + a native Windows Perl for the vendored OpenSSL build. You don't have
#     to install these yourself: if they're not on PATH the script downloads
#     portable copies once into %LOCALAPPDATA%\kf-buildtools and uses them. (To
#     manage them yourself instead, `choco install nasm strawberryperl` and pass
#     -SkipToolchain. Git for Windows' MSYS perl does NOT work for this build.)
#   - For -Sign: Windows SDK (signtool.exe)
#
# Tauri also produces an installer under src-tauri\target\release\bundle\
# (NSIS .exe / MSI). The -Sign step here signs the standalone binary.

[CmdletBinding()]
param(
    [switch]$Clean,
    [switch]$Sign,
    [string]$Version = "",
    [string]$CertName = "KubeFront",
    [string]$OutputName = "KubeFront.exe",
    [switch]$SkipToolchain
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path $PSScriptRoot -Parent
$ReleaseDir = Join-Path $ProjectRoot "release"
$ExePath = Join-Path $ProjectRoot "src-tauri\target\release\kube-front.exe"
$SignedExe = Join-Path $ReleaseDir $OutputName

# --- Vendored-OpenSSL build toolchain -------------------------------------------------
# kube uses openssl-tls, and we vendor OpenSSL (compiled from source) so installers are
# self-contained. That source build needs NASM and a NATIVE Windows perl at build time.
# The MSYS perl bundled with Git for Windows reports $^O = "msys" and does NOT work.
# This makes the script just-work: if a usable perl/nasm isn't on PATH, fetch portable
# copies once into %LOCALAPPDATA%\kf-buildtools and prepend them. Skip with -SkipToolchain.

function Test-NativePerl {
    $perl = Get-Command perl -ErrorAction SilentlyContinue
    if (-not $perl) { return $false }
    $os = (& $perl.Source -e 'print $^O' 2>$null)
    return ($os -eq "MSWin32")  # Strawberry/ActiveState = MSWin32; Git's MSYS perl = msys
}

function Get-PortableZip([string]$Url, [string]$Zip, [string]$DestDir, [string]$Marker) {
    if (Test-Path $Marker) { return }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    if (-not (Test-Path $Zip)) {
        Write-Host "  downloading $Url" -ForegroundColor DarkGray
        Invoke-WebRequest -Uri $Url -OutFile $Zip -UseBasicParsing
    }
    Write-Host "  extracting to $DestDir" -ForegroundColor DarkGray
    Expand-Archive -Path $Zip -DestinationPath $DestDir -Force
}

function Initialize-OpenSslToolchain {
    $cache = Join-Path $env:LOCALAPPDATA "kf-buildtools"
    New-Item -ItemType Directory -Force -Path $cache | Out-Null

    if (-not (Test-NativePerl)) {
        Write-Host "[toolchain] No native Windows perl found; using portable Strawberry Perl..." -ForegroundColor Yellow
        $perlBin = Join-Path $cache "strawberry\perl\bin"
        Get-PortableZip `
            -Url "https://github.com/StrawberryPerl/Perl-Dist-Strawberry/releases/download/SP_53822_64bit/strawberry-perl-5.38.2.2-64bit-portable.zip" `
            -Zip (Join-Path $cache "strawberry.zip") `
            -DestDir (Join-Path $cache "strawberry") `
            -Marker (Join-Path $perlBin "perl.exe")
        # Prepend ONLY perl\bin — not strawberry's c\bin, whose gcc/ld would shadow MSVC.
        $env:PATH = "$perlBin;$env:PATH"
    }

    if (-not (Get-Command nasm -ErrorAction SilentlyContinue)) {
        Write-Host "[toolchain] NASM not found; using portable NASM..." -ForegroundColor Yellow
        Get-PortableZip `
            -Url "https://www.nasm.us/pub/nasm/releasebuilds/2.16.03/win64/nasm-2.16.03-win64.zip" `
            -Zip (Join-Path $cache "nasm.zip") `
            -DestDir $cache `
            -Marker (Join-Path $cache "nasm-2.16.03\nasm.exe")
        $nasmDir = (Get-ChildItem -Path $cache -Filter "nasm-*" -Directory | Select-Object -First 1).FullName
        $env:PATH = "$nasmDir;$env:PATH"
    }

    if (-not (Test-NativePerl)) { throw "Could not provision a native Windows perl for the OpenSSL build." }
    if (-not (Get-Command nasm -ErrorAction SilentlyContinue)) { throw "Could not provision NASM for the OpenSSL build." }
    Write-Host "[toolchain] perl -> $((Get-Command perl).Source); nasm -> $((Get-Command nasm).Source)" -ForegroundColor Green
}

Write-Host "=== KubeFront Release Build (Windows) ===" -ForegroundColor Cyan
Set-Location $ProjectRoot

if (-not $SkipToolchain) { Initialize-OpenSslToolchain }

if ($Version) {
    Write-Host "`n[version] Stamping $Version into all manifests..." -ForegroundColor Yellow
    node scripts/set-version.mjs $Version
    if ($LASTEXITCODE -ne 0) { throw "Failed to set version to $Version" }
}

if ($Clean) {
    Write-Host "`n[clean] Removing previous build artifacts..." -ForegroundColor Yellow
    cargo clean --manifest-path src-tauri\Cargo.toml
    if (Test-Path (Join-Path $ProjectRoot "dist")) { Remove-Item -Recurse -Force (Join-Path $ProjectRoot "dist") }
}

Write-Host "`n[1/2] Installing frontend dependencies..." -ForegroundColor Yellow
npm ci

Write-Host "`n[2/2] Building frontend + Rust app + bundles..." -ForegroundColor Yellow
npm run tauri build

if (-not (Test-Path $ExePath)) {
    throw "Build succeeded but executable not found at $ExePath"
}

Write-Host "`nBundles are in: src-tauri\target\release\bundle\" -ForegroundColor Green
Write-Host "Standalone exe: $ExePath" -ForegroundColor Green

if (-not $Sign) {
    Write-Host "`nSkipping code signing (pass -Sign to enable)." -ForegroundColor DarkGray
    return
}

if (-not (Test-Path $ReleaseDir)) { New-Item -ItemType Directory -Path $ReleaseDir | Out-Null }

Write-Host "`n[sign 1/2] Ensuring self-signed code signing certificate exists..." -ForegroundColor Yellow
$Cert = Get-ChildItem -Path "Cert:\CurrentUser\My" |
        Where-Object { $_.Subject -like "*$CertName*" -and $_.EnhancedKeyUsageList -match "Code Signing" } |
        Select-Object -First 1

if (-not $Cert) {
    Write-Host "Creating new self-signed certificate '$CertName'..." -ForegroundColor DarkYellow
    $Cert = New-SelfSignedCertificate `
        -Type Custom -Subject "CN=$CertName" `
        -KeyAlgorithm RSA -KeyLength 2048 -HashAlgorithm SHA256 `
        -KeyUsage DigitalSignature -KeyExportPolicy Exportable `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -FriendlyName $CertName -NotAfter (Get-Date).AddYears(5)
    Write-Host "Certificate created: $($Cert.Thumbprint)" -ForegroundColor Green
} else {
    Write-Host "Using existing certificate: $($Cert.Thumbprint)" -ForegroundColor Green
}

Write-Host "`n[sign 2/2] Signing executable..." -ForegroundColor Yellow
$SignToolPaths = @(
    "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe",
    "C:\Program Files (x86)\Windows Kits\10\bin\*\x86\signtool.exe"
)
$SignTool = $null
foreach ($path in $SignToolPaths) {
    $found = Get-Item -Path $path -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) { $SignTool = $found.FullName; break }
}
if (-not $SignTool) {
    Write-Warning "signtool.exe not found. Install the Windows SDK or add signtool to PATH."
    Write-Host "Unsigned binary is available at: $ExePath" -ForegroundColor Yellow
    exit 1
}

& $SignTool sign /sha1 $Cert.Thumbprint /fd SHA256 `
    /tr http://timestamp.sectigo.com /td SHA256 /a $ExePath
if ($LASTEXITCODE -ne 0) { throw "Code signing failed with exit code $LASTEXITCODE" }

Copy-Item -Path $ExePath -Destination $SignedExe -Force
Write-Host "`nSigned executable: $SignedExe" -ForegroundColor Cyan
Write-Host "Note: self-signed — users will see a SmartScreen warning. Use a trusted CA for distribution." -ForegroundColor Yellow
