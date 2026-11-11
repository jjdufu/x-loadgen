//! HTTP listener and shared application state.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::Connection;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::info;

use super::auth::{login, logout, me, password};
use super::error::ApiError;
use crate::auth::{
    SESSION_COOKIE, ensure_admin_passwd, load_auth_state, password_file_log_line, touch_session,
};
use crate::cli::ServeArgs;
use crate::db::{DataPaths, open_central};

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
}

impl AppState {
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

/// Build the router used by `serve` and by tests.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/password", post(password))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/meta", get(meta))
        .with_state(state)
}

/// Open the data directory, optionally create `admin.passwd`, and listen.
///
/// # Errors
///
/// Returns an error if the data directory is not writable, the password file
/// cannot be created, the database cannot be opened, or the bind fails.
pub async fn run(args: ServeArgs) -> Result<()> {
    let prepared = prepare(args)?;
    let listener = TcpListener::bind(&prepared.listen)
        .await
        .with_context(|| format!("bind {}", prepared.listen))?;
    let addr = listener.local_addr().context("local address")?;
    info!(%addr, "x-loadgen listening");
    axum::serve(listener, router(prepared.state))
        .await
        .context("http server")
}

/// Inputs needed to start listening.
pub struct PreparedServer {
    /// Bind string after defaults have been applied.
    pub listen: String,
    /// Shared API state.
    pub state: AppState,
}

/// Create the data directory, password file (unless `--no-auth`), and state.
///
/// # Errors
///
/// Returns an error if the data directory or password file cannot be prepared.
pub fn prepare(args: ServeArgs) -> Result<PreparedServer> {
    let _addr = parse_listen(&args.listen)?;
    let paths = DataPaths::new(args.data_dir);
    require_writable_data_dir(&paths)?;
    if args.no_auth {
        info!("authentication disabled (--no-auth)");
    } else {
        let path = paths.admin_passwd();
        ensure_admin_passwd(&paths)?;
        info!("{}", password_file_log_line(&path));
    }
    let conn = open_central(&paths)?;
    let state = AppState {
        paths,
        db: Arc::new(Mutex::new(conn)),
        no_auth: args.no_auth,
        max_qps: args.max_qps,
        max_concurrency: args.max_concurrency,
    };
    Ok(PreparedServer {
        listen: args.listen,
        state,
    })
}

/// Bind `listen` (use `127.0.0.1:0` in tests) and return the bound address plus
/// a shutdown sender.
///
/// # Errors
///
/// Returns an error if the address cannot be bound.
pub async fn spawn(
    prepared: PreparedServer,
) -> Result<(
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<()>>,
)> {
    let listener = TcpListener::bind(&prepared.listen)
        .await
        .with_context(|| format!("bind {}", prepared.listen))?;
    let addr = listener.local_addr().context("local address")?;
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router(prepared.state))
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await
            .context("http server")?;
        Ok(())
    });
    // Give the accept loop a brief moment so a following client does not race
    // the first poll of the listener (usually unnecessary, but keeps tests quiet).
    tokio::time::sleep(Duration::from_millis(10)).await;
    Ok((addr, tx, handle))
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

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn meta(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MetaResponse>, ApiError> {
    require_ready_auth(&state, &headers)?;
    Ok(Json(MetaResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        auth_required: !state.no_auth,
        max_qps: state.max_qps,
        max_concurrency: state.max_concurrency,
        running_qps: 0,
        running_concurrency: 0,
    }))
}

fn require_ready_auth(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
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

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    version: String,
}

#[derive(Serialize)]
struct MetaResponse {
    version: String,
    auth_required: bool,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
    running_qps: u64,
    running_concurrency: u64,
}

/// Used by `cli` when the bind string is clearly invalid before starting tokio.
///
/// # Errors
///
/// Returns an error if `listen` cannot be parsed as a socket address.
pub fn parse_listen(listen: &str) -> Result<SocketAddr> {
    listen
        .parse()
        .with_context(|| format!("invalid listen address {listen}"))
}

/// Fail early when a data directory path cannot be created.
///
/// # Errors
///
/// Returns an error that includes the path.
pub fn require_writable_data_dir(paths: &DataPaths) -> Result<()> {
    paths
        .ensure()
        .with_context(|| format!("data directory is not writable: {}", paths.root().display()))?;
    let marker = paths.root().join(".write-test");
    match std::fs::write(&marker, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&marker);
            Ok(())
        }
        Err(err) => {
            bail!(
                "data directory is not writable: {}: {err}",
                paths.root().display()
            );
        }
    }
}
