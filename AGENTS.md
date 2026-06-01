# AGENTS.md — KubeFront Coding Guidelines

This file exists so AI agents (and humans) can work effectively on KubeFront without breaking architectural invariants.

> **Architecture note:** KubeFront is a **Tauri v2** desktop app. The UI is a
> **React + TypeScript** front end (`src/`) rendered in the OS WebView; the
> backend is **Rust + kube-rs** (`src-tauri/`). The two communicate only through
> Tauri IPC (typed `invoke` commands + `Channel` streams). It is no longer an
> egui app.

## Core Principles

1. **Clear front/back split** — UI lives in `src/` (React/TS). All Kubernetes and OS work lives in `src-tauri/`. The only contract between them is the command surface in `src-tauri/src/commands.rs` and its TypeScript mirror in `src/api.ts` + `src/types.ts`. Keep those in sync.
2. **Never block — everything async** — All Kubernetes work (client creation, list, logs) runs on Tauri's Tokio runtime inside `#[tauri::command] async fn`s. The frontend stays responsive; it *pulls* resource lists on a timer and *receives* log lines pushed over a `Channel`.
3. **Kube logic lives in `k8s/`** — `src-tauri/src/k8s/` (manager, store) owns every `kube::Client`, `Kubeconfig`, and streaming operation. `commands.rs` orchestrates; React never sees raw k8s objects, only small serializable projections (`PodRow`, `NodeRow`, `TableData`, …).
4. **One active context** — Multiple contexts/kubeconfigs are managed, but only one live `kube::Client` at a time, held in the managed `Backend` state behind a Tokio `Mutex`.
5. **K3S is first-class but not special** — The "this looks like K3S" heuristic lives in ONE place (`k8s/manager.rs::detect_k3s`). Everything else treats every cluster identically.
6. **Small, fast, local** — No bundled Chromium (OS WebView). Ring buffers for logs (cap ~5000 lines). Pull only what the active view needs.

## IPC Pattern (MANDATORY)

Backend command:
```rust
#[tauri::command]
pub async fn list_pods(
    state: State<'_, SharedBackend>,
    namespace: Option<String>,
) -> Result<Vec<PodRow>, String> { /* lock, clone client, list, project */ }
```

Frontend wrapper (`src/api.ts`):
```ts
listPods: (namespace: string | null) => invoke<PodRow[]>("list_pods", { namespace }),
```

- Every new command must be registered in `src-tauri/src/lib.rs` `generate_handler!`.
- Streaming (logs) uses `tauri::ipc::Channel<T>`; store a cancellation `oneshot` in `Backend.log_streams` so streams can be stopped on close / context switch.
- Hold the `Mutex` only briefly. For slow work (connecting), copy what you need out, drop the lock, then do the network call.

## Error Handling

- Connection / auth errors → returned in `KubeStatus.error` and surfaced in the UI as a non-fatal banner + "Reconnect". Never panic.
- Commands return `Result<T, String>`; map kube errors with `.map_err(|e| e.to_string())`.
- Log everything interesting with `tracing`.

## Kubeconfig & Multi-Cluster

- Always go through `KubeConfigManager`.
- Support the `KUBECONFIG` env var (kube handles this for defaults).
- The K3S detection heuristic must stay in the single `detect_k3s` function.
- Registered kubeconfigs + settings persist to `settings.json` via `AppState` (only serializable bits — never the live client).

## UI / React Rules

- Generic list pages are data-driven: add a `kind` to the backend `list_resource` match + a `store::*_table` projection, then a `TABLE_VIEWS` entry in `src/views.ts`. Avoid bespoke components unless a view genuinely needs them (Pods, Nodes, Clusters, Monitoring, Logging, Settings).
- Theming is CSS variables in `src/styles.css`. The accent color is single-sourced from the Rust `ColorScheme`; the frontend applies it as `--accent`. Status colors are `--status-*` vars.
- New floating windows (e.g. log windows) manage their own lifetime and call `stopLogs` on close.

## Testing & Verification

- Before any PR-like push:
  ```bash
  npm run build                                                    # tsc + vite
  cargo fmt   --manifest-path src-tauri/Cargo.toml -- --check
  cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
  ```
- `dist/` must exist before `cargo clippy`/`build` (the crate embeds it). Run `npm run build` first.
- Manual test on real clusters (k3d, native k3s, rancher-desktop, remote via kubeconfig) for resource views and logs.

## What NOT to Do

- Do not add `unwrap()` outside of tests or tiny infallible formatting.
- Do not return raw k8s-openapi objects across IPC — project to a small DTO.
- Do not put Kubernetes types or client logic in the React frontend.
- Do not let `src/api.ts`/`src/types.ts` drift from `commands.rs` DTOs.

## Useful Commands

```bash
npm install
npm run tauri dev                                  # dev with HMR + Rust app
npm run tauri build                                # release bundles
RUST_LOG=debug,kube=info npm run tauri dev         # verbose backend logs
```

When in doubt, keep the core scope (context switcher + Pods + Nodes + live logs) delightful, and grow the data-driven views from there.
