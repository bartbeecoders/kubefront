//! The axum router + handlers. Every route lives under `{base_path}/{conn}/api`
//! and mirrors one `LocalKube` operation. The reverse proxy maps its site segment
//! to this server and forwards `/{conn}/api/...`.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use kubefront_core::{
    normalize_scope, BackendStatus, ClusterSummary, LocalKube, NodeRow, PodRow, ResourceDetail,
    TableData,
};
use serde::Deserialize;
use tower_http::trace::TraceLayer;

use crate::error::ApiError;
use crate::pool::{ConnSlot, ConnectionPool};

type Pool = Arc<ConnectionPool>;

#[derive(Deserialize)]
struct NsQuery {
    namespace: Option<String>,
}

#[derive(Deserialize)]
struct LogQuery {
    container: Option<String>,
    tail: Option<i64>,
}

/// Build the full application router.
pub fn router(pool: Pool, base_path: &str) -> Router {
    let api = Router::new()
        .route("/status", get(status))
        .route("/summary", get(summary))
        .route("/pods", get(list_pods))
        .route("/nodes", get(list_nodes))
        .route("/resources/{kind}", get(list_resource))
        .route(
            "/resources/{kind}/{name}",
            get(get_resource).delete(delete_resource),
        )
        .route("/resources/{kind}/{name}/restart", post(restart_resource))
        .route("/configmaps/{namespace}/{name}", put(update_configmap))
        .route("/pods/{namespace}/{name}/describe", get(describe))
        .route("/pods/{namespace}/{name}/logs", get(logs));

    let routed = Router::new().nest("/{conn}/api", api);
    let routed = if base_path == "/" || base_path.is_empty() {
        routed
    } else {
        Router::new().nest(base_path, routed)
    };

    routed
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(pool)
}

/// Resolve the namespace scope: an explicit `?namespace=` (even "All") wins;
/// otherwise fall back to the connection's configured namespace.
fn resolve_ns(query: Option<String>, slot: &ConnSlot) -> Option<String> {
    match query {
        Some(ns) => normalize_scope(Some(ns)),
        None => normalize_scope(slot.cfg.namespace.clone()),
    }
}

/// Look up the connection slot (its static config), or 404.
fn slot_of(pool: &Pool, conn: &str) -> Result<Arc<ConnSlot>, ApiError> {
    pool.slot(conn)
        .ok_or_else(|| ApiError::UnknownConnection(conn.to_string()))
}

/// Look up the connection and its (lazily built) client.
async fn connect(pool: &Pool, conn: &str) -> Result<(Arc<ConnSlot>, LocalKube), ApiError> {
    let slot = slot_of(pool, conn)?;
    let client = slot.client().await?;
    Ok((slot, client))
}

async fn status(
    State(pool): State<Pool>,
    Path(conn): Path<String>,
) -> Result<Json<BackendStatus>, ApiError> {
    let (slot, client) = connect(&pool, &conn).await?;
    Ok(Json(BackendStatus {
        connected: true,
        cluster_version: client.cluster_version().to_string(),
        namespace: slot.cfg.namespace.clone(),
    }))
}

async fn summary(
    State(pool): State<Pool>,
    Path(conn): Path<String>,
    Query(q): Query<NsQuery>,
) -> Result<Json<ClusterSummary>, ApiError> {
    let (slot, client) = connect(&pool, &conn).await?;
    let ns = resolve_ns(q.namespace, &slot);
    Ok(Json(client.cluster_summary(ns).await))
}

async fn list_pods(
    State(pool): State<Pool>,
    Path(conn): Path<String>,
    Query(q): Query<NsQuery>,
) -> Result<Json<Vec<PodRow>>, ApiError> {
    let (slot, client) = connect(&pool, &conn).await?;
    let ns = resolve_ns(q.namespace, &slot);
    Ok(Json(client.list_pods(ns.as_deref()).await?))
}

async fn list_nodes(
    State(pool): State<Pool>,
    Path(conn): Path<String>,
) -> Result<Json<Vec<NodeRow>>, ApiError> {
    let (_slot, client) = connect(&pool, &conn).await?;
    Ok(Json(client.list_nodes().await?))
}

async fn list_resource(
    State(pool): State<Pool>,
    Path((conn, kind)): Path<(String, String)>,
    Query(q): Query<NsQuery>,
) -> Result<Json<TableData>, ApiError> {
    let (slot, client) = connect(&pool, &conn).await?;
    let ns = resolve_ns(q.namespace, &slot);
    Ok(Json(client.list_resource(&kind, ns.as_deref()).await?))
}

async fn get_resource(
    State(pool): State<Pool>,
    Path((conn, kind, name)): Path<(String, String, String)>,
    Query(q): Query<NsQuery>,
) -> Result<Json<ResourceDetail>, ApiError> {
    let (slot, client) = connect(&pool, &conn).await?;
    let ns = resolve_ns(q.namespace, &slot);
    Ok(Json(
        client.get_resource(&kind, ns.as_deref(), &name).await?,
    ))
}

async fn delete_resource(
    State(pool): State<Pool>,
    Path((conn, kind, name)): Path<(String, String, String)>,
    Query(q): Query<NsQuery>,
) -> Result<StatusCode, ApiError> {
    // Gate read-only connections BEFORE any cluster round-trip.
    let slot = slot_of(&pool, &conn)?;
    guard_writable(&slot)?;
    let client = slot.client().await?;
    let ns = resolve_ns(q.namespace, &slot);
    client.delete_resource(&kind, ns.as_deref(), &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restart_resource(
    State(pool): State<Pool>,
    Path((conn, kind, name)): Path<(String, String, String)>,
    Query(q): Query<NsQuery>,
) -> Result<StatusCode, ApiError> {
    let slot = slot_of(&pool, &conn)?;
    guard_writable(&slot)?;
    let client = slot.client().await?;
    let ns = resolve_ns(q.namespace, &slot);
    client.restart_resource(&kind, ns.as_deref(), &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_configmap(
    State(pool): State<Pool>,
    Path((conn, namespace, name)): Path<(String, String, String)>,
    Json(data): Json<BTreeMap<String, String>>,
) -> Result<StatusCode, ApiError> {
    let slot = slot_of(&pool, &conn)?;
    guard_writable(&slot)?;
    let client = slot.client().await?;
    client.update_configmap(&namespace, &name, data).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn describe(
    State(pool): State<Pool>,
    Path((conn, namespace, name)): Path<(String, String, String)>,
) -> Result<String, ApiError> {
    let (_slot, client) = connect(&pool, &conn).await?;
    Ok(client.describe_pod(&namespace, &name).await?)
}

async fn logs(
    State(pool): State<Pool>,
    Path((conn, namespace, name)): Path<(String, String, String)>,
    Query(q): Query<LogQuery>,
) -> Result<axum::response::Response, ApiError> {
    let (_slot, client) = connect(&pool, &conn).await?;
    let tail = q.tail.unwrap_or_else(|| pool.log_tail());
    let stream = client.log_stream(&namespace, &name, q.container, tail);
    Ok(crate::sse::log_sse(stream))
}

fn guard_writable(slot: &ConnSlot) -> Result<(), ApiError> {
    if slot.cfg.read_only {
        Err(ApiError::ReadOnly)
    } else {
        Ok(())
    }
}

/// Log + 404 any path that didn't match a route — helps debug proxy rewrite rules.
async fn fallback(uri: axum::http::Uri) -> (StatusCode, Json<crate::error::ErrorBody>) {
    tracing::warn!("404 no route for {uri}");
    (
        StatusCode::NOT_FOUND,
        Json(crate::error::ErrorBody {
            error: format!("No route for {uri}"),
        }),
    )
}
