//! Persist a finished window's bookkeeping into `run_windows`.

use rusqlite::{Connection, params};

use crate::db::StoreError;
use crate::engine::metrics::Metrics;

/// Counts and timestamps for one completed window.
pub struct WindowRecord<'a> {
    pub run_id: &'a str,
    pub window_index: i64,
    pub started_at: i64,
    pub ended_at: i64,
    pub samples: i64,
    pub success: i64,
    pub failure: i64,
    pub expected: i64,
    pub dropped: i64,
    pub bytes: i64,
}

/// Insert or replace one `run_windows` row from the metrics *delta* of this window.
///
/// # Errors
///
/// Returns a storage error if the row cannot be written.
pub fn write_window(
    conn: &Connection,
    record: &WindowRecord<'_>,
    metrics: &Metrics,
) -> Result<(), StoreError> {
    let (p50, p95, p99, max) = metrics.window_percentiles_us();
    conn.execute(
        "INSERT INTO run_windows (
            run_id, window_index, started_at, ended_at, samples, success, failure,
            expected, dropped_details, p50_us, p95_us, p99_us, max_us, bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(run_id, window_index) DO UPDATE SET
            ended_at = excluded.ended_at,
            samples = excluded.samples,
            success = excluded.success,
            failure = excluded.failure,
            expected = excluded.expected,
            dropped_details = excluded.dropped_details,
            p50_us = excluded.p50_us,
            p95_us = excluded.p95_us,
            p99_us = excluded.p99_us,
            max_us = excluded.max_us,
            bytes = excluded.bytes",
        params![
            record.run_id,
            record.window_index,
            record.started_at,
            record.ended_at,
            record.samples,
            record.success,
            record.failure,
            record.expected,
            record.dropped,
            p50.map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
            p95.map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
            p99.map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
            max.map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
            record.bytes,
        ],
    )?;
    Ok(())
}
