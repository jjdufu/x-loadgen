//! Report, sample-page, and SSE handlers for a single run.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::Stream;
use futures::stream::StreamExt;
use serde_json::{Value, json};

use super::error::ApiError;
use super::server::{AppState, require_ready_auth};
use crate::db::runs::RunRecord;
use crate::engine::LatencyStats;
use crate::report::{
    ReportInput, SamplesQuery, build_report, compare::build_comparison, compute_verdict, html,
    query_samples,
};
use crate::run_assert::Verdict;

/// Query string for `GET /api/v1/runs/compare`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompareQuery {
    pub base: String,
    pub target: String,
}

/// Side-by-side A/B comparison of two load-test reports.
///
/// # Errors
///
/// Returns 400 when `base`/`target` are empty or equal, 404 when unknown, or 401/403.
pub async fn compare_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<CompareQuery>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let Query(query) = query?;
    let base_id = query.base.trim();
    let target_id = query.target.trim();
    if base_id.is_empty() || target_id.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_request",
            "both base and target run IDs are required",
        ));
    }
    let base = load_report(&state, base_id)?;
    let target = load_report(&state, target_id)?;
    Ok(Json(build_comparison(&base, &target)))
}

/// JSON report (inline).
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn get_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    Ok(Json(load_report(&state, &id)?))
}

/// Download the same report as `report.json`.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn download_report_json(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let report = load_report(&state, &id)?;
    let body = serde_json::to_vec_pretty(&report).unwrap_or_else(|_| b"{}".to_vec());
    Ok(download_body(
        body,
        "application/json; charset=utf-8",
        &format!("run-{id}-report.json"),
    ))
}

/// Download a standalone HTML report.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn download_report_html(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let report = load_report(&state, &id)?;
    Ok(download_body(
        html::render(&report).into_bytes(),
        "text/html; charset=utf-8",
        &format!("run-{id}-report.html"),
    ))
}

/// Paginated sample rows from every worker database of the run.
///
/// # Errors
///
/// Returns an authentication, validation, or storage error mapped to an HTTP status.
pub async fn list_samples(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    query: Result<Query<SamplesQuery>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let Query(query) = query?;
    query
        .validate()
        .map_err(|message| ApiError::bad_request("invalid_request", message))?;
    let record = state.supervisor.get_run(&id)?;
    let page = query_samples(&state.paths, &id, &record.snapshot_json, &query)?;
    Ok(Json(json!({
        "items": page.items,
        "next": page.next,
    })))
}

/// Maximum concurrent SSE streams allowed across a server instance.
const MAX_CONCURRENT_SSE_STREAMS: usize = 64;

struct SseStreamGuard(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl SseStreamGuard {
    fn try_acquire(counter: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Option<Self> {
        use std::sync::atomic::Ordering;
        let mut cur = counter.load(Ordering::Acquire);
        loop {
            if cur >= MAX_CONCURRENT_SSE_STREAMS {
                return None;
            }
            match counter.compare_exchange_weak(cur, cur + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Some(Self(std::sync::Arc::clone(counter))),
                Err(actual) => cur = actual,
            }
        }
    }
}

impl Drop for SseStreamGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Server-sent events with a live stats snapshot once a second.
///
/// Emits one final `stats` snapshot and closes the stream as soon as the run is
/// no longer `running` or `stopping`.
///
/// # Errors
///
/// Returns an authentication, rate-limit, or not-found error before the stream starts.
pub async fn get_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let _ = state.supervisor.get_run(&id)?;
    let guard = SseStreamGuard::try_acquire(&state.active_sse)
        .ok_or_else(|| ApiError::too_many_requests("too many concurrent SSE streams"))?;
    let stream = futures::stream::unfold(
        (state, id, 0_u64, false, guard),
        |(state, id, tick, done, guard)| async move {
            if done {
                return None;
            }
            if tick > 0 {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            let (payload, active) = event_payload(&state, &id);
            let event = Event::default().event("stats").data(payload.to_string());
            Some((
                Ok(event),
                (state, id, tick.saturating_add(1), !active, guard),
            ))
        },
    );
    Ok(Sse::new(stream.boxed()).keep_alive(KeepAlive::default()))
}

fn load_report(state: &AppState, id: &str) -> Result<Value, ApiError> {
    let record = state.supervisor.get_run(id)?;
    let (live_counters, live_latency, live_ring) = live_inputs(state, &record);
    let conn = state.lock_db()?;
    let input = ReportInput {
        paths: &state.paths,
        conn: &conn,
        record: &record,
        live_counters,
        live_latency,
        live_ring,
    };
    Ok(build_report(&input)?)
}

/// Evaluate assertions for a known run record.
///
/// # Errors
///
/// Returns a storage error when window or histogram data cannot be read.
pub(crate) fn load_verdict(state: &AppState, record: &RunRecord) -> Result<Verdict, ApiError> {
    let (live_counters, live_latency, live_ring) = live_inputs(state, record);
    let conn = state.lock_db()?;
    let input = ReportInput {
        paths: &state.paths,
        conn: &conn,
        record,
        live_counters,
        live_latency,
        live_ring,
    };
    Ok(compute_verdict(&input)?)
}

fn live_inputs(
    state: &AppState,
    record: &RunRecord,
) -> (
    Option<crate::engine::CounterSnapshot>,
    Option<LatencyStats>,
    Option<Vec<crate::engine::RingSlot>>,
) {
    let Some(task) = state.supervisor.get(&record.id) else {
        return (None, None, None);
    };
    if task.processes() > 1 {
        let snaps = crate::engine::load_live_snapshots(&state.paths, &record.id, task.processes());
        if let Ok(merged) = crate::engine::merge_live(&snaps, 60_000_000) {
            let counters = crate::engine::CounterSnapshot {
                samples: merged.samples,
                success: merged.success,
                failure: merged.failure,
                expected: merged.expected,
                bytes: merged.bytes,
                dropped_details: merged.dropped_details,
                in_flight: merged.in_flight,
                peak_in_flight: merged.peak_in_flight,
                missed_sends: merged.missed_sends,
            };
            return (
                Some(counters),
                Some(merged.latency_stats),
                Some(merged.timeseries),
            );
        }
        return (None, None, None);
    }
    (
        Some(task.metrics.counters()),
        Some(task.metrics.latency_stats_us()),
        Some(task.metrics.timeseries_slots()),
    )
}

fn download_body(body: Vec<u8>, content_type: &'static str, filename: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    let safe_name: String = filename
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    if !safe_name.is_empty()
        && let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{safe_name}\""))
    {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    (headers, body).into_response()
}

fn event_payload(state: &AppState, id: &str) -> (Value, bool) {
    match state.supervisor.get_run(id) {
        Ok(record) => {
            let active = matches!(record.status.as_str(), "running" | "stopping");
            let (live_counters, live_latency, _) = live_inputs(state, &record);
            let payload = if let Some(cnt) = live_counters {
                let lat = live_latency.unwrap_or_default();
                json!({
                    "id": record.id,
                    "status": record.status,
                    "current_qps": record.current_qps,
                    "current_concurrency": record.current_concurrency,
                    "samples": cnt.samples,
                    "success": cnt.success,
                    "failure": cnt.failure,
                    "expected": cnt.expected,
                    "bytes": cnt.bytes,
                    "in_flight": cnt.in_flight,
                    "peak_in_flight": cnt.peak_in_flight,
                    "missed_sends": cnt.missed_sends,
                    "min_us": lat.min_us,
                    "avg_us": lat.avg_us,
                    "p50_us": lat.p50_us,
                    "p75_us": lat.p75_us,
                    "p90_us": lat.p90_us,
                    "p95_us": lat.p95_us,
                    "p99_us": lat.p99_us,
                    "p999_us": lat.p999_us,
                    "max_us": lat.max_us,
                })
            } else {
                json!({
                    "id": record.id,
                    "status": record.status,
                    "current_qps": record.current_qps,
                    "current_concurrency": record.current_concurrency,
                })
            };
            (payload, active)
        }
        Err(_) => (json!({ "error": "run not found" }), false),
    }
}
