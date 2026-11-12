//! HTTP handlers for Hoppscotch-style Environments, Global variables, and weighted Mazbri Tenant Pools.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::Value;

use super::error::ApiError;
use super::server::{AppState, require_ready_auth};
use crate::db::environments::{self, EnvironmentDetail, EnvironmentInput, EnvironmentSummary};
use crate::db::workspace::{GeneratedBindingKeypair, TenantSpec};

/// Query parameters for `GET /api/v1/environments/export`.
#[derive(Debug, Default, Deserialize)]
pub struct ExportEnvironmentsQuery {
    #[serde(default)]
    pub environment_id: Option<String>,
    #[serde(default)]
    pub include_secrets: bool,
}

/// Request body for `POST /api/v1/environments/{id}/tenants/{tid}/binding-keypair`.
#[derive(Debug, Deserialize)]
pub struct TenantKeypairRequest {
    pub alg: String,
    #[serde(default)]
    pub kid: Option<String>,
}

/// `GET /api/v1/environments`
///
/// # Errors
///
/// Returns 401/403 when unauthorized, or 500 on storage failure.
pub async fn list_environments(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<EnvironmentSummary>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(environments::list_environments(&db)?))
}

/// `POST /api/v1/environments`
///
/// # Errors
///
/// Returns 400 on validation failure, 409 on duplicate/reserved name, or 401/403 when unauthorized.
pub async fn create_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<EnvironmentInput>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let detail = environments::create_environment(&db, &input)?;
    Ok((StatusCode::CREATED, Json(detail)).into_response())
}

/// `GET /api/v1/environments/export`
///
/// # Errors
///
/// Returns 404 if `environment_id` is unknown, or 401/403 when unauthorized.
pub async fn export_environments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportEnvironmentsQuery>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let doc = environments::export_environments_json(
        &db,
        query.environment_id.as_deref(),
        query.include_secrets,
    )?;
    Ok(Json(doc))
}

/// `POST /api/v1/environments/import`
///
/// # Errors
///
/// Returns 400/409, 401/403, or 500 as appropriate.
pub async fn import_environments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let imported = environments::import_environments_json(&db, &payload)?;
    Ok((StatusCode::CREATED, Json(imported)).into_response())
}

/// `GET /api/v1/environments/{id}`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn get_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<EnvironmentDetail>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(environments::get_environment(&db, &id)?))
}

/// `PUT /api/v1/environments/{id}`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn update_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<EnvironmentInput>,
) -> Result<Json<EnvironmentDetail>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(environments::update_environment(&db, &id, &input)?))
}

/// `DELETE /api/v1/environments/{id}`
///
/// # Errors
///
/// Returns 400 if trying to delete `Global`, 404 when missing, or 401/403 when unauthorized.
pub async fn delete_environment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    environments::delete_environment(&db, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/environments/{id}/tenants`
///
/// # Errors
///
/// Returns 404 when the environment is missing, or 401/403 when unauthorized.
pub async fn list_tenants(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<TenantSpec>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(environments::list_tenants(&db, &id)?))
}

/// `POST /api/v1/environments/{id}/tenants`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn create_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(spec): Json<TenantSpec>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let created = environments::create_tenant(&db, &id, &spec)?;
    Ok((StatusCode::CREATED, Json(created)).into_response())
}

/// `GET /api/v1/environments/{id}/tenants/{tid}`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn get_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, tid)): Path<(String, String)>,
) -> Result<Json<TenantSpec>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(environments::get_tenant(&db, &id, &tid)?))
}

/// `PUT /api/v1/environments/{id}/tenants/{tid}`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn update_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, tid)): Path<(String, String)>,
    Json(spec): Json<TenantSpec>,
) -> Result<Json<TenantSpec>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(environments::update_tenant(&db, &id, &tid, &spec)?))
}

/// `DELETE /api/v1/environments/{id}/tenants/{tid}`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn delete_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, tid)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    environments::delete_tenant(&db, &id, &tid)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/environments/{id}/tenants/{tid}/binding-keypair`
///
/// Generates an asymmetric request-binding keypair (`ed25519`, `sm2-sign`, `ecdsa-*`, `rsa-*`),
/// persists the private key on the tenant row, and returns the SPKI public key PEM.
///
/// # Errors
///
/// Returns 400 for symmetric/invalid algorithm or missing `kid`, 404 if missing, 401/403 if unauthorized.
pub async fn generate_tenant_binding_keypair(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, tid)): Path<(String, String)>,
    Json(req): Json<TenantKeypairRequest>,
) -> Result<Json<GeneratedBindingKeypair>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let out = environments::generate_tenant_binding_keypair(
        &db,
        &id,
        &tid,
        &req.alg,
        req.kid.as_deref(),
    )?;
    Ok(Json(out))
}
