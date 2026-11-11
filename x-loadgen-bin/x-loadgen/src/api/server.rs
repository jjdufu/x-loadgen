//! HTTP listener and shared application state.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::info;

use super::auth::{login, logout, me, password};
use super::runs::{
    create_run, delete_run_handler, get_run, get_stats, get_timeline, get_timeseries, get_windows,
    list_runs, patch_traffic, resume_run, stop_run,
};
pub use super::state::AppState;
pub(crate) use super::state::{require_ready_auth, session_token_from_headers};
use super::targets::{
    create_product, create_target, create_tenant, delete_product, delete_target, delete_tenant,
    get_product, get_target, get_tenant, list_products, list_targets, list_tenants, update_product,
    update_target, update_tenant,
};
use super::templates::{
    create_template, delete_template, get_template, list_templates, update_template,
};

use super::error::ApiError;
use crate::auth::{ensure_admin_passwd, password_file_log_line};
use crate::cli::ServeArgs;
use crate::db::{DataPaths, open_central};

/// Build the router used by `serve` and by tests.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/password", post(password))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/meta", get(meta))
        .route("/api/v1/targets", get(list_targets).post(create_target))
        .route(
            "/api/v1/targets/{id}",
            get(get_target).put(update_target).delete(delete_target),
        )
        .route(
            "/api/v1/targets/{id}/tenants",
            get(list_tenants).post(create_tenant),
        )
        .route(
            "/api/v1/targets/{id}/tenants/{tid}",
            get(get_tenant).put(update_tenant).delete(delete_tenant),
        )
        .route(
            "/api/v1/targets/{id}/products",
            get(list_products).post(create_product),
        )
        .route(
            "/api/v1/targets/{id}/products/{pid}",
            get(get_product).put(update_product).delete(delete_product),
        )
        .route(
            "/api/v1/templates",
            get(list_templates).post(create_template),
        )
        .route(
            "/api/v1/templates/{id}",
            get(get_template)
                .put(update_template)
                .delete(delete_template),
        )
        .route("/api/v1/runs", get(list_runs).post(create_run))
        .route("/api/v1/runs/{id}", get(get_run).delete(delete_run_handler))
        .route("/api/v1/runs/{id}/stats", get(get_stats))
        .route("/api/v1/runs/{id}/timeseries", get(get_timeseries))
        .route("/api/v1/runs/{id}/timeline", get(get_timeline))
        .route("/api/v1/runs/{id}/windows", get(get_windows))
        .route("/api/v1/runs/{id}/stop", post(stop_run))
        .route("/api/v1/runs/{id}/resume", post(resume_run))
        .route("/api/v1/runs/{id}/traffic", patch(patch_traffic))
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
    let state = prepared.state.clone();
    axum::serve(listener, router(prepared.state))
        .with_graceful_shutdown(async move {
            crate::signal::wait_exit_signal().await;
            info!("shutdown signal received; stopping running tasks");
            state
                .supervisor
                .stop_all(crate::engine::StopKind::Signal)
                .await;
        })
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
    crate::db::runs::mark_stale_runs_interrupted(&conn)?;
    let state = AppState::new(
        paths,
        conn,
        args.no_auth,
        args.max_qps,
        args.max_concurrency,
    );
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
    let state = prepared.state.clone();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router(prepared.state))
            .with_graceful_shutdown(async move {
                let _ = rx.await;
                state
                    .supervisor
                    .stop_all(crate::engine::StopKind::Signal)
                    .await;
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
    let totals = state.supervisor.running_totals();
    Ok(Json(MetaResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        auth_required: !state.no_auth,
        max_qps: state.max_qps,
        max_concurrency: state.max_concurrency,
        running_qps: totals.qps,
        running_concurrency: totals.concurrency,
    }))
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
