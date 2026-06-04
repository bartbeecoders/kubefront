//! KubeConfigManager — loads kubeconfig (default + custom paths), lists contexts,
//! detects K3S heuristically, and manages the active kube::Client.
//!
//! MVP: sync loading + listing + K3S detection. Async Client creation added in Phase 1.

use anyhow::{Context, Result};
use home;
use kube::config::{KubeConfigOptions, Kubeconfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, serde::Serialize)]
pub struct ContextInfo {
    pub name: String,
    pub cluster: String,
    pub server: String,
    pub is_k3s: bool,
}

#[derive(Default)]
pub struct KubeConfigManager {
    pub kubeconfig: Option<Kubeconfig>,
    pub path: Option<PathBuf>,
    pub contexts: Vec<ContextInfo>,
    pub current_context: Option<String>,
}

impl KubeConfigManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the default kubeconfig (respects KUBECONFIG env + platform defaults).
    pub fn load_default(&mut self) -> Result<()> {
        let kc = Kubeconfig::read().context("failed to read default kubeconfig")?;

        // Try to record a conventional display path even for default loads
        let display_path = std::env::var("KUBECONFIG")
            .ok()
            .map(PathBuf::from)
            .or_else(|| home::home_dir().map(|h| h.join(".kube/config")));

        self.populate_from(kc, display_path)
    }

    /// Load from an explicit path.
    pub fn load_from_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        let kc = Kubeconfig::read_from(&path)
            .with_context(|| format!("failed to read kubeconfig at {}", path.display()))?;
        self.populate_from(kc, Some(path))
    }

    fn populate_from(&mut self, kc: Kubeconfig, path: Option<PathBuf>) -> Result<()> {
        self.kubeconfig = Some(kc.clone());
        self.path = path;

        let current = kc.current_context.clone();

        // Build lookup: cluster name -> server URL (and cluster struct if needed later)
        let mut cluster_servers: HashMap<String, String> = HashMap::new();
        for nc in &kc.clusters {
            if let Some(cluster) = &nc.cluster {
                if let Some(server) = &cluster.server {
                    cluster_servers.insert(nc.name.clone(), server.clone());
                }
            }
        }

        let mut infos = vec![];
        for named_ctx in &kc.contexts {
            let ctx = named_ctx.context.as_ref();
            let cluster_name = ctx
                .map(|c| c.cluster.clone())
                .unwrap_or_else(|| "unknown".into());
            let server = cluster_servers
                .get(&cluster_name)
                .cloned()
                .unwrap_or_default();

            // Single source of truth for K3S heuristic (AGENTS.md rule)
            let is_k3s = detect_k3s(&named_ctx.name, &cluster_name, &server);

            infos.push(ContextInfo {
                name: named_ctx.name.clone(),
                cluster: cluster_name,
                server,
                is_k3s,
            });
        }

        self.contexts = infos;
        self.current_context = current.or_else(|| self.contexts.first().map(|c| c.name.clone()));
        Ok(())
    }

    pub fn current_info(&self) -> Option<&ContextInfo> {
        let name = self.current_context.as_ref()?;
        self.contexts.iter().find(|c| &c.name == name)
    }

    /// Returns the context name that should be used for connecting (current or first available).
    pub fn effective_context(&self) -> Option<&str> {
        self.current_context
            .as_deref()
            .or_else(|| self.contexts.first().map(|c| c.name.as_str()))
    }

    /// Namespace declared by the effective context in the kubeconfig itself
    /// (`contexts[].context.namespace`), if any. Deploy kubeconfigs for
    /// namespace-scoped users typically set this.
    pub fn current_context_namespace(&self) -> Option<String> {
        let name = self.effective_context()?;
        self.kubeconfig
            .as_ref()?
            .contexts
            .iter()
            .find(|c| c.name == name)
            .and_then(|c| c.context.as_ref())
            .and_then(|c| c.namespace.clone())
    }

    /// Build `KubeConfigOptions` for the currently selected context.
    /// This is what we pass to `Config::from_kubeconfig` when creating a real Client.
    pub fn current_kubeconfig_options(&self) -> Option<KubeConfigOptions> {
        self.effective_context().map(|name| KubeConfigOptions {
            context: Some(name.to_string()),
            cluster: None,
            user: None,
        })
    }
}

/// Heuristic K3S detection (single place — see AGENTS.md).
fn detect_k3s(context_name: &str, cluster_name: &str, server: &str) -> bool {
    let s = server.to_lowercase();
    let name = format!("{} {}", context_name, cluster_name).to_lowercase();

    s.contains(":6443")
        || s.contains("127.0.0.1")
        || s.contains("localhost")
        || name.contains("k3s")
        || name.contains("k3d")
        || (name.contains("default") && s.contains("6443"))
}
