//! Transport-agnostic pod log streaming.
//!
//! [`log_stream`] yields a `Stream<Item = LogEvent>` and owns NO Tauri channel
//! or cancellation handle — cancellation is simply dropping the stream, which
//! closes the upstream kube watch. The desktop app pumps this into a Tauri
//! `Channel` (under a `oneshot` cancel); the backend hands it to axum SSE.

use futures_util::Stream;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, LogParams};

use crate::dto::LogEvent;

/// Stream a pod's logs as [`LogEvent`]s: a `header`, then `line`/`error` events,
/// then a final `ended`. `follow` is always on; `tail` bounds the backlog.
pub fn log_stream(
    client: kube::Client,
    namespace: String,
    pod: String,
    container: Option<String>,
    tail: i64,
) -> impl Stream<Item = LogEvent> + Send + 'static {
    async_stream::stream! {
        use futures_util::io::AsyncBufReadExt;
        use futures_util::StreamExt;

        yield LogEvent::header(format!("--- Streaming logs for {namespace}/{pod} ---"));

        let pods: Api<Pod> = Api::namespaced(client, &namespace);
        let params = LogParams {
            follow: true,
            tail_lines: Some(tail),
            timestamps: true,
            container,
            ..Default::default()
        };

        match pods.log_stream(&pod, &params).await {
            Ok(logs) => {
                let mut lines = logs.lines();
                while let Some(result) = lines.next().await {
                    match result {
                        Ok(line) if !line.is_empty() => yield LogEvent::line(line),
                        Ok(_) => {}
                        Err(e) => {
                            yield LogEvent::error(format!("Error reading log line: {e}"));
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                yield LogEvent::error(format!("Failed to open log stream: {e}"));
            }
        }

        yield LogEvent::ended();
    }
}
