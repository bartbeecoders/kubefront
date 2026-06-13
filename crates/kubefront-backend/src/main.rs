//! `kubefront-backend` — a headless REST server that fronts multiple Kubernetes
//! clusters for the KubeFront desktop app, behind a reverse proxy on :443.
//!
//! It performs NO authentication of its own: it TRUSTS THE REVERSE PROXY (which
//! terminates TLS + authenticates). Bind it only to a proxy-facing interface
//! (default loopback); a non-loopback bind logs a loud warning.

mod config;
mod error;
mod pool;
mod routes;
mod sse;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;

use config::BackendConfig;
use pool::ConnectionPool;

#[derive(Parser)]
#[command(name = "kubefront-backend", version, about)]
struct Args {
    /// Path to the TOML configuration file.
    #[arg(long, default_value = "backend.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let args = Args::parse();
    let cfg = BackendConfig::load(&args.config)
        .with_context(|| format!("loading config {}", args.config.display()))?;

    warn_if_public(&cfg.listen);

    let listen = cfg.listen.clone();
    let base_path = cfg.base_path.clone();
    let n = cfg.connections.len();
    for c in &cfg.connections {
        let ro = if c.read_only { " [read-only]" } else { "" };
        tracing::info!(
            "  connection '{}' ({}) → context '{}' of {}{ro}",
            c.id,
            c.name,
            c.context,
            c.kubeconfig.display()
        );
    }
    let pool = Arc::new(ConnectionPool::from_config(&cfg));
    let app = routes::router(pool, &base_path);

    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .with_context(|| format!("failed to bind {listen}"))?;
    tracing::info!(
        "kubefront-backend listening on {listen} (base_path={base_path}); {n} connection(s)"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("kubefront_backend=info,kubefront_core=info,tower_http=info,kube=warn")
    });
    tracing_subscriber::registry()
        .with(filter)
        // No ANSI: this is a headless server whose stdout is often a Windows
        // service console / redirected file that doesn't interpret color escapes
        // (they'd show up as literal `\x1b[2m…` garbage).
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();
}

/// Warn if bound to a non-loopback address — this backend has no auth of its own.
fn warn_if_public(listen: &str) {
    let is_loopback = if let Ok(addr) = listen.parse::<std::net::SocketAddr>() {
        addr.ip().is_loopback()
    } else {
        let host = listen.rsplit_once(':').map(|(h, _)| h).unwrap_or(listen);
        host == "127.0.0.1" || host == "localhost" || host == "::1"
    };
    if !is_loopback {
        tracing::warn!(
            "Binding to non-loopback address '{listen}'. This backend performs NO authentication \
             and TRUSTS the reverse proxy — make sure ONLY the proxy can reach it."
        );
    }
}

/// Resolve on Ctrl-C or SIGTERM for graceful shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut s) = signal(SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutdown signal received; draining");
}
