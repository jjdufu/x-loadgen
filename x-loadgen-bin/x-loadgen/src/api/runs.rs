//! HTTP handlers for load-test runs.

use std::time::Duration;

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::error::ApiError;
use super::report::load_verdict;
use super::server::{AppState, require_ready_auth};
use crate::db::runs::{
    ALL_STATUSES, CreateRunRequest, TrafficPatch, delete_run, list_timeline, list_windows,
};
use crate::engine::RING_CAPACITY;
use crate::engine::StopKind;
use crate::run_assert::{Verdict, VerdictResult, status_is_running};

/// Query string for `GET /api/v1/runs`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRunsQuery {
    /// Comma-separated statuses, e.g. `running,stopping`. Omitted = all.
    pub status: Option<String>,
}

/// Parse `running,stopping` into known statuses (deduplicated, empty = all).
fn parse_status_filter(raw: Option<&str>) -> Result<Vec<&'static str>, ApiError> {
    let mut out = Vec::new();
    for part in raw
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let Some(&known) = ALL_STATUSES.iter().find(|s| **s == part) else {
            return Err(ApiError::bad_request(
                "invalid_request",
                format!(
                    "unknown status {part:?}; expected one of {}",
                    ALL_STATUSES.join(", ")
                ),
            ));
        };
        if !out.contains(&known) {
            out.push(known);
        }
    }
    Ok(out)
}

/// Query string for `POST /api/v1/runs`.
#[derive(Debug, Deserialize, Default)]
pub struct CreateRunQuery {
    /// When true, the request waits until the run leaves `running`/`stopping`
    /// or `timeout` seconds elapse.
    #[serde(default)]
    pub wait: bool,
    /// Wait timeout in seconds (default 60, maximum 600).
    #[serde(default)]
    pub timeout: Option<u64>,
}

const DEFAULT_WAIT_SECS: u64 = 60;
const MAX_WAIT_SECS: u64 = 600;

/// Public run representation. List responses omit the snapshot; detail includes
/// a redacted copy.
#[derive(Debug, Serialize)]
pub struct RunView {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Value>,
    pub template_id: String,
    pub target_id: String,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<Verdict>,
}

/// List runs, optionally filtered by `status`.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn list_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<ListRunsQuery>, QueryRejection>,
) -> Result<Json<Vec<RunView>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let Query(query) = query?;
    let statuses = parse_status_filter(query.status.as_deref())?;
    let rows = state.supervisor.list(&statuses)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| view_from_record(&row, None, None))
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
    Query(query): Query<CreateRunQuery>,
    Json(body): Json<CreateRunRequest>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let record = state.supervisor.start_new(&body)?;
    if !query.wait {
        let view = view_with_verdict(&state, &record, None)?;
        return Ok((StatusCode::CREATED, Json(view)).into_response());
    }
    wait_for_run(&state, &record.id, query.timeout).await
}

async fn wait_for_run(
    state: &AppState,
    id: &str,
    timeout: Option<u64>,
) -> Result<Response, ApiError> {
    let secs = timeout.unwrap_or(DEFAULT_WAIT_SECS).clamp(1, MAX_WAIT_SECS);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let current = state.supervisor.get_run(id)?;
        if !status_is_running(&current.status) {
            let view = view_with_verdict(state, &current, None)?;
            return Ok((StatusCode::CREATED, Json(view)).into_response());
        }
        if tokio::time::Instant::now() >= deadline {
            let view = view_with_verdict(state, &current, None)?;
            let body = json!({
                "error": {
                    "code": "wait_timeout",
                    "message": format!(
                        "run {id} is still {} after waiting {secs} seconds",
                        current.status
                    )
                },
                "run": view,
            });
            return Ok((StatusCode::CONFLICT, Json(body)).into_response());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Script-friendly verdict: 200 pass/none, 422 fail, 409 still running.
///
/// The body is always a [`Verdict`]. `curl -f` exits non-zero on 422 and 409.
///
/// # Errors
///
/// Returns an authentication or not-found error.
pub async fn get_verdict(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let record = state.supervisor.get_run(&id)?;
    let verdict = load_verdict(&state, &record)?;
    let status = match verdict.result {
        VerdictResult::Pass | VerdictResult::None => StatusCode::OK,
        VerdictResult::Fail => StatusCode::UNPROCESSABLE_ENTITY,
        VerdictResult::Pending => StatusCode::CONFLICT,
    };
    Ok((status, Json(verdict)).into_response())
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
    Ok(Json(view_with_verdict(&state, &record, Some(snapshot))?))
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
    let processes = state.supervisor.get(&id).map_or_else(
        || u32::try_from(record.processes.max(0)).unwrap_or(0),
        |task| task.processes(),
    );
    if let Some(payload) = merged_live_stats(&state, &record, &id, processes) {
        return Ok(Json(payload));
    }
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
                    missed_sends: 0,
                },
                (None, None, None, None),
                0,
                None,
                record.current_qps,
                record.current_concurrency,
            )
        };
    let (mut samples, mut success, mut failure, mut expected, mut bytes, mut dropped) = (
        counters.samples,
        counters.success,
        counters.failure,
        counters.expected,
        counters.bytes,
        counters.dropped_details,
    );
    let (mut p50, mut p95, mut p99, mut max) = percentiles;
    if state.supervisor.get(&id).is_none() {
        let ((s, ok, fail, exp, b, d), (merged_p50, merged_p95, merged_p99, merged_max)) =
            stopped_run_stats_fallback(&state, &id, percentiles)?;
        (samples, success, failure, expected, bytes, dropped) = (s, ok, fail, exp, b, d);
        (p50, p95, p99, max) = (merged_p50, merged_p95, merged_p99, merged_max);
    }
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
        "missed_sends": counters.missed_sends,
        "p50_us": p50,
        "p95_us": p95,
        "p99_us": p99,
        "max_us": max,
        "window_index": window_index,
    })))
}

type CounterTuple = (u64, u64, u64, u64, u64, u64);
type PercentileTuple = (Option<u64>, Option<u64>, Option<u64>, Option<u64>);

fn stopped_run_stats_fallback(
    state: &AppState,
    id: &str,
    default_percentiles: PercentileTuple,
) -> Result<(CounterTuple, PercentileTuple), ApiError> {
    let conn = state.lock_db()?;
    let windows = list_windows(&conn, id).unwrap_or_default();
    let totals = if windows.is_empty() {
        let (fb, _) = crate::report::load_worker_checkpoint_totals(&state.paths, id);
        (
            fb["samples"].as_u64().unwrap_or(0),
            fb["success"].as_u64().unwrap_or(0),
            fb["failure"].as_u64().unwrap_or(0),
            fb["expected"].as_u64().unwrap_or(0),
            fb["bytes"].as_u64().unwrap_or(0),
            fb["dropped_details"].as_u64().unwrap_or(0),
        )
    } else {
        let (mut samples, mut success, mut failure, mut expected, mut bytes, mut dropped) =
            (0, 0, 0, 0, 0, 0);
        for window in windows {
            samples += window["samples"].as_u64().unwrap_or(0);
            success += window["success"].as_u64().unwrap_or(0);
            failure += window["failure"].as_u64().unwrap_or(0);
            expected += window["expected"].as_u64().unwrap_or(0);
            bytes += window["bytes"].as_u64().unwrap_or(0);
            dropped += window["dropped_details"].as_u64().unwrap_or(0);
        }
        (samples, success, failure, expected, bytes, dropped)
    };
    let mut blobs = crate::db::runs::latest_checkpoint_blobs(&conn, id).unwrap_or_default();
    if blobs.is_empty() {
        let (_, fb_blobs) = crate::report::load_worker_checkpoint_totals(&state.paths, id);
        blobs = fb_blobs;
    }
    let percentiles =
        crate::engine::merge_percentiles(&blobs, 60_000_000).unwrap_or(default_percentiles);
    Ok((totals, percentiles))
}

/// Second-level throughput series for live (single or multiprocess) and finished runs.
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
    let mut slots = if let Some(task) = state.supervisor.get(&id) {
        if task.processes() > 1 {
            let snaps = crate::engine::load_live_snapshots(&state.paths, &id, task.processes());
            crate::engine::merge_live(&snaps, 60_000_000)
                .map(|m| m.timeseries)
                .unwrap_or_default()
        } else {
            task.metrics.timeseries_slots()
        }
    } else {
        Vec::new()
    };
    if slots.is_empty() {
        let conn = state.lock_db()?;
        slots = crate::report::load_persisted_timeseries(&state.paths, &conn, &id);
    }
    let points = crate::report::ring_slots_to_json(&slots);
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
    let record = state.supervisor.get_run(&id)?;
    let processes = u32::try_from(record.processes).unwrap_or(1);
    if processes > 1 && crate::run_assert::status_is_running(&record.status) {
        let live = crate::engine::merge_worker_windows(&state.paths, &id, processes, None);
        if !live.is_empty() {
            return Ok(Json(json!({ "windows": live })));
        }
    }
    let conn = state.lock_db()?;
    let windows = list_windows(&conn, &id)?;
    if windows.is_empty() && processes > 1 {
        let merged =
            crate::engine::merge_worker_windows(&state.paths, &id, processes, record.ended_at);
        if !merged.is_empty() {
            return Ok(Json(json!({ "windows": merged })));
        }
    }
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
    Ok(Json(view_from_record(&record, None, None)))
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
    Ok(Json(view_from_record(&record, None, None)))
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
    Ok(Json(view_from_record(&record, None, None)))
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
    if state.supervisor.get(&id).is_some() {
        return Err(ApiError::conflict(
            "the run is still active or finalizing; wait for it to stop before deleting",
        ));
    }
    {
        let conn = state.lock_db()?;
        delete_run(&conn, &id)?;
    }
    let dir = state.paths.run_dir(&id);
    if dir.exists()
        && let Err(err) = std::fs::remove_dir_all(&dir)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        return Err(ApiError::internal(format!(
            "run record deleted but the run directory could not be removed: {err}"
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn view_with_verdict(
    state: &AppState,
    record: &crate::db::runs::RunRecord,
    snapshot: Option<Value>,
) -> Result<RunView, ApiError> {
    let verdict = load_verdict(state, record)?;
    Ok(view_from_record(record, snapshot, Some(verdict)))
}

fn view_from_record(
    record: &crate::db::runs::RunRecord,
    snapshot: Option<Value>,
    verdict: Option<Verdict>,
) -> RunView {
    let parsed_meta = serde_json::from_str::<Value>(&record.snapshot_json).ok();
    let title = parsed_meta
        .as_ref()
        .and_then(|v| v.get("title"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let model = parsed_meta
        .as_ref()
        .and_then(|v| v.get("template"))
        .and_then(|t| t.get("model"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let source = parsed_meta.and_then(|v| v.get("source").cloned());
    RunView {
        id: record.id.clone(),
        title,
        source,
        template_id: record.template_id.clone(),
        target_id: record.target_id.clone(),
        mode: record.mode.clone(),
        model,
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
        verdict,
    }
}

fn redact_snapshot(raw: &str) -> Value {
    crate::report::redact::redact_snapshot_json(raw)
}

fn merged_live_stats(
    state: &AppState,
    record: &crate::db::runs::RunRecord,
    id: &str,
    processes: u32,
) -> Option<Value> {
    if processes <= 1 {
        return None;
    }
    let snaps = crate::engine::load_live_snapshots(&state.paths, id, processes);
    if snaps.is_empty() {
        return None;
    }
    let merged = crate::engine::merge_live(&snaps, 60_000_000).ok()?;
    let window_index = match (merged.window_index, state.supervisor.get(id)) {
        (Some(w), Some(task)) => Some(w.max(task.window_index())),
        (None, Some(task)) => Some(task.window_index()),
        (w, None) => w,
    };
    Some(json!({
        "id": record.id,
        "status": record.status,
        "current_qps": record.current_qps,
        "current_concurrency": record.current_concurrency,
        "samples": merged.samples,
        "success": merged.success,
        "failure": merged.failure,
        "expected": merged.expected,
        "bytes": merged.bytes,
        "dropped_details": merged.dropped_details,
        "in_flight": merged.in_flight,
        "peak_in_flight": merged.peak_in_flight,
        "p50_us": merged.p50_us,
        "p95_us": merged.p95_us,
        "p99_us": merged.p99_us,
        "max_us": merged.max_us,
        "window_index": window_index,
    }))
}
