//! Resource projection helpers shared by the desktop app and the backend.
//!
//! Each resource type has a small projection function that turns a slice into a
//! [`TableData`] (headers + string rows) which the generic table renderer shows.
//! This keeps the command/handler layers free of K8s field plumbing.

use chrono::{DateTime, Utc};

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, Event, Namespace, PersistentVolume, PersistentVolumeClaim, Pod, Secret, Service,
    ServiceAccount,
};
use k8s_openapi::api::networking::v1::{Ingress, IngressClass, NetworkPolicy};
use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, Role, RoleBinding};
use k8s_openapi::api::storage::v1::StorageClass;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};

/// A simple headers + rows projection used by the generic table renderer.
/// `headers` is owned (`Vec<String>`) so the type round-trips over JSON (the
/// backend serializes it; the desktop `RemoteKube` deserializes it).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TableData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Build a `Vec<String>` of column headers from string literals — keeps the
/// projection call-sites terse now that `headers` is owned.
macro_rules! hdr {
    ($($h:expr),* $(,)?) => { vec![$($h.to_string()),*] };
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
        headers: hdr!["Name", "Status", "Age"],
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
        headers: hdr!["Name", "Namespace", "Type", "Cluster IP", "Ports", "Age"],
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
        headers: hdr![
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
        headers: hdr!["Name", "Namespace", "Ready", "Age"],
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
        headers: hdr![
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
        headers: hdr!["Name", "Namespace", "Completions", "Age"],
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
        headers: hdr![
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
        headers: hdr!["Name", "Namespace", "Data", "Age"],
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
        headers: hdr!["Name", "Namespace", "Type", "Data", "Age"],
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
        headers: hdr![
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
        headers: hdr![
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
        headers: hdr![
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
        headers: hdr!["Name", "Namespace", "Class", "Hosts", "Age"],
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
        headers: hdr!["Name", "Namespace", "Pod Selector", "Age"],
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
        headers: hdr!["Name", "Namespace", "Secrets", "Age"],
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
        headers: hdr!["Name", "Namespace", "Rules", "Age"],
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

pub fn crds_table(items: &[CustomResourceDefinition]) -> TableData {
    TableData {
        headers: hdr!["Name", "Group", "Kind", "Scope", "Version", "Age"],
        rows: items
            .iter()
            .map(|crd| {
                let spec = &crd.spec;
                // Prefer the storage version, falling back to the first served one.
                let version = spec
                    .versions
                    .iter()
                    .find(|v| v.storage)
                    .or_else(|| spec.versions.first())
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| "-".into());
                vec![
                    obj_name(&crd.metadata),
                    spec.group.clone(),
                    spec.names.kind.clone(),
                    spec.scope.clone(),
                    version,
                    obj_age(&crd.metadata),
                ]
            })
            .collect(),
    }
}

pub fn role_bindings_table(items: &[RoleBinding]) -> TableData {
    TableData {
        headers: hdr!["Name", "Namespace", "Role", "Subjects", "Age"],
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

pub fn ingress_classes_table(items: &[IngressClass]) -> TableData {
    TableData {
        headers: hdr!["Name", "Controller", "Age"],
        rows: items
            .iter()
            .map(|ic| {
                let controller = ic
                    .spec
                    .as_ref()
                    .and_then(|s| s.controller.clone())
                    .unwrap_or_else(|| "-".into());
                vec![obj_name(&ic.metadata), controller, obj_age(&ic.metadata)]
            })
            .collect(),
    }
}

pub fn cluster_roles_table(items: &[ClusterRole]) -> TableData {
    TableData {
        headers: hdr!["Name", "Rules", "Age"],
        rows: items
            .iter()
            .map(|cr| {
                let rules = cr.rules.as_ref().map(|v| v.len()).unwrap_or(0);
                vec![
                    obj_name(&cr.metadata),
                    rules.to_string(),
                    obj_age(&cr.metadata),
                ]
            })
            .collect(),
    }
}

pub fn cluster_role_bindings_table(items: &[ClusterRoleBinding]) -> TableData {
    TableData {
        headers: hdr!["Name", "Role", "Subjects", "Age"],
        rows: items
            .iter()
            .map(|crb| {
                let role = format!("{}/{}", crb.role_ref.kind, crb.role_ref.name);
                let subjects = crb
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
                    obj_name(&crb.metadata),
                    role,
                    subjects,
                    obj_age(&crb.metadata),
                ]
            })
            .collect(),
    }
}

// === Pod describe (kubectl-describe-style text) ===

/// Build a human-readable `kubectl describe`-style report for a pod. `events`
/// are the pod's involvedObject events (best-effort; may be empty). The output
/// is plain monospace text rendered in a modal.
pub fn describe_pod(pod: &Pod, events: &[Event]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let m = &pod.metadata;

    let _ = writeln!(s, "Name:             {}", m.name.as_deref().unwrap_or("-"));
    let _ = writeln!(
        s,
        "Namespace:        {}",
        m.namespace.as_deref().unwrap_or("-")
    );

    if let Some(spec) = &pod.spec {
        if let Some(pc) = &spec.priority_class_name {
            let _ = writeln!(s, "Priority Class:   {pc}");
        }
        if let Some(sa) = &spec.service_account_name {
            let _ = writeln!(s, "Service Account:  {sa}");
        }
        let _ = writeln!(
            s,
            "Node:             {}",
            spec.node_name.as_deref().unwrap_or("<none>")
        );
    }

    if let Some(st) = &pod.status {
        if let Some(start) = &st.start_time {
            let _ = writeln!(s, "Start Time:       {}", start.0);
        }
    }
    let _ = write!(s, "Labels:           ");
    write_map_block(&mut s, m.labels.as_ref(), 18);
    let _ = write!(s, "Annotations:      ");
    write_map_block(&mut s, m.annotations.as_ref(), 18);

    if let Some(st) = &pod.status {
        let _ = writeln!(
            s,
            "Status:           {}",
            st.phase.as_deref().unwrap_or("-")
        );
        if let Some(reason) = &st.reason {
            let _ = writeln!(s, "Reason:           {reason}");
        }
        if let Some(ip) = &st.pod_ip {
            let _ = writeln!(s, "IP:               {ip}");
        }
        if let Some(qos) = &st.qos_class {
            let _ = writeln!(s, "QoS Class:        {qos}");
        }
    }

    // --- Containers ---
    if let Some(spec) = &pod.spec {
        let statuses = pod
            .status
            .as_ref()
            .and_then(|s| s.container_statuses.as_ref());
        let _ = writeln!(s, "Containers:");
        for c in &spec.containers {
            let cs = statuses.and_then(|all| all.iter().find(|x| x.name == c.name));
            let _ = writeln!(s, "  {}:", c.name);
            let _ = writeln!(
                s,
                "    Image:          {}",
                c.image.as_deref().unwrap_or("-")
            );
            if let Some(ports) = &c.ports {
                let p = ports
                    .iter()
                    .map(|p| {
                        format!(
                            "{}/{}",
                            p.container_port,
                            p.protocol.as_deref().unwrap_or("TCP")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(s, "    Port(s):        {p}");
            }
            if let Some(cs) = cs {
                let (state, detail) = container_state(cs);
                let _ = writeln!(s, "    State:          {state}");
                if let Some(d) = detail {
                    let _ = writeln!(s, "      {d}");
                }
                let _ = writeln!(s, "    Ready:          {}", cs.ready);
                let _ = writeln!(s, "    Restart Count:  {}", cs.restart_count);
            }
            if let Some(res) = &c.resources {
                write_quantities(&mut s, "Requests", res.requests.as_ref());
                write_quantities(&mut s, "Limits", res.limits.as_ref());
            }
        }
    }

    // --- Conditions ---
    if let Some(conds) = pod.status.as_ref().and_then(|s| s.conditions.as_ref()) {
        if !conds.is_empty() {
            let _ = writeln!(s, "Conditions:");
            let _ = writeln!(s, "  {:<18}Status", "Type");
            for c in conds {
                let _ = writeln!(s, "  {:<18}{}", c.type_, c.status);
            }
        }
    }

    // --- Events --- (the part that makes describe worth it)
    let _ = writeln!(s, "Events:");
    if events.is_empty() {
        let _ = writeln!(s, "  <none>");
    } else {
        // Newest last, like kubectl (sort by last timestamp ascending).
        let mut evs: Vec<&Event> = events.iter().collect();
        evs.sort_by_key(|a| event_time(a));
        let _ = writeln!(
            s,
            "  {:<8}{:<22}{:<7}{:<24}Message",
            "Type", "Reason", "Age", "From"
        );
        for e in evs {
            let age = human_age(e.last_timestamp.as_ref().or(e.first_timestamp.as_ref()));
            let count = e.count.unwrap_or(1);
            let age = if count > 1 {
                format!("{age} (x{count})")
            } else {
                age
            };
            let from = e
                .source
                .as_ref()
                .and_then(|src| src.component.clone())
                .unwrap_or_else(|| "-".into());
            let _ = writeln!(
                s,
                "  {:<8}{:<22}{:<7}{:<24}{}",
                e.type_.as_deref().unwrap_or("-"),
                e.reason.as_deref().unwrap_or("-"),
                age,
                from,
                e.message.as_deref().unwrap_or("-").trim()
            );
        }
    }

    s
}

/// Container state as (one-word state, optional detail line).
fn container_state(cs: &k8s_openapi::api::core::v1::ContainerStatus) -> (String, Option<String>) {
    let Some(state) = &cs.state else {
        return ("Unknown".into(), None);
    };
    if let Some(r) = &state.running {
        let started = r.started_at.as_ref().map(|t| format!("Started: {}", t.0));
        ("Running".into(), started)
    } else if let Some(w) = &state.waiting {
        let reason = w.reason.clone().unwrap_or_default();
        let detail = w.message.clone().map(|m| format!("Message: {m}"));
        (format!("Waiting ({reason})"), detail)
    } else if let Some(t) = &state.terminated {
        let reason = t.reason.clone().unwrap_or_default();
        (
            format!("Terminated ({reason})"),
            Some(format!("Exit Code: {}", t.exit_code)),
        )
    } else {
        ("Unknown".into(), None)
    }
}

/// Write a `requests`/`limits` quantity block (indented under a container).
fn write_quantities(
    s: &mut String,
    label: &str,
    q: Option<
        &std::collections::BTreeMap<
            String,
            k8s_openapi::apimachinery::pkg::api::resource::Quantity,
        >,
    >,
) {
    use std::fmt::Write;
    let Some(q) = q else { return };
    if q.is_empty() {
        return;
    }
    let _ = writeln!(s, "    {label}:");
    for (k, v) in q {
        let _ = writeln!(s, "      {k}: {}", v.0);
    }
}

/// Render a metadata map inline (`k=v` per line, aligned under a header column).
fn write_map_block(
    s: &mut String,
    map: Option<&std::collections::BTreeMap<String, String>>,
    indent: usize,
) {
    use std::fmt::Write;
    match map {
        Some(m) if !m.is_empty() => {
            let pad = " ".repeat(indent);
            for (i, (k, v)) in m.iter().enumerate() {
                if i == 0 {
                    let _ = writeln!(s, "{k}={v}");
                } else {
                    let _ = writeln!(s, "{pad}{k}={v}");
                }
            }
        }
        _ => {
            let _ = writeln!(s, "<none>");
        }
    }
}

/// Best timestamp to sort/age an event by.
fn event_time(e: &Event) -> String {
    e.last_timestamp
        .as_ref()
        .or(e.first_timestamp.as_ref())
        .map(|t| t.0.to_string())
        .unwrap_or_default()
}
