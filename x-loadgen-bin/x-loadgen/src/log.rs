//! Rolling process logs under `<data-dir>/logs/`.

use anyhow::{Context, Result};
use std::fs;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

use crate::db::DataPaths;

/// Start a daily-rolling file appender and a tracing subscriber.
///
/// `prefix` is the file name prefix: `serve` or `run-<id>-wNN`.
/// Rolling files look like `<prefix>.YYYY-MM-DD` under the logs directory.
///
/// # Errors
///
/// Returns an error if the logs directory cannot be created.
pub fn init(paths: &DataPaths, prefix: &str) -> Result<WorkerGuard> {
    let dir = paths.logs_dir();
    fs::create_dir_all(&dir).with_context(|| format!("create logs directory {}", dir.display()))?;
    chmod_dir(&dir);
    let appender = rolling::daily(&dir, prefix);
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_ansi(false).with_writer(writer));
    // Tests and a second init (should not happen) must not panic.
    let _ = tracing::subscriber::set_global_default(subscriber);
    Ok(guard)
}

/// Log file prefix for a worker process (`run-<id>-wNN`).
#[must_use]
pub fn worker_prefix(run_id: &str, worker_index: u32) -> String {
    format!("run-{run_id}-w{worker_index:02}")
}

/// Log file prefix for the `serve` process.
#[must_use]
pub fn serve_prefix() -> &'static str {
    "serve"
}

fn chmod_dir(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o700);
            let _ = fs::set_permissions(path, perms);
        }
    }
}
