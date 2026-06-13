//! Tauri command surface — the bridge between the React frontend and the
//! Kubernetes backend. The heavy lifting now lives in the shared `kubefront_core`
//! crate ([`LocalKube`] + projections); this module orchestrates: it holds the
//! managed [`Backend`] state, builds connections, and adapts core results into
//! the `Result<T, String>` shape the frontend expects.
//!
//! Design notes:
//! - All shared state lives in [`Backend`], guarded by a Tokio `Mutex` and
//!   registered as Tauri managed state.
//! - Resource lists are *pulled* on demand by the frontend (polling). Each list
//!   command returns a small serializable projection, never raw k8s objects.
//! - Live pod logs are *pushed* through a Tauri [`Channel`], fed by the core's
//!   transport-agnostic `log_stream`.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use kube::config::{KubeConfigOptions, Kubeconfig};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::{oneshot, Mutex};

use kubefront_core::{
    normalize_scope, summarize, ClusterSummary, ContextInfo, KubeConfigManager, KubeStatus,
    LocalKube, LogEvent, NodeRow, PodRow, ResourceDetail, TableData,
};

use crate::conn::Active;
use crate::remote::RemoteKube;
use crate::state::{AppState, ColorScheme, ConnMode};

/// All mutable application state shared across commands.
#[derive(Default)]
pub struct Backend {
    pub manager: KubeConfigManager,
    /// The active connection — Local (direct kube client) or Remote (HTTP backend).
    pub active: Option<Active>,
    pub settings: AppState,
    /// Active log streams → cancellation sender. Dropping/sending stops the stream.
    pub log_streams: HashMap<u64, oneshot::Sender<()>>,
    pub next_stream_id: u64,
}

pub type SharedBackend = Mutex<Backend>;

impl Backend {
    fn status(&self, error: Option<String>) -> KubeStatus {
        match &self.active {
            // Remote: the manager (local kubeconfig contexts) is irrelevant; report
            // the endpoint + the friendly name and an empty context list. The
            // frontend treats `connected && contexts == []` as a remote connection.
            Some(active @ Active::Remote(_)) => KubeStatus {
                connected: true,
                cluster_version: Some(active.cluster_version()),
                current_context: self.active_name(),
                kubeconfig_path: active.endpoint().map(|s| s.to_string()),
                context_count: 0,
                contexts: vec![],
                error,
            },
            other => KubeStatus {
                connected: other.is_some(),
                cluster_version: other.as_ref().map(|a| a.cluster_version()),
                current_context: self.manager.current_context.clone(),
                kubeconfig_path: self.manager.path.as_ref().map(|p| p.display().to_string()),
                context_count: self.manager.contexts.len(),
                contexts: self.manager.contexts.clone(),
                error,
            },
        }
    }

    /// Friendly name of the active connection entry (shown for remote connections).
    fn active_name(&self) -> Option<String> {
        self.settings.active_kubeconfig().map(|e| e.name.clone())
    }

    /// Clear the active connection (e.g. after changing context/kubeconfig).
    fn disconnect(&mut self) {
        self.active = None;
    }
}

// ============================================================================
// Settings  (LOCAL-only — never touch the cluster/backend)
// ============================================================================

#[tauri::command]
pub async fn get_settings(state: State<'_, SharedBackend>) -> Result<AppState, String> {
    Ok(state.lock().await.settings.clone())
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, SharedBackend>,
    settings: AppState,
) -> Result<(), String> {
    let mut b = state.lock().await;
    settings.save_to_disk();
    b.settings = settings;
    Ok(())
}

/// The resolved accent color (hex, e.g. `#326ce5`) for the current settings.
#[tauri::command]
pub async fn resolved_accent(state: State<'_, SharedBackend>) -> Result<String, String> {
    Ok(state.lock().await.settings.resolved_accent_hex())
}

#[derive(Serialize)]
pub struct ColorSchemeInfo {
    pub key: String,
    pub label: String,
    pub hex: String,
}

/// Absolute path to the debug log file (shown in Settings so users can find it).
#[tauri::command]
pub fn log_path() -> Option<String> {
    crate::log_file_path().map(|p| p.display().to_string())
}

/// All selectable color-scheme presets (key, label, hex) for the Settings swatches.
#[tauri::command]
pub fn color_schemes() -> Vec<ColorSchemeInfo> {
    ColorScheme::ALL
        .iter()
        .map(|s| ColorSchemeInfo {
            key: format!("{s:?}"),
            label: s.label().to_string(),
            hex: s.accent_hex(),
        })
        .collect()
}

/// Remove a registered kubeconfig entry by id and persist settings.
#[tauri::command]
pub async fn remove_kubeconfig(
    state: State<'_, SharedBackend>,
    id: String,
) -> Result<AppState, String> {
    let mut b = state.lock().await;
    b.settings.unregister_kubeconfig_by_id(&id);
    b.settings.save_to_disk();
    Ok(b.settings.clone())
}

// ============================================================================
// Kubeconfig & connection  (DIRECT-only)
// ============================================================================

/// Load a kubeconfig (None = default discovery) into the manager. Does not connect.
#[tauri::command]
pub async fn load_kubeconfig(
    state: State<'_, SharedBackend>,
    path: Option<String>,
) -> Result<KubeStatus, String> {
    let mut b = state.lock().await;
    let res = match &path {
        Some(p) => b.manager.load_from_path(p),
        None => b.manager.load_default(),
    };
    match res {
        Ok(_) => {
            b.disconnect();
            Ok(b.status(None))
        }
        Err(e) => Ok(b.status(Some(e.to_string()))),
    }
}

/// Select the active context within the loaded kubeconfig.
#[tauri::command]
pub async fn set_context(
    state: State<'_, SharedBackend>,
    name: String,
) -> Result<KubeStatus, String> {
    let mut b = state.lock().await;
    if b.manager.contexts.iter().any(|c| c.name == name) {
        b.manager.current_context = Some(name);
        b.disconnect();
    }
    Ok(b.status(None))
}

/// Return the current connection / kubeconfig status without changing anything.
#[tauri::command]
pub async fn get_status(state: State<'_, SharedBackend>) -> Result<KubeStatus, String> {
    Ok(state.lock().await.status(None))
}

/// (Re)connect the active connection — dispatches by the active entry's mode.
#[tauri::command]
pub async fn connect(state: State<'_, SharedBackend>) -> Result<KubeStatus, String> {
    let mode = {
        let b = state.lock().await;
        b.settings
            .active_kubeconfig()
            .map(|e| (e.mode, e.id.clone()))
    };
    match mode {
        Some((ConnMode::Remote, id)) => connect_remote(state, id).await,
        _ => connect_direct(state).await,
    }
}

/// Create a live `kube::Client` for the current context and probe its version.
async fn connect_direct(state: State<'_, SharedBackend>) -> Result<KubeStatus, String> {
    // Extract everything we need, then drop the lock before the (slow) network work.
    let (kubeconfig, opts, ctx_name, server) = {
        let b = state.lock().await;
        let Some(opts) = b.manager.current_kubeconfig_options() else {
            return Ok(b.status(Some("No context selected in kubeconfig".into())));
        };
        // Use the kubeconfig we actually loaded from the user's chosen path — NOT the
        // default (~/.kube/config), so a context that only exists in the selected
        // file still connects.
        let Some(kubeconfig) = b.manager.kubeconfig.clone() else {
            return Ok(b.status(Some("No kubeconfig loaded".into())));
        };
        let ctx_name = b
            .manager
            .effective_context()
            .unwrap_or("unknown")
            .to_string();
        let server = b
            .manager
            .current_info()
            .map(|i| i.server.clone())
            .unwrap_or_default();
        (kubeconfig, opts, ctx_name, server)
    };

    tracing::info!("Connecting to context '{ctx_name}' → {server}");

    // One 15s timeout around the WHOLE thing — including the network probe (see
    // LocalKube::connect_from). Timeout vs. error is distinguished so the UI can
    // give the right hint.
    match LocalKube::connect_from(kubeconfig, opts, Duration::from_secs(15)).await {
        Ok(local) => {
            let version = local.cluster_version().to_string();
            let mut b = state.lock().await;
            b.active = Some(Active::Local(local));
            tracing::info!("Connected to '{ctx_name}' ({server}); cluster version {version}");

            // Persist the successful connection into settings.
            let path = b.manager.path.as_ref().map(|p| p.display().to_string());
            let ctx_ns = b.manager.current_context_namespace();
            b.settings.last_kubeconfig_path = path.clone();
            b.settings.last_context = b.manager.current_context.clone();
            if let (Some(path), Some(ctx)) = (&path, &b.manager.current_context) {
                let (path, ctx) = (path.clone(), ctx.clone());
                if let Some(entry) = b.settings.kubeconfigs.iter_mut().find(|k| k.path == path) {
                    entry.last_context = Some(ctx);
                    // First connect on this entry: adopt the namespace the kubeconfig's
                    // context declares, so namespace-scoped users work out of the box.
                    if entry.namespace.is_none() {
                        entry.namespace = ctx_ns.clone();
                    }
                }
            }
            b.settings.save_to_disk();
            Ok(b.status(None))
        }
        Err(kubefront_core::CoreError::Timeout(_)) => {
            tracing::error!("Connection to '{ctx_name}' ({server}) timed out after 15s");
            let mut b = state.lock().await;
            b.disconnect();
            Ok(b.status(Some(format!(
                "Connection to {server} timed out after 15s. Common causes: server unreachable from this machine, firewall, wrong network/VPN, or TLS verification failure. Try the same `kubectl` command from this exact terminal."
            ))))
        }
        Err(e) => {
            tracing::error!("Connection to '{ctx_name}' ({server}) failed: {e}");
            let mut b = state.lock().await;
            b.disconnect();
            Ok(b.status(Some(format!("Failed to connect to {server}: {e}"))))
        }
    }
}

/// Build a RemoteKube for the entry `id`, probe `GET /status`, make it active.
async fn connect_remote(state: State<'_, SharedBackend>, id: String) -> Result<KubeStatus, String> {
    let (endpoint, ca_path, insecure) = {
        let b = state.lock().await;
        let Some(entry) = b.settings.kubeconfigs.iter().find(|k| k.id == id) else {
            return Ok(b.status(Some(format!("Connection '{id}' not found"))));
        };
        let Some(endpoint) = entry.endpoint.clone() else {
            return Ok(b.status(Some("Remote connection has no endpoint".into())));
        };
        (endpoint, entry.ca_path.clone(), entry.insecure)
    };

    let ca_pem = match read_ca(&ca_path) {
        Ok(v) => v,
        Err(e) => {
            let mut b = state.lock().await;
            b.disconnect();
            return Ok(b.status(Some(e)));
        }
    };

    let mut remote = match RemoteKube::new(endpoint.clone(), ca_pem, insecure) {
        Ok(r) => r,
        Err(e) => {
            let mut b = state.lock().await;
            b.disconnect();
            return Ok(b.status(Some(e)));
        }
    };

    match remote.refresh_status().await {
        Ok(st) => {
            let mut b = state.lock().await;
            b.settings.set_active_kubeconfig(&id);
            // Seed the connection's namespace from the backend's configured scope, so
            // `effectiveNs` works for namespace-restricted backends (no cluster-wide 403).
            if let Some(entry) = b.settings.kubeconfigs.iter_mut().find(|k| k.id == id) {
                if entry.namespace.is_none() {
                    entry.namespace = st.namespace.clone();
                }
            }
            b.active = Some(Active::Remote(remote));
            b.settings.save_to_disk();
            tracing::info!(
                "Connected to remote '{endpoint}'; cluster version {}",
                st.cluster_version
            );
            Ok(b.status(None))
        }
        Err(e) => {
            tracing::error!("Remote connect to '{endpoint}' failed: {e}");
            let mut b = state.lock().await;
            b.disconnect();
            Ok(b.status(Some(format!("Failed to connect to {endpoint}: {e}"))))
        }
    }
}

/// Read an optional CA PEM bundle from disk for a remote connection.
fn read_ca(ca_path: &Option<String>) -> Result<Option<Vec<u8>>, String> {
    match ca_path {
        Some(p) if !p.trim().is_empty() => std::fs::read(p)
            .map(Some)
            .map_err(|e| format!("failed to read CA file {p}: {e}")),
        _ => Ok(None),
    }
}

/// Register (if needed), load, select last context for, and connect to a kubeconfig file.
#[tauri::command]
pub async fn switch_kubeconfig(
    state: State<'_, SharedBackend>,
    path: String,
) -> Result<KubeStatus, String> {
    {
        let mut b = state.lock().await;
        let id = b.settings.register_kubeconfig(path.clone());
        let last_ctx = b
            .settings
            .kubeconfigs
            .iter()
            .find(|k| k.id == id)
            .and_then(|k| k.last_context.clone());

        if let Err(e) = b.manager.load_from_path(&path) {
            return Ok(b.status(Some(format!("Failed to load kubeconfig {path}: {e}"))));
        }
        b.settings.set_active_kubeconfig(&id);
        b.settings.last_kubeconfig_path = Some(path.clone());

        if let Some(ctx_name) = last_ctx {
            if b.manager.contexts.iter().any(|c| c.name == ctx_name) {
                b.manager.current_context = Some(ctx_name);
            }
        }
        b.disconnect();
        b.settings.save_to_disk();
    }
    // Now connect with the freshly loaded config.
    connect(state).await
}

/// Register (if needed) + load a kubeconfig, select a SPECIFIC context, and connect.
/// Used by the Dashboard when the user clicks a cluster card.
#[tauri::command]
pub async fn open_cluster(
    state: State<'_, SharedBackend>,
    path: Option<String>,
    context: String,
) -> Result<KubeStatus, String> {
    {
        let mut b = state.lock().await;
        if let Some(p) = &path {
            let id = b.settings.register_kubeconfig(p.clone());
            if let Err(e) = b.manager.load_from_path(p) {
                return Ok(b.status(Some(format!("Failed to load kubeconfig {p}: {e}"))));
            }
            b.settings.set_active_kubeconfig(&id);
            b.settings.last_kubeconfig_path = Some(p.clone());
        } else if b.manager.kubeconfig.is_none() {
            if let Err(e) = b.manager.load_default() {
                return Ok(b.status(Some(format!("Failed to load default kubeconfig: {e}"))));
            }
        }
        if !b.manager.contexts.iter().any(|c| c.name == context) {
            return Ok(b.status(Some(format!(
                "Context '{context}' not found in the kubeconfig"
            ))));
        }
        b.manager.current_context = Some(context);
        b.disconnect();
        b.settings.save_to_disk();
    }
    connect(state).await
}

// ============================================================================
// Dashboard (cluster overview)  — DIRECT-only short-lived probes
// ============================================================================

/// List the contexts of an arbitrary kubeconfig file WITHOUT touching the active
/// connection. None = default discovery. Used by the Dashboard.
#[tauri::command]
pub async fn kubeconfig_contexts(path: Option<String>) -> Result<Vec<ContextInfo>, String> {
    let mut mgr = KubeConfigManager::new();
    match &path {
        Some(p) => mgr.load_from_path(p),
        None => mgr.load_default(),
    }
    .map_err(|e| e.to_string())?;
    Ok(mgr.contexts)
}

/// Live health snapshot for one cluster card on the Dashboard. Probes a
/// (kubeconfig, context) pair with a dedicated short-lived client — the active
/// connection is never disturbed. `namespace` scopes the pod/deployment counts.
#[tauri::command]
pub async fn cluster_summary(
    path: Option<String>,
    context: String,
    namespace: Option<String>,
) -> Result<ClusterSummary, String> {
    let kc = match &path {
        Some(p) => Kubeconfig::read_from(p),
        None => Kubeconfig::read(),
    };
    let kc = match kc {
        Ok(kc) => kc,
        Err(e) => {
            return Ok(ClusterSummary::unreachable(format!(
                "kubeconfig error: {e}"
            )))
        }
    };
    let opts = KubeConfigOptions {
        context: Some(context.clone()),
        cluster: None,
        user: None,
    };
    let scope = normalize_scope(namespace);

    let probe = async {
        let config = kube::Config::from_custom_kubeconfig(kc, &opts)
            .await
            .map_err(|e| format!("kubeconfig error: {e}"))?;
        let client =
            kube::Client::try_from(config).map_err(|e| format!("client build error: {e}"))?;
        Ok::<_, String>(summarize(client, scope).await)
    };

    match tokio::time::timeout(Duration::from_secs(10), probe).await {
        Ok(Ok(summary)) => Ok(summary),
        Ok(Err(e)) => Ok(ClusterSummary::unreachable(e)),
        Err(_) => Ok(ClusterSummary::unreachable("Timed out after 10s")),
    }
}

// ============================================================================
// Resource listing  (delegates to LocalKube)
// ============================================================================

#[tauri::command]
pub async fn list_pods(
    state: State<'_, SharedBackend>,
    namespace: Option<String>,
) -> Result<Vec<PodRow>, String> {
    let active = require_active(&state).await?;
    let scope = normalize_scope(namespace);
    active.list_pods(scope.as_deref()).await
}

#[tauri::command]
pub async fn list_nodes(state: State<'_, SharedBackend>) -> Result<Vec<NodeRow>, String> {
    let active = require_active(&state).await?;
    active.list_nodes().await
}

/// Generic list: returns a headers+rows table projection for the given `kind`.
#[tauri::command]
pub async fn list_resource(
    state: State<'_, SharedBackend>,
    kind: String,
    namespace: Option<String>,
) -> Result<TableData, String> {
    let active = require_active(&state).await?;
    let scope = normalize_scope(namespace);
    active.list_resource(&kind, scope.as_deref()).await
}

/// Fetch full detail (metadata + manifest) for a single resource of any kind.
#[tauri::command]
pub async fn get_resource(
    state: State<'_, SharedBackend>,
    kind: String,
    namespace: Option<String>,
    name: String,
) -> Result<ResourceDetail, String> {
    let active = require_active(&state).await?;
    let scope = normalize_scope(namespace);
    active.get_resource(&kind, scope.as_deref(), &name).await
}

// ============================================================================
// Resource actions (delete / restart / edit)
// ============================================================================

/// Delete a single resource of any supported kind.
#[tauri::command]
pub async fn delete_resource(
    state: State<'_, SharedBackend>,
    kind: String,
    namespace: Option<String>,
    name: String,
) -> Result<(), String> {
    let active = require_active(&state).await?;
    let scope = normalize_scope(namespace);
    active.delete_resource(&kind, scope.as_deref(), &name).await
}

/// Rolling restart for workloads; for a pod this deletes it (controller recreates).
#[tauri::command]
pub async fn restart_resource(
    state: State<'_, SharedBackend>,
    kind: String,
    namespace: Option<String>,
    name: String,
) -> Result<(), String> {
    let active = require_active(&state).await?;
    let scope = normalize_scope(namespace);
    active
        .restart_resource(&kind, scope.as_deref(), &name)
        .await
}

/// Replace a ConfigMap's `data` map (keys absent from `data` are removed).
#[tauri::command]
pub async fn update_configmap(
    state: State<'_, SharedBackend>,
    namespace: String,
    name: String,
    data: std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    let active = require_active(&state).await?;
    active.update_configmap(&namespace, &name, data).await
}

/// `kubectl describe pod`-style text report (status, containers, events).
#[tauri::command]
pub async fn describe_pod(
    state: State<'_, SharedBackend>,
    namespace: String,
    name: String,
) -> Result<String, String> {
    let active = require_active(&state).await?;
    active.describe_pod(&namespace, &name).await
}

// ============================================================================
// Live pod logs (streamed over a Tauri Channel)
// ============================================================================

#[tauri::command]
pub async fn stream_logs(
    state: State<'_, SharedBackend>,
    namespace: String,
    pod: String,
    container: Option<String>,
    on_event: Channel<LogEvent>,
) -> Result<u64, String> {
    let active = require_active(&state).await?;

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let id = {
        let mut b = state.lock().await;
        b.next_stream_id += 1;
        let id = b.next_stream_id;
        b.log_streams.insert(id, cancel_tx);
        id
    };

    // Either transport's stream is forwarded into the Channel until it ends or the
    // oneshot fires (stop_logs / window close). A Remote stream's task drop closes
    // the reqwest SSE connection; a Local stream's drop closes the kube watch.
    let mut stream = active.log_stream(namespace, pod, container, 200);
    tokio::spawn(async move {
        tokio::select! {
            _ = async {
                while let Some(ev) = stream.next().await {
                    let _ = on_event.send(ev);
                }
            } => {}
            _ = cancel_rx => {}
        }
    });

    Ok(id)
}

#[tauri::command]
pub async fn stop_logs(state: State<'_, SharedBackend>, id: u64) -> Result<(), String> {
    if let Some(tx) = state.lock().await.log_streams.remove(&id) {
        let _ = tx.send(());
    }
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

/// Clone the active connection out of the lock (held only briefly) — slow network
/// work then runs without holding the mutex. Both transports are Arc-cheap to clone.
async fn require_active(state: &State<'_, SharedBackend>) -> Result<Active, String> {
    state
        .lock()
        .await
        .active
        .clone()
        .ok_or_else(|| "Not connected to a cluster".to_string())
}

// ============================================================================
// Connection management (Remote connections + unified select/remove)
// ============================================================================

/// Register a remote backend connection (or update it if the endpoint exists).
#[tauri::command]
pub async fn add_remote_connection(
    state: State<'_, SharedBackend>,
    name: String,
    endpoint: String,
    ca_path: Option<String>,
    insecure: bool,
) -> Result<AppState, String> {
    let mut b = state.lock().await;
    b.settings.add_remote(name, endpoint, ca_path, insecure);
    b.settings.save_to_disk();
    Ok(b.settings.clone())
}

/// Edit an existing connection (Direct or Remote) in place and persist settings.
/// Keeps the entry's id stable; `endpoint`/`ca_path`/`insecure` are ignored for
/// Direct entries. Returns the updated settings.
#[tauri::command]
pub async fn update_connection(
    state: State<'_, SharedBackend>,
    id: String,
    name: String,
    description: Option<String>,
    namespace: Option<String>,
    endpoint: Option<String>,
    ca_path: Option<String>,
    insecure: bool,
) -> Result<AppState, String> {
    let mut b = state.lock().await;
    if !b
        .settings
        .update_connection(&id, name, description, namespace, endpoint, ca_path, insecure)
    {
        return Err(format!("Connection '{id}' not found"));
    }
    b.settings.save_to_disk();
    Ok(b.settings.clone())
}

/// Remove any connection (Direct or Remote) by id and persist settings.
#[tauri::command]
pub async fn remove_connection(
    state: State<'_, SharedBackend>,
    id: String,
) -> Result<AppState, String> {
    let mut b = state.lock().await;
    b.settings.unregister_kubeconfig_by_id(&id);
    b.settings.save_to_disk();
    Ok(b.settings.clone())
}

/// Probe a remote endpoint WITHOUT making it active (Settings "Test" button).
#[tauri::command]
pub async fn test_remote_connection(
    endpoint: String,
    ca_path: Option<String>,
    insecure: bool,
) -> Result<KubeStatus, String> {
    let ca_pem = read_ca(&ca_path)?;
    let mut remote = RemoteKube::new(endpoint.clone(), ca_pem, insecure)?;
    match remote.refresh_status().await {
        Ok(st) => Ok(KubeStatus {
            connected: true,
            cluster_version: Some(st.cluster_version),
            kubeconfig_path: Some(endpoint),
            ..Default::default()
        }),
        Err(e) => Ok(KubeStatus {
            connected: false,
            kubeconfig_path: Some(endpoint),
            error: Some(e),
            ..Default::default()
        }),
    }
}

/// Dashboard remote-card probe: a non-disturbing `GET /summary` for one remote
/// connection (mirrors `cluster_summary` for Direct connections).
#[tauri::command]
pub async fn remote_summary(
    state: State<'_, SharedBackend>,
    connection_id: String,
) -> Result<ClusterSummary, String> {
    let (endpoint, ca_path, insecure) = {
        let b = state.lock().await;
        let Some(entry) = b
            .settings
            .kubeconfigs
            .iter()
            .find(|k| k.id == connection_id)
        else {
            return Ok(ClusterSummary::unreachable(format!(
                "Connection '{connection_id}' not found"
            )));
        };
        let Some(endpoint) = entry.endpoint.clone() else {
            return Ok(ClusterSummary::unreachable(
                "Remote connection has no endpoint",
            ));
        };
        (endpoint, entry.ca_path.clone(), entry.insecure)
    };
    let ca_pem = match read_ca(&ca_path) {
        Ok(v) => v,
        Err(e) => return Ok(ClusterSummary::unreachable(e)),
    };
    let remote = match RemoteKube::new(endpoint, ca_pem, insecure) {
        Ok(r) => r,
        Err(e) => return Ok(ClusterSummary::unreachable(e)),
    };
    match remote.summary().await {
        Ok(s) => Ok(s),
        Err(e) => Ok(ClusterSummary::unreachable(e)),
    }
}

/// Make a connection active and connect to it, dispatching by its mode. Direct
/// connections behave like `switch_kubeconfig`; Remote connections build a client
/// and probe the backend.
#[tauri::command]
pub async fn select_connection(
    state: State<'_, SharedBackend>,
    id: String,
) -> Result<KubeStatus, String> {
    let entry = {
        state
            .lock()
            .await
            .settings
            .kubeconfigs
            .iter()
            .find(|k| k.id == id)
            .cloned()
    };
    let Some(entry) = entry else {
        let b = state.lock().await;
        return Ok(b.status(Some(format!("Connection '{id}' not found"))));
    };
    match entry.mode {
        ConnMode::Remote => connect_remote(state, id).await,
        ConnMode::Direct => switch_kubeconfig(state, entry.path).await,
    }
}
