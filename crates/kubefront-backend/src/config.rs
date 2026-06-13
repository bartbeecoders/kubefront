//! `backend.toml` parsing + validation.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Top-level server configuration (`backend.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct BackendConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_base_path")]
    pub base_path: String,
    #[serde(default = "default_timeout")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_tail")]
    pub log_request_tail: i64,
    /// One entry per cluster this backend exposes. `[[connection]]` in TOML.
    #[serde(default, rename = "connection")]
    pub connections: Vec<ConnectionConfig>,
}

/// One exposed cluster connection.
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionConfig {
    /// URL path segment selecting this connection (`/<id>/api/...`).
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Absolute path to the kubeconfig on this host.
    pub kubeconfig: PathBuf,
    /// Context within that kubeconfig.
    pub context: String,
    /// Optional namespace scope; blank/`None` = all namespaces. Surfaced to the
    /// desktop via `GET /status` so it can seed its effective namespace.
    #[serde(default)]
    pub namespace: Option<String>,
    /// When true, destructive verbs (DELETE / restart / configmap PUT) return 403.
    #[serde(default)]
    pub read_only: bool,
}

fn default_listen() -> String {
    "127.0.0.1:8080".into()
}
fn default_base_path() -> String {
    "/".into()
}
fn default_timeout() -> u64 {
    15
}
fn default_tail() -> i64 {
    200
}

impl BackendConfig {
    /// Read and validate the config at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        let cfg: BackendConfig =
            toml::from_str(&text).with_context(|| format!("invalid TOML in {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for c in &self.connections {
            if c.id == "api" {
                bail!(
                    "connection id must not be 'api' (it would collide with the /<id>/api/ route)"
                );
            }
            if !is_valid_id(&c.id) {
                bail!(
                    "invalid connection id '{}': must match ^[a-z0-9][a-z0-9-]{{0,62}}$",
                    c.id
                );
            }
            if !seen.insert(c.id.clone()) {
                bail!("duplicate connection id '{}'", c.id);
            }
        }
        Ok(())
    }
}

/// `^[a-z0-9][a-z0-9-]{0,62}$` — a URL- and DNS-label-safe segment.
fn is_valid_id(id: &str) -> bool {
    let len = id.len();
    if len == 0 || len > 63 {
        return false;
    }
    let bytes = id.as_bytes();
    let first_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
    first_ok
        && bytes
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_ids() {
        assert!(is_valid_id("connection1"));
        assert!(is_valid_id("k3s-server-1"));
        assert!(is_valid_id("a"));
    }

    #[test]
    fn invalid_ids() {
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("-leading"));
        assert!(!is_valid_id("Upper"));
        assert!(!is_valid_id("has space"));
        assert!(!is_valid_id("under_score"));
        assert!(!is_valid_id(&"x".repeat(64)));
    }

    #[test]
    fn parses_example_shape() {
        let cfg: BackendConfig = toml::from_str(
            r#"
            listen = "127.0.0.1:9000"
            [[connection]]
            id = "c1"
            kubeconfig = "/tmp/kc.yaml"
            context = "default"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.listen, "127.0.0.1:9000");
        assert_eq!(cfg.base_path, "/");
        assert_eq!(cfg.connections.len(), 1);
        assert_eq!(cfg.connections[0].id, "c1");
        assert!(!cfg.connections[0].read_only);
        cfg.validate().unwrap();
    }

    #[test]
    fn rejects_reserved_and_dup_ids() {
        let api = toml::from_str::<BackendConfig>(
            "[[connection]]\nid=\"api\"\nkubeconfig=\"/k\"\ncontext=\"d\"\n",
        )
        .unwrap();
        assert!(api.validate().is_err());

        let dup = toml::from_str::<BackendConfig>(
            "[[connection]]\nid=\"c\"\nkubeconfig=\"/k\"\ncontext=\"d\"\n\
             [[connection]]\nid=\"c\"\nkubeconfig=\"/k\"\ncontext=\"d\"\n",
        )
        .unwrap();
        assert!(dup.validate().is_err());
    }
}
