#!/usr/bin/env bash
#
# KubeFront — build & run for local testing (Linux / macOS)
#
# Starts BOTH halves needed to exercise the Remote path end-to-end on one box:
#   1. the headless kubefront-backend REST server (background), and
#   2. the Tauri desktop app — Vite HMR + Rust core (foreground).
# Both stop together when you quit the app or press Ctrl+C.
#
# Default (dev mode, fast iteration):
#   ./scripts/run.sh
#       → backend on 127.0.0.1:8080; connect the desktop app (Remote) to
#         http://127.0.0.1:8080/connection1
#
# Release: build the optimized binary (no installer bundles) and launch it.
#   ./scripts/run.sh --release
#   (Use scripts/build-release.sh when you want .deb/.rpm/.AppImage bundles.)
#
# Frontend only (Direct/local kube connection, no backend):
#   ./scripts/run.sh --no-backend
#
# Backend only (handy for testing the server in isolation):
#   ./scripts/run.sh --no-frontend
#
# Env overrides:
#   RUST_LOG=debug,kube=info ./scripts/run.sh     # verbose logs (both halves)
#   BACKEND_LISTEN=127.0.0.1:9000 ./scripts/run.sh
#   KUBECONFIG=/path/to/config ./scripts/run.sh   # used when generating backend.toml
#   BACKEND_CONFIG=/path/to/backend.toml ./scripts/run.sh

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

MODE="dev"
WITH_BACKEND=true
WITH_FRONTEND=true
for arg in "$@"; do
    case "$arg" in
        --release)     MODE="release" ;;
        --no-backend)  WITH_BACKEND=false ;;
        --no-frontend) WITH_FRONTEND=false ;;
        -h|--help)
            sed -n '2,33p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) echo "Unknown option: $arg" >&2; exit 1 ;;
    esac
done

# Tunables (env-overridable).
BACKEND_LISTEN="${BACKEND_LISTEN:-127.0.0.1:8080}"
BACKEND_CONFIG="${BACKEND_CONFIG:-$PROJECT_ROOT/backend.toml}"
KUBECONFIG_PATH="${KUBECONFIG:-$HOME/.kube/config}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PROJECT_ROOT/target}"

# First-run: install frontend deps if missing.
if $WITH_FRONTEND && [[ ! -d node_modules ]]; then
    echo "[setup] Installing frontend dependencies..."
    npm install
fi

# --- backend lifecycle -------------------------------------------------------

BACKEND_PID=""
cleanup() {
    if [[ -n "$BACKEND_PID" ]] && kill -0 "$BACKEND_PID" 2>/dev/null; then
        echo
        echo "[backend] Stopping (pid $BACKEND_PID)..."
        kill "$BACKEND_PID" 2>/dev/null || true
        wait "$BACKEND_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

ensure_backend_config() {
    [[ -f "$BACKEND_CONFIG" ]] && return 0

    if [[ ! -f "$KUBECONFIG_PATH" ]]; then
        echo "[backend] No backend.toml and no kubeconfig at $KUBECONFIG_PATH." >&2
        echo "          Set KUBECONFIG/BACKEND_CONFIG, or run with --no-backend." >&2
        exit 1
    fi

    local ctx
    ctx="$(awk '/^current-context:/ {print $2; exit}' "$KUBECONFIG_PATH")"
    ctx="${ctx:-default}"
    echo "[backend] Generating dev config $BACKEND_CONFIG"
    echo "          (kubeconfig=$KUBECONFIG_PATH, context=$ctx) — edit or delete freely."
    cat > "$BACKEND_CONFIG" <<EOF
# Auto-generated dev config for ./scripts/run.sh — loopback only, no auth.
# Edit freely; delete to regenerate from your current kubeconfig.
listen = "$BACKEND_LISTEN"
base_path = "/"

[[connection]]
id = "connection1"
name = "Local dev"
kubeconfig = "$KUBECONFIG_PATH"
context = "$ctx"
namespace = ""
read_only = false
EOF
}

start_backend() {
    ensure_backend_config

    local build_flag="" profile_dir="debug"
    if [[ "$MODE" == "release" ]]; then
        build_flag="--release"; profile_dir="release"
    fi
    local bin="$CARGO_TARGET_DIR/$profile_dir/kubefront-backend"

    echo "[backend] Building kubefront-backend ($profile_dir)..."
    cargo build $build_flag -p kubefront-backend

    echo "[backend] Starting on $BACKEND_LISTEN (config: $BACKEND_CONFIG)..."
    "$bin" --config "$BACKEND_CONFIG" &
    BACKEND_PID=$!

    # Wait for the listener to accept TCP (max ~10s) so the UI can connect immediately.
    local host="${BACKEND_LISTEN%:*}" port="${BACKEND_LISTEN##*:}"
    for _ in $(seq 1 50); do
        if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
            echo "[backend] Failed to start — see output above." >&2
            exit 1
        fi
        if (exec 3<>"/dev/tcp/$host/$port") 2>/dev/null; then
            exec 3>&- 3<&-
            echo "[backend] Ready → in the desktop app pick Remote and connect to:"
            echo "              http://$BACKEND_LISTEN/connection1"
            return 0
        fi
        sleep 0.2
    done
    echo "[backend] Warning: $BACKEND_LISTEN not accepting connections yet; continuing." >&2
}

# --- run ---------------------------------------------------------------------

if $WITH_BACKEND; then
    start_backend
fi

if ! $WITH_FRONTEND; then
    echo "[run] Backend only — press Ctrl+C to stop."
    wait "$BACKEND_PID"
    exit 0
fi

if [[ "$MODE" == "dev" ]]; then
    echo "[run] Starting KubeFront frontend in dev mode (Vite HMR + Rust app)..."
    npm run tauri dev
    exit 0
fi

echo "[run] Building optimized release (frontend + Rust app)..."
# --no-bundle: compile the optimized binary but SKIP installer bundling
# (.deb/.rpm/.AppImage). AppImage's linuxdeploy needs network + FUSE and isn't
# needed just to run the app locally — and its failure would otherwise abort the
# launch below. Use scripts/build-release.sh when you actually want bundles.
npm run tauri build -- --no-bundle

# Locate the compiled desktop binary (workspace target or src-tauri/target).
BIN=""
for cand in \
    "$CARGO_TARGET_DIR/release/kube-front" \
    "$PROJECT_ROOT/src-tauri/target/release/kube-front"; do
    if [[ -x "$cand" ]]; then BIN="$cand"; break; fi
done

if [[ "$(uname)" == "Darwin" ]]; then
    for macdir in \
        "$CARGO_TARGET_DIR/release/bundle/macos" \
        "$PROJECT_ROOT/src-tauri/target/release/bundle/macos"; do
        APP="$(find "$macdir" -maxdepth 1 -name '*.app' 2>/dev/null | head -1)"
        if [[ -n "$APP" ]]; then
            echo "[run] Launching $APP ..."
            exec open "$APP"
        fi
    done
fi

if [[ -n "$BIN" ]]; then
    echo "[run] Launching $BIN ..."
    exec "$BIN"
fi

echo "Build finished, but the kube-front binary was not found." >&2
echo "Check the bundle output under (src-tauri/)target/release/bundle/." >&2
exit 1
