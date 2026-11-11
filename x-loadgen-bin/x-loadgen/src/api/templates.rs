//! HTTP handlers for load-test templates.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use super::error::ApiError;
use super::server::{AppState, require_ready_auth};
use crate::db::templates::{self, Template, TemplateInput};

/// `GET /api/v1/templates`
///
/// # Errors
///
/// Returns 401/403 when unauthorized, or 500 on storage failure.
pub async fn list_templates(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Template>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(templates::list_templates(&db)?))
}

/// `POST /api/v1/templates`
///
/// # Errors
///
/// Returns 400 when the body is invalid or exceeds global caps, 409 on a name
/// conflict, 401/403 when unauthorized, or 500 on storage failure.
pub async fn create_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<TemplateInput>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let template = templates::create_template(&db, &input, state.max_qps, state.max_concurrency)?;
    Ok((StatusCode::CREATED, Json(template)).into_response())
}

/// `GET /api/v1/templates/:id`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn get_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Template>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(templates::get_template(&db, &id)?))
}

/// `PUT /api/v1/templates/:id`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn update_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<TemplateInput>,
) -> Result<Json<Template>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(templates::update_template(
        &db,
        &id,
        &input,
        state.max_qps,
        state.max_concurrency,
    )?))
}

/// `DELETE /api/v1/templates/:id`
///
/// # Errors
///
/// Returns 409 when an unfinished run still references the template.
pub async fn delete_template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    templates::delete_template(&db, &id)?;
    Ok(StatusCode::NO_CONTENT)
}
