# KubeFront Release Builder + Self-Signed Code Signing (Windows)
#
# Usage:
#   .\scripts\build-release.ps1
#   .\scripts\build-release.ps1 -Clean
#   .\scripts\build-release.ps1 -CertName "My Company Code Signing"
#
# Requirements:
#   - Rust installed (cargo)
#   - Windows SDK (for signtool.exe) - usually installed with Visual Studio
#
# This script will:
#   1. Create a self-signed code signing certificate (if it doesn't exist)
#   2. Build the project in release mode
#   3. Sign the resulting .exe with the self-signed certificate
#   4. Output the signed binary to dist/KubeFront.exe

[CmdletBinding()]
param(
    [switch]$Clean,
    [string]$CertName = "KubeFront",
    [string]$OutputName = "KubeFront.exe"
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path $PSScriptRoot -Parent
$DistDir = Join-Path $ProjectRoot "dist"
$ExePath = Join-Path $ProjectRoot "target\release\kube-front.exe"
$SignedExe = Join-Path $DistDir $OutputName

Write-Host "=== KubeFront Release Build + Self-Signed Signing ===" -ForegroundColor Cyan
Write-Host "Project root: $ProjectRoot"

# 1. Clean if requested
if ($Clean) {
    Write-Host "`n[1/5] Cleaning previous builds..." -ForegroundColor Yellow
    cargo clean
}

# 2. Ensure dist directory exists
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Path $DistDir | Out-Null
}

# 3. Build release
Write-Host "`n[2/5] Building release binary..." -ForegroundColor Yellow
cargo build --release

if (-not (Test-Path $ExePath)) {
    throw "Build succeeded but executable not found at $ExePath"
}

# 4. Create or find self-signed certificate
Write-Host "`n[3/5] Ensuring self-signed code signing certificate exists..." -ForegroundColor Yellow

$Cert = Get-ChildItem -Path "Cert:\CurrentUser\My" | 
        Where-Object { $_.Subject -like "*$CertName*" -and $_.EnhancedKeyUsageList -match "Code Signing" } | 
        Select-Object -First 1

if (-not $Cert) {
    Write-Host "Creating new self-signed certificate '$CertName'..." -ForegroundColor DarkYellow
    
    $Cert = New-SelfSignedCertificate `
        -Type Custom `
        -Subject "CN=$CertName" `
        -KeyAlgorithm RSA `
        -KeyLength 2048 `
        -HashAlgorithm SHA256 `
        -KeyUsage DigitalSignature `
        -KeyExportPolicy Exportable `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -FriendlyName $CertName `
        -NotAfter (Get-Date).AddYears(5)
    
    Write-Host "Certificate created with thumbprint: $($Cert.Thumbprint)" -ForegroundColor Green
} else {
    Write-Host "Using existing certificate: $($Cert.Thumbprint)" -ForegroundColor Green
}

# 5. Sign the executable
Write-Host "`n[4/5] Signing executable..." -ForegroundColor Yellow

# Find signtool (try common locations)
$SignToolPaths = @(
    "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe",
    "C:\Program Files (x86)\Windows Kits\10\bin\*\x86\signtool.exe",
    "C:\Program Files\Microsoft SDKs\Windows\v10.0A\bin\NETFX 4.8 Tools\signtool.exe"
)

$SignTool = $null
foreach ($path in $SignToolPaths) {
    $found = Get-Item -Path $path -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) {
        $SignTool = $found.FullName
        break
    }
}

if (-not $SignTool) {
    Write-Warning "signtool.exe not found in common locations."
    Write-Host "Please ensure Windows SDK is installed and add signtool to your PATH." -ForegroundColor Yellow
    Write-Host "You can still use the unsigned binary at: $ExePath" -ForegroundColor Yellow
    exit 1
}

Write-Host "Using signtool: $SignTool"

& $SignTool sign `
    /sha1 $Cert.Thumbprint `
    /fd SHA256 `
    /tr http://timestamp.sectigo.com `
    /td SHA256 `
    /a `
    $ExePath

if ($LASTEXITCODE -ne 0) {
    throw "Code signing failed with exit code $LASTEXITCODE"
}

# Copy signed binary to dist folder
Copy-Item -Path $ExePath -Destination $SignedExe -Force

Write-Host "`n[5/5] Done!" -ForegroundColor Green
Write-Host "Signed executable: $SignedExe" -ForegroundColor Cyan
Write-Host ""
Write-Host "Note: This is a self-signed certificate." -ForegroundColor Yellow
Write-Host "Users will see a warning when running the executable for the first time." -ForegroundColor Yellow
Write-Host "For distribution, consider getting a certificate from a trusted CA (DigiCert, Sectigo, etc.)." -ForegroundColor Yellow