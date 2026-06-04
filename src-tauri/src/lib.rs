//! KubeFront — Tauri application entry point (library half).
//!
//! Owns app bootstrap: logging, initial kubeconfig discovery, managed backend
//! state, and command registration. The actual UI is the React/Vite frontend
//! rendered in the native WebView. TLS is handled by OpenSSL (see Cargo.toml).

mod commands;
mod k8s;
mod state;

use commands::Backend;
use k8s::manager::KubeConfigManager;
use state::AppState;
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Directory where the debug log file lives (created if missing).
fn log_dir() -> Option<std::path::PathBuf> {
    let proj = directories::ProjectDirs::from("dev", "kube-front", "KubeFront")?;
    let dir = proj.data_local_dir().join("logs");
    std::fs::create_dir_all(&dir).ok();
    Some(dir)
}

/// Absolute path to the rolling debug log file.
pub fn log_file_path() -> Option<std::path::PathBuf> {
    log_dir().map(|d| d.join("kubefront.log"))
}

/// Initialize logging to both the console (dev) and a file (every platform).
/// The file is the reliable place to read logs — the Windows release build has
/// no console. The default level comes from the persisted `log_level` setting,
/// and `RUST_LOG` (if set) overrides everything.
fn init_logging(level: &str) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let level = match level {
        "trace" | "debug" | "info" | "warn" | "error" => level,
        _ => "info",
    };
    let make_filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            tracing_subscriber::EnvFilter::new(format!("kube_front={level},kube=warn"))
        })
    };

    match log_dir() {
        Some(dir) => {
            let appender = tracing_appender::rolling::never(&dir, "kubefront.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(appender);
            tracing_subscriber::registry()
                .with(make_filter())
                .with(tracing_subscriber::fmt::layer())
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .with_writer(non_blocking),
                )
                .init();
            Some(guard)
        }
        None => {
            tracing_subscriber::registry()
                .with(make_filter())
                .with(tracing_subscriber::fmt::layer())
                .init();
            None
        }
    }
}

/// Build the initial backend state: load settings + discover a kubeconfig so the
/// frontend has contexts to show on first paint. Mirrors the old egui startup.
fn build_initial_backend(mut settings: AppState) -> Backend {
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

    // Load settings early so the log level (from Settings) applies to startup too.
    let settings = AppState::load_from_disk();

    // Keep the appender guard alive for the whole program so buffered logs flush.
    let _log_guard = init_logging(&settings.log_level);

    tracing::info!("Starting KubeFront (Tauri)");
    if let Some(p) = log_file_path() {
        tracing::info!("Writing logs to {}", p.display());
    }

    let backend = build_initial_backend(settings);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(backend))
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::resolved_accent,
            commands::color_schemes,
            commands::log_path,
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
