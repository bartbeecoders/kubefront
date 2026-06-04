//! Tauri command surface — the bridge between the React frontend and the
//! Kubernetes backend (kube-rs). Every command is async and runs on Tauri's
//! Tokio runtime, so we can call kube-rs directly.
//!
//! Design notes:
//! - All shared state lives in [`Backend`], guarded by a Tokio `Mutex` and
//!   registered as Tauri managed state.
//! - Resource lists are *pulled* on demand by the frontend (polling), instead
//!   of the old egui push-channel model. Each list command returns a small
//!   serializable projection, never raw k8s objects.
//! - Live pod logs are *pushed* through a Tauri [`Channel`], which is the
//!   idiomatic v2 way to stream many events to a single caller.

use std::collections::HashMap;

use k8s_openapi::api::core::v1::{Node, Pod};
use kube::api::{Api, ListParams};
use kube::core::NamespaceResourceScope;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::{oneshot, Mutex};

use crate::k8s::manager::{ContextInfo, KubeConfigManager};
use crate::k8s::store::{self, human_age, TableData};
use crate::state::{AppState, ColorScheme};

/// All mutable application state shared across commands.
#[derive(Default)]
pub struct Backend {
    pub manager: KubeConfigManager,
    pub client: Option<kube::Client>,
    pub connected: bool,
    pub cluster_version: Option<String>,
    pub settings: AppState,
    /// Active log streams → cancellation sender. Dropping/sending stops the stream.
    pub log_streams: HashMap<u64, oneshot::Sender<()>>,
    pub next_stream_id: u64,
}

pub type SharedBackend = Mutex<Backend>;

// ============================================================================
// Serializable DTOs returned to the frontend
// ============================================================================

/// Snapshot of connection + kubeconfig state, returned by most config commands.
#[derive(Serialize, Default)]
pub struct KubeStatus {
    pub connected: bool,
    pub cluster_version: Option<String>,
    pub current_context: Option<String>,
    pub kubeconfig_path: Option<String>,
    pub context_count: usize,
    pub contexts: Vec<ContextInfo>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct PodRow {
    pub name: String,
    pub namespace: String,
    pub phase: String,
    pub ready: String,
    pub restarts: u32,
    pub age: String,
    pub node: String,
    pub containers: Vec<String>,
}

#[derive(Serialize)]
pub struct NodeRow {
    pub name: String,
    pub status: String,
    pub roles: String,
    pub version: String,
    pub age: String,
}

/// Full detail for a single selected resource of any kind.
#[derive(Serialize)]
pub struct ResourceDetail {
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub age: String,
    pub labels: Vec<(String, String)>,
    pub annotations: Vec<(String, String)>,
    /// The full object as pretty-printed JSON (managedFields stripped).
    pub manifest: String,
}

/// One streamed log event. `kind` is "header" | "line" | "error" | "ended".
#[derive(Serialize, Clone)]
pub struct LogEvent {
    pub kind: String,
    pub line: String,
}

impl Backend {
    fn status(&self, error: Option<String>) -> KubeStatus {
        KubeStatus {
            connected: self.connected,
            cluster_version: self.cluster_version.clone(),
            current_context: self.manager.current_context.clone(),
            kubeconfig_path: self.manager.path.as_ref().map(|p| p.display().to_string()),
            context_count: self.manager.contexts.len(),
            contexts: self.manager.contexts.clone(),
            error,
        }
    }
}

// ============================================================================
// Settings
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
/// Single source of truth for theming — the frontend sets this as a CSS var.
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
// Kubeconfig & connection
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
            b.connected = false;
            b.client = None;
            b.cluster_version = None;
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
        b.connected = false;
        b.client = None;
        b.cluster_version = None;
    }
    Ok(b.status(None))
}

/// Return the current connection / kubeconfig status without changing anything.
#[tauri::command]
pub async fn get_status(state: State<'_, SharedBackend>) -> Result<KubeStatus, String> {
    Ok(state.lock().await.status(None))
}

/// Create a live `kube::Client` for the current context and probe its version.
#[tauri::command]
pub async fn connect(state: State<'_, SharedBackend>) -> Result<KubeStatus, String> {
    // Extract everything we need, then drop the lock before the (slow) network work.
    let (kubeconfig, opts, ctx_name, server) = {
        let b = state.lock().await;
        let Some(opts) = b.manager.current_kubeconfig_options() else {
            return Ok(b.status(Some("No context selected in kubeconfig".into())));
        };
        // Use the kubeconfig we actually loaded from the user's chosen path — NOT the
        // default (~/.kube/config). `Config::from_kubeconfig` ignores the loaded file
        // and re-reads the default location, so a context that only exists in the
        // selected file fails with "failed to load current context".
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

    // One 15s timeout around the WHOLE thing — including the network probe. Building
    // the client (config parse + HTTP stack) is local and instant; the only network
    // round-trip is `probe_cluster_version`, so it must live inside the timeout or we
    // hang on unreachable servers / TLS failures. Its error is propagated (not
    // swallowed) so the real reason reaches the log and the UI.
    let connect_fut = async {
        let config = kube::Config::from_custom_kubeconfig(kubeconfig, &opts)
            .await
            .map_err(|e| format!("kubeconfig error: {e}"))?;
        let client =
            kube::Client::try_from(config).map_err(|e| format!("client build error: {e}"))?;
        let version = probe_cluster_version(&client).await?;
        Ok::<_, String>((client, version))
    };

    let result = tokio::time::timeout(std::time::Duration::from_secs(15), connect_fut).await;

    match result {
        Ok(Ok((client, version))) => {
            let mut b = state.lock().await;
            b.client = Some(client);
            b.connected = true;
            tracing::info!("Connected to '{ctx_name}' ({server}); cluster version {version}");
            b.cluster_version = Some(version);

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
        Ok(Err(e)) => {
            tracing::error!("Connection to '{ctx_name}' ({server}) failed: {e}");
            let mut b = state.lock().await;
            b.connected = false;
            b.client = None;
            b.cluster_version = None;
            Ok(b.status(Some(format!("Failed to connect to {server}: {e}"))))
        }
        Err(_) => {
            tracing::error!("Connection to '{ctx_name}' ({server}) timed out after 15s");
            let mut b = state.lock().await;
            b.connected = false;
            b.client = None;
            b.cluster_version = None;
            Ok(b.status(Some(format!(
                "Connection to {server} timed out after 15s. Common causes: server unreachable from this machine, firewall, wrong network/VPN, or TLS verification failure. Try the same `kubectl` command from this exact terminal."
            ))))
        }
    }
}

/// Register (if needed), load, select last context for, and connect to a kubeconfig file.
/// This is the one-shot the Settings "Switch" / "Add" buttons use.
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
        b.connected = false;
        b.client = None;
        b.cluster_version = None;
        b.settings.save_to_disk();
    }
    // Now connect with the freshly loaded config.
    connect(state).await
}

// ============================================================================
// Resource listing
// ============================================================================

#[tauri::command]
pub async fn list_pods(
    state: State<'_, SharedBackend>,
    namespace: Option<String>,
) -> Result<Vec<PodRow>, String> {
    let client = require_client(&state).await?;
    let scope = normalize_scope(namespace);
    let api: Api<Pod> = match &scope {
        Some(ns) => Api::namespaced(client, ns),
        None => Api::all(client),
    };
    let list = api
        .list(&ListParams::default())
        .await
        .map_err(|e| log_err("list pods", e))?;
    Ok(list.items.iter().map(pod_row).collect())
}

#[tauri::command]
pub async fn list_nodes(state: State<'_, SharedBackend>) -> Result<Vec<NodeRow>, String> {
    let client = require_client(&state).await?;
    let api: Api<Node> = Api::all(client);
    let list = api
        .list(&ListParams::default())
        .await
        .map_err(|e| log_err("list nodes", e))?;
    Ok(list.items.iter().map(node_row).collect())
}

/// Generic list: returns a headers+rows table projection for the given resource `kind`.
#[tauri::command]
pub async fn list_resource(
    state: State<'_, SharedBackend>,
    kind: String,
    namespace: Option<String>,
) -> Result<TableData, String> {
    use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
    use k8s_openapi::api::batch::v1::{CronJob, Job};
    use k8s_openapi::api::core::v1::{
        ConfigMap, Namespace, PersistentVolume, PersistentVolumeClaim, Secret, Service,
        ServiceAccount,
    };
    use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
    use k8s_openapi::api::rbac::v1::{Role, RoleBinding};
    use k8s_openapi::api::storage::v1::StorageClass;

    let client = require_client(&state).await?;
    let scope = normalize_scope(namespace);

    macro_rules! ns_table {
        ($ty:ty, $proj:path) => {{
            let items = list_namespaced::<$ty>(&client, &scope).await?;
            $proj(&items)
        }};
    }
    macro_rules! all_table {
        ($ty:ty, $proj:path) => {{
            let items = list_cluster::<$ty>(&client).await?;
            $proj(&items)
        }};
    }

    let table = match kind.as_str() {
        "namespaces" => all_table!(Namespace, store::namespaces_table),
        "services" => ns_table!(Service, store::services_table),
        "deployments" => ns_table!(Deployment, store::deployments_table),
        "statefulsets" => ns_table!(StatefulSet, store::statefulsets_table),
        "daemonsets" => ns_table!(DaemonSet, store::daemonsets_table),
        "jobs" => ns_table!(Job, store::jobs_table),
        "cronjobs" => ns_table!(CronJob, store::cronjobs_table),
        "configmaps" => ns_table!(ConfigMap, store::configmaps_table),
        "secrets" => ns_table!(Secret, store::secrets_table),
        "pvcs" => ns_table!(PersistentVolumeClaim, store::pvcs_table),
        "pvs" => all_table!(PersistentVolume, store::pvs_table),
        "storageclasses" => all_table!(StorageClass, store::storage_classes_table),
        "ingresses" => ns_table!(Ingress, store::ingresses_table),
        "networkpolicies" => ns_table!(NetworkPolicy, store::network_policies_table),
        "serviceaccounts" => ns_table!(ServiceAccount, store::service_accounts_table),
        "roles" => ns_table!(Role, store::roles_table),
        "rolebindings" => ns_table!(RoleBinding, store::role_bindings_table),
        other => return Err(format!("Unknown resource kind: {other}")),
    };
    Ok(table)
}

/// Fetch full detail (metadata + manifest) for a single resource of any kind.
#[tauri::command]
pub async fn get_resource(
    state: State<'_, SharedBackend>,
    kind: String,
    namespace: Option<String>,
    name: String,
) -> Result<ResourceDetail, String> {
    use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
    use k8s_openapi::api::batch::v1::{CronJob, Job};
    use k8s_openapi::api::core::v1::{
        ConfigMap, Namespace, PersistentVolume, PersistentVolumeClaim, Secret, Service,
        ServiceAccount,
    };
    use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
    use k8s_openapi::api::rbac::v1::{Role, RoleBinding};
    use k8s_openapi::api::storage::v1::StorageClass;

    let client = require_client(&state).await?;
    let ns = normalize_scope(namespace);

    macro_rules! ns_detail {
        ($ty:ty) => {{
            let ns = ns
                .as_deref()
                .ok_or("A namespace is required for this resource")?;
            let obj = get_namespaced::<$ty>(&client, ns, &name).await?;
            to_detail(obj, &kind)?
        }};
    }
    macro_rules! cluster_detail {
        ($ty:ty) => {{
            let obj = get_cluster::<$ty>(&client, &name).await?;
            to_detail(obj, &kind)?
        }};
    }

    let detail = match kind.as_str() {
        "pods" => ns_detail!(Pod),
        "nodes" => cluster_detail!(Node),
        "namespaces" => cluster_detail!(Namespace),
        "services" => ns_detail!(Service),
        "deployments" => ns_detail!(Deployment),
        "statefulsets" => ns_detail!(StatefulSet),
        "daemonsets" => ns_detail!(DaemonSet),
        "jobs" => ns_detail!(Job),
        "cronjobs" => ns_detail!(CronJob),
        "configmaps" => ns_detail!(ConfigMap),
        "secrets" => ns_detail!(Secret),
        "pvcs" => ns_detail!(PersistentVolumeClaim),
        "pvs" => cluster_detail!(PersistentVolume),
        "storageclasses" => cluster_detail!(StorageClass),
        "ingresses" => ns_detail!(Ingress),
        "networkpolicies" => ns_detail!(NetworkPolicy),
        "serviceaccounts" => ns_detail!(ServiceAccount),
        "roles" => ns_detail!(Role),
        "rolebindings" => ns_detail!(RoleBinding),
        other => return Err(format!("Unknown resource kind: {other}")),
    };
    Ok(detail)
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
    let client = require_client(&state).await?;

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let id = {
        let mut b = state.lock().await;
        b.next_stream_id += 1;
        let id = b.next_stream_id;
        b.log_streams.insert(id, cancel_tx);
        id
    };

    tokio::spawn(async move {
        use futures_util::io::AsyncBufReadExt;
        use futures_util::StreamExt;
        use kube::api::LogParams;

        let pods: Api<Pod> = Api::namespaced(client, &namespace);
        let params = LogParams {
            follow: true,
            tail_lines: Some(200),
            timestamps: true,
            container: container.clone(),
            ..Default::default()
        };

        let _ = on_event.send(LogEvent {
            kind: "header".into(),
            line: format!("--- Streaming logs for {namespace}/{pod} ---"),
        });

        match pods.log_stream(&pod, &params).await {
            Ok(logs) => {
                let mut lines = logs.lines();
                tokio::select! {
                    _ = async {
                        while let Some(result) = lines.next().await {
                            match result {
                                Ok(line) if !line.is_empty() => {
                                    let _ = on_event.send(LogEvent { kind: "line".into(), line });
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    let _ = on_event.send(LogEvent {
                                        kind: "error".into(),
                                        line: format!("Error reading log line: {e}"),
                                    });
                                    break;
                                }
                            }
                        }
                    } => {}
                    _ = cancel_rx => {}
                }
            }
            Err(e) => {
                let _ = on_event.send(LogEvent {
                    kind: "error".into(),
                    line: format!("Failed to open log stream: {e}"),
                });
            }
        }
        let _ = on_event.send(LogEvent {
            kind: "ended".into(),
            line: String::new(),
        });
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

async fn require_client(state: &State<'_, SharedBackend>) -> Result<kube::Client, String> {
    state
        .lock()
        .await
        .client
        .clone()
        .ok_or_else(|| "Not connected to a cluster".to_string())
}

/// Stringify an API error AND write it to the log — list/get failures must never
/// be silent (e.g. RBAC 403s on cluster-wide lists for namespace-scoped users).
fn log_err(what: &str, e: impl std::fmt::Display) -> String {
    let msg = e.to_string();
    tracing::warn!("{what} failed: {msg}");
    msg
}

/// "All" / empty → cluster-wide (None); otherwise a concrete namespace.
fn normalize_scope(namespace: Option<String>) -> Option<String> {
    match namespace {
        Some(ns) if ns != "All" && !ns.trim().is_empty() => Some(ns),
        _ => None,
    }
}

async fn list_cluster<K>(client: &kube::Client) -> Result<Vec<K>, String>
where
    K: kube::Resource + Clone + serde::de::DeserializeOwned + std::fmt::Debug,
    <K as kube::Resource>::DynamicType: Default,
{
    let api: Api<K> = Api::all(client.clone());
    api.list(&ListParams::default())
        .await
        .map(|l| l.items)
        .map_err(|e| log_err(&format!("list {}", short_kind::<K>()), e))
}

async fn list_namespaced<K>(client: &kube::Client, scope: &Option<String>) -> Result<Vec<K>, String>
where
    K: kube::Resource<Scope = NamespaceResourceScope>
        + Clone
        + serde::de::DeserializeOwned
        + std::fmt::Debug,
    <K as kube::Resource>::DynamicType: Default,
{
    let api: Api<K> = match scope {
        Some(ns) => Api::namespaced(client.clone(), ns),
        None => Api::all(client.clone()),
    };
    api.list(&ListParams::default())
        .await
        .map(|l| l.items)
        .map_err(|e| log_err(&format!("list {}", short_kind::<K>()), e))
}

async fn get_namespaced<K>(client: &kube::Client, namespace: &str, name: &str) -> Result<K, String>
where
    K: kube::Resource<Scope = NamespaceResourceScope>
        + Clone
        + serde::de::DeserializeOwned
        + std::fmt::Debug,
    <K as kube::Resource>::DynamicType: Default,
{
    let api: Api<K> = Api::namespaced(client.clone(), namespace);
    api.get(name)
        .await
        .map_err(|e| log_err(&format!("get {}", short_kind::<K>()), e))
}

async fn get_cluster<K>(client: &kube::Client, name: &str) -> Result<K, String>
where
    K: kube::Resource + Clone + serde::de::DeserializeOwned + std::fmt::Debug,
    <K as kube::Resource>::DynamicType: Default,
{
    let api: Api<K> = Api::all(client.clone());
    api.get(name)
        .await
        .map_err(|e| log_err(&format!("get {}", short_kind::<K>()), e))
}

/// Last path segment of a type name, e.g. "…::core::v1::Pod" → "Pod".
fn short_kind<K>() -> &'static str {
    std::any::type_name::<K>()
        .rsplit("::")
        .next()
        .unwrap_or("?")
}

/// Project any fetched object into a [`ResourceDetail`] (metadata + JSON manifest).
fn to_detail<K>(mut obj: K, kind: &str) -> Result<ResourceDetail, String>
where
    K: kube::Resource + serde::Serialize,
{
    // managedFields is verbose server bookkeeping — drop it from the manifest view.
    obj.meta_mut().managed_fields = None;

    let meta = obj.meta();
    let name = meta.name.clone().unwrap_or_default();
    let namespace = meta.namespace.clone();
    let age = human_age(meta.creation_timestamp.as_ref());
    let labels = meta
        .labels
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let annotations = meta
        .annotations
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let manifest = serde_json::to_string_pretty(&obj).map_err(|e| e.to_string())?;

    Ok(ResourceDetail {
        kind: kind.to_string(),
        name,
        namespace,
        age,
        labels,
        annotations,
        manifest,
    })
}

/// Connectivity probe + version fetch. Hits the apiserver `/version` endpoint,
/// which requires no RBAC (so it works for any authenticated user, unlike listing
/// nodes) and forces a real network round-trip + TLS handshake. The error is
/// returned — never swallowed — so a failed connection surfaces its true cause.
async fn probe_cluster_version(client: &kube::Client) -> Result<String, String> {
    let info = client
        .apiserver_version()
        .await
        .map_err(|e| e.to_string())?;
    // e.g. "v1.29.4+k3s1"; fall back to major.minor if git_version is blank.
    Ok(if info.git_version.is_empty() {
        format!("v{}.{}", info.major, info.minor)
    } else {
        info.git_version
    })
}

// === Pod / Node projections (ported from the egui app) ===

fn pod_row(pod: &Pod) -> PodRow {
    let status = pod.status.as_ref();
    let containers = pod
        .spec
        .as_ref()
        .map(|s| s.containers.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default();

    let (ready, total) = status
        .and_then(|s| s.container_statuses.as_ref())
        .map(|cs| (cs.iter().filter(|c| c.ready).count(), cs.len()))
        .unwrap_or((0, 0));

    let restarts = status
        .and_then(|s| s.container_statuses.as_ref())
        .map(|cs| cs.iter().map(|c| c.restart_count as u32).sum())
        .unwrap_or(0);

    PodRow {
        name: pod
            .metadata
            .name
            .clone()
            .unwrap_or_else(|| "unknown".into()),
        namespace: pod
            .metadata
            .namespace
            .clone()
            .unwrap_or_else(|| "default".into()),
        phase: status
            .and_then(|s| s.phase.clone())
            .unwrap_or_else(|| "Unknown".into()),
        ready: format!("{ready}/{total}"),
        restarts,
        age: human_age(pod.metadata.creation_timestamp.as_ref()),
        node: pod
            .spec
            .as_ref()
            .and_then(|s| s.node_name.clone())
            .unwrap_or_else(|| "-".into()),
        containers,
    }
}

fn node_row(node: &Node) -> NodeRow {
    let status = node
        .status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .and_then(|conds| conds.iter().find(|c| c.type_ == "Ready"))
        .map(|c| {
            if c.status == "True" {
                "Ready"
            } else {
                "NotReady"
            }
        })
        .unwrap_or("Unknown")
        .to_string();

    let roles = node
        .metadata
        .labels
        .as_ref()
        .map(|labels| {
            if labels.contains_key("node-role.kubernetes.io/control-plane")
                || labels.contains_key("node-role.kubernetes.io/master")
            {
                "control-plane".to_string()
            } else {
                "worker".to_string()
            }
        })
        .unwrap_or_else(|| "worker".into());

    NodeRow {
        name: node
            .metadata
            .name
            .clone()
            .unwrap_or_else(|| "unknown".into()),
        status,
        roles,
        version: node
            .status
            .as_ref()
            .and_then(|s| s.node_info.as_ref())
            .map(|i| i.kubelet_version.clone())
            .unwrap_or_else(|| "unknown".into()),
        age: human_age(node.metadata.creation_timestamp.as_ref()),
    }
}
