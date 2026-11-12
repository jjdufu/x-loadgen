//! Persistence for runs, control, timeline, and window summaries.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::collections::RequestDraft;
use super::error::StoreError;
use super::profiles::LoadConfig;
use super::workspace::{new_id, now_ms};

/// Task statuses stored in `runs.status`.
pub const STATUS_RUNNING: &str = "running";
pub const STATUS_STOPPING: &str = "stopping";
pub const STATUS_STOPPED: &str = "stopped";
pub const STATUS_INTERRUPTED: &str = "interrupted";
pub const STATUS_FINISHED: &str = "finished";
pub const STATUS_FAILED: &str = "failed";

/// A row of the `runs` table plus optional parsed snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    pub id: String,
    pub template_id: String,
    pub target_id: String,
    pub mode: String,
    pub status: String,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub current_qps: Option<i64>,
    pub current_concurrency: Option<i64>,
    pub processes: i64,
    #[serde(skip)]
    pub snapshot_json: String,
    pub interrupt_ranges: Option<Value>,
    pub error: Option<String>,
    pub created_at: i64,
}

/// One timeline event.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub id: i64,
    pub run_id: String,
    pub ts: i64,
    pub event: String,
    pub payload: Option<Value>,
}

/// Fields needed to insert a new run.
pub struct NewRun<'a> {
    pub template_id: &'a str,
    pub target_id: &'a str,
    pub mode: &'a str,
    pub processes: i64,
    pub current_qps: Option<i64>,
    pub current_concurrency: i64,
    pub snapshot_json: &'a str,
}

/// Create a running row, control record, and `start` timeline event.
///
/// # Errors
///
/// Returns a storage error if a row cannot be inserted.
pub fn insert_run(conn: &Connection, new_run: &NewRun<'_>) -> Result<RunRecord, StoreError> {
    let id = new_id();
    let now = now_ms()?;
    conn.execute(
        "INSERT INTO runs (
            id, template_id, target_id, mode, status, started_at, ended_at,
            current_qps, current_concurrency, processes, snapshot_json,
            interrupt_ranges_json, error, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10, NULL, NULL, ?6)",
        params![
            id,
            new_run.template_id,
            new_run.target_id,
            new_run.mode,
            STATUS_RUNNING,
            now,
            new_run.current_qps,
            new_run.current_concurrency,
            new_run.processes,
            new_run.snapshot_json,
        ],
    )?;
    conn.execute(
        "INSERT INTO run_control (run_id, stop_flag, qps, concurrency, epoch, updated_at)
         VALUES (?1, 0, ?2, ?3, 0, ?4)",
        params![id, new_run.current_qps, new_run.current_concurrency, now],
    )?;
    append_timeline(
        conn,
        &id,
        "start",
        Some(serde_json::json!({ "mode": new_run.mode })),
    )?;
    get_run(conn, &id)
}

/// Load one run.
///
/// # Errors
///
/// Returns not-found when the id is unknown.
pub fn get_run(conn: &Connection, id: &str) -> Result<RunRecord, StoreError> {
    conn.query_row(
        "SELECT id, template_id, target_id, mode, status, started_at, ended_at,
                current_qps, current_concurrency, processes, snapshot_json,
                interrupt_ranges_json, error, created_at
         FROM runs WHERE id = ?1",
        params![id],
        run_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("run {id} not found")))
}

/// List runs, optionally filtered by status.
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn list_runs(conn: &Connection, status: Option<&str>) -> Result<Vec<RunRecord>, StoreError> {
    if let Some(status) = status {
        let mut stmt = conn.prepare(
            "SELECT id, template_id, target_id, mode, status, started_at, ended_at,
                    current_qps, current_concurrency, processes, snapshot_json,
                    interrupt_ranges_json, error, created_at
             FROM runs WHERE status = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![status], run_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, template_id, target_id, mode, status, started_at, ended_at,
                    current_qps, current_concurrency, processes, snapshot_json,
                    interrupt_ranges_json, error, created_at
             FROM runs ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], run_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}

/// Configured traffic of every run currently `running` or `stopping`.
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn running_traffic(conn: &Connection) -> Result<Vec<(String, i64, i64)>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, COALESCE(current_qps, 0), COALESCE(current_concurrency, 0)
         FROM runs WHERE status IN ('running', 'stopping')",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?))
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

/// Update status and optional error / end timestamp.
///
/// # Errors
///
/// Returns a storage error if the row cannot be updated.
pub fn set_status(
    conn: &Connection,
    id: &str,
    status: &str,
    error: Option<&str>,
    ended: bool,
) -> Result<(), StoreError> {
    let now = now_ms()?;
    if ended {
        conn.execute(
            "UPDATE runs SET status = ?1, error = ?2, ended_at = ?3 WHERE id = ?4",
            params![status, error, now, id],
        )?;
    } else {
        conn.execute(
            "UPDATE runs SET status = ?1, error = ?2 WHERE id = ?3",
            params![status, error, id],
        )?;
    }
    Ok(())
}

/// Write current configured traffic.
///
/// # Errors
///
/// Returns a storage error if the row cannot be updated.
pub fn set_current_traffic(
    conn: &Connection,
    id: &str,
    qps: Option<i64>,
    concurrency: i64,
) -> Result<(), StoreError> {
    conn.execute(
        "UPDATE runs SET current_qps = ?1, current_concurrency = ?2 WHERE id = ?3",
        params![qps, concurrency, id],
    )?;
    let now = now_ms()?;
    conn.execute(
        "UPDATE run_control SET qps = ?1, concurrency = ?2, epoch = epoch + 1, updated_at = ?3
         WHERE run_id = ?4",
        params![qps, concurrency, now, id],
    )?;
    Ok(())
}

/// Set `stop_flag` on the control row.
///
/// # Errors
///
/// Returns a storage error if the row cannot be updated.
pub fn set_stop_flag(conn: &Connection, id: &str, flag: bool) -> Result<(), StoreError> {
    let now = now_ms()?;
    conn.execute(
        "UPDATE run_control SET stop_flag = ?1, updated_at = ?2 WHERE run_id = ?3",
        params![i64::from(flag), now, id],
    )?;
    Ok(())
}

/// Append a timeline event.
///
/// # Errors
///
/// Returns a storage error if the row cannot be inserted.
pub fn append_timeline(
    conn: &Connection,
    run_id: &str,
    event: &str,
    payload: Option<Value>,
) -> Result<(), StoreError> {
    let now = now_ms()?;
    let payload_json = payload.map(|v| v.to_string());
    conn.execute(
        "INSERT INTO run_timeline (run_id, ts, event, payload_json) VALUES (?1, ?2, ?3, ?4)",
        params![run_id, now, event, payload_json],
    )?;
    Ok(())
}

/// List timeline events for a run.
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn list_timeline(conn: &Connection, run_id: &str) -> Result<Vec<TimelineEvent>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, ts, event, payload_json FROM run_timeline
         WHERE run_id = ?1 ORDER BY ts, id",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        let raw: Option<String> = row.get(4)?;
        let payload = raw
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(serde_json::from_str)
            .transpose()
            .unwrap_or(None);
        Ok(TimelineEvent {
            id: row.get(0)?,
            run_id: row.get(1)?,
            ts: row.get(2)?,
            event: row.get(3)?,
            payload,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

/// List window rows for a run.
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn list_windows(conn: &Connection, run_id: &str) -> Result<Vec<Value>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT window_index, started_at, ended_at, samples, success, failure, expected,
                dropped_details, p50_us, p95_us, p99_us, max_us, bytes
         FROM run_windows WHERE run_id = ?1 ORDER BY window_index",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        Ok(serde_json::json!({
            "window_index": row.get::<_, i64>(0)?,
            "started_at": row.get::<_, i64>(1)?,
            "ended_at": row.get::<_, Option<i64>>(2)?,
            "samples": row.get::<_, i64>(3)?,
            "success": row.get::<_, i64>(4)?,
            "failure": row.get::<_, i64>(5)?,
            "expected": row.get::<_, i64>(6)?,
            "dropped_details": row.get::<_, i64>(7)?,
            "p50_us": row.get::<_, Option<i64>>(8)?,
            "p95_us": row.get::<_, Option<i64>>(9)?,
            "p99_us": row.get::<_, Option<i64>>(10)?,
            "max_us": row.get::<_, Option<i64>>(11)?,
            "bytes": row.get::<_, i64>(12)?,
        }))
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::from)
}

/// Append an interrupt range and store the JSON.
///
/// # Errors
///
/// Returns a storage error if the row cannot be updated.
pub fn append_interrupt_range(
    conn: &Connection,
    id: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<(), StoreError> {
    let run = get_run(conn, id)?;
    let mut ranges = match run.interrupt_ranges {
        Some(Value::Array(items)) => items,
        _ => Vec::new(),
    };
    ranges.push(serde_json::json!({ "start": start_ms, "end": end_ms }));
    let json = Value::Array(ranges).to_string();
    conn.execute(
        "UPDATE runs SET interrupt_ranges_json = ?1 WHERE id = ?2",
        params![json, id],
    )?;
    Ok(())
}

/// Delete a run row (and cascaded control / timeline / windows).
///
/// # Errors
///
/// Returns conflict when the run is still running or stopping, or a storage error.
pub fn delete_run(conn: &Connection, id: &str) -> Result<(), StoreError> {
    let run = get_run(conn, id)?;
    if run.status == STATUS_RUNNING || run.status == STATUS_STOPPING {
        return Err(StoreError::conflict(
            "the run is still running; stop it before deleting",
        ));
    }
    conn.execute("DELETE FROM run_windows WHERE run_id = ?1", params![id])?;
    conn.execute("DELETE FROM run_checkpoints WHERE run_id = ?1", params![id])?;
    conn.execute("DELETE FROM run_timeline WHERE run_id = ?1", params![id])?;
    conn.execute("DELETE FROM run_control WHERE run_id = ?1", params![id])?;
    conn.execute("DELETE FROM runs WHERE id = ?1", params![id])?;
    Ok(())
}

/// Mark leftover `running`/`stopping` rows as `interrupted` after a process restart.
///
/// # Errors
///
/// Returns a storage error if a row cannot be updated.
pub fn mark_stale_runs_interrupted(conn: &Connection) -> Result<Vec<String>, StoreError> {
    let stale = list_runs(conn, Some(STATUS_RUNNING))?;
    let stopping = list_runs(conn, Some(STATUS_STOPPING))?;
    let mut ids = Vec::new();
    for run in stale.into_iter().chain(stopping) {
        set_status(
            conn,
            &run.id,
            STATUS_INTERRUPTED,
            Some("process restarted"),
            true,
        )?;
        set_stop_flag(conn, &run.id, true)?;
        append_timeline(
            conn,
            &run.id,
            "interrupted",
            Some(serde_json::json!({ "reason": "process restarted" })),
        )?;
        ids.push(run.id);
    }
    Ok(ids)
}

/// Move a stopped or interrupted run back to `running` so it can resume.
///
/// # Errors
///
/// Returns a storage error if the row cannot be updated.
pub fn reopen_for_resume(conn: &Connection, id: &str) -> Result<(), StoreError> {
    let now = now_ms()?;
    conn.execute(
        "UPDATE runs SET status = ?1, ended_at = NULL, error = NULL WHERE id = ?2",
        params![STATUS_RUNNING, id],
    )?;
    conn.execute(
        "UPDATE run_control SET stop_flag = 0, updated_at = ?1 WHERE run_id = ?2",
        params![now, id],
    )?;
    append_timeline(conn, id, "resume", None)?;
    Ok(())
}

/// Latest checkpoint row from a worker database, if any.
///
/// # Errors
///
/// Returns a storage error if the query fails.
/// One row from `worker_checkpoints`.
pub struct WorkerCheckpoint {
    pub ts: i64,
    pub window_index: Option<i64>,
    pub counters_json: String,
    pub histogram_blob: Option<Vec<u8>>,
}

/// Latest checkpoint row from a worker database, if any.
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn latest_worker_checkpoint(conn: &Connection) -> Result<Option<WorkerCheckpoint>, StoreError> {
    let row = conn
        .query_row(
            "SELECT ts, window_index, counters_json, histogram_blob
             FROM worker_checkpoints ORDER BY ts DESC LIMIT 1",
            [],
            |row| {
                Ok(WorkerCheckpoint {
                    ts: row.get(0)?,
                    window_index: row.get(1)?,
                    counters_json: row.get(2)?,
                    histogram_blob: row.get(3)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// Highest persisted window index for a run, if any window was written.
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn last_window_index(conn: &Connection, run_id: &str) -> Result<Option<i64>, StoreError> {
    let value: Option<i64> = conn.query_row(
        "SELECT MAX(window_index) FROM run_windows WHERE run_id = ?1 AND ended_at IS NOT NULL",
        params![run_id],
        |row| row.get(0),
    )?;
    Ok(value)
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    let interrupt: Option<String> = row.get(11)?;
    let interrupt_ranges = interrupt
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(serde_json::from_str)
        .transpose()
        .unwrap_or(None);
    Ok(RunRecord {
        id: row.get(0)?,
        template_id: row.get(1)?,
        target_id: row.get(2)?,
        mode: row.get(3)?,
        status: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        current_qps: row.get(7)?,
        current_concurrency: row.get(8)?,
        processes: row.get(9)?,
        snapshot_json: row.get(10)?,
        interrupt_ranges,
        error: row.get(12)?,
        created_at: row.get(13)?,
    })
}

/// Weight selection for one request when running a Collection Load Test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedRequestWeight {
    pub request_id: String,
    #[serde(default = "default_one")]
    pub weight: i64,
}

const fn default_one() -> i64 {
    1
}

/// Body for `POST /api/v1/runs`.
///
/// Supports launching a load test from:
/// 1. A saved single request (`request_id`),
/// 2. A saved collection/folder (`collection_id`, optionally filtered/re-weighted via `selected_requests`), or
/// 3. Unsaved Hoppscotch tab drafts (`adhoc_request` / `adhoc_requests`).
///
/// Load parameters can come from a saved `load_profile_id` / `load_profile` (id or name)
/// and/or inline overrides (`model`, `qps`, `concurrency`, `requests`, `duration_secs`, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateRunRequest {
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub collection_id: Option<String>,
    #[serde(default)]
    pub selected_requests: Option<Vec<SelectedRequestWeight>>,
    #[serde(default)]
    pub adhoc_request: Option<RequestDraft>,
    #[serde(default)]
    pub adhoc_requests: Option<Vec<RequestDraft>>,
    #[serde(default)]
    pub environment_id: Option<String>,
    #[serde(default)]
    pub variables_override: Option<std::collections::BTreeMap<String, String>>,

    /// Saved Load Profile ID or unique name.
    #[serde(default, alias = "template_id", alias = "template")]
    pub load_profile_id: Option<String>,
    #[serde(default)]
    pub load_profile: Option<String>,

    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub qps: Option<i64>,
    #[serde(default)]
    pub concurrency: Option<i64>,
    #[serde(default)]
    pub requests: Option<i64>,
    #[serde(default)]
    pub duration_secs: Option<i64>,
    #[serde(default)]
    pub warmup_secs: Option<i64>,
    #[serde(default)]
    pub ramp_secs: Option<i64>,
    #[serde(default)]
    pub timeout_secs: Option<i64>,
    #[serde(default)]
    pub connect_timeout_secs: Option<i64>,
    #[serde(default)]
    pub token_refresh_skew_secs: Option<i64>,
    #[serde(default)]
    pub include_request_id: Option<bool>,
    #[serde(default)]
    pub processes: Option<i64>,
    #[serde(default)]
    pub qps_mode: Option<String>,
    #[serde(default)]
    pub requests_mode: Option<String>,
    #[serde(default)]
    pub loop_enabled: Option<bool>,
    #[serde(default)]
    pub rounds: Option<i64>,
    #[serde(default)]
    pub clear_rounds: Option<bool>,
    #[serde(default)]
    pub loop_interval_secs: Option<i64>,
    #[serde(default)]
    pub expected_reasons: Option<Vec<String>>,
    #[serde(default)]
    pub detail_retain_days: Option<i64>,
    #[serde(default)]
    pub detail_retain_rows: Option<i64>,
    #[serde(default)]
    pub assert: Option<crate::run_assert::AssertSpec>,
}

impl CreateRunRequest {
    /// Optional saved Load Profile lookup key (`load_profile` preferred, else `load_profile_id`).
    #[must_use]
    pub fn profile_key(&self) -> Option<&str> {
        let named = self
            .load_profile
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let by_id = self
            .load_profile_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        named.or(by_id)
    }

    /// Apply inline load overrides onto `config`.
    ///
    /// Setting `requests` clears `duration_secs` and vice versa so the XOR budget
    /// rule holds when overriding a profile or defaults.
    ///
    /// # Errors
    ///
    /// Returns validation error when both `requests` and `duration_secs` are set
    /// simultaneously or when `clear_rounds` is combined with `rounds`.
    pub fn apply_to(&self, config: &mut LoadConfig) -> Result<(), StoreError> {
        if self.requests.is_some() && self.duration_secs.is_some() {
            return Err(StoreError::invalid(
                "cannot specify both requests and duration_secs",
            ));
        }
        if self.clear_rounds == Some(true) && self.rounds.is_some() {
            return Err(StoreError::invalid(
                "cannot set both rounds and clear_rounds",
            ));
        }
        if let Some(m) = self
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            config.model = m.to_string();
            if config.model == "closed_loop" && self.qps.is_none() {
                config.qps = None;
            }
        }
        if let Some(v) = self.qps {
            config.qps = Some(v);
        }
        if let Some(v) = self.concurrency {
            config.concurrency = v;
        }
        if let Some(v) = self.requests {
            config.requests = Some(v);
            config.duration_secs = None;
        }
        if let Some(v) = self.duration_secs {
            config.duration_secs = Some(v);
            config.requests = None;
        }
        if let Some(v) = self.warmup_secs {
            config.warmup_secs = v;
        }
        if let Some(v) = self.ramp_secs {
            config.ramp_secs = v;
        }
        if let Some(v) = self.timeout_secs {
            config.timeout_secs = v;
        }
        if let Some(v) = self.connect_timeout_secs {
            config.connect_timeout_secs = v;
        }
        if let Some(v) = self.token_refresh_skew_secs {
            config.token_refresh_skew_secs = v;
        }
        if let Some(v) = self.include_request_id {
            config.include_request_id = v;
        }
        if let Some(v) = self.processes {
            config.processes = v;
        }
        if let Some(v) = self.qps_mode.clone() {
            config.qps_mode = v;
        }
        if let Some(v) = self.requests_mode.clone() {
            config.requests_mode = v;
        }
        if let Some(v) = self.loop_enabled {
            config.loop_enabled = v;
        }
        if self.clear_rounds == Some(true) {
            config.rounds = None;
        } else if let Some(v) = self.rounds {
            config.rounds = Some(v);
        }
        if let Some(v) = self.loop_interval_secs {
            config.loop_interval_secs = v;
        }
        if let Some(v) = self.expected_reasons.clone() {
            config.expected_reasons = v;
        }
        if let Some(v) = self.detail_retain_days {
            config.detail_retain_days = Some(v);
        }
        if let Some(v) = self.detail_retain_rows {
            config.detail_retain_rows = Some(v);
        }
        Ok(())
    }

    /// Assertion thresholds to freeze in the run snapshot, if any.
    #[must_use]
    pub fn assert_spec(
        &self,
        fallback: Option<&crate::run_assert::AssertSpec>,
    ) -> Option<crate::run_assert::AssertSpec> {
        match self.assert.as_ref() {
            Some(spec) if !spec.is_empty() => Some(spec.clone()),
            Some(_) => None,
            None => fallback.filter(|s| !s.is_empty()).cloned(),
        }
    }
}

/// Body for `PATCH /api/v1/runs/:id/traffic`.
#[derive(Debug, Deserialize)]
pub struct TrafficPatch {
    pub qps: Option<u64>,
    pub concurrency: Option<u64>,
}

/// Live control knobs written by `serve` and read by workers.
#[derive(Debug, Clone)]
pub struct RunControl {
    pub stop_flag: bool,
    pub qps: Option<i64>,
    pub concurrency: Option<i64>,
    pub epoch: i64,
}

/// Load the control row for a run.
///
/// # Errors
///
/// Returns not-found when the row is missing, or a storage error.
pub fn get_run_control(conn: &Connection, run_id: &str) -> Result<RunControl, StoreError> {
    conn.query_row(
        "SELECT stop_flag, qps, concurrency, epoch FROM run_control WHERE run_id = ?1",
        params![run_id],
        |row| {
            let flag: i64 = row.get(0)?;
            Ok(RunControl {
                stop_flag: flag != 0,
                qps: row.get(1)?,
                concurrency: row.get(2)?,
                epoch: row.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| StoreError::not_found(format!("run_control for {run_id} not found")))
}

/// Latest histogram blob for each worker of a run, from `run_checkpoints`.
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn latest_checkpoint_blobs(
    conn: &Connection,
    run_id: &str,
) -> Result<Vec<Vec<u8>>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT histogram_blob FROM run_checkpoints AS c
         WHERE run_id = ?1
           AND ts = (
             SELECT MAX(ts) FROM run_checkpoints
             WHERE run_id = c.run_id AND worker_index = c.worker_index
           )
           AND histogram_blob IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![run_id], |row| row.get::<_, Vec<u8>>(0))?;
    let mut blobs = Vec::new();
    for row in rows {
        blobs.push(row?);
    }
    Ok(blobs)
}
