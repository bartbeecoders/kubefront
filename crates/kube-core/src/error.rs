//! `CoreError` — the single error type for every k8s operation.
//!
//! Two consumers need different things from the same error:
//! - the desktop app maps it to a `String` (the frontend matches on
//!   `Result<T, String>` text), so `Display` MUST reproduce today's exact
//!   messages — see the golden tests at the bottom;
//! - the backend maps it to an HTTP status via [`CoreError::http_status`] and
//!   echoes `Display` in the JSON `{error}` body, so a remote error deserializes
//!   to a `String` byte-identical to the local one.

/// Error from any Kubernetes operation. `Display` is the user-facing message.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Timeout(String),
    #[error("{0}")]
    Upstream(String),
    #[error("{0}")]
    Kubeconfig(String),
    #[error("Not connected to a cluster")]
    NotConnected,
    #[error("{0}")]
    Other(String),
}

impl CoreError {
    /// HTTP status the backend returns for this error.
    pub fn http_status(&self) -> u16 {
        match self {
            CoreError::NotFound(_) => 404,
            CoreError::Forbidden(_) => 403,
            CoreError::Timeout(_) => 504,
            CoreError::Upstream(_) | CoreError::Kubeconfig(_) | CoreError::NotConnected => 502,
            CoreError::Other(_) => 500,
        }
    }
}

/// Classify a `kube::Error` by its API status code while preserving the exact
/// `Display` string (so local and remote error text stays identical).
impl From<kube::Error> for CoreError {
    fn from(e: kube::Error) -> Self {
        let msg = e.to_string();
        match e {
            kube::Error::Api(resp) => match resp.code {
                404 => CoreError::NotFound(msg),
                403 => CoreError::Forbidden(msg),
                _ => CoreError::Upstream(msg),
            },
            _ => CoreError::Upstream(msg),
        }
    }
}

/// Stringify a kube error AND log it — list/get/delete failures must never be
/// silent (e.g. RBAC 403s on cluster-wide lists for namespace-scoped users).
/// Returns a classified [`CoreError`] whose `Display` equals the kube message.
pub(crate) fn kube_err(what: &str, e: kube::Error) -> CoreError {
    let ce = CoreError::from(e);
    tracing::warn!("{what} failed: {ce}");
    ce
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_connected_message_is_stable() {
        assert_eq!(
            CoreError::NotConnected.to_string(),
            "Not connected to a cluster"
        );
    }

    #[test]
    fn passthrough_variants_display_inner() {
        assert_eq!(
            CoreError::Other("A namespace is required for this resource".into()).to_string(),
            "A namespace is required for this resource"
        );
        assert_eq!(
            CoreError::Other("Unknown resource kind: widgets".into()).to_string(),
            "Unknown resource kind: widgets"
        );
        assert_eq!(
            CoreError::Kubeconfig("kubeconfig error: boom".into()).to_string(),
            "kubeconfig error: boom"
        );
    }

    #[test]
    fn http_status_mapping() {
        assert_eq!(CoreError::NotFound("x".into()).http_status(), 404);
        assert_eq!(CoreError::Forbidden("x".into()).http_status(), 403);
        assert_eq!(CoreError::Timeout("x".into()).http_status(), 504);
        assert_eq!(CoreError::Upstream("x".into()).http_status(), 502);
        assert_eq!(CoreError::NotConnected.http_status(), 502);
        assert_eq!(CoreError::Other("x".into()).http_status(), 500);
    }
}
