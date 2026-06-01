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

## Installing a release

Prebuilt installers are attached to each [GitHub Release](https://github.com/bartbeecoders/kubefront/releases).

### Windows (`.exe` / `.msi`)

The installers are **self-signed**, not signed by a trusted Certificate Authority.
Windows SmartScreen will therefore show **"Windows protected your PC / unknown
publisher"** and may mention potential risk. This is expected for self-signed apps
(it is *not* a sign the file is infected) — a self-signed certificate cannot remove
this warning; only a CA-issued (EV / Azure Trusted Signing) certificate can.

To install anyway:

1. Run the `.exe`. On the blue SmartScreen dialog, click **More info → Run anyway**.
2. If the file was marked "blocked" after download: right-click the `.exe` →
   **Properties** → tick **Unblock** → **OK**, then run it.

**Optional — trust the publisher on machines you control.** Each release also
attaches `kubefront-windows-signing-pubcert.cer`. Importing it removes the
*"unknown publisher"* prompt on that machine (SmartScreen reputation warnings may
still appear). As an Administrator:

```powershell
Import-Certificate -FilePath .\kubefront-windows-signing-pubcert.cer `
  -CertStoreLocation Cert:\LocalMachine\Root
Import-Certificate -FilePath .\kubefront-windows-signing-pubcert.cer `
  -CertStoreLocation Cert:\LocalMachine\TrustedPublisher
```

> The signing certificate is generated fresh for each release, so re-import the
> new `.cer` when you upgrade. Only do this on machines you own/administer.

### Linux (`.deb` / `.AppImage` / `.rpm`)

Artifacts have GPG detached signatures (`*.asc`). To verify before installing:

```bash
gpg --import kubefront-linux-signing-pubkey.asc
gpg --verify KubeFront_<version>_amd64.AppImage.asc KubeFront_<version>_amd64.AppImage
```

Then install the `.deb` (`sudo apt install ./KubeFront_*.deb`), make the
`.AppImage` executable (`chmod +x`), or install the `.rpm`.

## Development

```bash
npm run build                                   # type-check + build the frontend
cargo fmt   --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
RUST_LOG=debug,kube=info npm run tauri dev      # verbose backend logs
```

> The Rust crate embeds the built frontend (`dist/`) via `generate_context!`. Run `npm run build`
> once before `cargo clippy`/`cargo build` so `dist/` exists.

### Logs & troubleshooting

KubeFront writes a debug log to a file on every platform (the console isn't
available in the Windows release build), and shows its exact path in
**Settings → Log level → "Log file:"**. Default locations:

| OS | Log file |
|----|----------|
| Linux | `~/.local/share/KubeFront/logs/kubefront.log` |
| Windows | `%LOCALAPPDATA%\kube-front\KubeFront\data\logs\kubefront.log` |
| macOS | `~/Library/Application Support/dev.kube-front.app/logs/kubefront.log` |

To capture a verbose log (e.g. to debug a connection/kubeconfig problem):

1. In the app, set **Settings → Log level → DEBUG**, then restart KubeFront and
   reproduce the issue — no environment variable needed.
2. Open the log file shown in Settings.

`RUST_LOG` still overrides the level when running from a terminal, e.g.
`RUST_LOG=debug,kube=debug npm run tauri dev`.

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
