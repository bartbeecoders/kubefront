//! Kubernetes client management, resource fetching, and background tasks.
//! All heavy lifting (kube-rs calls, async, config parsing) lives here.

pub mod manager;
pub mod resources;
pub mod tasks;