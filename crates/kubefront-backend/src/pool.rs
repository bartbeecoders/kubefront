//! Lazy connection pool: one [`LocalKube`] per configured connection, built on
//! first use and cached. Each connection has its OWN lock, so a slow (or hanging)
//! connect to one cluster never blocks requests to another.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use kube::config::{KubeConfigOptions, Kubeconfig};
use kubefront_core::{CoreError, LocalKube};
use tokio::sync::RwLock;

use crate::config::{BackendConfig, ConnectionConfig};

/// One pooled connection: its static config + a lazily-built, cached client.
pub struct ConnSlot {
    pub cfg: ConnectionConfig,
    client: RwLock<Option<LocalKube>>,
    connect_timeout: Duration,
}

impl ConnSlot {
    /// Return the connected client, building + caching it on first use. The build
    /// happens under the write lock with a double-check so concurrent first
    /// requests don't each open a client. Build errors are NOT cached.
    pub async fn client(&self) -> Result<LocalKube, CoreError> {
        if let Some(k) = self.client.read().await.as_ref() {
            return Ok(k.clone());
        }
        let mut guard = self.client.write().await;
        if let Some(k) = guard.as_ref() {
            return Ok(k.clone());
        }
        let kc = Kubeconfig::read_from(&self.cfg.kubeconfig).map_err(|e| {
            let err = CoreError::Kubeconfig(format!(
                "kubeconfig error reading {}: {e}",
                self.cfg.kubeconfig.display()
            ));
            // The HTTP layer only surfaces this in the response body; log it so the
            // operator sees WHY a request 502'd, not just the status code.
            tracing::error!("connection '{}': {err}", self.cfg.id);
            err
        })?;
        let opts = KubeConfigOptions {
            context: Some(self.cfg.context.clone()),
            cluster: None,
            user: None,
        };
        let local = LocalKube::connect_from(kc, opts, self.connect_timeout)
            .await
            .map_err(|e| {
                tracing::error!(
                    "connection '{}' (context '{}') failed to connect: {e}",
                    self.cfg.id,
                    self.cfg.context
                );
                e
            })?;
        *guard = Some(local.clone());
        Ok(local)
    }
}

/// All configured connections, keyed by id.
pub struct ConnectionPool {
    slots: HashMap<String, Arc<ConnSlot>>,
    log_tail: i64,
}

impl ConnectionPool {
    pub fn from_config(cfg: &BackendConfig) -> Self {
        let connect_timeout = Duration::from_secs(cfg.request_timeout_secs);
        let slots = cfg
            .connections
            .iter()
            .map(|c| {
                (
                    c.id.clone(),
                    Arc::new(ConnSlot {
                        cfg: c.clone(),
                        client: RwLock::new(None),
                        connect_timeout,
                    }),
                )
            })
            .collect();
        Self {
            slots,
            log_tail: cfg.log_request_tail,
        }
    }

    /// The slot for `id`, or `None` if no such connection is configured (404).
    pub fn slot(&self, id: &str) -> Option<Arc<ConnSlot>> {
        self.slots.get(id).cloned()
    }

    /// Default trailing-line count for the logs endpoint.
    pub fn log_tail(&self) -> i64 {
        self.log_tail
    }
}
