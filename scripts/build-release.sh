#!/usr/bin/env bash
#
# KubeFront Release Builder (Linux / native)
#
# Builds the frontend, compiles the Rust app, and produces native bundles
# (.deb / .AppImage / .rpm) using the Tauri bundler.
#
# Cross-compiling a Tauri app from Linux to Windows is not supported (the
# WebView2 runtime is Windows-only) — build the Windows artifacts on Windows
# with scripts/build-release.ps1, or let the Release GitHub workflow do it.
#
# Requirements:
#   - Node.js 18+ and npm
#   - Rust toolchain
#   - Tauri Linux deps: libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
#       libjavascriptcoregtk-4.1-dev librsvg2-dev libappindicator3-dev patchelf
#   - Build tools for the vendored OpenSSL (compiled from source): a C compiler,
#       make, and perl — all standard on a build host (e.g. apt install build-essential perl)
#
# Usage:
#   ./scripts/build-release.sh
#   ./scripts/build-release.sh --clean
#   ./scripts/build-release.sh --version 0.2.3   # stamp all manifests before building

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

CLEAN=false
VERSION=""
while [[ $# -gt 0 ]]; do
    case $1 in
        --clean) CLEAN=true; shift ;;
        --version) VERSION="${2:-}"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "=== KubeFront Release Build (Linux) ==="

if [[ -n "$VERSION" ]]; then
    echo "[version] Stamping $VERSION into all manifests..."
    node scripts/set-version.mjs "$VERSION"
fi

if $CLEAN; then
    echo "[clean] Removing previous build artifacts..."
    cargo clean --manifest-path src-tauri/Cargo.toml || true
    rm -rf dist
fi

echo "[1/2] Installing frontend dependencies..."
npm ci

echo "[2/2] Building frontend + Rust app + native bundles..."
npm run tauri build

echo ""
echo "Build complete. Bundles:"
ls -R src-tauri/target/release/bundle 2>/dev/null || echo "  (none found — check the tauri build output above)"
