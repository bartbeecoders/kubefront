#!/usr/bin/env pwsh
#
# Shared cargo environment setup, dot-sourced by build.ps1 / run.ps1.
#
# This machine has no working Perl/NASM on PATH, so the vendored OpenSSL
# (openssl-sys, pulled in by `kube`'s openssl-tls and reqwest's
# native-tls-vendored) cannot build from scratch. A prebuilt vendored OpenSSL
# already exists under src-tauri\target\ (from before the workspace split), so
# we point cargo at that target dir to reuse it.
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

function Set-KubefrontCargoEnv {
    param(
        [string]$ProjectRoot,
        [switch]$NoWorkaround
    )

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
        Write-Host "[env] No prebuilt OpenSSL found under $PrebuiltTarget; using default target dir."
        Write-Host "[env] If the build fails on openssl-sys, install Strawberry Perl + NASM, or"
        Write-Host "[env] restore src-tauri\target from a previous successful build."
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
