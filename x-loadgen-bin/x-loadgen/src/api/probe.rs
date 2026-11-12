//! HTTP handlers for interactive single-request `Send` (`POST /api/v1/probe`)
//! and stateless Mazbri binding keypair generation (`POST /api/v1/crypto/binding-keypair`).

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;

use super::error::ApiError;
use super::server::{AppState, require_ready_auth};
use crate::db::workspace::{GeneratedBindingKeypair, create_binding_keypair};
use crate::probe::{ProbeRequest, ProbeResponse, prepare_probe, run_probe};

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
    let resp = run_probe(target, seq).await?;
    Ok(Json(resp))
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
