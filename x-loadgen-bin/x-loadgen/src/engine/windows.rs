//! Persist a finished window's bookkeeping into `run_windows`.

use rusqlite::{Connection, params};

use crate::db::StoreError;
use crate::engine::metrics::{CounterSnapshot, Metrics};

/// Counts and timestamps for one window segment.
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
    pub missed_sends: i64,
    pub completed: bool,
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
            expected, dropped_details, p50_us, p95_us, p99_us, max_us, bytes, missed_sends, completed
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
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
            bytes = excluded.bytes,
            missed_sends = excluded.missed_sends,
            completed = excluded.completed",
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
            record.missed_sends,
            i64::from(record.completed),
        ],
    )?;
    Ok(())
}

/// Insert an open `run_windows` row (`ended_at = NULL`, zero counters) when a
/// window starts, so live verdicts can time the window in progress.
/// [`write_window`] fills it in when the window ends.
///
/// # Errors
///
/// Returns a storage error if the row cannot be written.
pub fn open_window(
    conn: &Connection,
    run_id: &str,
    window_index: i64,
    started_at: i64,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO run_windows (run_id, window_index, started_at, completed) VALUES (?1, ?2, ?3, 0)
         ON CONFLICT(run_id, window_index) DO UPDATE SET
            started_at = excluded.started_at,
            ended_at = NULL,
            samples = 0, success = 0, failure = 0, expected = 0, dropped_details = 0,
            p50_us = NULL, p95_us = NULL, p99_us = NULL, max_us = NULL,
            bytes = 0, missed_sends = 0, completed = 0",
        params![run_id, window_index, started_at],
    )?;
    Ok(())
}

/// Close any leftover open `run_windows` rows (for example after an abrupt
/// process crash before `write_window` ran) and recover counter deltas from the
/// latest checkpoint so resumed runs start from a clean, closed window history.
///
/// # Errors
///
/// Returns a storage error if reading or updating `run_windows` fails.
pub fn seal_open_db_windows(
    conn: &Connection,
    run_id: &str,
    cp_ts: Option<i64>,
    cp_counters: Option<&CounterSnapshot>,
    fallback_end_ms: i64,
) -> Result<(), StoreError> {
    let open: Vec<(i64, i64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT window_index, started_at, samples FROM run_windows
             WHERE run_id = ?1 AND ended_at IS NULL ORDER BY window_index",
        )?;
        let rows = stmt.query_map(params![run_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if open.is_empty() {
        return Ok(());
    }
    let closed_totals: (i64, i64, i64, i64, i64, i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(samples), 0), COALESCE(SUM(success), 0), COALESCE(SUM(failure), 0),
                COALESCE(SUM(expected), 0), COALESCE(SUM(dropped_details), 0),
                COALESCE(SUM(bytes), 0), COALESCE(SUM(missed_sends), 0)
         FROM run_windows WHERE run_id = ?1 AND ended_at IS NOT NULL",
        params![run_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        },
    )?;
    let last_pos = open.len() - 1;
    for (pos, (window_index, started_at, existing_samples)) in open.into_iter().enumerate() {
        let end = cp_ts
            .filter(|&ts| ts >= started_at)
            .unwrap_or(fallback_end_ms)
            .max(started_at);
        if pos == last_pos
            && existing_samples == 0
            && let Some(cnt) = cp_counters
        {
            let diff = |total: u64, closed: i64| -> i64 {
                let closed_u = u64::try_from(closed.max(0)).unwrap_or(0);
                i64::try_from(total.saturating_sub(closed_u)).unwrap_or(i64::MAX)
            };
            conn.execute(
                "UPDATE run_windows SET
                    ended_at = ?1,
                    samples = ?2,
                    success = ?3,
                    failure = ?4,
                    expected = ?5,
                    dropped_details = ?6,
                    bytes = ?7,
                    missed_sends = ?8,
                    completed = 0
                 WHERE run_id = ?9 AND window_index = ?10",
                params![
                    end,
                    diff(cnt.samples, closed_totals.0),
                    diff(cnt.success, closed_totals.1),
                    diff(cnt.failure, closed_totals.2),
                    diff(cnt.expected, closed_totals.3),
                    diff(cnt.dropped_details, closed_totals.4),
                    diff(cnt.bytes, closed_totals.5),
                    diff(cnt.missed_sends, closed_totals.6),
                    run_id,
                    window_index,
                ],
            )?;
        } else {
            conn.execute(
                "UPDATE run_windows SET ended_at = ?1, completed = 0
                 WHERE run_id = ?2 AND window_index = ?3",
                params![end, run_id, window_index],
            )?;
        }
    }
    Ok(())
}
