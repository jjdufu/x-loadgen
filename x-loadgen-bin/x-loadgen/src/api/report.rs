//! Report, sample-page, and SSE handlers for a single run.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
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
use crate::report::{
    ReportInput, SamplesQuery, build_report, compute_verdict, html, query_samples,
};
use crate::run_assert::Verdict;

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
    Query(query): Query<SamplesQuery>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let record = state.supervisor.get_run(&id)?;
    let page = query_samples(&state.paths, &id, &record.snapshot_json, &query)?;
    Ok(Json(json!({
        "items": page.items,
        "next": page.next,
    })))
}

/// Server-sent events with a live stats snapshot once a second.
///
/// # Errors
///
/// Returns an authentication or not-found error before the stream starts.
pub async fn get_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let _ = state.supervisor.get_run(&id)?;
    let stream = futures::stream::unfold((state, id, 0_u64), |(state, id, tick)| async move {
        if tick > 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let payload = event_payload(&state, &id);
        let event = Event::default().event("stats").data(payload.to_string());
        Some((Ok(event), (state, id, tick.saturating_add(1))))
    });
    Ok(Sse::new(stream.boxed()).keep_alive(KeepAlive::default()))
}

fn load_report(state: &AppState, id: &str) -> Result<Value, ApiError> {
    let record = state.supervisor.get_run(id)?;
    let (live_counters, live_percentiles, live_ring) = live_inputs(state, &record);
    let conn = state.lock_db()?;
    let input = ReportInput {
        paths: &state.paths,
        conn: &conn,
        record: &record,
        live_counters,
        live_percentiles,
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
    let (live_counters, live_percentiles, live_ring) = live_inputs(state, record);
    let conn = state.lock_db()?;
    let input = ReportInput {
        paths: &state.paths,
        conn: &conn,
        record,
        live_counters,
        live_percentiles,
        live_ring,
    };
    Ok(compute_verdict(&input)?)
}

fn live_inputs(
    state: &AppState,
    record: &RunRecord,
) -> (
    Option<crate::engine::CounterSnapshot>,
    Option<crate::engine::HistogramPercentiles>,
    Option<Vec<crate::engine::RingSlot>>,
) {
    match state.supervisor.get(&record.id) {
        Some(task) => (
            Some(task.metrics.counters()),
            Some(task.metrics.percentiles_us()),
            Some(task.metrics.ring_snapshot()),
        ),
        None => (None, None, None),
    }
}

fn download_body(body: Vec<u8>, content_type: &'static str, filename: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    (headers, body).into_response()
}

fn event_payload(state: &AppState, id: &str) -> Value {
    match state.supervisor.get_run(id) {
        Ok(record) => json!({
            "id": record.id,
            "status": record.status,
            "current_qps": record.current_qps,
            "current_concurrency": record.current_concurrency,
        }),
        Err(err) => json!({ "error": err.to_string() }),
    }
}
