//! Kubernetes client management, resource fetching, and projections.
//! All heavy lifting (kube-rs calls, async, config parsing) lives here.
//! UI-agnostic: every type that crosses the IPC boundary is `serde`-serializable.

pub mod manager;
pub mod store;
