//! KubeFront — Tauri application entry point (library half).
//!
//! Owns app bootstrap: logging, the rustls crypto provider, initial kubeconfig
//! discovery, managed backend state, and command registration. The actual UI is
//! the React/Vite frontend rendered in the native WebView.

mod commands;
mod k8s;
mod state;

use commands::Backend;
use k8s::manager::KubeConfigManager;
use state::AppState;
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Build the initial backend state: load settings + discover a kubeconfig so the
/// frontend has contexts to show on first paint. Mirrors the old egui startup.
fn build_initial_backend() -> Backend {
    let mut settings = AppState::load_from_disk();

    let mut manager = KubeConfigManager::new();

    // Initial load priority (multi-kubeconfig aware):
    // 1. active_kubeconfig_path  2. kubeconfig_path  3. last_kubeconfig_path  4. default discovery
    let preferred = settings
        .active_kubeconfig_path
        .clone()
        .or_else(|| settings.kubeconfig_path.clone())
        .or_else(|| settings.last_kubeconfig_path.clone());

    match &preferred {
        Some(p) => {
            if let Err(e) = manager.load_from_path(p) {
                tracing::error!("Configured kubeconfig load failed ({p}): {e}; falling back");
                let _ = manager.load_default();
            } else {
                tracing::info!("Loaded kubeconfig from settings path: {p}");
            }
        }
        None => match manager.load_default() {
            Ok(_) => tracing::info!(
                "Loaded default kubeconfig with {} context(s)",
                manager.contexts.len()
            ),
            Err(e) => tracing::error!("Initial default kubeconfig load failed: {e}"),
        },
    }

    // Restore last used context if it still exists.
    if let Some(last_ctx) = &settings.last_context {
        if manager.contexts.iter().any(|c| &c.name == last_ctx) {
            manager.current_context = Some(last_ctx.clone());
        }
    }

    // Ensure the active/preferred kubeconfig is registered for the manager UI.
    if let Some(p) = settings.effective_kubeconfig_path() {
        settings.register_kubeconfig(p.to_string());
    }

    Backend {
        manager,
        settings,
        ..Default::default()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // WebKitGTK renders a black/blank screen on some Linux GPU + driver combos
    // (notably NVIDIA, and Wayland with the DMABUF renderer). Disabling the
    // DMABUF renderer is the standard, well-known fix. Must be set before the
    // WebView is created. Users can still override it via the environment.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kube_front=info,kube=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting KubeFront (Tauri)");

    // rustls 0.23+ requires an explicit default CryptoProvider before the first
    // TLS connection (kube + rustls-tls + ring). Install it once, up front.
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider())
            .expect("Failed to install rustls ring CryptoProvider (ring feature missing?)");
    }

    let backend = build_initial_backend();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(backend))
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::resolved_accent,
            commands::color_schemes,
            commands::remove_kubeconfig,
            commands::load_kubeconfig,
            commands::set_context,
            commands::get_status,
            commands::connect,
            commands::switch_kubeconfig,
            commands::list_pods,
            commands::list_nodes,
            commands::list_resource,
            commands::get_resource,
            commands::stream_logs,
            commands::stop_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running KubeFront");
}
