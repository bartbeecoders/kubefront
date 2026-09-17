#!/usr/bin/env pwsh
#
# Shared cargo environment setup, dot-sourced by build.ps1 / run.ps1 /
# build-release.ps1 / build-testrun.ps1.
#
# Vendored OpenSSL (openssl-sys, pulled in by kube's openssl-tls and reqwest's
# native-tls-vendored) needs a C compiler + a *native* Windows Perl, plus NASM.
# Git for Windows' MSYS perl does not work.
#
# If a previous build already left openssl-sys under src-tauri\target, we reuse
# that target dir so Perl/NASM are not required. Otherwise we download portable
# Strawberry Perl + NASM once into %LOCALAPPDATA%\kf-buildtools and prepend them
# to PATH (same as build-release.ps1).
#
# CI runners have Strawberry Perl + NASM, so they build vendored OpenSSL
# normally against the default repo-root target. Skip the override there by
# setting KUBEFRONT_NO_OPENSSL_WORKAROUND=1 (CI does this) or passing
# -NoOpenSslWorkaround to the calling script.

function Test-StaleTargetDir {
    param([string]$ProjectRoot)

    # Tauri bakes absolute paths into generated permission .toml files under
    # target/debug/build/tauri-*/out/. If the project was moved to a different
    # drive or path, those cached paths become invalid and the build fails with
    # "failed to read plugin permissions: ... The system cannot find the path".
    # Detect this and tell the user to run `cargo clean`.

    $TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $ProjectRoot "target" }
    $TauriBuildDirs = Get-ChildItem -Path (Join-Path $TargetDir "debug\build") -Filter "tauri-*" -Directory -ErrorAction SilentlyContinue
    if (-not $TauriBuildDirs) { return $false }

    $currentDrive = $ProjectRoot.Substring(0, 2).ToUpper()  # e.g. "F:"
    foreach ($dir in $TauriBuildDirs) {
        $permDir = Join-Path $dir.FullName "out\permissions"
        if (-not (Test-Path $permDir)) { continue }
        $tomlFiles = Get-ChildItem -Path $permDir -Filter "*.toml" -Recurse -ErrorAction SilentlyContinue
        foreach ($f in $tomlFiles) {
            $content = Get-Content $f.FullName -Raw -ErrorAction SilentlyContinue
            if (-not $content) { continue }
            # Look for absolute Windows paths referencing a different drive letter.
            $matches = [regex]::Matches($content, '([A-Za-z]):\\[^\s"''<>|]+')
            foreach ($m in $matches) {
                $fileDrive = $m.Groups[1].Value.ToUpper() + ":"
                if ($fileDrive -ne $currentDrive) {
                    return $true
                }
            }
        }
    }
    return $false
}

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
    New-Item -ItemType Directory -Force -Path $DestDir | Out-Null
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

function Set-KubefrontCargoEnv {
    param(
        [string]$ProjectRoot,
        [switch]$NoWorkaround
    )

    # rustup puts cargo on the *user* PATH; existing terminals (and this script
    # when launched from one) will not see it until PATH is refreshed.
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
        if (Test-Path (Join-Path $cargoBin "cargo.exe")) {
            $env:Path = "$cargoBin;$env:Path"
            Write-Host "[env] Added $cargoBin to PATH for this session."
        }
        else {
            Write-Host "[env] cargo not found. Install Rust (https://rustup.rs/) and open a new terminal." -ForegroundColor Red
            throw "cargo is not installed or not on PATH"
        }
    }

    if ($NoWorkaround -or $env:KUBEFRONT_NO_OPENSSL_WORKAROUND -eq "1") {
        Write-Host "[env] OpenSSL workaround disabled; using default cargo target dir."
        return
    }

    # If the user already pinned a target dir, respect it.
    if ($env:CARGO_TARGET_DIR) {
        Write-Host "[env] CARGO_TARGET_DIR already set to $($env:CARGO_TARGET_DIR); leaving as-is."
        return
    }

    $PrebuiltTarget = Join-Path $ProjectRoot "src-tauri\target"
    $PrebuiltOpenSsl = Join-Path $PrebuiltTarget "release\build"

    $hasPrebuilt = (Test-Path $PrebuiltOpenSsl) -and `
        ((Get-ChildItem -Path $PrebuiltOpenSsl -Filter "openssl-sys-*" -Directory -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0)

    if ($hasPrebuilt) {
        $env:CARGO_TARGET_DIR = $PrebuiltTarget
        Write-Host "[env] Reusing prebuilt vendored OpenSSL: CARGO_TARGET_DIR=$PrebuiltTarget"
    }
    else {
        Write-Host "[env] No prebuilt OpenSSL found under $PrebuiltTarget; building vendored OpenSSL from source."
        Initialize-OpenSslToolchain
    }

    # Guard: if the project was moved (e.g. E:\ -> F:\), stale absolute paths in
    # Tauri's cached permission files will break the build. Detect and auto-clean.
    $TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $ProjectRoot "target" }
    if (Test-Path $TargetDir) {
        if (Test-StaleTargetDir -ProjectRoot $ProjectRoot) {
            Write-Host "[env] Stale build cache detected (project moved?). Running cargo clean..." -ForegroundColor Yellow
            cargo clean
            if ($LASTEXITCODE -ne 0) {
                Write-Host "[env] cargo clean failed; please run it manually." -ForegroundColor Red
            }
        }
    }
}
