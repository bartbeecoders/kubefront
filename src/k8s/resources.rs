//! Typed resource helpers (Pods, Nodes, etc.) and projections.
//! Used by the UI views.

use k8s_openapi::api::core::v1::{Node, Pod};

pub fn pod_status(pod: &Pod) -> String {
    pod.status
        .as_ref()
        .and_then(|s| s.phase.clone())
        .unwrap_or_else(|| "Unknown".into())
}

// More helpers added in Phase 2
pub fn node_roles(_node: &Node) -> String {
    "control-plane".into()
}