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

KubeFront is a Cargo **workspace** of three crates:

```
crates/kube-core/         shared Kubernetes logic + serde DTOs (LocalKube, projections, log_stream)
crates/kubefront-backend/ headless axum REST server (multi-cluster, sits behind a reverse proxy)
src-tauri/                the desktop app (React WebView + Rust), depends on kube-core
```

The desktop app reaches each cluster one of two ways:

```
Direct (port 6443):  desktop ──kube::Client (OpenSSL)──▶ cluster API server
Remote (port 443):   desktop ──HTTPS──▶ reverse proxy ──▶ kubefront-backend ──kube::Client──▶ cluster
```

- **`kube-core`** owns every `kube::Client` / `Kubeconfig`, the resource projections, and the
  `log_stream`. It is shared verbatim by the desktop (Direct mode) and the backend server, so the
  Kubernetes logic lives in exactly one place.
- **`src-tauri`** (desktop) renders the UI and pulls resource lists on a timer via typed
  `invoke()` calls. Each command dispatches to either a local client (Direct) or an HTTP
  `RemoteKube` (Remote) — the React layer is identical in both modes. Pod logs stream to the UI
  over a Tauri `Channel`.
- **`kubefront-backend`** holds the real clients for clusters reachable only through a reverse
  proxy (e.g. globally-dispersed OT sites). See *Connecting through a reverse proxy* below.
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
npm run tauri build      # produces installers in target/release/bundle/
```

## Connecting through a reverse proxy (kubefront-backend)

Some clusters can only be reached through a reverse proxy on **port 443** — for example
Kubernetes/K3S clusters in segregated OT networks. For these, run the **`kubefront-backend`**
server *next to the cluster* (where it can reach the API server on 6443) and point the desktop
app at it. The desktop and the backend share the same Kubernetes logic (`kube-core`), so a remote
connection behaves identically to a direct one.

### Run the backend

```bash
cargo build --release -p kubefront-backend
cp crates/kubefront-backend/backend.toml.example backend.toml   # then edit it
./target/release/kubefront-backend --config backend.toml
```

`backend.toml` lists the clusters this instance exposes:

```toml
listen = "127.0.0.1:8080"   # bind loopback; let only the reverse proxy reach it
base_path = "/"             # "/" = the proxy strips its own site segment

[[connection]]
id = "connection1"          # URL segment: /connection1/api/...
name = "K3S Server 1"
kubeconfig = "/etc/kubefront/k3s-server1.yaml"
context = "default"
namespace = ""              # optional scope; "" = all namespaces
read_only = false           # true ⇒ delete / restart / edit return 403
```

> **Security — trust boundary.** The backend performs **no authentication of its own**: it trusts
> the reverse proxy to terminate TLS and authenticate. Bind it to loopback (or an internal
> interface only the proxy reaches) and **never expose it directly** — a non-loopback bind logs a
> loud warning. Use `read_only = true` for connections that should be view-only.

### Reverse proxy

The proxy maps a per-site path segment to the backend and forwards the rest. With nginx:

```nginx
location /k3s-server1/ {
    proxy_pass http://127.0.0.1:8080/;   # strips /k3s-server1, forwards /connection1/api/...
    proxy_buffering off;                 # required for live log streaming (SSE)
}
```

A URL like `https://server/k3s-server1/connection1/api` has **two identifiers**: `k3s-server1`
selects the **site/backend** (proxy routing), and `connection1` selects the **connection** within
that backend (the `[[connection]] id`).

### Add it in the desktop app

**Settings → Remote Connections** → enter a name and the endpoint
(`https://server/k3s-server1/connection1`), optionally a CA bundle for an internal/self-signed
proxy certificate, then **Test** and **Add remote**. The connection then appears on the Dashboard
and in the connection list; views, logs, and edits all work exactly like a direct connection.

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
cargo fmt --all -- --check                      # whole workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p kube-core                         # DTO round-trips + error-string golden tests
RUST_LOG=debug,kube=info npm run tauri dev      # verbose backend logs
```

> The desktop crate embeds the built frontend (`dist/`) via `generate_context!`. Run `npm run build`
> once before `cargo clippy`/`cargo build` so `dist/` exists. (Building `kube-core` or
> `kubefront-backend` alone does not need `dist/`.)
>
> Windows builds need **NASM** + **Perl** for the vendored OpenSSL that both the desktop and the
> backend link.

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
