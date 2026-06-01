#!/usr/bin/env bash
#
# KubeFront — build & run (Linux / macOS)
#
# Default: dev mode (Vite HMR + Rust app, fast iteration).
#   ./scripts/run.sh
#
# Release: build optimized bundles, then launch the compiled binary.
#   ./scripts/run.sh --release
#
# Pass RUST_LOG for verbose backend logs, e.g.:
#   RUST_LOG=debug,kube=info ./scripts/run.sh

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

MODE="dev"
if [[ "${1:-}" == "--release" ]]; then
    MODE="release"
fi

# First-run: install frontend deps if missing.
if [[ ! -d node_modules ]]; then
    echo "[setup] Installing frontend dependencies..."
    npm install
fi

if [[ "$MODE" == "dev" ]]; then
    echo "[run] Starting KubeFront in dev mode (Vite HMR + Rust app)..."
    exec npm run tauri dev
fi

echo "[run] Building optimized release (frontend + Rust app + bundles)..."
npm run tauri build

# Locate and launch the compiled binary.
BIN="$PROJECT_ROOT/src-tauri/target/release/kube-front"
if [[ "$(uname)" == "Darwin" ]]; then
    APP="$(find "$PROJECT_ROOT/src-tauri/target/release/bundle/macos" -maxdepth 1 -name '*.app' 2>/dev/null | head -1)"
    if [[ -n "$APP" ]]; then
        echo "[run] Launching $APP ..."
        exec open "$APP"
    fi
fi

if [[ -x "$BIN" ]]; then
    echo "[run] Launching $BIN ..."
    exec "$BIN"
fi

echo "Build finished, but the binary was not found at $BIN."
echo "Check the bundle output under src-tauri/target/release/bundle/."
exit 1
