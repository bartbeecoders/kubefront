//! The active connection — either a Local (direct kube::Client) or Remote (HTTP
//! to a backend) transport. Every mode-aware command goes through here; the
//! `match` is the entire cost of dual-transport. All methods normalize to
//! `Result<T, String>` (the shape the frontend expects), mapping `CoreError`
//! `Display` to a string identical to what the backend returns.

use std::collections::BTreeMap;

use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use kubefront_core::{LocalKube, LogEvent, NodeRow, PodRow, ResourceDetail, TableData};

use crate::remote::RemoteKube;

/// The active cluster connection.
#[derive(Clone)]
pub enum Active {
    Local(LocalKube),
    Remote(RemoteKube),
}

impl Active {
    pub fn cluster_version(&self) -> String {
        match self {
            Active::Local(l) => l.cluster_version().to_string(),
            Active::Remote(r) => r.cluster_version().to_string(),
        }
    }

    /// Remote endpoint base URL, if this is a Remote connection.
    pub fn endpoint(&self) -> Option<&str> {
        match self {
            Active::Remote(r) => Some(r.base()),
            Active::Local(_) => None,
        }
    }

    pub async fn list_pods(&self, ns: Option<&str>) -> Result<Vec<PodRow>, String> {
        match self {
            Active::Local(l) => l.list_pods(ns).await.map_err(|e| e.to_string()),
            Active::Remote(r) => r.list_pods(ns).await,
        }
    }

    pub async fn list_nodes(&self) -> Result<Vec<NodeRow>, String> {
        match self {
            Active::Local(l) => l.list_nodes().await.map_err(|e| e.to_string()),
            Active::Remote(r) => r.list_nodes().await,
        }
    }

    pub async fn list_resource(&self, kind: &str, ns: Option<&str>) -> Result<TableData, String> {
        match self {
            Active::Local(l) => l.list_resource(kind, ns).await.map_err(|e| e.to_string()),
            Active::Remote(r) => r.list_resource(kind, ns).await,
        }
    }

    pub async fn get_resource(
        &self,
        kind: &str,
        ns: Option<&str>,
        name: &str,
    ) -> Result<ResourceDetail, String> {
        match self {
            Active::Local(l) => l
                .get_resource(kind, ns, name)
                .await
                .map_err(|e| e.to_string()),
            Active::Remote(r) => r.get_resource(kind, ns, name).await,
        }
    }

    pub async fn delete_resource(
        &self,
        kind: &str,
        ns: Option<&str>,
        name: &str,
    ) -> Result<(), String> {
        match self {
            Active::Local(l) => l
                .delete_resource(kind, ns, name)
                .await
                .map_err(|e| e.to_string()),
            Active::Remote(r) => r.delete_resource(kind, ns, name).await,
        }
    }

    pub async fn restart_resource(
        &self,
        kind: &str,
        ns: Option<&str>,
        name: &str,
    ) -> Result<(), String> {
        match self {
            Active::Local(l) => l
                .restart_resource(kind, ns, name)
                .await
                .map_err(|e| e.to_string()),
            Active::Remote(r) => r.restart_resource(kind, ns, name).await,
        }
    }

    pub async fn update_configmap(
        &self,
        namespace: &str,
        name: &str,
        data: BTreeMap<String, String>,
    ) -> Result<(), String> {
        match self {
            Active::Local(l) => l
                .update_configmap(namespace, name, data)
                .await
                .map_err(|e| e.to_string()),
            Active::Remote(r) => r.update_configmap(namespace, name, data).await,
        }
    }

    pub async fn describe_pod(&self, namespace: &str, name: &str) -> Result<String, String> {
        match self {
            Active::Local(l) => l
                .describe_pod(namespace, name)
                .await
                .map_err(|e| e.to_string()),
            Active::Remote(r) => r.describe_pod(namespace, name).await,
        }
    }

    /// A boxed log stream for either transport, pumped into the Tauri Channel by
    /// `stream_logs`.
    pub fn log_stream(
        &self,
        namespace: String,
        pod: String,
        container: Option<String>,
        tail: i64,
    ) -> BoxStream<'static, LogEvent> {
        match self {
            Active::Local(l) => l.log_stream(&namespace, &pod, container, tail).boxed(),
            Active::Remote(r) => r.log_stream(namespace, pod, container, tail).boxed(),
        }
    }
}
