# AGENTS.md — KubeFront Coding Guidelines

This file exists so AI agents (and humans) can work effectively on KubeFront without breaking architectural invariants.

## Core Principles

1. **Pure Rust only** — No Node, no npm, no web frameworks, no JS/TS. The entire app (UI + logic) is Rust + egui.
2. **Never block the UI thread** — All Kubernetes work (client creation, list/watch, logs) MUST run on the Tokio background runtime. UI thread only does `try_recv()` on channels + `ctx.request_repaint()`.
3. **Kube logic lives in `k8s/`** — `src/k8s/` (manager, resources, tasks) owns every `kube::Client`, `Kubeconfig`, watcher, and streaming operation. UI code (app.rs + ui/) only renders and sends high-level intents.
4. **One active context in MVP** — We support multiple contexts via the loaded Kubeconfig, but only one live `Client` at a time. The manager is designed so concurrent multi-cluster (fleet) can be added later with minimal churn.
5. **K3S is first-class but not special** — Heuristics for "this looks like K3S" live in ONE place (`k8s/manager.rs`). The rest of the app treats every cluster identically.
6. **Small, fast, local** — Target < 50 MB RAM for typical K3S-sized clusters (single node or small). Virtual tables from day one. Ring buffers for logs.

## Async & Channels Pattern (MANDATORY)

```rust
// In a background task
let ctx = egui_ctx.clone(); // cheap
tokio::spawn(async move {
    // ... kube work ...
    let _ = tx.send(update);
    ctx.request_repaint();   // critical
});
```

In `App::update`:
```rust
while let Ok(msg) = rx.try_recv() { ... }
```

Use `std::sync::mpsc` for simplicity in MVP. Consider `egui_inbox` only after Phase 3 if boilerplate becomes painful.

## Error Handling

- Connection / auth errors → surface in UI as non-fatal banners + "Reconnect" action. Never panic.
- Use `anyhow::Result` at the boundary of async tasks, `thiserror` for library errors inside `k8s/`.
- Log everything interesting with `tracing`.

## Kubeconfig & Multi-Cluster

- Always go through `KubeConfigManager`.
- Support `KUBECONFIG` env var (the `kube` crate does this automatically for defaults).
- K3S detection heuristic (server URL contains 127.0.0.1:6443 / localhost:6443 / :6443 + name patterns) must be in one function.
- When adding future multi-file support, use stable `(source_id, context_name)` keys exactly like FerrisScope does.

## UI / egui Rules

- Tables: always `egui_extras::TableBuilder` with index indirection for filter/sort (see Phase 2 pattern).
- Status colors: use constants from `ui/theme.rs` (K3S_PURPLE, STATUS_RUNNING, etc.).
- New floating windows or panels → manage lifetime + cancellation tokens explicitly.
- Persistence: only put serializable bits in `AppState`. Heavy clients stay in memory only.

## Testing & Verification

- `cargo fmt -- --check && cargo clippy -- -D warnings && cargo test` before any PR-like push.
- Manual test on real clusters (k3d, native k3s, rancher-desktop, remote via kubeconfig) is required for resource views and logs.
- When adding a new background task, always provide a way to abort it cleanly on context switch or shutdown.

## What NOT to Do

- Do not add `unwrap()` outside of tests or tiny infallible formatting.
- Do not start a second watcher for data that already has an active one (future reflectors).
- Do not put Kubernetes types or client logic in `app.rs` or `ui/`.
- Do not introduce Node.js or web tech "just for the UI".

## Useful Commands

```bash
cargo fmt
cargo clippy -- -D warnings
cargo run
RUST_LOG=debug,kube=info cargo run
```

Follow the plan in `Vibecoding/instructions.md` + the implementation plan.md in the session dir. When in doubt, keep the MVP scope (context switcher + Pods + Nodes + live logs) and make it delightful.

Happy shipping a fast, native K3S frontend.
