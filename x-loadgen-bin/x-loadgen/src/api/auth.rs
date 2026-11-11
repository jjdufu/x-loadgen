//! HTTP handlers for login, logout, password change, and `/auth/me`.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::error::ApiError;
use super::server::{AppState, authenticate};
use crate::auth::{
    ChangePasswordError, auth_cookie, change_password, clear_auth_cookie, current_credential,
    load_auth_state,
};

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub token: String,
}

/// Login response body. `token` is the same login token (also set as a cookie).
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
/// Verifies the login token and sets an `HttpOnly` cookie that holds the same
/// token. The token does not expire; it only changes when the password is
/// changed or reset.
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
    if !state
        .token_cache
        .verify(&body.token, &credential, auth_state.updated_at)?
    {
        return Err(ApiError::unauthorized("invalid token"));
    }
    let cookie = auth_cookie(&body.token);
    let body = LoginResponse {
        token: body.token,
        must_change_password: auth_state.must_change_password(),
    };
    Ok((StatusCode::OK, [(header::SET_COOKIE, cookie)], Json(body)).into_response())
}

/// `POST /api/v1/auth/logout`.
///
/// Clears the auth cookie. The login token itself stays valid.
pub async fn logout() -> Response {
    (
        StatusCode::OK,
        [(header::SET_COOKIE, clear_auth_cookie())],
        Json(serde_json::json!({"ok": true})),
    )
        .into_response()
}

/// `GET /api/v1/auth/me`.
///
/// # Errors
///
/// Returns 401 when authentication is required but no valid token is present,
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
    let auth_state = authenticate(&state, &headers)?;
    Ok(Json(MeResponse {
        auth_required: true,
        must_change_password: auth_state.must_change_password(),
    }))
}

/// `POST /api/v1/auth/password`.
///
/// On success the auth cookie is replaced with the new token. The previous
/// token is rejected immediately.
///
/// # Errors
///
/// Returns 401 when the caller is unauthenticated or the old token is wrong,
/// 400 when the new token is invalid, or 500 when the password file cannot be
/// deleted or the hash cannot be stored.
pub async fn password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PasswordRequest>,
) -> Result<Response, ApiError> {
    if state.no_auth {
        return Err(ApiError::bad_request(
            "auth_disabled",
            "password change is not used when the server is started with --no-auth",
        ));
    }
    // The caller must already present the current token (cookie or Bearer).
    authenticate(&state, &headers)?;
    let mut db = state.lock_db()?;
    change_password(&state.paths, &mut db, &body.old_token, &body.new_token)
        .map_err(map_change_error)?;
    drop(db);
    state.token_cache.invalidate();
    let cookie = auth_cookie(&body.new_token);
    let body = MeResponse {
        auth_required: true,
        must_change_password: false,
    };
    Ok((StatusCode::OK, [(header::SET_COOKIE, cookie)], Json(body)).into_response())
}

fn map_change_error(err: ChangePasswordError) -> ApiError {
    match err {
        ChangePasswordError::IncorrectOld => ApiError::unauthorized("old token is incorrect"),
        ChangePasswordError::TooShort { min } => ApiError::bad_request(
            "invalid_request",
            format!("new token must be at least {min} characters"),
        ),
        ChangePasswordError::InvalidChars => ApiError::bad_request(
            "invalid_request",
            "new token must be printable ASCII without whitespace, quotes, semicolons, commas or backslashes",
        ),
        ChangePasswordError::SameAsOld => ApiError::bad_request(
            "invalid_request",
            "new token must differ from the old token",
        ),
        ChangePasswordError::Other(err) => ApiError::internal(err.to_string()),
    }
}
