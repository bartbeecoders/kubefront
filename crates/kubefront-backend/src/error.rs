//! HTTP error mapping. Every handler returns [`ApiError`]; its `IntoResponse`
//! produces the status + a JSON `{ "error": "..." }` body whose message is
//! byte-identical to the desktop's local error string (so the frontend sees the
//! same text whether it talked to a Local or Remote connection).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use kubefront_core::CoreError;
use serde::Serialize;

#[derive(Serialize)]
pub struct ErrorBody {
    pub error: String,
}

/// An error from a request handler.
pub enum ApiError {
    /// No connection with that id is configured → 404.
    UnknownConnection(String),
    /// The connection is `read_only` and a destructive verb was attempted → 403.
    ReadOnly,
    /// A Kubernetes operation failed → mapped via [`CoreError::http_status`].
    Core(CoreError),
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        ApiError::Core(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            ApiError::UnknownConnection(id) => {
                (StatusCode::NOT_FOUND, format!("Unknown connection '{id}'"))
            }
            ApiError::ReadOnly => (
                StatusCode::FORBIDDEN,
                "This connection is read-only".to_string(),
            ),
            ApiError::Core(e) => {
                let status = StatusCode::from_u16(e.http_status())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, e.to_string())
            }
        };
        (status, Json(ErrorBody { error })).into_response()
    }
}
