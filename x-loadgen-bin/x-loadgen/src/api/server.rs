//! HTTP listener and shared application state.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::{info, warn};

use super::auth::{login, logout, me, password};
use super::collections::{
    create_collection, create_request, delete_collection, delete_request, export_collections,
    get_collection, get_request, import_collections, list_collections, update_collection,
    update_request,
};
use super::environments::{
    create_environment, create_tenant, delete_environment, delete_tenant, export_environments,
    generate_tenant_binding_keypair, get_environment, get_tenant, import_environments,
    list_environments, list_tenants, update_environment, update_tenant,
};
use super::openapi::openapi;
use super::probe::{
    clear_probes_handler, delete_probe_handler, generate_stateless_binding_keypair,
    get_probe_handler, list_probes_handler, probe_handler,
};
use super::profiles::{
    create_load_profile, delete_load_profile, get_load_profile, list_load_profiles,
    update_load_profile,
};
use super::runs::{
    create_run, delete_run_handler, get_run, get_run_replay, get_stats, get_timeline,
    get_timeseries, get_verdict, get_windows, list_runs, patch_traffic, resume_run, stop_run,
};
pub use super::state::AppState;
pub(crate) use super::state::{authenticate, require_ready_auth};

use super::error::ApiError;
use crate::auth::{ensure_admin_passwd, load_auth_state, password_file_log_line};
use crate::cli::ServeArgs;
use crate::db::{DataPaths, open_central};

/// Build the router used by `serve` and by tests.
#[allow(clippy::too_many_lines)]
pub fn router(state: AppState) -> Router {
    let guard_state = state.clone();
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/openapi.json", get(openapi))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/password", post(password))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/meta", get(meta))
        .route(
            "/api/v1/collections",
            get(list_collections).post(create_collection),
        )
        .route("/api/v1/collections/export", get(export_collections))
        .route("/api/v1/collections/import", post(import_collections))
        .route(
            "/api/v1/collections/{id}",
            get(get_collection)
                .put(update_collection)
                .delete(delete_collection),
        )
        .route("/api/v1/requests", post(create_request))
        .route(
            "/api/v1/requests/{id}",
            get(get_request).put(update_request).delete(delete_request),
        )
        .route(
            "/api/v1/environments",
            get(list_environments).post(create_environment),
        )
        .route("/api/v1/environments/export", get(export_environments))
        .route("/api/v1/environments/import", post(import_environments))
        .route(
            "/api/v1/environments/{id}",
            get(get_environment)
                .put(update_environment)
                .delete(delete_environment),
        )
        .route(
            "/api/v1/environments/{id}/tenants",
            get(list_tenants).post(create_tenant),
        )
        .route(
            "/api/v1/environments/{id}/tenants/{tid}",
            get(get_tenant).put(update_tenant).delete(delete_tenant),
        )
        .route(
            "/api/v1/environments/{id}/tenants/{tid}/binding-keypair",
            post(generate_tenant_binding_keypair),
        )
        .route(
            "/api/v1/load-profiles",
            get(list_load_profiles).post(create_load_profile),
        )
        .route(
            "/api/v1/load-profiles/{id}",
            get(get_load_profile)
                .put(update_load_profile)
                .delete(delete_load_profile),
        )
        .route("/api/v1/probe", post(probe_handler))
        .route(
            "/api/v1/probes",
            get(list_probes_handler).delete(clear_probes_handler),
        )
        .route(
            "/api/v1/probes/{id}",
            get(get_probe_handler).delete(delete_probe_handler),
        )
        .route(
            "/api/v1/crypto/binding-keypair",
            post(generate_stateless_binding_keypair),
        )
        .route("/api/v1/runs", get(list_runs).post(create_run))
        .route("/api/v1/runs/compare", get(super::report::compare_reports))
        .route("/api/v1/runs/{id}", get(get_run).delete(delete_run_handler))
        .route("/api/v1/runs/{id}/replay", get(get_run_replay))
        .route("/api/v1/runs/{id}/stats", get(get_stats))
        .route("/api/v1/runs/{id}/timeseries", get(get_timeseries))
        .route("/api/v1/runs/{id}/timeline", get(get_timeline))
        .route("/api/v1/runs/{id}/windows", get(get_windows))
        .route("/api/v1/runs/{id}/verdict", get(get_verdict))
        .route("/api/v1/runs/{id}/stop", post(stop_run))
        .route("/api/v1/runs/{id}/resume", post(resume_run))
        .route("/api/v1/runs/{id}/traffic", patch(patch_traffic))
        .route("/api/v1/runs/{id}/events", get(super::report::get_events))
        .route("/api/v1/runs/{id}/report", get(super::report::get_report))
        .route(
            "/api/v1/runs/{id}/report.json",
            get(super::report::download_report_json),
        )
        .route(
            "/api/v1/runs/{id}/report.html",
            get(super::report::download_report_html),
        )
        .route(
            "/api/v1/runs/{id}/samples",
            get(super::report::list_samples),
        )
        .fallback(super::static_files::static_handler)
        .layer(middleware::from_fn_with_state(
            guard_state,
            origin_and_host_guard,
        ))
        .with_state(state)
}

async fn origin_and_host_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let headers = request.headers();
    if state.loopback_only
        && request.uri().path().starts_with("/api/")
        && let Some(host_val) = headers.get(header::HOST).and_then(|v| v.to_str().ok())
        && !is_loopback_host_header(host_val)
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "invalid_host",
            "non-loopback Host header rejected on loopback listener",
        ));
    }

    if matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        if let Some(fetch_site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok())
            && fetch_site.trim().eq_ignore_ascii_case("cross-site")
        {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "cross_origin_forbidden",
                "cross-site state-changing request rejected",
            ));
        }

        if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
            let host = headers
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !origin_matches_host(origin, host) {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "cross_origin_forbidden",
                    "request Origin does not match Host",
                ));
            }
        }
    }

    Ok(next.run(request).await)
}

fn is_loopback_host_header(host: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(&format!("http://{}", host.trim())) else {
        return false;
    };
    let Some(host_str) = parsed.host_str() else {
        return false;
    };
    if host_str.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let bare = host_str
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host_str);
    bare.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn origin_matches_host(origin: &str, host: &str) -> bool {
    if host.trim().is_empty() {
        return false;
    }
    let Ok(origin_url) = reqwest::Url::parse(origin.trim()) else {
        return false;
    };
    if !matches!(origin_url.scheme(), "http" | "https") {
        return false;
    }
    let Ok(host_url) = reqwest::Url::parse(&format!("{}://{}", origin_url.scheme(), host.trim()))
    else {
        return false;
    };
    origin_url.host_str() == host_url.host_str()
        && origin_url.port_or_known_default() == host_url.port_or_known_default()
}

/// Open the data directory, optionally create `admin.passwd`, and listen.
///
/// # Errors
///
/// Returns an error if the data directory is not writable, the password file
/// cannot be created, the database cannot be opened, or the bind fails.
pub async fn run(args: ServeArgs) -> Result<()> {
    let prepared = prepare(args)?;
    let _log_guard = crate::log::init(&prepared.state.paths, crate::log::serve_prefix())?;
    let janitor_paths = prepared.state.paths.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let paths = janitor_paths.clone();
            match tokio::task::spawn_blocking(move || crate::db::janitor::sweep(&paths)).await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    tracing::warn!(error = %err, "detail retention sweep failed");
                }
                Err(err) => {
                    tracing::warn!(error = %err, "detail retention sweep task panicked");
                }
            }
        }
    });
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
    let addr = parse_listen(&args.listen)?;
    let paths = DataPaths::new(args.data_dir);
    require_writable_data_dir(&paths)?;
    let conn = open_central(&paths)?;
    if args.no_auth {
        if addr.ip().is_loopback() {
            info!("authentication disabled (--no-auth on loopback)");
        } else {
            warn!(
                %addr,
                "SECURITY WARNING: authentication disabled (--no-auth) while listening on a non-loopback address; any host reaching this port has full access"
            );
        }
    } else if load_auth_state(&conn)?.password_hash.is_some() {
        // The bootstrap file is deleted on password change; a restart must not
        // bring it back. `reset-passwd` is the only way to issue a new one.
        info!("admin password already changed; admin.passwd is not recreated");
    } else {
        let path = paths.admin_passwd();
        ensure_admin_passwd(&paths)?;
        info!("{}", password_file_log_line(&path));
    }
    crate::db::runs::mark_stale_runs_interrupted(&conn)?;
    let state = AppState::new(
        paths,
        conn,
        args.no_auth,
        args.max_qps,
        args.max_concurrency,
    )
    .with_loopback_only(addr.ip().is_loopback())
    .with_probe_history(args.probe_history);
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
