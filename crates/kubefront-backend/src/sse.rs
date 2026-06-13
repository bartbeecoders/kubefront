//! Server-Sent Events adapter for pod logs.
//!
//! Each [`LogEvent`] becomes ONE JSON object in a single `data:` frame — robust
//! to log lines that themselves contain newlines. The stream is handed straight
//! to `Sse::new` with no owning task, so a client disconnect drops the stream,
//! which closes the upstream kube log watch.

use std::convert::Infallible;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::{Stream, StreamExt};
use kubefront_core::LogEvent;

/// Wrap a [`LogEvent`] stream as an SSE HTTP response.
pub fn log_sse(stream: impl Stream<Item = LogEvent> + Send + 'static) -> Response {
    let mapped = stream
        .map(|ev| {
            let data = serde_json::to_string(&ev).unwrap_or_else(|_| {
                r#"{"kind":"error","line":"failed to encode log event"}"#.to_string()
            });
            Ok::<_, Infallible>(Event::default().data(data))
        })
        .boxed();

    Sse::new(mapped)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("ping"),
        )
        .into_response()
}
