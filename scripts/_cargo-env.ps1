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
}
