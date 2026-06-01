# KubeFront

**A modern desktop frontend for K3S and Kubernetes clusters.**

Lightweight, cross-platform (Linux + Windows + macOS), uses your existing `kubectl` kubeconfig. Built with **Tauri v2** (Rust backend, native WebView) + **React/TypeScript** front end + **kube-rs**.

![KubeFront screenshot placeholder](assets/screenshot.png)

## Features

- Load any kubectl-compatible kubeconfig (default, custom file, or the classic K3S path).
- Context switcher with K3S detection badges + multi-kubeconfig management (friendly names, descriptions).
- Live Pods table (filter, namespace, status colors, age) with a detail panel.
- Nodes table with status/roles/kubelet version.
- 18 resource views: Namespaces, Deployments, StatefulSets, DaemonSets, Jobs, CronJobs, ConfigMaps, Secrets, Services, Storage (PVC/PV/StorageClass), Network (Services/Ingress/NetworkPolicy), Access Control (ServiceAccounts/Roles/RoleBindings), plus a Monitoring overview.
- Click any pod → live, streaming log windows (multiple, draggable, follow mode, per-container, filter, copy).
- Live theming: Dark / Light / Custom accent (8 presets + color picker), font scaling.
- Persists settings + last used kubeconfig/context to `settings.json`.

## Architecture

```
┌─ Native window (Tauri WebView) ──────────────┐
│  React + TypeScript + Vite  (src/)           │
│            ↕  Tauri IPC commands / Channels  │
│  Rust backend (src-tauri/)                   │
│    • commands.rs  — IPC bridge               │
│    • k8s/         — kube-rs client + lists   │
│    • state.rs     — persisted settings       │
└──────────────────────────────────────────────┘
```

- **Frontend** (`src/`) renders the UI and pulls resource lists on a timer via typed `invoke()` calls.
- **Backend** (`src-tauri/`) owns every `kube::Client` / `Kubeconfig` and runs all Kubernetes work on Tauri's Tokio runtime. Pod logs are streamed to the UI over a Tauri `Channel`.
- No bundled Chromium — Tauri uses the OS WebView (WebKitGTK on Linux, WebView2 on Windows, WKWebView on macOS).

See [AGENTS.md](./AGENTS.md) for architecture rules and invariants.

## Requirements

- **Rust** 1.80+ (2021 edition)
- **Node.js** 18+ and npm
- A working kubeconfig (or access to one)
- **Linux only** — system WebView deps:
  ```bash
  sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev \
    libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev patchelf
  ```

## Quick Start

```bash
git clone https://github.com/bartbeecoders/kubefront
cd kubefront

npm install
npm run tauri dev        # dev: Vite HMR + Rust app
```

On first launch it tries `~/.kube/config` (or `$KUBECONFIG`). Use the top-bar buttons to load a
different file or the common K3S location (`/etc/rancher/k3s/k3s.yaml` — you may need to `sudo cp`
it into your home or fix permissions).

### Build a release bundle

```bash
npm run tauri build      # produces installers in src-tauri/target/release/bundle/
```

## Development

```bash
npm run build                                   # type-check + build the frontend
cargo fmt   --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
RUST_LOG=debug,kube=info npm run tauri dev      # verbose backend logs
```

> The Rust crate embeds the built frontend (`dist/`) via `generate_context!`. Run `npm run build`
> once before `cargo clippy`/`cargo build` so `dist/` exists.

## Roadmap

Post-feature-parity ideas:
- Deployments / Services / Events detail drill-downs
- Multi-cluster concurrent view
- Port-forward UI + embedded exec
- Write actions (scale, delete, restart) with confirmations
- Better CRD + custom resource support

## License

MIT or Apache-2.0 (your choice).

## Acknowledgments

- [kube-rs](https://kube.rs) — the excellent Rust Kubernetes client
- [Tauri](https://tauri.app) — build smaller, faster desktop apps with a web frontend and a Rust core
- [React](https://react.dev) + [Vite](https://vite.dev)

Built for K3S users who want a fast, local, no-nonsense desktop experience.
