//! HTTP handlers for interactive single-request `Send` (`POST /api/v1/probe`),
//! the probe history (`/api/v1/probes`), and stateless Mazbri binding keypair
//! generation (`POST /api/v1/crypto/binding-keypair`).

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::warn;

use super::error::ApiError;
use super::server::{AppState, require_ready_auth};
use crate::db::StoreError;
use crate::db::probes::{
    NewProbe, ProbeRecord, ProbesQuery, delete_probe, get_probe, insert_probe, list_probes,
};
use crate::db::workspace::{GeneratedBindingKeypair, create_binding_keypair};
use crate::engine::snapshot::ExecutionTarget;
use crate::probe::{ProbeRequest, ProbeResponse, prepare_probe, run_probe};
use crate::report::redact::{collect_secrets, scrub_text};

/// Body for `POST /api/v1/crypto/binding-keypair`.
#[derive(Debug, Deserialize)]
pub struct StatelessKeypairRequest {
    pub alg: String,
    #[serde(default = "default_kid")]
    pub kid: String,
}

fn default_kid() -> String {
    "k1".to_string()
}

/// `POST /api/v1/probe`
///
/// Executes a single saved or inline request (`http` or `mazbri`) and returns
/// response headers, pretty/raw body, assertion details, unresolved variable
/// warnings, and `MazbriWireDebug` envelope telemetry.
///
/// # Errors
///
/// Returns 400/404 on validation or missing entity, 401/403 when unauthorized.
pub async fn probe_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ProbeRequest>,
) -> Result<Json<ProbeResponse>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let seq = req.seq.unwrap_or(0);
    let target = {
        let db = state.lock_db()?;
        prepare_probe(&db, &req)?
    };
    let context = HistoryContext::new(&req, &target);
    let mut resp = run_probe(target, seq, req.trace_sampled, state.probe_captures()).await?;
    state.remember_probe_captures(&resp.captures);
    let entry = context.entry(&resp);
    let db = state.lock_db()?;
    match insert_probe(&db, &entry, state.probe_history) {
        Ok(id) => resp.history_id = id,
        // History is a convenience: the probe itself already happened.
        Err(error) => warn!(%error, "could not store the probe in the history"),
    }
    Ok(Json(resp))
}

/// What the history needs from the compiled target before the probe consumes it.
struct HistoryContext {
    saved_request_id: Option<String>,
    request_name: String,
    protocol: String,
    environment_id: Option<String>,
    environment_name: Option<String>,
    product_id: Option<String>,
    secrets: Vec<String>,
}

impl HistoryContext {
    fn new(req: &ProbeRequest, target: &ExecutionTarget) -> Self {
        let request = target.requests.first();
        Self {
            // The workbench sends the open tab as a draft that carries the saved id.
            saved_request_id: req
                .request_id
                .clone()
                .or_else(|| req.request.as_ref().and_then(|draft| draft.id.clone())),
            request_name: request.map_or_default(|r| r.name.clone()),
            protocol: request.map_or_default(|r| r.protocol.clone()),
            environment_id: target.environment_id.clone(),
            environment_name: target.environment_name.clone(),
            product_id: request.map(crate::engine::snapshot::ExecutionRequest::sample_product_id),
            secrets: collect_secrets(&serde_json::to_value(target).unwrap_or(Value::Null)),
        }
    }

    /// History entry with every known secret value masked.
    fn entry(&self, resp: &ProbeResponse) -> NewProbe {
        let scrub = |text: &str| scrub_text(text, &self.secrets);
        NewProbe {
            saved_request_id: self.saved_request_id.clone(),
            request_name: self.request_name.clone(),
            protocol: self.protocol.clone(),
            method: resp.request_preview.method.clone(),
            url: scrub(&resp.request_preview.url),
            environment_id: self.environment_id.clone(),
            environment_name: self.environment_name.clone(),
            tenant_name: resp.tenant_name.clone(),
            product_id: self.product_id.clone(),
            request_id: resp.request_id.clone(),
            trace_id: resp.trace_id.clone(),
            traceparent: resp.traceparent.clone(),
            trace_echo: resp.trace_echo.as_str().to_string(),
            http_status: resp.http_status,
            transport_ok: resp.transport_ok,
            passed_assertions: resp.passed_assertions,
            latency_us: resp.latency_us,
            response_bytes: resp.response_bytes,
            error_class: resp.error_class.clone(),
            error: resp.error.as_deref().map(scrub),
            captures: resp
                .captures
                .iter()
                .map(|(name, value)| (name.clone(), scrub(value)))
                .collect(),
            response_body: scrub(&resp.body_text),
        }
    }
}

/// `GET /api/v1/probes`: newest probes first, without response bodies.
///
/// # Errors
///
/// Returns 400 for bad query parameters, 401/403 when unauthorized.
pub async fn list_probes_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<ProbesQuery>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let Query(query) = query?;
    let db = state.lock_db()?;
    let (items, next_before_id) = list_probes(&db, &query)?;
    Ok(Json(json!({
        "items": items,
        "next_before_id": next_before_id,
    })))
}

/// `GET /api/v1/probes/{id}`: one probe with its stored response body.
///
/// # Errors
///
/// Returns 404 for unknown ids, 401/403 when unauthorized.
pub async fn get_probe_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ProbeRecord>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let id = parse_probe_id(&id)?;
    let db = state.lock_db()?;
    Ok(Json(get_probe(&db, id)?))
}

/// `DELETE /api/v1/probes/{id}`
///
/// # Errors
///
/// Returns 404 for unknown ids, 401/403 when unauthorized.
pub async fn delete_probe_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    let id = parse_probe_id(&id)?;
    let db = state.lock_db()?;
    delete_probe(&db, id)?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_probe_id(raw: &str) -> Result<i64, StoreError> {
    raw.parse()
        .map_err(|_| StoreError::not_found(format!("probe {raw} not found")))
}

/// `POST /api/v1/crypto/binding-keypair`
///
/// Stateless keypair generator for unsaved / inline Mazbri tabs (returns both
/// `private_key_pem` and `public_key_pem`).
///
/// # Errors
///
/// Returns 400 for symmetric/unsupported algorithms or empty `kid`, 401/403 when unauthorized.
pub async fn generate_stateless_binding_keypair(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<StatelessKeypairRequest>,
) -> Result<Json<GeneratedBindingKeypair>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let (_private_pem, out) = create_binding_keypair(&req.alg, &req.kid, true)?;
    Ok(Json(out))
}
