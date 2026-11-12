//! HTTP handlers for Hoppscotch-style Collections, nested Folders, and Requests.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::Value;

use super::error::ApiError;
use super::server::{AppState, require_ready_auth};
use crate::db::collections::{
    self, CollectionInput, CollectionNode, CollectionRecord, RequestDraft, SavedRequest,
};

/// Query parameters for `GET /api/v1/collections/export`.
#[derive(Debug, Default, Deserialize)]
pub struct ExportCollectionsQuery {
    #[serde(default)]
    pub collection_id: Option<String>,
    #[serde(default)]
    pub include_secrets: bool,
}

/// Query parameters for `POST /api/v1/collections/import`.
#[derive(Debug, Default, Deserialize)]
pub struct ImportCollectionsQuery {
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// `GET /api/v1/collections`
///
/// # Errors
///
/// Returns 401/403 when unauthorized, or 500 on storage failure.
pub async fn list_collections(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<CollectionNode>>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(collections::list_collection_tree(&db)?))
}

/// `POST /api/v1/collections`
///
/// # Errors
///
/// Returns 400 on validation failure, 404 if `parent_id` is missing, 401/403 when unauthorized.
pub async fn create_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CollectionInput>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let created = collections::create_collection(&db, &input)?;
    let subtree = collections::get_collection_subtree(&db, &created.id)?;
    Ok((StatusCode::CREATED, Json(subtree)).into_response())
}

/// `GET /api/v1/collections/export`
///
/// # Errors
///
/// Returns 404 when `collection_id` is unknown, or 401/403 when unauthorized.
pub async fn export_collections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportCollectionsQuery>,
) -> Result<Json<Value>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let doc = collections::export_collections_json(
        &db,
        query.collection_id.as_deref(),
        query.include_secrets,
    )?;
    Ok(Json(doc))
}

/// `POST /api/v1/collections/import`
///
/// Accepts either Hoppscotch REST Collection JSON or native `x-loadgen` Collection JSON.
///
/// # Errors
///
/// Returns 400 on invalid JSON structure, 404 if `parent_id` is unknown, or 401/403 when unauthorized.
pub async fn import_collections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ImportCollectionsQuery>,
    Json(payload): Json<Value>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let nodes = collections::import_collections_json(&db, &payload, query.parent_id.as_deref())?;
    Ok((StatusCode::CREATED, Json(nodes)).into_response())
}

/// `GET /api/v1/collections/{id}`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn get_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<CollectionNode>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(collections::get_collection_subtree(&db, &id)?))
}

/// `PUT /api/v1/collections/{id}`
///
/// # Errors
///
/// Returns 400/404, 401/403, or 500 as appropriate.
pub async fn update_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<CollectionInput>,
) -> Result<Json<CollectionRecord>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(collections::update_collection(&db, &id, &input)?))
}

/// `DELETE /api/v1/collections/{id}`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn delete_collection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    collections::delete_collection(&db, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/requests`
///
/// # Errors
///
/// Returns 400/404, 401/403, or 500 as appropriate.
pub async fn create_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(draft): Json<RequestDraft>,
) -> Result<Response, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    let saved = collections::create_request(&db, &draft)?;
    Ok((StatusCode::CREATED, Json(saved)).into_response())
}

/// `GET /api/v1/requests/{id}`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn get_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<SavedRequest>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(collections::get_request(&db, &id)?))
}

/// `PUT /api/v1/requests/{id}`
///
/// # Errors
///
/// Returns 400/404, 401/403, or 500 as appropriate.
pub async fn update_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(draft): Json<RequestDraft>,
) -> Result<Json<SavedRequest>, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    Ok(Json(collections::update_request(&db, &id, &draft)?))
}

/// `DELETE /api/v1/requests/{id}`
///
/// # Errors
///
/// Returns 404 when missing, or 401/403 when unauthorized.
pub async fn delete_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_ready_auth(&state, &headers)?;
    let db = state.lock_db()?;
    collections::delete_request(&db, &id)?;
    Ok(StatusCode::NO_CONTENT)
}
