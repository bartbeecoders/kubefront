# KubeFront

**A modern, pure-Rust desktop frontend for K3S and Kubernetes clusters.**

Lightweight, cross-platform (Linux + Windows), 100% Rust, uses your existing `kubectl` kubeconfig. Built with egui + kube-rs.

![KubeFront screenshot placeholder](assets/screenshot.png)

## Features (MVP)

- Load any kubectl-compatible kubeconfig (default, custom file, or the classic K3S path).
- Context switcher with K3S detection badges.
- Live Pods table (filter, namespace, status colors, age).
- Nodes table.
- Click any pod → live log streaming window (multiple windows supported, follow mode).
- Clean dark theme tuned for ops work.
- Persists last used kubeconfig + context.

## Why KubeFront?

- **Pure Rust** — no Electron, no bundled Chromium, no Node toolchain.
- **Tiny & fast** on small K3S clusters (the common case).
- **Respects your kubeconfig** — works with k3d, k3s, rancher-desktop, minikube, EKS, GKE, AKS, etc.
- Native feel on both Linux (Wayland/X11) and Windows.

## Requirements

- Rust 1.80+ (2021 edition)
- A working kubeconfig (or access to one)

## Quick Start

```bash
git clone https://github.com/your-org/kube-front
cd kube-front

cargo run --release
```

On first launch it will try `~/.kube/config` (or `KUBECONFIG`).

Use the top bar buttons to load a different file or the common K3S location (`/etc/rancher/k3s/k3s.yaml` — you may need to `sudo cp` it into your home or run with appropriate permissions).

## Development

```bash
cargo fmt
cargo clippy -- -D warnings
cargo run
RUST_LOG=debug cargo run     # verbose logs
```

See [AGENTS.md](./AGENTS.md) for architecture rules (especially the async channel contract).

## Roadmap

See the detailed implementation plan (internal) for phased delivery.

Post-MVP ideas:
- Deployments / Services / Events
- Multi-cluster concurrent view
- Port-forward UI + embedded exec
- Write actions (scale, delete, restart) with confirmations
- Better CRD + custom resource support

## License

MIT or Apache-2.0 (your choice).

## Acknowledgments

- [kube-rs](https://kube.rs) — the excellent Rust Kubernetes client
- [egui](https://github.com/emilk/egui) — immediate mode GUI that makes native tools fun again
- The many Rust K8s desktop projects (Kubeli, FerrisScope, Kubezilla, etc.) that proved this shape works

Built for K3S users who want a fast, local, no-nonsense desktop experience.
