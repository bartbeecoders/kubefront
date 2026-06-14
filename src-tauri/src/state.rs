use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Supported theme modes for the settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    Custom,
}

/// Named color scheme presets available in Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorScheme {
    #[default]
    Default,
    K3sPurple,
    KubernetesBlue,
    Emerald,
    Amber,
    Cyan,
    Rose,
    Slate,
}

impl ColorScheme {
    /// Returns a human-friendly name for the preset.
    pub fn label(&self) -> &'static str {
        match self {
            ColorScheme::Default => "Default",
            ColorScheme::K3sPurple => "K3S Purple",
            ColorScheme::KubernetesBlue => "Kubernetes Blue",
            ColorScheme::Emerald => "Emerald",
            ColorScheme::Amber => "Amber",
            ColorScheme::Cyan => "Cyan",
            ColorScheme::Rose => "Rose",
            ColorScheme::Slate => "Slate",
        }
    }

    /// Returns the accent color for this preset as an `[r, g, b]` triple.
    pub fn accent(&self) -> [u8; 3] {
        match self {
            ColorScheme::Default => [50, 108, 229],   // K8S blue
            ColorScheme::K3sPurple => [139, 92, 246], // K3S purple
            ColorScheme::KubernetesBlue => [50, 102, 229],
            ColorScheme::Emerald => [16, 185, 129],
            ColorScheme::Amber => [245, 158, 11],
            ColorScheme::Cyan => [6, 182, 212],
            ColorScheme::Rose => [244, 63, 94],
            ColorScheme::Slate => [100, 116, 139],
        }
    }

    /// Returns the accent color for this preset as a CSS hex string (e.g. `#326ce5`).
    pub fn accent_hex(&self) -> String {
        let [r, g, b] = self.accent();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// All available presets in display order.
    pub const ALL: &[ColorScheme] = &[
        ColorScheme::Default,
        ColorScheme::K3sPurple,
        ColorScheme::KubernetesBlue,
        ColorScheme::Emerald,
        ColorScheme::Amber,
        ColorScheme::Cyan,
        ColorScheme::Rose,
        ColorScheme::Slate,
    ];
}

/// How a connection reaches its cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnMode {
    /// Local `kube::Client` built from a kubeconfig (today's behavior, port 6443).
    #[default]
    Direct,
    /// HTTP to a `kubefront-backend` behind a reverse proxy (port 443).
    Remote,
}

/// The orchestrator a cluster runs on. User-declared metadata (distinct from the
/// auto-detected K3S heuristic, which is per-context). `Option` on the entry means
/// "unspecified", so no default variant is needed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterType {
    K3s,
    K8s,
    Aks,
}

/// The deployment environment a connection targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Environment {
    Dev,
    Val,
    Prod,
}

/// Represents one registered connection — either a local kubeconfig (Direct) or a
/// remote backend endpoint (Remote). Old settings.json files deserialize as Direct
/// because every new field has a `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubeconfigEntry {
    /// Stable identifier (the kubeconfig path for Direct, the endpoint for Remote).
    pub id: String,

    /// Absolute path to the kubeconfig file (Direct). For Remote this mirrors the
    /// endpoint and is not a real file path.
    pub path: String,

    /// Friendly name shown in the UI (e.g. "Production K3S", "Staging EKS").
    pub name: String,

    /// Optional longer description.
    #[serde(default)]
    pub description: Option<String>,

    /// Last context the user used in this specific kubeconfig (Direct only).
    #[serde(default)]
    pub last_context: Option<String>,

    /// Namespace to scope resource lists to while this connection is active.
    /// None/empty falls back to the global `default_namespace`. Seeded from the
    /// kubeconfig context (Direct) or the backend's reported namespace (Remote).
    #[serde(default)]
    pub namespace: Option<String>,

    /// Direct (local kubeconfig) or Remote (backend endpoint).
    #[serde(default)]
    pub mode: ConnMode,

    /// Base URL of the backend connection (Remote only), e.g.
    /// `https://host/k3s-server1/connection1`.
    #[serde(default)]
    pub endpoint: Option<String>,

    /// Optional path to a PEM CA bundle to trust for the Remote endpoint (for OT
    /// reverse proxies with an internal CA).
    #[serde(default)]
    pub ca_path: Option<String>,

    /// Skip TLS verification for the Remote endpoint (self-signed proxy). Unsafe.
    #[serde(default)]
    pub insecure: bool,

    // === World view / inventory metadata (all optional, added later) ===
    /// City this cluster lives in (free text, e.g. "Visp"). Used as a map label.
    #[serde(default)]
    pub city: Option<String>,

    /// Country this cluster lives in (free text, e.g. "Switzerland"). Drives the
    /// world-map position when explicit coordinates are absent (frontend geocodes
    /// the country name to a centroid).
    #[serde(default)]
    pub country: Option<String>,

    /// Optional explicit latitude (decimal degrees). Overrides the country centroid
    /// so a plant can be pinned precisely on the dashboard map.
    #[serde(default)]
    pub latitude: Option<f64>,

    /// Optional explicit longitude (decimal degrees). See `latitude`.
    #[serde(default)]
    pub longitude: Option<f64>,

    /// Orchestrator type (K3S / K8S / AKS). User-declared.
    #[serde(default)]
    pub cluster_type: Option<ClusterType>,

    /// Manufacturing plant this cluster belongs to (free text).
    #[serde(default)]
    pub plant: Option<String>,

    /// Deployment environment (dev / val / prod).
    #[serde(default)]
    pub environment: Option<Environment>,
}

impl KubeconfigEntry {
    /// A Direct (local kubeconfig) entry.
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unnamed".to_string());

        Self {
            id: path.clone(),
            path,
            name,
            description: None,
            last_context: None,
            namespace: None,
            mode: ConnMode::Direct,
            endpoint: None,
            ca_path: None,
            insecure: false,
            city: None,
            country: None,
            latitude: None,
            longitude: None,
            cluster_type: None,
            plant: None,
            environment: None,
        }
    }

    /// A Remote (backend endpoint) entry. Identified by its endpoint URL.
    pub fn new_remote(
        name: impl Into<String>,
        endpoint: impl Into<String>,
        ca_path: Option<String>,
        insecure: bool,
    ) -> Self {
        let endpoint = endpoint.into();
        Self {
            id: endpoint.clone(),
            path: endpoint.clone(),
            name: name.into(),
            description: None,
            last_context: None,
            namespace: None,
            mode: ConnMode::Remote,
            endpoint: Some(endpoint),
            ca_path,
            insecure,
            city: None,
            country: None,
            latitude: None,
            longitude: None,
            cluster_type: None,
            plant: None,
            environment: None,
        }
    }
}

/// Editable fields for [`AppState::update_connection`], mirrored 1:1 by the
/// `ConnectionPatch` TS type and the `update_connection` IPC command. Collected
/// into a struct so the metadata can grow without an ever-longer argument list.
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionPatch {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    /// Remote-only; ignored for Direct entries.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Remote-only.
    #[serde(default)]
    pub ca_path: Option<String>,
    /// Remote-only.
    #[serde(default)]
    pub insecure: bool,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub cluster_type: Option<ClusterType>,
    #[serde(default)]
    pub plant: Option<String>,
    #[serde(default)]
    pub environment: Option<Environment>,
}

/// Serializable application state persisted to `settings.json`.
/// New fields have `#[serde(default)]` so old saved state still loads.
#[derive(Clone, Serialize, Deserialize)]
pub struct AppState {
    /// All registered kubeconfigs with friendly names and metadata.
    /// This is the source of truth for multi-kubeconfig management.
    #[serde(default)]
    pub kubeconfigs: Vec<KubeconfigEntry>,

    /// ID of the currently active kubeconfig entry.
    #[serde(default)]
    pub active_kubeconfig_id: Option<String>,

    // === Legacy fields (kept for migration) ===
    #[serde(default)]
    pub registered_kubeconfigs: Vec<String>, // old format
    #[serde(default)]
    pub active_kubeconfig_path: Option<String>,
    #[serde(default)]
    pub kubeconfig_path: Option<String>, // old preferred path

    /// Default namespace for resource views (empty or "All" means cluster-wide / all namespaces).
    #[serde(default = "default_namespace")]
    pub default_namespace: String,

    /// UI theme preference.
    #[serde(default)]
    pub theme_mode: ThemeMode,

    /// Base font scale (1.0 = normal). Applied to text styles.
    #[serde(default = "default_font_scale")]
    pub font_scale: f32,

    /// Desired log level (persisted; takes effect on next launch unless RUST_LOG is set).
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Custom accent color (RGB) used when theme_mode == Custom.
    /// This can be set either manually or via a ColorScheme preset.
    #[serde(default)]
    pub custom_accent: Option<[u8; 3]>,

    /// Currently selected color scheme preset (shown in Settings).
    #[serde(default)]
    pub color_scheme: ColorScheme,

    // Legacy / other prefs
    /// Last successfully loaded kubeconfig path (absolute). Kept for backward compat.
    #[serde(default)]
    pub last_kubeconfig_path: Option<String>,

    /// Last active context name within that config.
    #[serde(default)]
    pub last_context: Option<String>,

    /// UI preferences.
    #[serde(default)]
    pub show_right_panel: bool,
    #[serde(default = "default_auto_refresh")]
    pub auto_refresh_secs: u64,
}

fn default_namespace() -> String {
    "All".to_string()
}
fn default_font_scale() -> f32 {
    1.0
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_auto_refresh() -> u64 {
    5
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            kubeconfigs: vec![],
            active_kubeconfig_id: None,
            // legacy
            registered_kubeconfigs: vec![],
            active_kubeconfig_path: None,
            kubeconfig_path: None,
            default_namespace: "All".to_string(),
            theme_mode: ThemeMode::Dark,
            font_scale: 1.0,
            log_level: "info".to_string(),
            custom_accent: None,
            color_scheme: ColorScheme::Default,
            last_kubeconfig_path: None,
            last_context: None,
            show_right_panel: true,
            auto_refresh_secs: 5,
        }
    }
}

impl AppState {
    /// Returns the path to `settings.json`.
    pub fn settings_path() -> Option<PathBuf> {
        let proj_dirs = directories::ProjectDirs::from("dev", "kube-front", "KubeFront")?;
        let config_dir = proj_dirs.config_dir();
        std::fs::create_dir_all(config_dir).ok();
        Some(config_dir.join("settings.json"))
    }

    /// Load settings from `settings.json`. Falls back to default if file doesn't exist or is invalid.
    pub fn load_from_disk() -> Self {
        if let Some(path) = Self::settings_path() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(mut loaded) = serde_json::from_str::<Self>(&contents) {
                    // Run migration from old format
                    loaded.migrate_from_legacy();
                    return loaded;
                }
            }
        }
        let mut default = Self::default();
        default.migrate_from_legacy();
        default
    }

    /// Save current settings to `settings.json`.
    pub fn save_to_disk(&self) {
        if let Some(path) = Self::settings_path() {
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    /// Migrate old `registered_kubeconfigs: Vec<String>` into the new `kubeconfigs` structure.
    pub(crate) fn migrate_from_legacy(&mut self) {
        if !self.kubeconfigs.is_empty() {
            return; // already migrated
        }

        let old_paths: Vec<String> = if !self.registered_kubeconfigs.is_empty() {
            self.registered_kubeconfigs.clone()
        } else if let Some(p) = &self.active_kubeconfig_path {
            vec![p.clone()]
        } else if let Some(p) = &self.kubeconfig_path {
            vec![p.clone()]
        } else if let Some(p) = &self.last_kubeconfig_path {
            vec![p.clone()]
        } else {
            vec![]
        };

        for path in old_paths {
            let entry = KubeconfigEntry::new(path);
            self.kubeconfigs.push(entry);
        }

        // Try to set active from legacy fields
        if self.active_kubeconfig_id.is_none() {
            if let Some(p) = &self.active_kubeconfig_path {
                if let Some(entry) = self.kubeconfigs.iter().find(|k| &k.path == p) {
                    self.active_kubeconfig_id = Some(entry.id.clone());
                }
            }
        }
    }

    /// Returns the accent color that should currently be used for the theme,
    /// as an `[r, g, b]` triple. Prefers an explicit `custom_accent` if set,
    /// otherwise falls back to the selected `color_scheme` preset.
    pub fn resolved_accent(&self) -> [u8; 3] {
        self.custom_accent
            .unwrap_or_else(|| self.color_scheme.accent())
    }

    /// The resolved accent as a CSS hex string (e.g. `#326ce5`).
    pub fn resolved_accent_hex(&self) -> String {
        let [r, g, b] = self.resolved_accent();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// Returns the currently active KubeconfigEntry, if any.
    pub fn active_kubeconfig(&self) -> Option<&KubeconfigEntry> {
        let id = self.active_kubeconfig_id.as_ref()?;
        self.kubeconfigs.iter().find(|k| &k.id == id)
    }

    /// Returns the path of the active kubeconfig (convenience).
    pub fn active_kubeconfig_path(&self) -> Option<&str> {
        self.active_kubeconfig().map(|k| k.path.as_str())
    }

    /// Register a new kubeconfig (or update if path already exists).
    /// Returns the ID of the entry.
    pub fn register_kubeconfig(&mut self, path: String) -> String {
        // Normalize path a bit
        let path = path.trim().to_string();

        if let Some(existing) = self.kubeconfigs.iter_mut().find(|k| k.path == path) {
            return existing.id.clone();
        }

        let entry = KubeconfigEntry::new(path);
        let id = entry.id.clone();
        self.kubeconfigs.push(entry);
        id
    }

    /// Register a remote backend connection (or update its metadata if the
    /// endpoint already exists). Returns the entry id.
    pub fn add_remote(
        &mut self,
        name: String,
        endpoint: String,
        ca_path: Option<String>,
        insecure: bool,
    ) -> String {
        let endpoint = endpoint.trim().trim_end_matches('/').to_string();
        if let Some(existing) = self
            .kubeconfigs
            .iter_mut()
            .find(|k| k.endpoint.as_deref() == Some(endpoint.as_str()))
        {
            existing.name = name;
            existing.ca_path = ca_path;
            existing.insecure = insecure;
            return existing.id.clone();
        }
        let entry = KubeconfigEntry::new_remote(name, endpoint, ca_path, insecure);
        let id = entry.id.clone();
        self.kubeconfigs.push(entry);
        id
    }

    /// Update an existing connection's editable fields in place, keeping its `id`
    /// (and thus `active_kubeconfig_id`) stable. `endpoint`/`ca_path`/`insecure`
    /// only apply to Remote entries; for Direct entries the path is immutable
    /// (pick a different file via "Add kubeconfig" instead). World-view metadata
    /// (location, cluster type, plant, environment) applies to both. Returns false
    /// if no entry with `id` exists.
    pub fn update_connection(&mut self, id: &str, patch: ConnectionPatch) -> bool {
        let Some(entry) = self.kubeconfigs.iter_mut().find(|k| k.id == id) else {
            return false;
        };
        entry.name = patch.name;
        entry.description = patch.description;
        entry.namespace = patch.namespace;
        entry.city = patch.city;
        entry.country = patch.country;
        entry.latitude = patch.latitude;
        entry.longitude = patch.longitude;
        entry.cluster_type = patch.cluster_type;
        entry.plant = patch.plant;
        entry.environment = patch.environment;
        if entry.mode == ConnMode::Remote {
            if let Some(ep) = patch.endpoint {
                let ep = ep.trim().trim_end_matches('/').to_string();
                if !ep.is_empty() {
                    entry.endpoint = Some(ep.clone());
                    // Mirror into `path` so the list's "Path / Endpoint" column and
                    // any path-based display stay in sync. `id` is untouched.
                    entry.path = ep;
                }
            }
            entry.ca_path = patch.ca_path;
            entry.insecure = patch.insecure;
        }
        true
    }

    /// Remove a connection (Direct or Remote) by ID.
    pub fn unregister_kubeconfig_by_id(&mut self, id: &str) {
        self.kubeconfigs.retain(|k| k.id != id);

        if self.active_kubeconfig_id.as_deref() == Some(id) {
            self.active_kubeconfig_id = None;
        }
    }

    /// Set the active kubeconfig by ID.
    pub fn set_active_kubeconfig(&mut self, id: &str) {
        if self.kubeconfigs.iter().any(|k| k.id == id) {
            self.active_kubeconfig_id = Some(id.to_string());
        }
    }

    // === Legacy support (for migration) ===
    pub fn effective_kubeconfig_path(&self) -> Option<&str> {
        self.active_kubeconfig_path()
            .or(self.kubeconfig_path.as_deref())
            .or(self.last_kubeconfig_path.as_deref())
    }
}
