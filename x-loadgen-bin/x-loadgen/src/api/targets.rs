//! HTTP handlers for targets, tenants, and products.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use super::error::ApiError;
use super::server::{AppState, require_ready_auth};
use crate::db::targets::{
    self, Product, ProductInput, TargetDetail, TargetInput, TargetSummary, Tenant, TenantInput,
};

/// `GET /api/v1/targets`
///
/// # Errors
///
/// Returns 401/403 when the caller is not authorized, or 500 on storage failure.
pub async fn list_targets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<TargetSummary>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(targets::list_targets(&db)?))
}

/// `POST /api/v1/targets`
///
/// # Errors
///
/// Returns 400 on validation failure, 409 on a name conflict, 401/403 when
/// unauthorized, or 500 on storage failure.
pub async fn create_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<TargetInput>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let mut db = state.lock_db()?;
    let detail = targets::create_target(&mut db, &input)?;
    Ok((StatusCode::CREATED, Json(detail)).into_response())
}

/// `GET /api/v1/targets/:id`
///
/// # Errors
///
/// Returns 404 when missing, 401/403 when unauthorized, or 500 on storage failure.
pub async fn get_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<TargetDetail>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(targets::get_target(&db, &id)?))
}

/// `PUT /api/v1/targets/:id`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn update_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<TargetInput>,
) -> Result<Json<TargetDetail>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let mut db = state.lock_db()?;
    Ok(Json(targets::update_target(&mut db, &id, &input)?))
}

/// `DELETE /api/v1/targets/:id`
///
/// # Errors
///
/// Returns 409 when templates or unfinished runs still reference the target.
pub async fn delete_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    targets::delete_target(&db, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/targets/:id/tenants`
///
/// # Errors
///
/// Returns 404 when the target is missing, or 401/403 when unauthorized.
pub async fn list_tenants(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Tenant>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(targets::list_tenants(&db, &id, false)?))
}

/// `POST /api/v1/targets/:id/tenants`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn create_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<TenantInput>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let tenant = targets::create_tenant(&db, &id, &input)?;
    Ok((StatusCode::CREATED, Json(tenant)).into_response())
}

/// `GET /api/v1/targets/:id/tenants/:tid`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn get_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, tid)): Path<(String, String)>,
) -> Result<Json<Tenant>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(targets::get_tenant(&db, &id, &tid)?))
}

/// `PUT /api/v1/targets/:id/tenants/:tid`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn update_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, tid)): Path<(String, String)>,
    Json(input): Json<TenantInput>,
) -> Result<Json<Tenant>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(targets::update_tenant(&db, &id, &tid, &input)?))
}

/// `DELETE /api/v1/targets/:id/tenants/:tid`
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
    targets::delete_tenant(&db, &id, &tid)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/targets/:id/products`
///
/// # Errors
///
/// Returns 404 when the target is missing, or 401/403 when unauthorized.
pub async fn list_products(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Product>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(targets::list_products(&db, &id)?))
}

/// `POST /api/v1/targets/:id/products`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn create_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ProductInput>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let product = targets::create_product(&db, &id, &input)?;
    Ok((StatusCode::CREATED, Json(product)).into_response())
}

/// `GET /api/v1/targets/:id/products/:pid`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn get_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Result<Json<Product>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(targets::get_product(&db, &id, &pid)?))
}

/// `PUT /api/v1/targets/:id/products/:pid`
///
/// # Errors
///
/// Returns 400/404/409, 401/403, or 500 as appropriate.
pub async fn update_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
    Json(input): Json<ProductInput>,
) -> Result<Json<Product>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(targets::update_product(&db, &id, &pid, &input)?))
}

/// `DELETE /api/v1/targets/:id/products/:pid`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn delete_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    targets::delete_product(&db, &id, &pid)?;
    Ok(StatusCode::NO_CONTENT)
}
