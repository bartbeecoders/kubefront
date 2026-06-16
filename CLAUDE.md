# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Read [AGENTS.md](./AGENTS.md) first** — it holds the architectural invariants and the mandatory IPC/error/UI rules. This file is a quick orientation + command reference; AGENTS.md is the source of truth for *how* to change things.

## What this is

KubeFront is a desktop frontend for K3S/Kubernetes clusters: a **Tauri v2** app (Rust core + React/TypeScript WebView), plus a headless **axum** server for clusters reachable only through a reverse proxy. It uses the OS WebView (no bundled Chromium) and the user's existing kubeconfig.

## Workspace layout (Cargo workspace, 3 crates)

- **`crates/kube-core/`** — all Kubernetes logic + serde DTOs, shared verbatim by desktop and backend. Owns every `kube::Client`/`Kubeconfig`. Key files: `local.rs` (`LocalKube` client), `store.rs` (resource→DTO projections like `PodRow`/`TableData`), `dto.rs`, `manager.rs` (`KubeConfigManager`, `detect_k3s`), `logstream.rs`, `error.rs` (`CoreError` — its error *strings* are matched by the frontend; golden tests live here).
- **`crates/kubefront-backend/`** — headless REST server. Multi-connection `ConnectionPool` keyed by `backend.toml` id (`pool.rs`), routes in `routes.rs`, SSE log streaming in `sse.rs`, config in `config.rs`. Performs **no auth of its own** — trusts the reverse proxy; bind to loopback.
- **`src-tauri/`** — desktop app. `commands.rs` is the `#[tauri::command]` surface (the contract with React); `state.rs` holds `Backend` (one active connection: Local or Remote behind a Tokio `Mutex`); `remote.rs` is the HTTP `RemoteKube` client; `conn.rs` connection helpers; `lib.rs` registers every command in `generate_handler!`.
- **`src/`** — React/TS frontend. `api.ts` + `types.ts` mirror `commands.rs` (keep in sync). `views.ts` has the `TABLE_VIEWS` registry; data-driven list pages live in `src/views/`, reusable pieces in `src/components/`. Theming via CSS vars in `styles.css`.

## Data flow

Desktop reaches a cluster two ways, identical to React: **Direct** (`invoke` → local `kube::Client`) or **Remote** (`invoke` → HTTP `RemoteKube` → `kubefront-backend` → `kube::Client`). React *pulls* resource lists on a timer; pod logs are *pushed* over a Tauri `Channel`. Backend exchanges `kube-core` DTOs as JSON, so DTO changes must be backward-compatible (add fields as `#[serde(default)]`; never rename/remove without a version bump).

## Commands

```bash
npm install
npm run tauri dev                                   # dev: Vite HMR + Rust app
RUST_LOG=debug,kube=info npm run tauri dev          # verbose backend logs
npm run tauri build                                 # release bundles → target/release/bundle/
```

Before any push (order matters — `src-tauri` embeds `dist/`):

```bash
npm run build                                        # tsc + vite; MUST run before clippy/build of src-tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p kube-core                              # DTO round-trips + error-string golden tests
cargo test -p kubefront-backend                      # config tests
```

`kube-core` and `kubefront-backend` build without `dist/`; only `src-tauri` needs it.

Run a single Rust test: `cargo test -p kube-core <test_name>`.

Version is single-sourced across `package.json` / `tauri.conf.json` / `Cargo.*`: `npm run check-version` (CI gate), `npm run bump` to change it.

## Run the backend server

```bash
cargo build --release -p kubefront-backend
cp crates/kubefront-backend/backend.toml.example backend.toml   # edit it
./target/release/kubefront-backend --config backend.toml
```

## Gotchas

- **TLS is OpenSSL, not rustls** (vendored/statically linked) — rustls rejects the v1 client certs some k3s clusters issue. Windows builds need NASM + Perl to build vendored OpenSSL.
- Hold the `Backend` `Mutex` only briefly: copy what you need out, drop the lock, then do network calls.
- Don't reintroduce kube-rs calls in `src-tauri` or duplicate them in the backend — orchestrate only; all k8s logic stays in `kube-core`. The K3S heuristic stays in the single `detect_k3s`.
