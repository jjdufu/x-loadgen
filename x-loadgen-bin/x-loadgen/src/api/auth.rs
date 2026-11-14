//! HTTP handlers for login, logout, password change, and `/auth/me`.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::error::ApiError;
use super::server::{AppState, authenticate};
use super::state::{auth_token_from_headers, request_is_https};
use crate::auth::{
    ChangePasswordError, auth_cookie, clear_auth_cookie, commit_password_hash, current_credential,
    hash_password, load_auth_state, validate_new_token, verify_credential,
};

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub token: String,
}

/// Login response body. Never echoes the admin password; the opaque session ID
/// is delivered exclusively in the `HttpOnly` auth cookie.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
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
/// Verifies the admin token outside the database mutex (`spawn_blocking`),
/// enforces failed-login rate limiting, and sets an `HttpOnly` cookie holding a
/// random server-side session ID (`s_<64-hex>`).
///
/// # Errors
///
/// Returns 400 when the body is missing a token, 401 when the token is wrong,
/// 429 when rate-limited, or 500 on storage failure.
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
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
    if !state.auth_limiter.check_allowed() {
        return Err(ApiError::too_many_requests(
            "too many failed authentication attempts; retry later",
        ));
    }

    let (auth_state, credential) = {
        let db = state.lock_db()?;
        let auth_state = load_auth_state(&db)?;
        let credential = current_credential(&state.paths, &auth_state)?;
        (auth_state, credential)
    };

    let cache = state.token_cache.clone();
    let presented = body.token;
    let updated_at = auth_state.updated_at;
    let ok = tokio::task::spawn_blocking(move || cache.verify(&presented, &credential, updated_at))
        .await
        .map_err(|err| ApiError::internal(format!("auth verify task panicked: {err}")))??;

    if !ok {
        state.auth_limiter.record_failure();
        return Err(ApiError::unauthorized("invalid token"));
    }

    let session_id = state.sessions.issue(auth_state.updated_at)?;
    let cookie = auth_cookie(&session_id, request_is_https(&headers));
    let resp_body = LoginResponse {
        must_change_password: auth_state.must_change_password(),
    };
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(resp_body),
    )
        .into_response())
}

/// `POST /api/v1/auth/logout`.
///
/// Revokes the active server-side session (if any) and clears the auth cookie.
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = auth_token_from_headers(&headers) {
        state.sessions.revoke(&token);
    }
    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            clear_auth_cookie(request_is_https(&headers)),
        )],
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
/// Verifies `old_token` and computes the new Argon2id hash inside `spawn_blocking`
/// before briefly locking SQLite to commit the hash, revoking all previous
/// sessions and issuing a fresh session cookie.
///
/// # Errors
///
/// Returns 401 when the caller is unauthenticated or `old_token` is wrong,
/// 400 when `new_token` is invalid, or 500 on storage failure.
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
    authenticate(&state, &headers)?;
    validate_new_token(&body.new_token, &body.old_token).map_err(map_change_error)?;

    let credential = {
        let db = state.lock_db()?;
        let auth_state = load_auth_state(&db)?;
        current_credential(&state.paths, &auth_state)?
    };

    let old_token = body.old_token;
    let new_token = body.new_token;
    let hash = tokio::task::spawn_blocking(move || -> Result<String, ChangePasswordError> {
        if !verify_credential(&old_token, &credential)? {
            return Err(ChangePasswordError::IncorrectOld);
        }
        Ok(hash_password(&new_token)?)
    })
    .await
    .map_err(|err| ApiError::internal(format!("password hash task panicked: {err}")))?
    .map_err(map_change_error)?;

    let updated_at = {
        let mut db = state.lock_db()?;
        commit_password_hash(&state.paths, &mut db, &hash)?
    };

    state.token_cache.invalidate();
    state.sessions.clear();
    let session_id = state.sessions.issue(updated_at)?;
    let cookie = auth_cookie(&session_id, request_is_https(&headers));
    let resp_body = MeResponse {
        auth_required: true,
        must_change_password: false,
    };
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(resp_body),
    )
        .into_response())
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
