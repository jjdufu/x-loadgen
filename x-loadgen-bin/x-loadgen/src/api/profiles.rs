//! HTTP handlers for reusable Load Profiles.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use super::error::ApiError;
use super::server::{AppState, require_ready_auth};
use crate::db::profiles::{self, LoadProfile, LoadProfileInput};

/// `GET /api/v1/load-profiles`
///
/// # Errors
///
/// Returns 401/403 when unauthorized, or 500 on storage failure.
pub async fn list_load_profiles(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<LoadProfile>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(profiles::list_load_profiles(&db)?))
}

/// `POST /api/v1/load-profiles`
///
/// # Errors
///
/// Returns 400 when invalid or exceeding global caps, 409 on name conflict, 401/403 when unauthorized.
pub async fn create_load_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<LoadProfileInput>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let profile = profiles::create_load_profile(&db, &input, state.max_qps, state.max_concurrency)?;
    Ok((StatusCode::CREATED, Json(profile)).into_response())
}

/// `GET /api/v1/load-profiles/{id}`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn get_load_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<LoadProfile>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(profiles::get_load_profile(&db, &id)?))
}

/// `PUT /api/v1/load-profiles/{id}`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn update_load_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<LoadProfileInput>,
) -> Result<Json<LoadProfile>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(profiles::update_load_profile(
        &db,
        &id,
        &input,
        state.max_qps,
        state.max_concurrency,
    )?))
}

/// `DELETE /api/v1/load-profiles/{id}`
///
/// # Errors
///
/// Returns 409 if an active/interrupted run still references the profile, or 404 when missing.
pub async fn delete_load_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    profiles::delete_load_profile(&db, &id)?;
    Ok(StatusCode::NO_CONTENT)
}
