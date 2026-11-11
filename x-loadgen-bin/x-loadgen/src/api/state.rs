//! Shared HTTP application state and login-token checks.

use std::sync::{Arc, Mutex, MutexGuard};

use axum::http::{HeaderMap, header};
use rusqlite::Connection;

use super::error::ApiError;
use crate::auth::{AUTH_COOKIE, AuthState, TokenCache, current_credential, load_auth_state};
use crate::db::DataPaths;
use crate::engine::Supervisor;

/// Shared state for the HTTP API.
#[derive(Clone)]
pub struct AppState {
    /// Data directory used by this process.
    pub paths: DataPaths,
    db: Arc<Mutex<Connection>>,
    /// When true, requests skip authentication.
    pub no_auth: bool,
    /// Optional global QPS cap from `serve`.
    pub max_qps: Option<u64>,
    /// Optional global concurrency cap from `serve`.
    pub max_concurrency: Option<u64>,
    /// In-process run coordinator.
    pub supervisor: Supervisor,
    /// In-memory cache of the verified login token (avoids per-request argon2).
    pub token_cache: TokenCache,
}

impl AppState {
    /// Build state around an already-open central database.
    #[must_use]
    pub fn new(
        paths: DataPaths,
        db: Connection,
        no_auth: bool,
        max_qps: Option<u64>,
        max_concurrency: Option<u64>,
    ) -> Self {
        let db = Arc::new(Mutex::new(db));
        let supervisor = Supervisor::new(paths.clone(), Arc::clone(&db), max_qps, max_concurrency);
        Self {
            paths,
            db,
            no_auth,
            max_qps,
            max_concurrency,
            supervisor,
            token_cache: TokenCache::new(),
        }
    }

    /// Lock the central database connection.
    ///
    /// # Errors
    ///
    /// Returns an internal error if the mutex was poisoned; the guard is still
    /// recovered so the process can continue serving.
    pub fn lock_db(&self) -> Result<MutexGuard<'_, Connection>, ApiError> {
        Ok(self
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }
}

/// Read the login token from a `Cookie` or `Authorization: Bearer` header.
#[must_use]
pub fn auth_token_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        let rest = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "));
        if let Some(token) = rest {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookies.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(AUTH_COOKIE)
            && let Some(value) = value.strip_prefix('=')
        {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Verify the presented cookie or Bearer token and return the current auth state.
///
/// # Errors
///
/// Returns unauthorized when the token is missing or wrong, or 500 on storage
/// failure. Does not enforce `must_change_password`.
pub(crate) fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<AuthState, ApiError> {
    let token =
        auth_token_from_headers(headers).ok_or_else(|| ApiError::unauthorized("missing token"))?;
    let db = state.lock_db()?;
    let auth_state = load_auth_state(&db)?;
    let credential = current_credential(&state.paths, &auth_state)?;
    if !state
        .token_cache
        .verify(&token, &credential, auth_state.updated_at)?
    {
        return Err(ApiError::unauthorized("invalid token"));
    }
    Ok(auth_state)
}

/// Require a valid login token unless `--no-auth` is set, and reject requests
/// that still use the initial bootstrap token (`password_change_required`).
///
/// # Errors
///
/// Returns unauthorized or `password_change_required` when the token is
/// missing, invalid, or still on the initial password.
pub(crate) fn require_ready_auth(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if state.no_auth {
        return Ok(());
    }
    let auth_state = authenticate(state, headers)?;
    if auth_state.must_change_password() {
        return Err(ApiError::password_change_required());
    }
    Ok(())
}
