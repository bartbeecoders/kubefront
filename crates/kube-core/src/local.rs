//! [`LocalKube`] — owns a live `kube::Client` and performs every Kubernetes
//! operation KubeFront supports. The method bodies are extracted verbatim from
//! the old `src-tauri/src/commands.rs`; only the Tauri `State`/lock plumbing and
//! the `Result<_, String>` mapping were lifted out (errors are now [`CoreError`],
//! whose `Display` reproduces the original strings).

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::Stream;
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::api::{Api, DeleteParams, ListParams, Patch, PatchParams, PostParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::core::NamespaceResourceScope;

use crate::dto::{ClusterSummary, LogEvent, NodeRow, PodRow, ResourceDetail};
use crate::error::{kube_err, CoreError};
use crate::store::{self, human_age, TableData};

/// A connected Kubernetes client plus the cluster version probed at connect time.
/// Cheap to `Clone` — `kube::Client` is internally `Arc`-backed.
#[derive(Clone)]
pub struct LocalKube {
    client: kube::Client,
    cluster_version: String,
}

impl LocalKube {
    /// The underlying client (cheap Arc clone).
    pub fn client(&self) -> kube::Client {
        self.client.clone()
    }

    /// Cluster version string probed at connect time (e.g. "v1.29.4+k3s1").
    pub fn cluster_version(&self) -> &str {
        &self.cluster_version
    }

    /// Build a live client for `opts` against `kubeconfig` and probe its version.
    ///
    /// One `timeout` wraps the WHOLE thing — including the network probe. Building
    /// the client is local and instant; the only network round-trip is
    /// `probe_cluster_version`, so it must live inside the timeout or we hang on
    /// unreachable servers / TLS failures. We use the loaded kubeconfig (NOT the
    /// default `~/.kube/config`) so a context that only exists in the chosen file
    /// still connects.
    pub async fn connect_from(
        kubeconfig: Kubeconfig,
        opts: KubeConfigOptions,
        timeout: Duration,
    ) -> Result<Self, CoreError> {
        let fut = async {
            let config = kube::Config::from_custom_kubeconfig(kubeconfig, &opts)
                .await
                .map_err(|e| CoreError::Kubeconfig(format!("kubeconfig error: {e}")))?;
            let client = kube::Client::try_from(config)
                .map_err(|e| CoreError::Other(format!("client build error: {e}")))?;
            let version = probe_cluster_version(&client).await?;
            Ok::<_, CoreError>(Self {
                client,
                cluster_version: version,
            })
        };

        match tokio::time::timeout(timeout, fut).await {
            Ok(res) => res,
            Err(_) => Err(CoreError::Timeout(format!(
                "Connection timed out after {}s",
                timeout.as_secs()
            ))),
        }
    }

    pub async fn list_pods(&self, namespace: Option<&str>) -> Result<Vec<PodRow>, CoreError> {
        let api: Api<Pod> = match namespace {
            Some(ns) => Api::namespaced(self.client.clone(), ns),
            None => Api::all(self.client.clone()),
        };
        let list = api
            .list(&ListParams::default())
            .await
            .map_err(|e| kube_err("list pods", e))?;
        Ok(list.items.iter().map(pod_row).collect())
    }

    pub async fn list_nodes(&self) -> Result<Vec<NodeRow>, CoreError> {
        let api: Api<Node> = Api::all(self.client.clone());
        let list = api
            .list(&ListParams::default())
            .await
            .map_err(|e| kube_err("list nodes", e))?;
        Ok(list.items.iter().map(node_row).collect())
    }

    /// Generic list: returns a headers+rows table projection for `kind`.
    pub async fn list_resource(
        &self,
        kind: &str,
        namespace: Option<&str>,
    ) -> Result<TableData, CoreError> {
        use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
        use k8s_openapi::api::batch::v1::{CronJob, Job};
        use k8s_openapi::api::core::v1::{
            ConfigMap, Namespace, PersistentVolume, PersistentVolumeClaim, Secret, Service,
            ServiceAccount,
        };
        use k8s_openapi::api::networking::v1::{Ingress, IngressClass, NetworkPolicy};
        use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, Role, RoleBinding};
        use k8s_openapi::api::storage::v1::StorageClass;
        use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;

        let client = self.client.clone();
        let scope = namespace.map(|s| s.to_string());

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

        let table = match kind {
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
            "ingressclasses" => all_table!(IngressClass, store::ingress_classes_table),
            "networkpolicies" => ns_table!(NetworkPolicy, store::network_policies_table),
            "serviceaccounts" => ns_table!(ServiceAccount, store::service_accounts_table),
            "roles" => ns_table!(Role, store::roles_table),
            "rolebindings" => ns_table!(RoleBinding, store::role_bindings_table),
            "clusterroles" => all_table!(ClusterRole, store::cluster_roles_table),
            "clusterrolebindings" => {
                all_table!(ClusterRoleBinding, store::cluster_role_bindings_table)
            }
            "crds" => all_table!(CustomResourceDefinition, store::crds_table),
            other => return Err(CoreError::Other(format!("Unknown resource kind: {other}"))),
        };
        Ok(table)
    }

    /// Fetch full detail (metadata + manifest) for a single resource of any kind.
    pub async fn get_resource(
        &self,
        kind: &str,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<ResourceDetail, CoreError> {
        use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
        use k8s_openapi::api::batch::v1::{CronJob, Job};
        use k8s_openapi::api::core::v1::{
            ConfigMap, Namespace, PersistentVolume, PersistentVolumeClaim, Secret, Service,
            ServiceAccount,
        };
        use k8s_openapi::api::networking::v1::{Ingress, IngressClass, NetworkPolicy};
        use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, Role, RoleBinding};
        use k8s_openapi::api::storage::v1::StorageClass;
        use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;

        let client = self.client.clone();

        macro_rules! ns_detail {
            ($ty:ty) => {{
                let ns = namespace.ok_or_else(|| {
                    CoreError::Other("A namespace is required for this resource".into())
                })?;
                let obj = get_namespaced::<$ty>(&client, ns, name).await?;
                to_detail(obj, kind)?
            }};
        }
        macro_rules! cluster_detail {
            ($ty:ty) => {{
                let obj = get_cluster::<$ty>(&client, name).await?;
                to_detail(obj, kind)?
            }};
        }

        let detail = match kind {
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
            "ingressclasses" => cluster_detail!(IngressClass),
            "networkpolicies" => ns_detail!(NetworkPolicy),
            "serviceaccounts" => ns_detail!(ServiceAccount),
            "roles" => ns_detail!(Role),
            "rolebindings" => ns_detail!(RoleBinding),
            "clusterroles" => cluster_detail!(ClusterRole),
            "clusterrolebindings" => cluster_detail!(ClusterRoleBinding),
            "crds" => cluster_detail!(CustomResourceDefinition),
            other => return Err(CoreError::Other(format!("Unknown resource kind: {other}"))),
        };
        Ok(detail)
    }

    /// Delete a single resource of any supported kind. Deleting a namespace
    /// cascades to everything inside it, so the UI guards it behind an extra
    /// confirmation; nodes remain deliberately NOT deletable.
    pub async fn delete_resource(
        &self,
        kind: &str,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<(), CoreError> {
        use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
        use k8s_openapi::api::batch::v1::{CronJob, Job};
        use k8s_openapi::api::core::v1::{
            ConfigMap, Namespace, PersistentVolume, PersistentVolumeClaim, Secret, Service,
            ServiceAccount,
        };
        use k8s_openapi::api::networking::v1::{Ingress, IngressClass, NetworkPolicy};
        use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, Role, RoleBinding};
        use k8s_openapi::api::storage::v1::StorageClass;
        use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;

        let client = self.client.clone();

        macro_rules! ns_delete {
            ($ty:ty) => {{
                let ns = namespace.ok_or_else(|| {
                    CoreError::Other("A namespace is required for this resource".into())
                })?;
                let api: Api<$ty> = Api::namespaced(client.clone(), ns);
                api.delete(name, &DeleteParams::default())
                    .await
                    .map_err(|e| kube_err(&format!("delete {}", short_kind::<$ty>()), e))?;
            }};
        }
        macro_rules! cluster_delete {
            ($ty:ty) => {{
                let api: Api<$ty> = Api::all(client.clone());
                api.delete(name, &DeleteParams::default())
                    .await
                    .map_err(|e| kube_err(&format!("delete {}", short_kind::<$ty>()), e))?;
            }};
        }

        match kind {
            "pods" => ns_delete!(Pod),
            "services" => ns_delete!(Service),
            "deployments" => ns_delete!(Deployment),
            "statefulsets" => ns_delete!(StatefulSet),
            "daemonsets" => ns_delete!(DaemonSet),
            "jobs" => ns_delete!(Job),
            "cronjobs" => ns_delete!(CronJob),
            "configmaps" => ns_delete!(ConfigMap),
            "secrets" => ns_delete!(Secret),
            "pvcs" => ns_delete!(PersistentVolumeClaim),
            "pvs" => cluster_delete!(PersistentVolume),
            "namespaces" => cluster_delete!(Namespace),
            "storageclasses" => cluster_delete!(StorageClass),
            "ingresses" => ns_delete!(Ingress),
            "ingressclasses" => cluster_delete!(IngressClass),
            "networkpolicies" => ns_delete!(NetworkPolicy),
            "serviceaccounts" => ns_delete!(ServiceAccount),
            "roles" => ns_delete!(Role),
            "rolebindings" => ns_delete!(RoleBinding),
            "clusterroles" => cluster_delete!(ClusterRole),
            "clusterrolebindings" => cluster_delete!(ClusterRoleBinding),
            "crds" => cluster_delete!(CustomResourceDefinition),
            other => {
                return Err(CoreError::Other(format!(
                    "Deleting '{other}' is not supported"
                )))
            }
        }
        tracing::info!("Deleted {kind} {}/{name}", namespace.unwrap_or("-"));
        Ok(())
    }

    /// Rolling restart. For workload controllers this patches a `restartedAt`
    /// annotation into the pod template; for a bare pod it's a delete (the owning
    /// controller, if any, recreates it).
    pub async fn restart_resource(
        &self,
        kind: &str,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<(), CoreError> {
        use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};

        if kind == "pods" {
            return self.delete_resource(kind, namespace, name).await;
        }

        let client = self.client.clone();
        let ns = namespace
            .ok_or_else(|| CoreError::Other("A namespace is required for this resource".into()))?;

        let now = chrono::Utc::now().to_rfc3339();
        let patch = serde_json::json!({
            "spec": { "template": { "metadata": { "annotations": {
                "kubectl.kubernetes.io/restartedAt": now
            }}}}
        });

        macro_rules! rollout {
            ($ty:ty) => {{
                let api: Api<$ty> = Api::namespaced(client.clone(), ns);
                api.patch(name, &PatchParams::default(), &Patch::Strategic(&patch))
                    .await
                    .map_err(|e| kube_err(&format!("restart {}", short_kind::<$ty>()), e))?;
            }};
        }

        match kind {
            "deployments" => rollout!(Deployment),
            "statefulsets" => rollout!(StatefulSet),
            "daemonsets" => rollout!(DaemonSet),
            other => {
                return Err(CoreError::Other(format!(
                    "Restarting '{other}' is not supported"
                )))
            }
        }
        tracing::info!("Restarted {kind} {ns}/{name}");
        Ok(())
    }

    /// Replace a ConfigMap's `data` entries. The live object is read first (for
    /// its `resourceVersion`) and PUT back, so keys removed in the UI are actually
    /// deleted. `managedFields` is stripped before the write.
    pub async fn update_configmap(
        &self,
        namespace: &str,
        name: &str,
        data: BTreeMap<String, String>,
    ) -> Result<(), CoreError> {
        use k8s_openapi::api::core::v1::ConfigMap;

        let ns = normalize_scope(Some(namespace.to_string())).ok_or_else(|| {
            CoreError::Other("A namespace is required to edit a configmap".into())
        })?;
        let api: Api<ConfigMap> = Api::namespaced(self.client.clone(), &ns);

        let mut cm = api
            .get(name)
            .await
            .map_err(|e| kube_err("get configmap", e))?;
        cm.data = if data.is_empty() { None } else { Some(data) };
        cm.metadata.managed_fields = None;

        api.replace(name, &PostParams::default(), &cm)
            .await
            .map_err(|e| kube_err("update configmap", e))?;
        tracing::info!("Updated configmap {ns}/{name}");
        Ok(())
    }

    /// Produce a `kubectl describe pod`-style text report (status, containers,
    /// conditions and the pod's recent Events). Events are best-effort.
    pub async fn describe_pod(&self, namespace: &str, name: &str) -> Result<String, CoreError> {
        use k8s_openapi::api::core::v1::Event;

        let ns = normalize_scope(Some(namespace.to_string()))
            .ok_or_else(|| CoreError::Other("A namespace is required to describe a pod".into()))?;

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &ns);
        let pod = pods.get(name).await.map_err(|e| kube_err("get pod", e))?;

        let events_api: Api<Event> = Api::namespaced(self.client.clone(), &ns);
        let selector = format!("involvedObject.kind=Pod,involvedObject.name={name}");
        let events = events_api
            .list(&ListParams::default().fields(&selector))
            .await
            .map(|l| l.items)
            .unwrap_or_default();

        Ok(store::describe_pod(&pod, &events))
    }

    /// Live health snapshot for this cluster (used by the backend's `/summary`).
    pub async fn cluster_summary(&self, namespace: Option<String>) -> ClusterSummary {
        summarize(self.client.clone(), normalize_scope(namespace)).await
    }

    /// Stream a pod's logs as [`LogEvent`]s (see [`crate::log_stream`]).
    pub fn log_stream(
        &self,
        namespace: &str,
        pod: &str,
        container: Option<String>,
        tail: i64,
    ) -> impl Stream<Item = LogEvent> + Send + 'static {
        crate::logstream::log_stream(
            self.client.clone(),
            namespace.to_string(),
            pod.to_string(),
            container,
            tail,
        )
    }
}

// ============================================================================
// Cluster summary (shared by the desktop Dashboard probe and the backend)
// ============================================================================

/// Count nodes/pods/deployments/namespaces for one cluster. The version probe is
/// the reachability gate (needs no RBAC); everything after is best-effort, so a
/// namespace-scoped user still gets the counts they're allowed to see. `scope`
/// must already be normalized (None = cluster-wide).
pub async fn summarize(client: kube::Client, scope: Option<String>) -> ClusterSummary {
    use k8s_openapi::api::apps::v1::Deployment;
    use k8s_openapi::api::core::v1::Namespace;

    let version = match probe_cluster_version(&client).await {
        Ok(v) => v,
        Err(e) => return ClusterSummary::unreachable(e.to_string()),
    };

    let nodes = Api::<Node>::all(client.clone())
        .list(&ListParams::default())
        .await
        .ok();
    let pods_api: Api<Pod> = match &scope {
        Some(ns) => Api::namespaced(client.clone(), ns),
        None => Api::all(client.clone()),
    };
    let pods = pods_api.list(&ListParams::default()).await.ok();
    let deploy_api: Api<Deployment> = match &scope {
        Some(ns) => Api::namespaced(client.clone(), ns),
        None => Api::all(client.clone()),
    };
    let deployments = deploy_api.list(&ListParams::default()).await.ok();
    let namespaces = Api::<Namespace>::all(client.clone())
        .list(&ListParams::default())
        .await
        .ok();

    ClusterSummary {
        reachable: true,
        version: Some(version),
        nodes_total: nodes.as_ref().map(|l| l.items.len() as u32),
        nodes_ready: nodes
            .as_ref()
            .map(|l| l.items.iter().filter(|n| node_is_ready(n)).count() as u32),
        pods_total: pods.as_ref().map(|l| l.items.len() as u32),
        pods_running: pods.as_ref().map(|l| {
            l.items
                .iter()
                .filter(|p| p.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Running"))
                .count() as u32
        }),
        namespaces: namespaces.as_ref().map(|l| l.items.len() as u32),
        deployments: deployments.as_ref().map(|l| l.items.len() as u32),
        error: None,
    }
}

fn node_is_ready(node: &Node) -> bool {
    node.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .and_then(|conds| conds.iter().find(|c| c.type_ == "Ready"))
        .map(|c| c.status == "True")
        .unwrap_or(false)
}

// ============================================================================
// Helpers
// ============================================================================

/// "All" / empty → cluster-wide (None); otherwise a concrete namespace.
pub fn normalize_scope(namespace: Option<String>) -> Option<String> {
    match namespace {
        Some(ns) if ns != "All" && !ns.trim().is_empty() => Some(ns),
        _ => None,
    }
}

async fn list_cluster<K>(client: &kube::Client) -> Result<Vec<K>, CoreError>
where
    K: kube::Resource + Clone + serde::de::DeserializeOwned + std::fmt::Debug,
    <K as kube::Resource>::DynamicType: Default,
{
    let api: Api<K> = Api::all(client.clone());
    api.list(&ListParams::default())
        .await
        .map(|l| l.items)
        .map_err(|e| kube_err(&format!("list {}", short_kind::<K>()), e))
}

async fn list_namespaced<K>(
    client: &kube::Client,
    scope: &Option<String>,
) -> Result<Vec<K>, CoreError>
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
        .map_err(|e| kube_err(&format!("list {}", short_kind::<K>()), e))
}

async fn get_namespaced<K>(
    client: &kube::Client,
    namespace: &str,
    name: &str,
) -> Result<K, CoreError>
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
        .map_err(|e| kube_err(&format!("get {}", short_kind::<K>()), e))
}

async fn get_cluster<K>(client: &kube::Client, name: &str) -> Result<K, CoreError>
where
    K: kube::Resource + Clone + serde::de::DeserializeOwned + std::fmt::Debug,
    <K as kube::Resource>::DynamicType: Default,
{
    let api: Api<K> = Api::all(client.clone());
    api.get(name)
        .await
        .map_err(|e| kube_err(&format!("get {}", short_kind::<K>()), e))
}

/// Last path segment of a type name, e.g. "…::core::v1::Pod" → "Pod".
fn short_kind<K>() -> &'static str {
    std::any::type_name::<K>()
        .rsplit("::")
        .next()
        .unwrap_or("?")
}

/// Project any fetched object into a [`ResourceDetail`] (metadata + JSON manifest).
fn to_detail<K>(mut obj: K, kind: &str) -> Result<ResourceDetail, CoreError>
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
    let manifest =
        serde_json::to_string_pretty(&obj).map_err(|e| CoreError::Other(e.to_string()))?;

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
/// which requires no RBAC and forces a real network round-trip + TLS handshake.
async fn probe_cluster_version(client: &kube::Client) -> Result<String, CoreError> {
    let info = client
        .apiserver_version()
        .await
        .map_err(|e| CoreError::Upstream(e.to_string()))?;
    Ok(if info.git_version.is_empty() {
        format!("v{}.{}", info.major, info.minor)
    } else {
        info.git_version
    })
}

// === Pod / Node projections ===

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
