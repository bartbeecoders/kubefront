#!/usr/bin/env bash
#
# KubeFront Release Builder + Self-Signed Code Signing (Linux / Cross-compile)
#
# This script builds a Windows release executable and signs it using osslsigncode.
# It is intended to be run from Linux/macOS when cross-compiling for Windows.
#
# Requirements:
#   - Rust with Windows target: rustup target add x86_64-pc-windows-msvc
#   - osslsigncode (apt install osslsigncode / brew install osslsigncode)
#   - A self-signed certificate (generated below)
#
# Usage:
#   ./scripts/build-release.sh
#   ./scripts/build-release.sh --clean
#

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/dist"
TARGET="x86_64-pc-windows-msvc"
EXE_NAME="kube-front.exe"
SIGNED_NAME="KubeFront.exe"

CERT_DIR="$PROJECT_ROOT/scripts/certs"
CERT_PFX="$CERT_DIR/kube-front-selfsigned.pfx"
CERT_KEY="$CERT_DIR/kube-front-selfsigned.key"
CERT_CRT="$CERT_DIR/kube-front-selfsigned.crt"

CLEAN=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --clean) CLEAN=true; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "=== KubeFront Cross-Compile + Self-Signed Signing (Linux) ==="

mkdir -p "$DIST_DIR"
mkdir -p "$CERT_DIR"

# 1. Generate self-signed certificate if it doesn't exist
if [[ ! -f "$CERT_PFX" ]]; then
    echo "[1/5] Generating self-signed code signing certificate..."
    openssl req -x509 -newkey rsa:2048 -keyout "$CERT_KEY" -out "$CERT_CRT" \
        -days 1825 -nodes -subj "/CN=KubeFront Self-Signed" \
        -addext "extendedKeyUsage=codeSigning"

    # Convert to PFX (required by osslsigncode)
    openssl pkcs12 -export -in "$CERT_CRT" -inkey "$CERT_KEY" \
        -out "$CERT_PFX" -passout pass:"" -name "KubeFront Code Signing"
    echo "Self-signed certificate created at: $CERT_PFX"
else
    echo "[1/5] Using existing certificate: $CERT_PFX"
fi

# 2. Clean if requested
if $CLEAN; then
    echo "[2/5] Cleaning..."
    cargo clean
else
    echo "[2/5] Skipping clean"
fi

# 3. Build for Windows
echo "[3/5] Building release for Windows ($TARGET)..."
rustup target add "$TARGET" 2>/dev/null || true

cargo build --release --target "$TARGET"

SOURCE_EXE="$PROJECT_ROOT/target/$TARGET/release/$EXE_NAME"

if [[ ! -f "$SOURCE_EXE" ]]; then
    echo "Error: Built executable not found at $SOURCE_EXE"
    exit 1
fi

# 4. Sign with osslsigncode
echo "[4/5] Signing executable with osslsigncode..."
osslsigncode sign \
    -pkcs12 "$CERT_PFX" \
    -pass "" \
    -n "KubeFront" \
    -i "https://github.com/your-org/kube-front" \
    -t "http://timestamp.sectigo.com" \
    -in "$SOURCE_EXE" \
    -out "$DIST_DIR/$SIGNED_NAME"

echo "[5/5] Build complete!"
echo ""
echo "Signed executable: $DIST_DIR/$SIGNED_NAME"
echo ""
echo "Warning: This is a self-signed certificate."
echo "Windows will show a warning to users."
echo "For real distribution, obtain a certificate from a trusted CA."