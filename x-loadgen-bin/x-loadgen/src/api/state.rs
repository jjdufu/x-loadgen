//! Shared HTTP application state and login-token / session checks.

use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::http::{HeaderMap, header};
use rusqlite::Connection;

use super::error::ApiError;
use crate::auth::{
    AUTH_COOKIE, AuthRateLimiter, AuthState, SESSION_TOKEN_PREFIX, SessionStore, TokenCache,
    current_credential, load_auth_state,
};
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
    /// In-memory HMAC digest cache of the verified stateless Bearer token.
    pub token_cache: TokenCache,
    /// Server-side store of opaque browser session tokens (`s_<64-hex>`).
    pub sessions: SessionStore,
    /// Sliding-window failed authentication rate limiter.
    pub auth_limiter: AuthRateLimiter,
    /// Active SSE stream count across this server instance.
    pub active_sse: Arc<AtomicUsize>,
    /// True when the HTTP listener is bound to a loopback address (`127.0.0.1` / `::1`).
    pub loopback_only: bool,
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
            sessions: SessionStore::new(),
            auth_limiter: AuthRateLimiter::new(),
            active_sse: Arc::new(AtomicUsize::new(0)),
            loopback_only: false,
        }
    }

    /// Mark whether the server listener is bound strictly to loopback.
    #[must_use]
    pub fn with_loopback_only(mut self, loopback_only: bool) -> Self {
        self.loopback_only = loopback_only;
        self
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

/// Source of the credential extracted from HTTP request headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresentedAuth {
    /// `Authorization: Bearer <token>` (accepts either session `s_...` or admin token).
    Bearer(String),
    /// `Cookie: x_loadgen_token=<session_id>` (accepts opaque session tokens only).
    Cookie(String),
}

/// Extract a structured [`PresentedAuth`] from `Authorization: Bearer` or `Cookie`.
#[must_use]
pub fn presented_auth_from_headers(headers: &HeaderMap) -> Option<PresentedAuth> {
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
                return Some(PresentedAuth::Bearer(token.to_string()));
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
                return Some(PresentedAuth::Cookie(value.to_string()));
            }
        }
    }
    None
}

/// Read the raw token string from a `Cookie` or `Authorization: Bearer` header.
#[must_use]
pub fn auth_token_from_headers(headers: &HeaderMap) -> Option<String> {
    match presented_auth_from_headers(headers)? {
        PresentedAuth::Bearer(token) | PresentedAuth::Cookie(token) => Some(token),
    }
}

/// True when TLS termination was indicated by a reverse proxy (`X-Forwarded-Proto: https`).
#[must_use]
pub fn request_is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|proto| {
            proto
                .split(',')
                .next()
                .is_some_and(|first| first.trim().eq_ignore_ascii_case("https"))
        })
}

/// Verify the presented session cookie or Bearer token and return the current auth state.
///
/// Releases the SQLite mutex before performing any token or password hash check.
///
/// # Errors
///
/// Returns 401 when the token is missing or wrong, 429 when rate-limited, or 500
/// on storage failure. Does not enforce `must_change_password`.
pub(crate) fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<AuthState, ApiError> {
    let presented = presented_auth_from_headers(headers)
        .ok_or_else(|| ApiError::unauthorized("missing token"))?;

    match presented {
        PresentedAuth::Cookie(session_id) => {
            let auth_state = {
                let db = state.lock_db()?;
                load_auth_state(&db)?
            };
            if !state.sessions.verify(&session_id, auth_state.updated_at) {
                return Err(ApiError::unauthorized("invalid token"));
            }
            Ok(auth_state)
        }
        PresentedAuth::Bearer(token) if token.starts_with(SESSION_TOKEN_PREFIX) => {
            let auth_state = {
                let db = state.lock_db()?;
                load_auth_state(&db)?
            };
            if !state.sessions.verify(&token, auth_state.updated_at) {
                return Err(ApiError::unauthorized("invalid token"));
            }
            Ok(auth_state)
        }
        PresentedAuth::Bearer(token) => {
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
            if !state
                .token_cache
                .verify(&token, &credential, auth_state.updated_at)?
            {
                state.auth_limiter.record_failure();
                return Err(ApiError::unauthorized("invalid token"));
            }
            Ok(auth_state)
        }
    }
}

/// Require a valid session or Bearer token unless `--no-auth` is set, and reject
/// requests that still use the initial bootstrap token (`password_change_required`).
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
