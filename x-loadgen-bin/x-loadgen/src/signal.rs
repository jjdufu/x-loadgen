//! Process signals used by `serve` for graceful shutdown.

use tokio::signal::unix::{SignalKind, signal};

/// Wait until the first SIGINT or SIGTERM is delivered to this process.
pub async fn wait_exit_signal() {
    let mut sigint = match signal(SignalKind::interrupt()) {
        Ok(handle) => handle,
        Err(err) => {
            tracing::warn!(error = %err, "failed to register SIGINT handler; falling back to ctrl_c");
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(handle) => handle,
        Err(err) => {
            tracing::warn!(error = %err, "failed to register SIGTERM handler");
            let _ = sigint.recv().await;
            return;
        }
    };
    tokio::select! {
        _ = sigint.recv() => {}
        _ = sigterm.recv() => {}
    }
}
