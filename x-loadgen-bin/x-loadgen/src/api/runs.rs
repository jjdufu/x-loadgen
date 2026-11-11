//! HTTP handlers for load-test runs.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::error::ApiError;
use super::server::{AppState, require_ready_auth};
use crate::db::runs::{CreateRunRequest, TrafficPatch, delete_run, list_timeline, list_windows};
use crate::db::targets::TargetDetail;
use crate::engine::RING_CAPACITY;
use crate::engine::StopKind;

/// Query string for `GET /api/v1/runs`.
#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
    pub status: Option<String>,
}

/// Public run representation. List responses omit the snapshot; detail includes
/// a redacted copy.
#[derive(Debug, Serialize)]
pub struct RunView {
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
    pub interrupt_ranges: Option<Value>,
    pub error: Option<String>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Value>,
}

/// List runs, optionally filtered by `status`.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn list_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListRunsQuery>,
) -> Result<Json<Vec<RunView>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let rows = state.supervisor.list(query.status.as_deref())?;
    Ok(Json(
        rows.into_iter()
            .map(|row| view_from_record(&row, None))
            .collect(),
    ))
}

/// Start a run from a template. Already-running tasks are not a conflict.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn create_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<RunView>), ApiError> {
    require_ready_auth(&state, &headers)?;
    let record = state
        .supervisor
        .start_new(&body.template_id, body.mode.as_deref())?;
    Ok((StatusCode::CREATED, Json(view_from_record(&record, None))))
}

/// Run detail with a redacted snapshot.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn get_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RunView>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let record = state.supervisor.get_run(&id)?;
    let snapshot = redact_snapshot(&record.snapshot_json);
    Ok(Json(view_from_record(&record, Some(snapshot))))
}

/// Live or last-known counters and configured traffic.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn get_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let record = state.supervisor.get_run(&id)?;
    let (counters, percentiles, in_flight, window_index, qps, concurrency) =
        if let Some(task) = state.supervisor.get(&id) {
            let counters = task.metrics.counters();
            let percentiles = task.metrics.percentiles_us();
            (
                counters,
                percentiles,
                counters.in_flight,
                Some(task.window_index()),
                i64::try_from(task.current_qps()).ok(),
                i64::try_from(task.current_concurrency()).ok(),
            )
        } else {
            (
                crate::engine::CounterSnapshot {
                    samples: 0,
                    success: 0,
                    failure: 0,
                    expected: 0,
                    bytes: 0,
                    dropped_details: 0,
                    in_flight: 0,
                    peak_in_flight: 0,
                },
                (None, None, None, None),
                0,
                None,
                record.current_qps,
                record.current_concurrency,
            )
        };
    // After a run has stopped, prefer persisted window sums when live metrics are gone.
    let mut samples = counters.samples;
    let mut success = counters.success;
    let mut failure = counters.failure;
    let mut expected = counters.expected;
    let mut bytes = counters.bytes;
    let mut dropped = counters.dropped_details;
    if state.supervisor.get(&id).is_none() {
        let conn = state.lock_db()?;
        if let Ok(windows) = list_windows(&conn, &id) {
            samples = 0;
            success = 0;
            failure = 0;
            expected = 0;
            bytes = 0;
            dropped = 0;
            for window in windows {
                samples += window["samples"].as_u64().unwrap_or(0);
                success += window["success"].as_u64().unwrap_or(0);
                failure += window["failure"].as_u64().unwrap_or(0);
                expected += window["expected"].as_u64().unwrap_or(0);
                bytes += window["bytes"].as_u64().unwrap_or(0);
                dropped += window["dropped_details"].as_u64().unwrap_or(0);
            }
        }
    }
    let (p50, p95, p99, max) = percentiles;
    Ok(Json(json!({
        "id": record.id,
        "status": record.status,
        "current_qps": qps,
        "current_concurrency": concurrency,
        "samples": samples,
        "success": success,
        "failure": failure,
        "expected": expected,
        "bytes": bytes,
        "dropped_details": dropped,
        "in_flight": in_flight,
        "peak_in_flight": counters.peak_in_flight,
        "p50_us": p50,
        "p95_us": p95,
        "p99_us": p99,
        "max_us": max,
        "window_index": window_index,
    })))
}

/// Second-level ring buffer for a live run (empty once the process has dropped it).
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn get_timeseries(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let _ = state.supervisor.get_run(&id)?;
    let slots = state
        .supervisor
        .get(&id)
        .map(|task| task.metrics.ring_snapshot())
        .unwrap_or_default();
    let points: Vec<Value> = slots
        .into_iter()
        .filter(|slot| slot.unix_sec != 0)
        .map(|slot| {
            json!({
                "unix_sec": slot.unix_sec,
                "samples": slot.samples,
                "success": slot.success,
                "failure": slot.failure,
                "expected": slot.expected,
                "bytes": slot.bytes,
                "warmup": slot.warmup,
            })
        })
        .collect();
    Ok(Json(json!({
        "capacity": RING_CAPACITY,
        "points": points,
    })))
}

/// Timeline events for a run.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn get_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let _ = state.supervisor.get_run(&id)?;
    let conn = state.lock_db()?;
    let events = list_timeline(&conn, &id)?;
    Ok(Json(json!({ "events": events })))
}

/// Window bookkeeping rows.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn get_windows(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let _ = state.supervisor.get_run(&id)?;
    let conn = state.lock_db()?;
    let windows = list_windows(&conn, &id)?;
    Ok(Json(json!({ "windows": windows })))
}

/// Graceful stop. The run becomes `stopping` and then `stopped`.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn stop_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RunView>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let record = state.supervisor.request_stop(&id, StopKind::User)?;
    Ok(Json(view_from_record(&record, None)))
}

/// Resume a `stopped` or `interrupted` run. `finished` / `failed` return 409.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn resume_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RunView>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let record = state.supervisor.resume(&id)?;
    Ok(Json(view_from_record(&record, None)))
}

/// Change configured QPS and/or concurrency of a running task.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn patch_traffic(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<TrafficPatch>,
) -> Result<Json<RunView>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let record = state
        .supervisor
        .patch_traffic(&id, body.qps, body.concurrency)?;
    Ok(Json(view_from_record(&record, None)))
}

/// Delete a finished (or otherwise non-running) run and its directory.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn delete_run_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    {
        let conn = state.lock_db()?;
        delete_run(&conn, &id)?;
    }
    let dir = state.paths.run_dir(&id);
    if dir.exists()
        && let Err(err) = std::fs::remove_dir_all(&dir)
    {
        return Err(ApiError::internal(format!(
            "run record deleted but the run directory could not be removed: {err}"
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn view_from_record(record: &crate::db::runs::RunRecord, snapshot: Option<Value>) -> RunView {
    RunView {
        id: record.id.clone(),
        template_id: record.template_id.clone(),
        target_id: record.target_id.clone(),
        mode: record.mode.clone(),
        status: record.status.clone(),
        started_at: record.started_at,
        ended_at: record.ended_at,
        current_qps: record.current_qps,
        current_concurrency: record.current_concurrency,
        processes: record.processes,
        interrupt_ranges: record.interrupt_ranges.clone(),
        error: record.error.clone(),
        created_at: record.created_at,
        snapshot,
    }
}

fn redact_snapshot(raw: &str) -> Value {
    let Ok(mut value) = serde_json::from_str::<Value>(raw) else {
        return Value::Null;
    };
    if let Some(target) = value.get_mut("target")
        && let Ok(detail) = serde_json::from_value::<TargetDetail>(target.clone())
    {
        let mut redacted = detail;
        redacted.tenants = redacted
            .tenants
            .into_iter()
            .map(crate::db::targets::Tenant::redacted)
            .collect();
        redacted.client_key = None;
        if let Ok(cleaned) = serde_json::to_value(&redacted) {
            *target = cleaned;
        }
    }
    value
}
