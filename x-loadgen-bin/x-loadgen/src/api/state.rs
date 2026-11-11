//! Shared HTTP application state and session checks.

use std::sync::{Arc, Mutex, MutexGuard};

use axum::http::{HeaderMap, header};
use rusqlite::Connection;

use super::error::ApiError;
use crate::auth::{SESSION_COOKIE, load_auth_state, touch_session};
use crate::db::DataPaths;
use crate::engine::Supervisor;

/// Shared state for the HTTP API.
#[derive(Clone)]
pub struct AppState {
    /// Data directory used by this process.
    pub paths: DataPaths,
    db: Arc<Mutex<Connection>>,
    /// When true, requests skip session checks.
    pub no_auth: bool,
    /// Optional global QPS cap from `serve`.
    pub max_qps: Option<u64>,
    /// Optional global concurrency cap from `serve`.
    pub max_concurrency: Option<u64>,
    /// In-process run coordinator.
    pub supervisor: Supervisor,
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

/// Read the session token from a `Cookie` or `Authorization: Bearer` header.
#[must_use]
pub fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
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
        if let Some(value) = part.strip_prefix(SESSION_COOKIE)
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

/// Require a ready session unless `--no-auth` is set.
///
/// # Errors
///
/// Returns unauthorized or `password_change_required` when the session is
/// missing, invalid, or still on the initial password.
pub(crate) fn require_ready_auth(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if state.no_auth {
        return Ok(());
    }
    let token = session_token_from_headers(headers)
        .ok_or_else(|| ApiError::unauthorized("missing session"))?;
    let db = state.lock_db()?;
    if touch_session(&db, &token)?.is_none() {
        return Err(ApiError::unauthorized("invalid or expired session"));
    }
    let auth_state = load_auth_state(&db)?;
    if auth_state.password_hash.is_none() || auth_state.must_change {
        return Err(ApiError::password_change_required());
    }
    Ok(())
}
