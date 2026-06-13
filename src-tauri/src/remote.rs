//! [`RemoteKube`] — the desktop's HTTP transport to a `kubefront-backend` behind
//! a reverse proxy on :443. Mirrors [`kubefront_core::LocalKube`]'s surface but
//! issues REST calls and deserializes the SAME DTOs, so a Remote connection is
//! indistinguishable from a Direct one to the rest of the app. Errors are
//! re-surfaced as the backend's `{error}` string → byte-identical to Local.
//!
//! TLS uses reqwest's `native-tls-vendored` = the SAME vendored OpenSSL the kube
//! client links against. Optional per-connection custom CA + insecure-skip-verify
//! support OT reverse proxies with an internal / self-signed certificate.

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use kubefront_core::{
    BackendStatus, ClusterSummary, LogEvent, NodeRow, PodRow, ResourceDetail, TableData,
};
use serde::Deserialize;

/// Per-request timeout for the small JSON calls. NOT applied to the long-lived
/// log stream (which only gets a connect timeout from the client builder).
const JSON_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

/// HTTP client pinned at one backend connection's base URL, e.g.
/// `https://host/k3s-server1/connection1`. The REST routes hang off `/api/...`.
#[derive(Clone)]
pub struct RemoteKube {
    base: String,
    http: reqwest::Client,
    cluster_version: String,
}

impl RemoteKube {
    /// Build the client for `endpoint`. Does NOT touch the network (no probe).
    pub fn new(endpoint: String, ca_pem: Option<Vec<u8>>, insecure: bool) -> Result<Self, String> {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .user_agent("kubefront-desktop");

        if let Some(pem) = ca_pem {
            let cert = reqwest::Certificate::from_pem(&pem)
                .map_err(|e| format!("invalid CA certificate: {e}"))?;
            builder = builder.add_root_certificate(cert);
        }
        if insecure {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let http = builder
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        Ok(Self {
            base: endpoint.trim_end_matches('/').to_string(),
            http,
            cluster_version: String::new(),
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn cluster_version(&self) -> &str {
        &self.cluster_version
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// Probe `GET /api/status`, caching the cluster version. Used at connect time.
    pub async fn refresh_status(&mut self) -> Result<BackendStatus, String> {
        let st: BackendStatus =
            recv_json(self.http.get(self.url("/api/status")).timeout(JSON_TIMEOUT)).await?;
        self.cluster_version = st.cluster_version.clone();
        Ok(st)
    }

    pub async fn summary(&self) -> Result<ClusterSummary, String> {
        recv_json(
            self.http
                .get(self.url("/api/summary"))
                .timeout(JSON_TIMEOUT),
        )
        .await
    }

    pub async fn list_pods(&self, namespace: Option<&str>) -> Result<Vec<PodRow>, String> {
        recv_json(
            self.http
                .get(self.url("/api/pods"))
                .query(&[("namespace", ns_param(namespace))])
                .timeout(JSON_TIMEOUT),
        )
        .await
    }

    pub async fn list_nodes(&self) -> Result<Vec<NodeRow>, String> {
        recv_json(self.http.get(self.url("/api/nodes")).timeout(JSON_TIMEOUT)).await
    }

    pub async fn list_resource(
        &self,
        kind: &str,
        namespace: Option<&str>,
    ) -> Result<TableData, String> {
        recv_json(
            self.http
                .get(self.url(&format!("/api/resources/{kind}")))
                .query(&[("namespace", ns_param(namespace))])
                .timeout(JSON_TIMEOUT),
        )
        .await
    }

    pub async fn get_resource(
        &self,
        kind: &str,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<ResourceDetail, String> {
        recv_json(
            self.http
                .get(self.url(&format!("/api/resources/{kind}/{name}")))
                .query(&[("namespace", ns_param(namespace))])
                .timeout(JSON_TIMEOUT),
        )
        .await
    }

    pub async fn delete_resource(
        &self,
        kind: &str,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<(), String> {
        recv_empty(
            self.http
                .delete(self.url(&format!("/api/resources/{kind}/{name}")))
                .query(&[("namespace", ns_param(namespace))])
                .timeout(JSON_TIMEOUT),
        )
        .await
    }

    pub async fn restart_resource(
        &self,
        kind: &str,
        namespace: Option<&str>,
        name: &str,
    ) -> Result<(), String> {
        recv_empty(
            self.http
                .post(self.url(&format!("/api/resources/{kind}/{name}/restart")))
                .query(&[("namespace", ns_param(namespace))])
                .timeout(JSON_TIMEOUT),
        )
        .await
    }

    pub async fn update_configmap(
        &self,
        namespace: &str,
        name: &str,
        data: BTreeMap<String, String>,
    ) -> Result<(), String> {
        recv_empty(
            self.http
                .put(self.url(&format!("/api/configmaps/{namespace}/{name}")))
                .json(&data)
                .timeout(JSON_TIMEOUT),
        )
        .await
    }

    pub async fn describe_pod(&self, namespace: &str, name: &str) -> Result<String, String> {
        recv_text(
            self.http
                .get(self.url(&format!("/api/pods/{namespace}/{name}/describe")))
                .timeout(JSON_TIMEOUT),
        )
        .await
    }

    /// Consume the backend's SSE logs endpoint and yield parsed [`LogEvent`]s.
    /// On transport failure or completion it emits a terminal `ended` (matching
    /// the Direct path), so the desktop's log window closes cleanly.
    pub fn log_stream(
        &self,
        namespace: String,
        pod: String,
        container: Option<String>,
        tail: i64,
    ) -> impl Stream<Item = LogEvent> + Send + 'static {
        let http = self.http.clone();
        let url = self.url(&format!("/api/pods/{namespace}/{pod}/logs"));
        let mut query: Vec<(&str, String)> = vec![("tail", tail.to_string())];
        if let Some(c) = container {
            query.push(("container", c));
        }

        async_stream::stream! {
            let resp = http.get(&url).query(&query).send().await;
            match resp {
                Ok(resp) if resp.status().is_success() => {
                    let mut bytes = resp.bytes_stream();
                    let mut buf: Vec<u8> = Vec::new();
                    while let Some(chunk) = bytes.next().await {
                        match chunk {
                            Ok(b) => {
                                buf.extend_from_slice(&b);
                                // One JSON LogEvent per SSE `data:` line.
                                while let Some(pos) = buf.iter().position(|&c| c == b'\n') {
                                    let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                                    let line = String::from_utf8_lossy(&line_bytes);
                                    let line = line.trim_end_matches(['\r', '\n']);
                                    if let Some(data) = line.strip_prefix("data:") {
                                        let data = data.trim_start();
                                        if !data.is_empty() {
                                            if let Ok(ev) = serde_json::from_str::<LogEvent>(data) {
                                                yield ev;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                yield LogEvent::error(format!("log stream error: {e}"));
                                break;
                            }
                        }
                    }
                }
                Ok(resp) => {
                    let status = resp.status();
                    yield LogEvent::error(parse_error(resp, status).await);
                }
                Err(e) => {
                    yield LogEvent::error(format!("failed to open log stream: {e}"));
                }
            }
            yield LogEvent::ended();
        }
    }
}

/// Map a normalized scope to the query value the backend expects ("All" =
/// cluster-wide, which the backend normalizes back to None).
fn ns_param(namespace: Option<&str>) -> &str {
    namespace.unwrap_or("All")
}

async fn recv_json<T: serde::de::DeserializeOwned>(
    req: reqwest::RequestBuilder,
) -> Result<T, String> {
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request to backend failed: {e}"))?;
    let status = resp.status();
    if status.is_success() {
        resp.json::<T>()
            .await
            .map_err(|e| format!("failed to parse backend response: {e}"))
    } else {
        Err(parse_error(resp, status).await)
    }
}

async fn recv_empty(req: reqwest::RequestBuilder) -> Result<(), String> {
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request to backend failed: {e}"))?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(parse_error(resp, status).await)
    }
}

async fn recv_text(req: reqwest::RequestBuilder) -> Result<String, String> {
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request to backend failed: {e}"))?;
    let status = resp.status();
    if status.is_success() {
        resp.text()
            .await
            .map_err(|e| format!("failed to read backend response: {e}"))
    } else {
        Err(parse_error(resp, status).await)
    }
}

/// Extract the backend's `{error}` message; fall back to the HTTP status.
async fn parse_error(resp: reqwest::Response, status: reqwest::StatusCode) -> String {
    match resp.json::<ErrorBody>().await {
        Ok(b) => b.error,
        Err(_) => format!("backend returned HTTP {}", status.as_u16()),
    }
}
