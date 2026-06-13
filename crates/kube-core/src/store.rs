//! Central store for fetched cluster resources + projection helpers.
//!
//! All resource lists (other than Pods/Nodes, which live directly on the app
//! for the legacy detail/log flows) are kept here in [`ClusterResources`].
//! Background tasks deliver updates as a single [`ResourceUpdate`] enum so the
//! event channel doesn't explode with one variant per resource type.
//!
//! Each resource type also has a small projection function that turns a slice
//! into a [`TableData`] (headers + string rows) which the generic table widget
//! in `ui::components` renders. This keeps `app.rs` free of K8s field plumbing.

use chrono::{DateTime, Utc};

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, Namespace, PersistentVolume, PersistentVolumeClaim, Secret, Service, ServiceAccount,
};
use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
use k8s_openapi::api::rbac::v1::{Role, RoleBinding};
use k8s_openapi::api::storage::v1::StorageClass;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};

/// A simple headers + rows projection used by the generic table renderer.
#[derive(serde::Serialize)]
pub struct TableData {
    pub headers: Vec<&'static str>,
    pub rows: Vec<Vec<String>>,
}

// === Shared metadata helpers ===

fn obj_name(m: &ObjectMeta) -> String {
    m.name.clone().unwrap_or_else(|| "unknown".into())
}

fn obj_ns(m: &ObjectMeta) -> String {
    m.namespace.clone().unwrap_or_else(|| "-".into())
}

fn obj_age(m: &ObjectMeta) -> String {
    human_age(m.creation_timestamp.as_ref())
}

/// Human readable age string like Kubernetes (e.g. "4d", "12h", "34m", "5s", "<1s").
/// Shared by the Pods/Nodes views in `app.rs` and every resource projection here.
pub fn human_age(ts: Option<&Time>) -> String {
    let Some(ts) = ts else {
        return "-".into();
    };

    let created_str = ts.0.to_string();
    let created = match DateTime::parse_from_rfc3339(&created_str) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => return created_str,
    };

    let now = Utc::now();
    let dur = now.signed_duration_since(created);

    if dur.num_days() > 0 {
        format!("{}d", dur.num_days())
    } else if dur.num_hours() > 0 {
        format!("{}h", dur.num_hours())
    } else if dur.num_minutes() > 0 {
        format!("{}m", dur.num_minutes())
    } else if dur.num_seconds() > 0 {
        format!("{}s", dur.num_seconds())
    } else {
        "<1s".into()
    }
}

// === Per-resource projections ===

pub fn namespaces_table(items: &[Namespace]) -> TableData {
    TableData {
        headers: vec!["Name", "Status", "Age"],
        rows: items
            .iter()
            .map(|ns| {
                let status = ns
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.clone())
                    .unwrap_or_else(|| "Active".into());
                vec![obj_name(&ns.metadata), status, obj_age(&ns.metadata)]
            })
            .collect(),
    }
}

pub fn services_table(items: &[Service]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Type", "Cluster IP", "Ports", "Age"],
        rows: items
            .iter()
            .map(|svc| {
                let spec = svc.spec.as_ref();
                let typ = spec
                    .and_then(|s| s.type_.clone())
                    .unwrap_or_else(|| "ClusterIP".into());
                let cluster_ip = spec
                    .and_then(|s| s.cluster_ip.clone())
                    .unwrap_or_else(|| "-".into());
                let ports = spec
                    .and_then(|s| s.ports.as_ref())
                    .map(|ports| {
                        ports
                            .iter()
                            .map(|p| {
                                let proto = p.protocol.clone().unwrap_or_else(|| "TCP".into());
                                format!("{}/{}", p.port, proto)
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "-".into());
                vec![
                    obj_name(&svc.metadata),
                    obj_ns(&svc.metadata),
                    typ,
                    cluster_ip,
                    ports,
                    obj_age(&svc.metadata),
                ]
            })
            .collect(),
    }
}

pub fn deployments_table(items: &[Deployment]) -> TableData {
    TableData {
        headers: vec![
            "Name",
            "Namespace",
            "Ready",
            "Up-to-date",
            "Available",
            "Age",
        ],
        rows: items
            .iter()
            .map(|d| {
                let desired = d.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0);
                let status = d.status.as_ref();
                let ready = status.and_then(|s| s.ready_replicas).unwrap_or(0);
                let updated = status.and_then(|s| s.updated_replicas).unwrap_or(0);
                let available = status.and_then(|s| s.available_replicas).unwrap_or(0);
                vec![
                    obj_name(&d.metadata),
                    obj_ns(&d.metadata),
                    format!("{}/{}", ready, desired),
                    updated.to_string(),
                    available.to_string(),
                    obj_age(&d.metadata),
                ]
            })
            .collect(),
    }
}

pub fn statefulsets_table(items: &[StatefulSet]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Ready", "Age"],
        rows: items
            .iter()
            .map(|s| {
                let desired = s.spec.as_ref().and_then(|sp| sp.replicas).unwrap_or(0);
                let ready = s
                    .status
                    .as_ref()
                    .and_then(|st| st.ready_replicas)
                    .unwrap_or(0);
                vec![
                    obj_name(&s.metadata),
                    obj_ns(&s.metadata),
                    format!("{}/{}", ready, desired),
                    obj_age(&s.metadata),
                ]
            })
            .collect(),
    }
}

pub fn daemonsets_table(items: &[DaemonSet]) -> TableData {
    TableData {
        headers: vec![
            "Name",
            "Namespace",
            "Desired",
            "Current",
            "Ready",
            "Up-to-date",
            "Available",
            "Age",
        ],
        rows: items
            .iter()
            .map(|d| {
                let st = d.status.as_ref();
                let desired = st.map(|s| s.desired_number_scheduled).unwrap_or(0);
                let current = st.map(|s| s.current_number_scheduled).unwrap_or(0);
                let ready = st.map(|s| s.number_ready).unwrap_or(0);
                let updated = st.and_then(|s| s.updated_number_scheduled).unwrap_or(0);
                let available = st.and_then(|s| s.number_available).unwrap_or(0);
                vec![
                    obj_name(&d.metadata),
                    obj_ns(&d.metadata),
                    desired.to_string(),
                    current.to_string(),
                    ready.to_string(),
                    updated.to_string(),
                    available.to_string(),
                    obj_age(&d.metadata),
                ]
            })
            .collect(),
    }
}

pub fn jobs_table(items: &[Job]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Completions", "Age"],
        rows: items
            .iter()
            .map(|j| {
                let succeeded = j.status.as_ref().and_then(|s| s.succeeded).unwrap_or(0);
                let completions = j.spec.as_ref().and_then(|s| s.completions).unwrap_or(1);
                vec![
                    obj_name(&j.metadata),
                    obj_ns(&j.metadata),
                    format!("{}/{}", succeeded, completions),
                    obj_age(&j.metadata),
                ]
            })
            .collect(),
    }
}

pub fn cronjobs_table(items: &[CronJob]) -> TableData {
    TableData {
        headers: vec![
            "Name",
            "Namespace",
            "Schedule",
            "Suspend",
            "Active",
            "Last Schedule",
            "Age",
        ],
        rows: items
            .iter()
            .map(|c| {
                let spec = c.spec.as_ref();
                let schedule = spec
                    .map(|s| s.schedule.clone())
                    .unwrap_or_else(|| "-".into());
                let suspend = spec.and_then(|s| s.suspend).unwrap_or(false);
                let active = c
                    .status
                    .as_ref()
                    .and_then(|s| s.active.as_ref())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let last = c
                    .status
                    .as_ref()
                    .and_then(|s| s.last_schedule_time.as_ref())
                    .map(|t| human_age(Some(t)))
                    .unwrap_or_else(|| "-".into());
                vec![
                    obj_name(&c.metadata),
                    obj_ns(&c.metadata),
                    schedule,
                    suspend.to_string(),
                    active.to_string(),
                    last,
                    obj_age(&c.metadata),
                ]
            })
            .collect(),
    }
}

pub fn configmaps_table(items: &[ConfigMap]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Data", "Age"],
        rows: items
            .iter()
            .map(|cm| {
                let data = cm.data.as_ref().map(|d| d.len()).unwrap_or(0)
                    + cm.binary_data.as_ref().map(|d| d.len()).unwrap_or(0);
                vec![
                    obj_name(&cm.metadata),
                    obj_ns(&cm.metadata),
                    data.to_string(),
                    obj_age(&cm.metadata),
                ]
            })
            .collect(),
    }
}

pub fn secrets_table(items: &[Secret]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Type", "Data", "Age"],
        rows: items
            .iter()
            .map(|s| {
                let typ = s.type_.clone().unwrap_or_else(|| "Opaque".into());
                let data = s.data.as_ref().map(|d| d.len()).unwrap_or(0)
                    + s.string_data.as_ref().map(|d| d.len()).unwrap_or(0);
                vec![
                    obj_name(&s.metadata),
                    obj_ns(&s.metadata),
                    typ,
                    data.to_string(),
                    obj_age(&s.metadata),
                ]
            })
            .collect(),
    }
}

pub fn pvcs_table(items: &[PersistentVolumeClaim]) -> TableData {
    TableData {
        headers: vec![
            "Name",
            "Namespace",
            "Status",
            "Volume",
            "Capacity",
            "Storage Class",
            "Age",
        ],
        rows: items
            .iter()
            .map(|p| {
                let status = p
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.clone())
                    .unwrap_or_else(|| "-".into());
                let volume = p
                    .spec
                    .as_ref()
                    .and_then(|s| s.volume_name.clone())
                    .unwrap_or_else(|| "-".into());
                let capacity = p
                    .status
                    .as_ref()
                    .and_then(|s| s.capacity.as_ref())
                    .and_then(|c| c.get("storage"))
                    .map(|q| q.0.clone())
                    .unwrap_or_else(|| "-".into());
                let sc = p
                    .spec
                    .as_ref()
                    .and_then(|s| s.storage_class_name.clone())
                    .unwrap_or_else(|| "-".into());
                vec![
                    obj_name(&p.metadata),
                    obj_ns(&p.metadata),
                    status,
                    volume,
                    capacity,
                    sc,
                    obj_age(&p.metadata),
                ]
            })
            .collect(),
    }
}

pub fn pvs_table(items: &[PersistentVolume]) -> TableData {
    TableData {
        headers: vec![
            "Name",
            "Capacity",
            "Access Modes",
            "Reclaim Policy",
            "Status",
            "Storage Class",
            "Age",
        ],
        rows: items
            .iter()
            .map(|p| {
                let spec = p.spec.as_ref();
                let capacity = spec
                    .and_then(|s| s.capacity.as_ref())
                    .and_then(|c| c.get("storage"))
                    .map(|q| q.0.clone())
                    .unwrap_or_else(|| "-".into());
                let modes = spec
                    .and_then(|s| s.access_modes.as_ref())
                    .map(|m| m.join(","))
                    .unwrap_or_else(|| "-".into());
                let reclaim = spec
                    .and_then(|s| s.persistent_volume_reclaim_policy.clone())
                    .unwrap_or_else(|| "-".into());
                let status = p
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.clone())
                    .unwrap_or_else(|| "-".into());
                let sc = spec
                    .and_then(|s| s.storage_class_name.clone())
                    .unwrap_or_else(|| "-".into());
                vec![
                    obj_name(&p.metadata),
                    capacity,
                    modes,
                    reclaim,
                    status,
                    sc,
                    obj_age(&p.metadata),
                ]
            })
            .collect(),
    }
}

pub fn storage_classes_table(items: &[StorageClass]) -> TableData {
    TableData {
        headers: vec![
            "Name",
            "Provisioner",
            "Reclaim Policy",
            "Volume Binding",
            "Age",
        ],
        rows: items
            .iter()
            .map(|sc| {
                vec![
                    obj_name(&sc.metadata),
                    sc.provisioner.clone(),
                    sc.reclaim_policy.clone().unwrap_or_else(|| "-".into()),
                    sc.volume_binding_mode.clone().unwrap_or_else(|| "-".into()),
                    obj_age(&sc.metadata),
                ]
            })
            .collect(),
    }
}

pub fn ingresses_table(items: &[Ingress]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Class", "Hosts", "Age"],
        rows: items
            .iter()
            .map(|ing| {
                let spec = ing.spec.as_ref();
                let class = spec
                    .and_then(|s| s.ingress_class_name.clone())
                    .unwrap_or_else(|| "-".into());
                let hosts = spec
                    .and_then(|s| s.rules.as_ref())
                    .map(|rules| {
                        let hs: Vec<String> = rules.iter().filter_map(|r| r.host.clone()).collect();
                        if hs.is_empty() {
                            "*".into()
                        } else {
                            hs.join(", ")
                        }
                    })
                    .unwrap_or_else(|| "*".into());
                vec![
                    obj_name(&ing.metadata),
                    obj_ns(&ing.metadata),
                    class,
                    hosts,
                    obj_age(&ing.metadata),
                ]
            })
            .collect(),
    }
}

pub fn network_policies_table(items: &[NetworkPolicy]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Pod Selector", "Age"],
        rows: items
            .iter()
            .map(|np| {
                let selector = np
                    .spec
                    .as_ref()
                    .and_then(|s| s.pod_selector.as_ref())
                    .and_then(|sel| sel.match_labels.as_ref())
                    .map(|labels| {
                        if labels.is_empty() {
                            "<all pods>".into()
                        } else {
                            labels
                                .iter()
                                .map(|(k, v)| format!("{}={}", k, v))
                                .collect::<Vec<_>>()
                                .join(",")
                        }
                    })
                    .unwrap_or_else(|| "<all pods>".into());
                vec![
                    obj_name(&np.metadata),
                    obj_ns(&np.metadata),
                    selector,
                    obj_age(&np.metadata),
                ]
            })
            .collect(),
    }
}

pub fn service_accounts_table(items: &[ServiceAccount]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Secrets", "Age"],
        rows: items
            .iter()
            .map(|sa| {
                let secrets = sa.secrets.as_ref().map(|s| s.len()).unwrap_or(0);
                vec![
                    obj_name(&sa.metadata),
                    obj_ns(&sa.metadata),
                    secrets.to_string(),
                    obj_age(&sa.metadata),
                ]
            })
            .collect(),
    }
}

pub fn roles_table(items: &[Role]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Rules", "Age"],
        rows: items
            .iter()
            .map(|r| {
                let rules = r.rules.as_ref().map(|v| v.len()).unwrap_or(0);
                vec![
                    obj_name(&r.metadata),
                    obj_ns(&r.metadata),
                    rules.to_string(),
                    obj_age(&r.metadata),
                ]
            })
            .collect(),
    }
}

pub fn role_bindings_table(items: &[RoleBinding]) -> TableData {
    TableData {
        headers: vec!["Name", "Namespace", "Role", "Subjects", "Age"],
        rows: items
            .iter()
            .map(|rb| {
                let role = format!("{}/{}", rb.role_ref.kind, rb.role_ref.name);
                let subjects = rb
                    .subjects
                    .as_ref()
                    .map(|s| {
                        s.iter()
                            .map(|sub| sub.name.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "-".into());
                vec![
                    obj_name(&rb.metadata),
                    obj_ns(&rb.metadata),
                    role,
                    subjects,
                    obj_age(&rb.metadata),
                ]
            })
            .collect(),
    }
}
