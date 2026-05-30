//! Background task spawners and channel types.
//! - Resource refresh pollers / watchers
//! - Log streamers
//!
//! All tasks must send via channels and call request_repaint from the UI thread copy.

use std::sync::mpsc::Sender;

pub fn spawn_pod_poller(_tx: Sender<String>) {
    // Real tokio::spawn + kube list/watch in Phase 2
}

pub fn spawn_log_stream(_pod: &str, _ns: &str, _tx: Sender<String>) {
    // Real implementation in Phase 3 using Api::logs_stream
}