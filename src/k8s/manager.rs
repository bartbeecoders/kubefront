//! KubeConfigManager — loads kubeconfig (default + custom paths), lists contexts,
//! detects K3S heuristically, and manages the active kube::Client.
//!
//! MVP: sync loading + listing + K3S detection. Async Client creation added in Phase 1.

use anyhow::{Context, Result};
use kube::config::{Kubeconfig, KubeConfigOptions};
use home;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ContextInfo {
    pub name: String,
    pub cluster: String,
    pub server: String,
    pub is_k3s: bool,
}

pub struct KubeConfigManager {
    pub kubeconfig: Option<Kubeconfig>,
    pub path: Option<PathBuf>,
    pub contexts: Vec<ContextInfo>,
    pub current_context: Option<String>,
}

impl Default for KubeConfigManager {
    fn default() -> Self {
        Self {
            kubeconfig: None,
            path: None,
            contexts: vec![],
            current_context: None,
        }
    }
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
        let kc = Kubeconfig::read_from(&path).with_context(|| format!("failed to read kubeconfig at {}", path.display()))?;
        self.populate_from(kc, Some(path))
    }

    fn populate_from(&mut self, kc: Kubeconfig, path: Option<PathBuf>) -> Result<()> {
        self.kubeconfig = Some(kc.clone());
        self.path = path;

        let current = kc.current_context.clone();

        let mut infos = vec![];
        for named_ctx in &kc.contexts {
            // Ultra-minimal and safe for Phase 1.
            // We always have the context name. Cluster/server enrichment can be improved later
            // once we lock down the exact shape of kube 3.x Kubeconfig types.
            let cluster_name = "cluster".to_string();

            // Name-based K3S detection is surprisingly effective for most real-world K3S setups.
            let is_k3s = named_ctx.name.to_lowercase().contains("k3s")
                || named_ctx.name.to_lowercase().contains("k3d")
                || named_ctx.name.to_lowercase() == "default";

            infos.push(ContextInfo {
                name: named_ctx.name.clone(),
                cluster: cluster_name,
                server: String::new(),
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
        self.current_context.as_deref().or_else(|| {
            self.contexts.first().map(|c| c.name.as_str())
        })
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

    /// Asynchronously create a real `kube::Client` for the effective context.
    /// Must be called from within a Tokio runtime (we spawn it from the background runtime).
    pub async fn create_client_for_current(&self) -> Result<kube::Client> {
        let opts = self
            .current_kubeconfig_options()
            .context("no context selected in kubeconfig")?;

        let config = kube::Config::from_kubeconfig(&opts)
            .await
            .context("failed to build Config from kubeconfig for selected context")?;

        let client = kube::Client::try_from(config)
            .context("failed to construct kube::Client")?;

        Ok(client)
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