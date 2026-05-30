use eframe::{egui, NativeOptions};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod app;
mod k8s;
mod state;
mod ui;

use app::KubeFrontApp;

fn main() -> eframe::Result {
    // Initialize structured logging (use RUST_LOG=debug,kube=info etc.)
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kube_front=info,kube=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting KubeFront");

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("KubeFront — K3S & Kubernetes Desktop")
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "KubeFront",
        options,
        Box::new(|cc| {
            // Enable persistence
            cc.egui_ctx.set_theme(egui::Theme::Dark);

            // Apply polished KubeFront theme
            crate::ui::theme::apply_kube_theme(&cc.egui_ctx);

            Ok(Box::new(KubeFrontApp::new(cc)))
        }),
    )
}

/// Load window icon (fallback to None if missing)
fn load_icon() -> egui::IconData {
    // TODO(Phase 4): embed real icon from assets/icon.png
    egui::IconData {
        rgba: vec![],
        width: 0,
        height: 0,
    }
}
