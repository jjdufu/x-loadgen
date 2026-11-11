//! HTTP handlers for login, logout, password change, and `/auth/me`.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::error::ApiError;
use super::server::{AppState, session_token_from_headers};
use crate::auth::{
    ChangePasswordError, change_password, clear_session_cookie, current_credential, delete_session,
    issue_session, load_auth_state, session_cookie, touch_session, verify_credential,
};

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub token: String,
}

/// Login response body. `token` is the new session token (also set as a cookie).
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub must_change_password: bool,
}

/// `GET /api/v1/auth/me` body.
#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub auth_required: bool,
    pub must_change_password: bool,
}

/// Password change request.
#[derive(Debug, Deserialize)]
pub struct PasswordRequest {
    pub old_token: String,
    pub new_token: String,
}

/// `POST /api/v1/auth/login`.
///
/// # Errors
///
/// Returns 400 when the body is missing a token, 401 when the token is wrong,
/// or 500 on storage failure.
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    if state.no_auth {
        return Err(ApiError::bad_request(
            "auth_disabled",
            "login is not used when the server is started with --no-auth",
        ));
    }
    if body.token.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_request",
            "token is required",
        ));
    }
    let db = state.lock_db()?;
    let auth_state = load_auth_state(&db)?;
    let credential = current_credential(&state.paths, &auth_state)?;
    if !verify_credential(&body.token, &credential)? {
        return Err(ApiError::unauthorized("invalid token"));
    }
    let session_token = issue_session(&db)?;
    let must_change_password = auth_state.password_hash.is_none() || auth_state.must_change;
    let cookie = session_cookie(&session_token);
    let body = LoginResponse {
        token: session_token,
        must_change_password,
    };
    Ok((StatusCode::OK, [(header::SET_COOKIE, cookie)], Json(body)).into_response())
}

/// `POST /api/v1/auth/logout`.
///
/// # Errors
///
/// Returns 500 if the session cannot be deleted.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = session_token_from_headers(&headers) {
        let db = state.lock_db()?;
        delete_session(&db, &token)?;
    }
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, clear_session_cookie())],
        Json(serde_json::json!({"ok": true})),
    )
        .into_response())
}

/// `GET /api/v1/auth/me`.
///
/// # Errors
///
/// Returns 401 when authentication is required but no valid session is present,
/// or 500 on storage failure.
pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, ApiError> {
    if state.no_auth {
        return Ok(Json(MeResponse {
            auth_required: false,
            must_change_password: false,
        }));
    }
    let token = session_token_from_headers(&headers)
        .ok_or_else(|| ApiError::unauthorized("missing session"))?;
    let db = state.lock_db()?;
    let session = touch_session(&db, &token)?;
    if session.is_none() {
        return Err(ApiError::unauthorized("invalid or expired session"));
    }
    let auth_state = load_auth_state(&db)?;
    Ok(Json(MeResponse {
        auth_required: true,
        must_change_password: auth_state.password_hash.is_none() || auth_state.must_change,
    }))
}

/// `POST /api/v1/auth/password`.
///
/// # Errors
///
/// Returns 401 when the old token is wrong, 400 when the new token is invalid,
/// or 500 when the password file cannot be deleted or the hash cannot be stored.
pub async fn password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PasswordRequest>,
) -> Result<Json<MeResponse>, ApiError> {
    if state.no_auth {
        return Err(ApiError::bad_request(
            "auth_disabled",
            "password change is not used when the server is started with --no-auth",
        ));
    }
    let token = session_token_from_headers(&headers)
        .ok_or_else(|| ApiError::unauthorized("missing session"))?;
    let mut db = state.lock_db()?;
    let session = touch_session(&db, &token)?
        .ok_or_else(|| ApiError::unauthorized("invalid or expired session"))?;
    change_password(
        &state.paths,
        &mut db,
        &body.old_token,
        &body.new_token,
        Some(&session.id),
    )
    .map_err(map_change_error)?;
    Ok(Json(MeResponse {
        auth_required: true,
        must_change_password: false,
    }))
}

fn map_change_error(err: ChangePasswordError) -> ApiError {
    match err {
        ChangePasswordError::IncorrectOld => ApiError::unauthorized("old token is incorrect"),
        ChangePasswordError::TooShort { min } => ApiError::bad_request(
            "invalid_request",
            format!("new token must be at least {min} characters"),
        ),
        ChangePasswordError::SameAsOld => ApiError::bad_request(
            "invalid_request",
            "new token must differ from the old token",
        ),
        ChangePasswordError::Other(err) => ApiError::internal(err.to_string()),
    }
}
