//! `kube-core` — the shared Kubernetes layer for KubeFront.
//!
//! All heavy lifting (kube-rs calls, async, kubeconfig parsing, resource
//! projections) lives here so it can be used by BOTH the desktop app
//! (`src-tauri`, direct path) and the headless `kubefront-backend` server.
//!
//! UI- and transport-agnostic: every type that crosses an IPC or HTTP boundary
//! is a `serde` `Serialize + Deserialize` DTO in [`dto`]. No `tauri`, `axum`, or
//! `reqwest` deps here.
//!
//! - [`manager::KubeConfigManager`] — load kubeconfigs, list contexts, detect K3S.
//! - [`LocalKube`] — owns a live `kube::Client` and performs every operation.
//! - [`log_stream`] — a transport-agnostic `Stream<Item = LogEvent>` of pod logs.
//! - [`store`] — `kubectl`-style table projections + `describe_pod`.

pub mod dto;
pub mod error;
pub mod local;
pub mod logstream;
pub mod manager;
pub mod store;

pub use dto::{
    BackendStatus, ClusterSummary, KubeStatus, LogEvent, NodeRow, PodRow, ResourceDetail, TableData,
};
pub use error::CoreError;
pub use local::{normalize_scope, summarize, LocalKube};
pub use logstream::log_stream;
pub use manager::{ContextInfo, KubeConfigManager};
