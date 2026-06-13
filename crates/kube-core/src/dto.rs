//! Serializable DTOs that cross the IPC (desktop) and HTTP (backend) boundaries.
//!
//! Every type here derives BOTH `Serialize` and `Deserialize` so the desktop
//! app can deserialize the exact same shapes the backend serializes. The
//! TypeScript mirrors live in `src/types.ts`.

use serde::{Deserialize, Serialize};

pub use crate::manager::ContextInfo;
pub use crate::store::TableData;

/// Lightweight handshake returned by the backend's `GET /status`. Lets the
/// desktop build a [`KubeStatus`] for a remote connection (which has no local
/// kubeconfig of its own). `namespace` is the connection's configured scope, used
/// to seed the desktop's effective namespace so namespace-scoped backends work.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendStatus {
    pub connected: bool,
    pub cluster_version: String,
    pub namespace: Option<String>,
}

/// Snapshot of connection + kubeconfig state, returned by most config commands.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KubeStatus {
    pub connected: bool,
    pub cluster_version: Option<String>,
    pub current_context: Option<String>,
    pub kubeconfig_path: Option<String>,
    pub context_count: usize,
    pub contexts: Vec<ContextInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRow {
    pub name: String,
    pub status: String,
    pub roles: String,
    pub version: String,
    pub age: String,
}

/// Full detail for a single selected resource of any kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub kind: String,
    pub line: String,
}

impl LogEvent {
    pub fn header(line: impl Into<String>) -> Self {
        Self {
            kind: "header".into(),
            line: line.into(),
        }
    }
    pub fn line(line: impl Into<String>) -> Self {
        Self {
            kind: "line".into(),
            line: line.into(),
        }
    }
    pub fn error(line: impl Into<String>) -> Self {
        Self {
            kind: "error".into(),
            line: line.into(),
        }
    }
    pub fn ended() -> Self {
        Self {
            kind: "ended".into(),
            line: String::new(),
        }
    }
}

/// Live health snapshot for one cluster card on the Dashboard.
/// Counts are `None` when that list is not permitted (RBAC) — the UI shows "—".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterSummary {
    pub reachable: bool,
    pub version: Option<String>,
    pub nodes_total: Option<u32>,
    pub nodes_ready: Option<u32>,
    pub pods_total: Option<u32>,
    pub pods_running: Option<u32>,
    pub namespaces: Option<u32>,
    pub deployments: Option<u32>,
    pub error: Option<String>,
}

impl ClusterSummary {
    /// An "unreachable" summary carrying only an error message.
    pub fn unreachable(err: impl Into<String>) -> Self {
        Self {
            reachable: false,
            version: None,
            nodes_total: None,
            nodes_ready: None,
            pods_total: None,
            pods_running: None,
            namespaces: None,
            deployments: None,
            error: Some(err.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The backend serializes these DTOs and the desktop `RemoteKube` deserializes
    /// them — both via THESE structs — so a JSON round-trip must be lossless.
    #[test]
    fn table_data_round_trips() {
        let t = TableData {
            headers: vec!["Name".into(), "Age".into()],
            rows: vec![vec!["pod-a".into(), "3d".into()]],
        };
        let json = serde_json::to_string(&t).unwrap();
        // headers must be a JSON array of strings the TS `string[]` mirror expects.
        assert!(json.contains(r#""headers":["Name","Age"]"#));
        let back: TableData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.headers, t.headers);
        assert_eq!(back.rows, t.rows);
    }

    #[test]
    fn resource_detail_labels_are_pairs() {
        let d = ResourceDetail {
            kind: "pods".into(),
            name: "p".into(),
            namespace: Some("ns".into()),
            age: "1d".into(),
            labels: vec![("app".into(), "web".into())],
            annotations: vec![],
            manifest: "{}".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        // labels mirror the TS `[string, string][]` (array of two-element arrays).
        assert!(json.contains(r#""labels":[["app","web"]]"#));
        let back: ResourceDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(back.labels, d.labels);
        assert_eq!(back.namespace, d.namespace);
    }

    #[test]
    fn log_event_and_status_round_trip() {
        let ev = LogEvent::line("hello");
        let back: LogEvent = serde_json::from_str(&serde_json::to_string(&ev).unwrap()).unwrap();
        assert_eq!(back.kind, "line");
        assert_eq!(back.line, "hello");

        let st = BackendStatus {
            connected: true,
            cluster_version: "v1.29.4+k3s1".into(),
            namespace: Some("apps".into()),
        };
        let back: BackendStatus =
            serde_json::from_str(&serde_json::to_string(&st).unwrap()).unwrap();
        assert_eq!(back.cluster_version, st.cluster_version);
        assert_eq!(back.namespace, st.namespace);
    }
}
