//! Multi-process workers, split QPS, merged stats, isolation, and log files.

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

fn target_payload(name: &str, gateway: &str) -> Value {
    json!({
        "name": name,
        "gateway_base_url": gateway,
        "insecure_skip_verify": true,
        "tenants": [{
            "name": "plain",
            "weight": 1,
            "api_key": "k",
            "encryption_mode": "none"
        }],
        "products": [{
            "name": "probe",
            "product_id": "P001",
            "path": "/ok",
            "method": "GET",
            "weight": 1
        }]
    })
}

fn mp_template(name: &str, target_id: &str, qps: u64) -> Value {
    json!({
        "name": name,
        "target_id": target_id,
        "model": "open_loop",
        "qps": qps,
        "concurrency": 4,
        "duration_secs": 20,
        "warmup_secs": 0,
        "ramp_secs": 0,
        "timeout_secs": 5,
        "processes": 2,
        "qps_mode": "total",
        "requests_mode": "total",
        "loop_enabled": false
    })
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

fn spawn_serve(bin: &Path, listen: &str, data_dir: &Path) -> (Child, PathBuf) {
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
    (child, log_path)
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
    let (status, target) = post_json(
        http,
        &format!("{base}/api/v1/targets"),
        &target_payload(name, gateway),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{target}");
    let target_id = target["id"].as_str().unwrap();
    let (status, template) = post_json(
        http,
        &format!("{base}/api/v1/templates"),
        &mp_template(&format!("tmpl-{name}"), target_id, qps),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{template}");
    let (status, run) = post_json(
        http,
        &format!("{base}/api/v1/runs"),
        &json!({"template_id": template["id"], "mode": "once"}),
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

    // Live snapshots are published once a second; wait until they catch up
    // with the worker databases.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
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
    let _ = child.kill();
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
    let _ = (child.kill(), status_a);
}

fn mp_request_template(name: &str, target_id: &str, requests: u64, qps: u64) -> Value {
    json!({
        "name": name,
        "target_id": target_id,
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
        "loop_enabled": false
    })
}

/// A short request-count once run with two workers must reach `finished` on
/// its own after the samples are complete. Stopping the run is not allowed
/// here: that was masking workers that stayed alive after `in_flight` hit 0.
#[tokio::test]
async fn processes_two_request_once_finishes_without_stop() {
    let tmp = TempDir::new("once-finish");
    let (mock, _shutdown_mock) = start_mock().await;
    let listen = format!("127.0.0.1:{}", free_port());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_x-loadgen"));
    let (mut child, log) = spawn_serve(&bin, &listen, &tmp.path);
    let base = format!("http://{listen}");
    let http = client();
    wait_healthy(&http, &base, &log).await;

    let request_count = 10_u64;
    let (created, target) = post_json(
        &http,
        &format!("{base}/api/v1/targets"),
        &target_payload("once-finish", &mock),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{target}");
    let target_id = target["id"].as_str().unwrap();
    let (created, template) = post_json(
        &http,
        &format!("{base}/api/v1/templates"),
        &mp_request_template("tmpl-once-finish", target_id, request_count, 20),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{template}");
    let (created, run) = post_json(
        &http,
        &format!("{base}/api/v1/runs"),
        &json!({"template_id": template["id"], "mode": "once"}),
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
    let _ = child.kill();
}

const MP_TENANT_KEY: &str = "eC1sb2FkZ2VuLXRlc3QtdGVuYW50LWtleS0zMmJ5dGU=";
const MP_BINDING_KEY: &str = "eC1sb2FkZ2VuLWJpbmRpbmcta2V5";
const MP_TENANT_UUID: &str = "1a2b3c4d-0000-4000-8000-00000000c003";

/// Gateway view of the encrypted tenant used by the multi-process test.
fn mazbri_mock_config() -> x_loadgen_mockgw::MockConfig {
    x_loadgen_mockgw::MockConfig {
        tenants: vec![x_loadgen_mockgw::MockTenant {
            api_key: "ak-mp".into(),
            encryption_mode: "full".into(),
            tenant_id: Some(MP_TENANT_UUID.into()),
            tenant_key_base64: Some(MP_TENANT_KEY.into()),
            key_version: Some("1".into()),
            suite: Some("suite/tenant-chacha20".into()),
            binding_kid: Some("kid-mp".into()),
            binding_alg: Some("hmac-sha256".into()),
            binding_verify_key: Some(MP_BINDING_KEY.into()),
            ..x_loadgen_mockgw::MockTenant::default()
        }],
        ..x_loadgen_mockgw::MockConfig::default()
    }
}

/// Client view of the same tenant, as entered on the target page.
fn mazbri_target_payload(gateway: &str) -> Value {
    json!({
        "name": "mazbri-mp",
        "protocol": "mazbri",
        "gateway_base_url": gateway,
        "tenants": [{
            "name": "sealed",
            "api_key": "ak-mp",
            "encryption_mode": "full",
            "tenant_id": MP_TENANT_UUID,
            "tenant_key_base64": MP_TENANT_KEY,
            "key_version": "1",
            "suite": "suite/tenant-chacha20",
            "binding_kid": "kid-mp",
            "binding_alg": "hmac-sha256",
            "binding_key": MP_BINDING_KEY
        }],
        "products": [{
            "name": "echo",
            "product_id": "P-ECHO",
            "path": "/upstream/echo",
            "method": "POST",
            "body_json": {"seq": "{{seq}}"},
            "expect_checks_json": [{"path": "echo.product_id", "eq": "P-ECHO"}]
        }]
    })
}

/// Two worker processes against the mock mazbri gateway with full encryption
/// and HMAC request binding: each worker exchanges its own token, every
/// request is sealed, signed, opened, and judged on the decrypted response.
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

    let (created, target) = post_json(
        &http,
        &format!("{base}/api/v1/targets"),
        &mazbri_target_payload(&gateway.base_url()),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{target}");
    let request_count = 12_u64;
    let (created, template) = post_json(
        &http,
        &format!("{base}/api/v1/templates"),
        &mp_request_template(
            "tmpl-mazbri-mp",
            target["id"].as_str().unwrap(),
            request_count,
            40,
        ),
    )
    .await;
    assert_eq!(created, reqwest::StatusCode::CREATED, "{template}");
    let (created, run) = post_json(
        &http,
        &format!("{base}/api/v1/runs"),
        &json!({"template_id": template["id"], "mode": "once"}),
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
    assert_eq!(stats.bindings_verified, request_count);
    assert_eq!(stats.envelopes_opened, request_count);
    assert_eq!(stats.responses_sealed, request_count);
    let _ = child.kill();
    gateway.stop().await;
}
