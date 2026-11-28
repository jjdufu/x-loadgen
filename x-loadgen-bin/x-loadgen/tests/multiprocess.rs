//! Multi-process workers, split QPS, merged stats/reports/verdicts, isolation, and log files.
#![allow(clippy::too_many_lines)]

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use serde_json::{Value, json};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "x-loadgen-mp-{}-{}-{n}-{tag}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis())
        ));
        fs::create_dir_all(&path).expect("temp dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client")
}

async fn body(response: reqwest::Response) -> Value {
    response.json::<Value>().await.expect("json")
}

async fn post_json(
    http: &reqwest::Client,
    url: &str,
    payload: &Value,
) -> (reqwest::StatusCode, Value) {
    let response = http.post(url).json(payload).send().await.expect("post");
    (response.status(), body(response).await)
}

async fn get_json(http: &reqwest::Client, url: &str) -> (reqwest::StatusCode, Value) {
    let response = http.get(url).send().await.expect("get");
    (response.status(), body(response).await)
}

async fn start_mock() -> (String, tokio::sync::oneshot::Sender<()>) {
    use axum::Router;
    use axum::routing::get;
    let app = Router::new().route("/ok", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock");
    let addr = listener.local_addr().expect("addr");
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await
            .expect("mock");
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    (format!("http://{addr}"), tx)
}

fn mp_duration_profile(name: &str, qps: u64) -> Value {
    mp_duration_profile_mode(name, qps, "total")
}

fn mp_duration_profile_mode(name: &str, qps: u64, qps_mode: &str) -> Value {
    json!({
        "name": name,
        "model": "open_loop",
        "qps": qps,
        "concurrency": 4,
        "duration_secs": 20,
        "warmup_secs": 0,
        "ramp_secs": 0,
        "timeout_secs": 5,
        "processes": 2,
        "qps_mode": qps_mode,
        "requests_mode": "total",
        "loop_enabled": false
    })
}

fn mp_request_profile(name: &str, requests: u64, qps: u64) -> Value {
    json!({
        "name": name,
        "model": "open_loop",
        "qps": qps,
        "concurrency": 2,
        "requests": requests,
        "warmup_secs": 0,
        "ramp_secs": 0,
        "timeout_secs": 5,
        "processes": 2,
        "qps_mode": "total",
        "requests_mode": "total",
        "loop_enabled": false,
        "assert": {"p99_ms": 5000.0, "max_error_rate": 0.0}
    })
}

async fn seed_http_item(
    http: &reqwest::Client,
    base: &str,
    name: &str,
    gateway: &str,
) -> (String, String) {
    let (st, env) = post_json(
        http,
        &format!("{base}/api/v1/environments"),
        &json!({
            "name": format!("env-{name}"),
            "gateway_base_url": gateway,
            "tls": {"insecure_skip_verify": true},
            "tenants": [{"name": "plain", "weight": 1, "api_key": "k", "encryption_mode": "none"}]
        }),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{env}");
    let env_id = env["id"].as_str().unwrap().to_string();

    let (st, col) = post_json(
        http,
        &format!("{base}/api/v1/collections"),
        &json!({"name": format!("col-{name}")}),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{col}");

    let (st, req) = post_json(
        http,
        &format!("{base}/api/v1/requests"),
        &json!({
            "collection_id": col["id"],
            "name": "probe",
            "protocol": "http",
            "method": "GET",
            "endpoint": format!("{gateway}/ok"),
            "mazbri": {"product_id": "P001"},
            "weight": 1
        }),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{req}");
    (env_id, req["id"].as_str().unwrap().to_string())
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

struct ChildGuard(Child);

impl ChildGuard {
    fn shutdown(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = Command::new("kill")
                .args(["-TERM", &self.0.id().to_string()])
                .status();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while std::time::Instant::now() < deadline {
                if self.0.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn spawn_serve(bin: &Path, listen: &str, data_dir: &Path) -> (ChildGuard, PathBuf) {
    let log_path = data_dir.join("serve.spawn.log");
    let log = fs::File::create(&log_path).expect("log");
    let child = Command::new(bin)
        .arg("serve")
        .arg("--no-auth")
        .arg("--listen")
        .arg(listen)
        .arg("--data-dir")
        .arg(data_dir)
        .stdout(Stdio::from(log.try_clone().expect("clone")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("spawn serve");
    (ChildGuard(child), log_path)
}

async fn wait_healthy(http: &reqwest::Client, base: &str, log: &Path) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if http
            .get(format!("{base}/health"))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return;
        }
        let text = fs::read_to_string(log).unwrap_or_default();
        assert!(
            tokio::time::Instant::now() < deadline,
            "serve not healthy: {text}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_file(path: &Path) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if path.is_file() {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "missing file {}",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn start_named_run(
    http: &reqwest::Client,
    base: &str,
    gateway: &str,
    name: &str,
    qps: u64,
) -> String {
    let (env_id, req_id) = seed_http_item(http, base, name, gateway).await;
    let (status, profile) = post_json(
        http,
        &format!("{base}/api/v1/load-profiles"),
        &mp_duration_profile(&format!("tmpl-{name}"), qps),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{profile}");
    let (status, run) = post_json(
        http,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req_id,
            "environment_id": env_id,
            "load_profile_id": profile["id"],
            "mode": "once"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    assert_eq!(run["status"], "running");
    assert_eq!(run["processes"], 2);
    run["id"].as_str().unwrap().to_string()
}

fn sample_count(path: &Path) -> u64 {
    let conn = Connection::open(path).expect("open worker");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM samples", [], |row| row.get(0))
        .unwrap_or(0);
    u64::try_from(n.max(0)).unwrap_or(0)
}

fn log_matches(logs: &Path, needle: &str) -> bool {
    fs::read_dir(logs).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().starts_with(needle))
    })
}

#[tokio::test]
async fn processes_two_split_qps_and_merge_stats() {
    let tmp = TempDir::new("split");
    let (mock, _shutdown_mock) = start_mock().await;
    let listen = format!("127.0.0.1:{}", free_port());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    let (mut child, log) = spawn_serve(&bin, &listen, &tmp.path);
    let base = format!("http://{listen}");
    let http = client();
    wait_healthy(&http, &base, &log).await;

    let id = start_named_run(&http, &base, &mock, "split", 50).await;
    let run_dir = tmp.path.join("runs").join(&id);
    wait_file(&run_dir.join("w00.sqlite")).await;
    wait_file(&run_dir.join("w01.sqlite")).await;
    wait_file(&run_dir.join("w00-live.json")).await;
    wait_file(&run_dir.join("w01-live.json")).await;

    let live0: Value =
        serde_json::from_slice(&fs::read(run_dir.join("w00-live.json")).unwrap()).unwrap();
    let live1: Value =
        serde_json::from_slice(&fs::read(run_dir.join("w01-live.json")).unwrap()).unwrap();
    let qps =
        live0["current_qps"].as_u64().unwrap_or(0) + live1["current_qps"].as_u64().unwrap_or(0);
    assert_eq!(
        qps, 50,
        "worker QPS shares should sum to 50: {live0} {live1}"
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let (from_stats, from_db, live) = loop {
        let (code, live) = get_json(&http, &format!("{base}/api/v1/runs/{id}/stats")).await;
        assert_eq!(code, reqwest::StatusCode::OK, "{live}");
        let from_db =
            sample_count(&run_dir.join("w00.sqlite")) + sample_count(&run_dir.join("w01.sqlite"));
        let from_stats = live["samples"].as_u64().unwrap_or(0);
        if from_stats == from_db && from_db > 0 {
            break (from_stats, from_db, live);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "stats samples {from_stats} != db sum {from_db}: {live}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq!(from_stats, from_db);
    let _ = live;

    let logs = tmp.path.join("logs");
    assert!(
        log_matches(&logs, &format!("run-{id}-w00")),
        "missing w00 log in {}",
        logs.display()
    );
    assert!(
        log_matches(&logs, &format!("run-{id}-w01")),
        "missing w01 log in {}",
        logs.display()
    );

    let central = Connection::open(tmp.path.join("loadgen.sqlite")).expect("central");
    let samples_table: i64 = central
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='samples'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(1);
    assert_eq!(
        samples_table, 0,
        "central database must not hold detail samples"
    );

    let _ = http
        .post(format!("{base}/api/v1/runs/{id}/stop"))
        .send()
        .await;
    child.shutdown();
}

#[tokio::test]
async fn two_multiprocess_runs_are_isolated_and_killing_one_worker_stays_local() {
    let tmp = TempDir::new("iso");
    let (mock, _shutdown_mock) = start_mock().await;
    let listen = format!("127.0.0.1:{}", free_port());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    let (mut child, log) = spawn_serve(&bin, &listen, &tmp.path);
    let base = format!("http://{listen}");
    let http = client();
    wait_healthy(&http, &base, &log).await;

    let id_a = start_named_run(&http, &base, &mock, "iso-a", 40).await;
    let id_b = start_named_run(&http, &base, &mock, "iso-b", 40).await;
    let dir_a = tmp.path.join("runs").join(&id_a);
    let dir_b = tmp.path.join("runs").join(&id_b);
    for path in [
        dir_a.join("w00.sqlite"),
        dir_a.join("w01.sqlite"),
        dir_b.join("w00.sqlite"),
        dir_b.join("w01.sqlite"),
        dir_a.join("w00.pid"),
    ] {
        wait_file(&path).await;
    }
    let logs = tmp.path.join("logs");
    assert!(log_matches(&logs, &format!("run-{id_a}-w00")));
    assert!(log_matches(&logs, &format!("run-{id_a}-w01")));
    assert!(log_matches(&logs, &format!("run-{id_b}-w00")));
    assert!(log_matches(&logs, &format!("run-{id_b}-w01")));

    let pid_text = fs::read_to_string(dir_a.join("w00.pid")).expect("pid");
    let pid = pid_text.trim().parse::<u32>().expect("pid number");
    let killed = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .expect("kill worker");
    assert!(killed.success());

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut status_a;
    loop {
        let conn = Connection::open(tmp.path.join("loadgen.sqlite")).expect("central");
        status_a = conn
            .query_row(
                "SELECT status FROM runs WHERE id = ?1",
                rusqlite::params![id_a],
                |row| row.get::<_, String>(0),
            )
            .expect("status a");
        let status_b: String = conn
            .query_row(
                "SELECT status FROM runs WHERE id = ?1",
                rusqlite::params![id_b],
                |row| row.get(0),
            )
            .expect("status b");
        if status_a == "interrupted" || status_a == "stopped" {
            assert_eq!(
                status_b, "running",
                "run B should keep running, got {status_b}"
            );
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "run A stayed at {status_a}, run B at {status_b}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = http
        .post(format!("{base}/api/v1/runs/{id_b}/stop"))
        .send()
        .await;
    child.shutdown();
    let _ = status_a;
}

#[tokio::test]
async fn processes_two_request_once_finishes_and_consolidates_report_and_verdict() {
    let tmp = TempDir::new("once-finish");
    let (mock, _shutdown_mock) = start_mock().await;
    let listen = format!("127.0.0.1:{}", free_port());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    let (mut child, log) = spawn_serve(&bin, &listen, &tmp.path);
    let base = format!("http://{listen}");
    let http = client();
    wait_healthy(&http, &base, &log).await;

    let request_count = 10_u64;
    let (env_id, req_id) = seed_http_item(&http, &base, "once-finish", &mock).await;
    let (created, profile) = post_json(
        &http,
        &format!("{base}/api/v1/load-profiles"),
        &mp_request_profile("tmpl-once-finish", request_count, 20),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{profile}");
    let (created, run) = post_json(
        &http,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req_id,
            "environment_id": env_id,
            "load_profile_id": profile["id"],
            "mode": "once"
        }),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{run}");
    assert_eq!(run["processes"], 2);
    let id = run["id"].as_str().unwrap().to_string();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let (final_status, samples, in_flight) = loop {
        let (code, body) = get_json(&http, &format!("{base}/api/v1/runs/{id}")).await;
        assert_eq!(code, reqwest::StatusCode::OK, "{body}");
        let (code, counters) = get_json(&http, &format!("{base}/api/v1/runs/{id}/stats")).await;
        assert_eq!(code, reqwest::StatusCode::OK, "{counters}");
        let run_status = body["status"].as_str().unwrap_or_default().to_string();
        let samples = counters["samples"].as_u64().unwrap_or(0);
        let in_flight = counters["in_flight"].as_u64().unwrap_or(0);
        if run_status == "finished" {
            break (run_status, samples, in_flight);
        }
        assert!(
            !matches!(run_status.as_str(), "failed" | "interrupted" | "stopped"),
            "run ended as {run_status} instead of finished: {body} {counters}"
        );
        assert!(
            tokio::time::Instant::now() < deadline,
            "processes=2 once stayed at {run_status} after samples={samples} in_flight={in_flight}: {body} {counters}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(final_status, "finished");
    assert_eq!(samples, request_count, "expected all request-count samples");
    assert_eq!(in_flight, 0);

    // Consolidated report and verdict must reflect worker samples & histograms!
    let (st, report) = get_json(&http, &format!("{base}/api/v1/runs/{id}/report")).await;
    assert_eq!(st, reqwest::StatusCode::OK, "{report}");
    assert_eq!(report["counters"]["samples"], request_count, "{report}");
    assert!(
        report["percentiles"]["p99_us"].as_u64().is_some(),
        "{report}"
    );

    let (st, verdict) = get_json(&http, &format!("{base}/api/v1/runs/{id}/verdict")).await;
    assert_eq!(st, reqwest::StatusCode::OK, "{verdict}");
    assert_eq!(verdict["result"], "pass", "{verdict}");
    assert_eq!(verdict["actual"]["samples"], request_count, "{verdict}");

    child.shutdown();
}

const MP_TENANT_KEY: &str = "eC1sb2FkZ2VuLXRlc3QtdGVuYW50LWtleS0zMmJ5dGU=";
const MP_TENANT_UUID: &str = "1a2b3c4d-0000-4000-8000-00000000c003";

fn mazbri_mock_config() -> x_loadgen_mockgw::MockConfig {
    x_loadgen_mockgw::MockConfig {
        tenants: vec![x_loadgen_mockgw::MockTenant {
            api_key: "ak-mp".into(),
            encryption_mode: "full".into(),
            tenant_id: Some(MP_TENANT_UUID.into()),
            tenant_key_base64: Some(MP_TENANT_KEY.into()),
            suite: Some("suite/tenant-chacha20".into()),
            ..x_loadgen_mockgw::MockTenant::default()
        }],
        ..x_loadgen_mockgw::MockConfig::default()
    }
}

#[tokio::test]
async fn processes_two_encrypted_mazbri_once_finishes_with_every_request_successful() {
    let gateway = x_loadgen_mockgw::MockGateway::start("127.0.0.1:0", &mazbri_mock_config())
        .await
        .expect("mock gateway");

    let tmp = TempDir::new("mazbri-mp");
    let listen = format!("127.0.0.1:{}", free_port());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    let (mut child, log) = spawn_serve(&bin, &listen, &tmp.path);
    let base = format!("http://{listen}");
    let http = client();
    wait_healthy(&http, &base, &log).await;

    let (created, env) = post_json(
        &http,
        &format!("{base}/api/v1/environments"),
        &json!({
            "name": "mazbri-mp",
            "gateway_base_url": gateway.base_url(),
            "tenants": [{
                "name": "sealed",
                "api_key": "ak-mp",
                "encryption_mode": "full",
                "tenant_id": MP_TENANT_UUID,
                "tenant_key_base64": MP_TENANT_KEY,
                "suite": "suite/tenant-chacha20"
            }]
        }),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{env}");

    let (created, col) = post_json(
        &http,
        &format!("{base}/api/v1/collections"),
        &json!({"name": "mazbri-mp-col", "default_protocol": "mazbri"}),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{col}");

    let (created, req) = post_json(
        &http,
        &format!("{base}/api/v1/requests"),
        &json!({
            "collection_id": col["id"],
            "name": "echo",
            "protocol": "mazbri",
            "method": "POST",
            "endpoint": "/upstream/echo",
            "body": {
                "content_type": "application/json",
                "body": "{\"seq\":\"{{seq}}\"}"
            },
            "mazbri": {
                "product_id": "P-ECHO",
                "tenant_source": "environment"
            },
            "assertions": {
                "expect_transport": "ok",
                "checks": [{"path": "echo.product_id", "eq": "P-ECHO"}]
            }
        }),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{req}");

    let request_count = 12_u64;
    let (created, profile) = post_json(
        &http,
        &format!("{base}/api/v1/load-profiles"),
        &mp_request_profile("tmpl-mazbri-mp", request_count, 40),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{profile}");
    let (created, run) = post_json(
        &http,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req["id"],
            "environment_id": env["id"],
            "load_profile_id": profile["id"],
            "mode": "once"
        }),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{run}");
    assert_eq!(run["processes"], 2);
    let id = run["id"].as_str().unwrap().to_string();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let counters = loop {
        let (_, body) = get_json(&http, &format!("{base}/api/v1/runs/{id}")).await;
        let (_, counters) = get_json(&http, &format!("{base}/api/v1/runs/{id}/stats")).await;
        let run_status = body["status"].as_str().unwrap_or_default().to_string();
        if run_status == "finished" {
            break counters;
        }
        assert!(
            !matches!(run_status.as_str(), "failed" | "interrupted" | "stopped"),
            "encrypted run ended as {run_status}: {body} {counters}"
        );
        assert!(
            tokio::time::Instant::now() < deadline,
            "encrypted processes=2 run stayed at {run_status}: {counters}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(counters["samples"], request_count, "{counters}");
    assert_eq!(counters["success"], request_count, "{counters}");
    assert_eq!(counters["failure"], 0, "{counters}");
    let stats = gateway.stats();
    assert_eq!(stats.tokens_issued, 2, "one token per worker process");
    assert_eq!(stats.envelopes_opened, request_count);
    assert_eq!(stats.responses_sealed, request_count);
    child.shutdown();
    gateway.stop().await;
}

#[tokio::test]
async fn per_process_qps_is_not_multiplied_twice_and_stop_keeps_counters() {
    let tmp = TempDir::new("perproc");
    let (mock, _shutdown_mock) = start_mock().await;
    let listen = format!("127.0.0.1:{}", free_port());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    let (mut child, log) = spawn_serve(&bin, &listen, &tmp.path);
    let base = format!("http://{listen}");
    let http = client();
    wait_healthy(&http, &base, &log).await;

    let (env_id, req_id) = seed_http_item(&http, &base, "perproc", &mock).await;
    let (status, profile) = post_json(
        &http,
        &format!("{base}/api/v1/load-profiles"),
        &mp_duration_profile_mode("tmpl-perproc", 20, "per_process"),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{profile}");
    let (status, run) = post_json(
        &http,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req_id,
            "environment_id": env_id,
            "load_profile_id": profile["id"],
            "mode": "once"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    // 20 per process x 2 processes = 40 run-wide.
    assert_eq!(run["current_qps"], 40, "{run}");
    let id = run["id"].as_str().unwrap().to_string();
    let run_dir = tmp.path.join("runs").join(&id);
    wait_file(&run_dir.join("w00-live.json")).await;
    wait_file(&run_dir.join("w01-live.json")).await;
    let live0: Value =
        serde_json::from_slice(&fs::read(run_dir.join("w00-live.json")).unwrap()).unwrap();
    let live1: Value =
        serde_json::from_slice(&fs::read(run_dir.join("w01-live.json")).unwrap()).unwrap();
    assert_eq!(live0["current_qps"], 20, "{live0}");
    assert_eq!(live1["current_qps"], 20, "{live1}");

    // Stop mid-window (the 20 s window has no checkpoint boundary yet).
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    let _ = http
        .post(format!("{base}/api/v1/runs/{id}/stop"))
        .send()
        .await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let (_, record) = get_json(&http, &format!("{base}/api/v1/runs/{id}")).await;
        if record["status"] == "stopped" {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "run did not stop: {record}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let from_db =
        sample_count(&run_dir.join("w00.sqlite")) + sample_count(&run_dir.join("w01.sqlite"));
    let (code, run_stats) = get_json(&http, &format!("{base}/api/v1/runs/{id}/stats")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{run_stats}");
    let samples = run_stats["samples"].as_u64().unwrap_or(0);
    assert!(from_db > 0, "no samples written");
    assert_eq!(
        samples, from_db,
        "final counters lost on stop: stats {run_stats} vs db {from_db}"
    );
    // ~2.5 s at 40 QPS is ~100; the old split ran each worker at 40 (80 run-wide, ~200).
    assert!(samples <= 160, "too many samples for 40 QPS: {samples}");
    child.shutdown();
}

#[tokio::test]
async fn multiprocess_loop_verdict_excludes_loop_gaps() {
    let tmp = TempDir::new("mploop");
    let (mock, _shutdown_mock) = start_mock().await;
    let listen = format!("127.0.0.1:{}", free_port());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    let (mut child, log) = spawn_serve(&bin, &listen, &tmp.path);
    let base = format!("http://{listen}");
    let http = client();
    wait_healthy(&http, &base, &log).await;

    let (env_id, req_id) = seed_http_item(&http, &base, "mploop", &mock).await;
    let (status, profile) = post_json(
        &http,
        &format!("{base}/api/v1/load-profiles"),
        &json!({
            "name": "tmpl-mploop",
            "model": "open_loop",
            "qps": 20,
            "concurrency": 4,
            "duration_secs": 2,
            "warmup_secs": 0,
            "ramp_secs": 0,
            "timeout_secs": 5,
            "processes": 2,
            "qps_mode": "total",
            "requests_mode": "total",
            "loop_enabled": true,
            "rounds": 2,
            "loop_interval_secs": 3,
            "assert": {"min_qps_ratio": 0.8}
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{profile}");
    let (status, run) = post_json(
        &http,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req_id,
            "environment_id": env_id,
            "load_profile_id": profile["id"],
            "mode": "loop"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_string();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let (_, record) = get_json(&http, &format!("{base}/api/v1/runs/{id}")).await;
        if record["status"] == "finished" {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "run did not finish: {record}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let (_, windows) = get_json(&http, &format!("{base}/api/v1/runs/{id}/windows")).await;
    let rows = windows["windows"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 2, "one merged row per loop window: {windows}");
    let summed: u64 = rows
        .iter()
        .map(|w| w["samples"].as_u64().unwrap_or(0))
        .sum();

    let (code, verdict) = get_json(&http, &format!("{base}/api/v1/runs/{id}/verdict")).await;
    let actual = &verdict["actual"];
    assert_eq!(actual["samples"].as_u64(), Some(summed), "{verdict}");
    // Two 2 s windows: ~4 s of sending. The 3 s gap must not be counted.
    let elapsed = actual["elapsed_ms"].as_i64().unwrap_or(0);
    assert!(
        (3_500..=5_000).contains(&elapsed),
        "effective elapsed {elapsed} ms should exclude the loop gap: {verdict}"
    );
    assert_eq!(
        code,
        reqwest::StatusCode::OK,
        "qps ratio should pass: {verdict}"
    );
    child.shutdown();
}

#[tokio::test]
async fn multiprocess_loop_resume_continues_window_index_and_remaining_rounds() {
    let tmp = TempDir::new("mp-resume-gap");
    let (mock, _shutdown_mock) = start_mock().await;
    let listen = format!("127.0.0.1:{}", free_port());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    let (mut child, log) = spawn_serve(&bin, &listen, &tmp.path);
    let base = format!("http://{listen}");
    let http = client();
    wait_healthy(&http, &base, &log).await;

    let (env_id, req_id) = seed_http_item(&http, &base, "mp-resume-gap", &mock).await;
    let (status, profile) = post_json(
        &http,
        &format!("{base}/api/v1/load-profiles"),
        &json!({
            "name": "tmpl-mp-resume-gap",
            "model": "open_loop",
            "qps": 20,
            "concurrency": 4,
            "duration_secs": 1,
            "warmup_secs": 0,
            "ramp_secs": 0,
            "timeout_secs": 5,
            "processes": 2,
            "qps_mode": "total",
            "requests_mode": "total",
            "loop_enabled": true,
            "rounds": 3,
            "loop_interval_secs": 3,
            "assert": {"min_qps_ratio": 0.8}
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{profile}");
    let (status, run) = post_json(
        &http,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req_id,
            "environment_id": env_id,
            "load_profile_id": profile["id"],
            "mode": "loop"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_string();

    // Wait until window 0 completes and workers sit in the 3 s loop interval.
    let gap_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let (_, win_res) = get_json(&http, &format!("{base}/api/v1/runs/{id}/windows")).await;
        let rows = win_res["windows"].as_array().cloned().unwrap_or_default();
        if rows.len() == 1 && rows[0]["ended_at"].is_number() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < gap_deadline,
            "window 0 did not close before gap deadline: {win_res}"
        );
        tokio::time::sleep(Duration::from_millis(80)).await;
    }

    let (st, _) = post_json(&http, &format!("{base}/api/v1/runs/{id}/stop"), &json!({})).await;
    assert_eq!(st, reqwest::StatusCode::OK);
    let stop_deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let (_, record) = get_json(&http, &format!("{base}/api/v1/runs/{id}")).await;
        if record["status"] == "stopped" {
            break;
        }
        assert!(
            tokio::time::Instant::now() < stop_deadline,
            "run did not stop: {record}"
        );
        tokio::time::sleep(Duration::from_millis(80)).await;
    }

    let (_, pre_win) = get_json(&http, &format!("{base}/api/v1/runs/{id}/windows")).await;
    let pre_rows = pre_win["windows"].as_array().cloned().unwrap_or_default();
    assert_eq!(pre_rows.len(), 1, "{pre_win}");
    assert_eq!(pre_rows[0]["window_index"], 0, "{pre_win}");
    assert_eq!(pre_rows[0]["completed"], true, "{pre_win}");

    tokio::time::sleep(Duration::from_millis(300)).await;
    let (st, resumed) = post_json(
        &http,
        &format!("{base}/api/v1/runs/{id}/resume"),
        &json!({}),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::OK, "{resumed}");

    // Immediately after resume, live stats must not regress window_index back to 0.
    let (st, live_stats) = get_json(&http, &format!("{base}/api/v1/runs/{id}/stats")).await;
    assert_eq!(st, reqwest::StatusCode::OK, "{live_stats}");
    assert!(
        live_stats["window_index"].as_i64().unwrap_or(0) >= 1,
        "live stats window_index regressed to 0 on resume: {live_stats}"
    );

    let finish_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let (_, record) = get_json(&http, &format!("{base}/api/v1/runs/{id}")).await;
        if record["status"] == "finished" {
            break;
        }
        assert!(
            !matches!(record["status"].as_str(), Some("failed" | "interrupted")),
            "resumed run failed: {record}"
        );
        assert!(
            tokio::time::Instant::now() < finish_deadline,
            "resumed run did not finish: {record}"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let (_, windows) = get_json(&http, &format!("{base}/api/v1/runs/{id}/windows")).await;
    let rows = windows["windows"].as_array().cloned().unwrap_or_default();
    let indices: Vec<i64> = rows
        .iter()
        .filter_map(|w| w["window_index"].as_i64())
        .collect();
    assert_eq!(
        indices,
        vec![0, 1, 2],
        "window_index must continue [0, 1, 2] without restarting at 0: {windows}"
    );
    for row in &rows {
        assert_eq!(row["completed"], true, "{windows}");
    }
    assert!(
        rows[0]["ended_at"].as_i64().unwrap() <= rows[1]["started_at"].as_i64().unwrap()
            && rows[1]["ended_at"].as_i64().unwrap() <= rows[2]["started_at"].as_i64().unwrap(),
        "windows must be strictly non-overlapping across resume: {windows}"
    );

    let (code, verdict) = get_json(&http, &format!("{base}/api/v1/runs/{id}/verdict")).await;
    assert_eq!(
        code,
        reqwest::StatusCode::OK,
        "resumed loop verdict should pass without counting stop downtime: {verdict}"
    );
    child.shutdown();
}

#[tokio::test]
async fn multiprocess_mid_window_resume_advances_window_index_and_finishes_budget() {
    let tmp = TempDir::new("mp-resume-mid");
    let (mock, _shutdown_mock) = start_mock().await;
    let listen = format!("127.0.0.1:{}", free_port());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    let (mut child, log) = spawn_serve(&bin, &listen, &tmp.path);
    let base = format!("http://{listen}");
    let http = client();
    wait_healthy(&http, &base, &log).await;

    let (env_id, req_id) = seed_http_item(&http, &base, "mp-resume-mid", &mock).await;
    let (status, profile) = post_json(
        &http,
        &format!("{base}/api/v1/load-profiles"),
        &json!({
            "name": "tmpl-mp-resume-mid",
            "model": "open_loop",
            "qps": 10,
            "concurrency": 4,
            "requests": 40,
            "warmup_secs": 0,
            "ramp_secs": 0,
            "timeout_secs": 5,
            "processes": 2,
            "qps_mode": "total",
            "requests_mode": "total",
            "loop_enabled": true,
            "rounds": 2,
            "loop_interval_secs": 0
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{profile}");
    let (status, run) = post_json(
        &http,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req_id,
            "environment_id": env_id,
            "load_profile_id": profile["id"],
            "mode": "loop"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_string();

    // Round 0 needs 40 requests at 10 QPS (~4 s); stop once a few requests landed.
    let mid_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let (_, live_counters) = get_json(&http, &format!("{base}/api/v1/runs/{id}/stats")).await;
        let samples = live_counters["samples"].as_u64().unwrap_or(0);
        if samples >= 6 {
            assert!(
                samples < 40,
                "round 0 finished before stop: {live_counters}"
            );
            break;
        }
        assert!(
            tokio::time::Instant::now() < mid_deadline,
            "mid-window samples did not advance: {live_counters}"
        );
        tokio::time::sleep(Duration::from_millis(80)).await;
    }

    let (st, _) = post_json(&http, &format!("{base}/api/v1/runs/{id}/stop"), &json!({})).await;
    assert_eq!(st, reqwest::StatusCode::OK);
    let stop_deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let (_, record) = get_json(&http, &format!("{base}/api/v1/runs/{id}")).await;
        if record["status"] == "stopped" {
            break;
        }
        assert!(
            tokio::time::Instant::now() < stop_deadline,
            "run did not stop: {record}"
        );
        tokio::time::sleep(Duration::from_millis(80)).await;
    }

    let (_, pre_win) = get_json(&http, &format!("{base}/api/v1/runs/{id}/windows")).await;
    let pre_rows = pre_win["windows"].as_array().cloned().unwrap_or_default();
    assert_eq!(pre_rows.len(), 1, "{pre_win}");
    assert_eq!(pre_rows[0]["window_index"], 0, "{pre_win}");
    assert_eq!(pre_rows[0]["completed"], false, "{pre_win}");

    let (st, resumed) = post_json(
        &http,
        &format!("{base}/api/v1/runs/{id}/resume"),
        &json!({}),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::OK, "{resumed}");

    let finish_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let (_, record) = get_json(&http, &format!("{base}/api/v1/runs/{id}")).await;
        if record["status"] == "finished" {
            break;
        }
        assert!(
            !matches!(record["status"].as_str(), Some("failed" | "interrupted")),
            "resumed mid-window run failed: {record}"
        );
        assert!(
            tokio::time::Instant::now() < finish_deadline,
            "resumed mid-window run did not finish: {record}"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let (_, windows) = get_json(&http, &format!("{base}/api/v1/runs/{id}/windows")).await;
    let rows = windows["windows"].as_array().cloned().unwrap_or_default();
    let indices: Vec<i64> = rows
        .iter()
        .filter_map(|w| w["window_index"].as_i64())
        .collect();
    let completed: Vec<bool> = rows
        .iter()
        .filter_map(|w| w["completed"].as_bool())
        .collect();
    assert_eq!(
        indices,
        vec![0, 1, 2],
        "expected partial window 0 + remainder window 1 + round 2 window 2: {windows}"
    );
    assert_eq!(completed, vec![false, true, true], "{windows}");
    let total_samples: u64 = rows.iter().filter_map(|w| w["samples"].as_u64()).sum();
    assert_eq!(
        total_samples, 80,
        "2 rounds x 40 requests must equal 80 across split windows: {windows}"
    );
    child.shutdown();
}
