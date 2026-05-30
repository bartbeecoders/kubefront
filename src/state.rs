use serde::{Deserialize, Serialize};

/// Serializable application state persisted by eframe.
#[derive(Default, Serialize, Deserialize)]
pub struct AppState {
    /// Last successfully loaded kubeconfig path (absolute).
    pub last_kubeconfig_path: Option<String>,

    /// Last active context name within that config.
    pub last_context: Option<String>,

    /// UI preferences (expanded in later phases).
    pub show_right_panel: bool,
    pub auto_refresh_secs: u64,
}

impl AppState {
    pub const STORAGE_KEY: &'static str = "kube_front_app_state";
}