//! HTTP tests for interactive Probe (`Send`) and load-test run lifecycle:
//! multi-task caps, loop windows, live traffic tuning, stop, resume, and collection runs.
#![allow(clippy::too_many_lines)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use serde_json::{Value, json};

use super::server::{prepare, spawn};
use crate::cli::ServeArgs;
use crate::db::{DataPaths, Sample, write_pending};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "x-loadgen-run-{}-{}-{n}-{tag}",
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

fn serve_args(dir: &Path, max_qps: Option<u64>) -> ServeArgs {
    ServeArgs {
        listen: "127.0.0.1:0".to_string(),
        data_dir: dir.to_path_buf(),
        no_auth: true,
        max_qps,
        max_concurrency: None,
    }
}

async fn start_api(dir: &Path, max_qps: Option<u64>) -> (String, tokio::sync::oneshot::Sender<()>) {
    let prepared = prepare(serve_args(dir, max_qps)).expect("prepare");
    let (addr, shutdown, _join) = spawn(prepared).await.expect("spawn");
    (format!("http://{addr}"), shutdown)
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

async fn start_mock() -> (String, tokio::sync::oneshot::Sender<()>) {
    use axum::Json;
    use axum::Router;
    use axum::extract::Query;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use std::collections::HashMap;

    let app = Router::new()
        .route("/ok", get(|| async { "ok" }))
        .route(
            "/json-echo",
            get(|Query(q): Query<HashMap<String, String>>| async move {
                Json(json!({
                    "code": 0,
                    "seq": q.get("seq").cloned().unwrap_or_default(),
                    "status": "ready"
                }))
            }),
        )
        .route(
            "/fail",
            post(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
        )
        .route("/v1/pay", post(|| async { "ok" }));
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
            .expect("mock server");
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    (format!("http://{addr}"), tx)
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    payload: &Value,
) -> (reqwest::StatusCode, Value) {
    let response = client.post(url).json(payload).send().await.expect("post");
    let status = response.status();
    (status, body(response).await)
}

async fn get_json(client: &reqwest::Client, url: &str) -> (reqwest::StatusCode, Value) {
    let response = client.get(url).send().await.expect("get");
    let status = response.status();
    (status, body(response).await)
}

async fn wait_status(client: &reqwest::Client, base: &str, id: &str, want: &[&str]) -> Value {
    let url = format!("{base}/api/v1/runs/{id}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let (status, value) = get_json(client, &url).await;
        assert_eq!(status, reqwest::StatusCode::OK, "{value}");
        let got = value["status"].as_str().unwrap_or("");
        if want.contains(&got) {
            return value;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "run {id} stayed at {got}, wanted one of {want:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Seed a workspace (Environment + Collection + `SavedRequest` + `LoadProfile`) and return
/// `(environment_id, request_id, profile_id)`.
async fn seed_workspace_item(
    client: &reqwest::Client,
    base: &str,
    gateway: &str,
    name: &str,
    fail_endpoint: bool,
    profile_payload: &Value,
) -> (String, String, String) {
    let (st, env) = post_json(
        client,
        &format!("{base}/api/v1/environments"),
        &json!({
            "name": format!("env-{name}"),
            "gateway_base_url": gateway,
            "variables": [{"key": "baseUrl", "value": gateway, "secret": false, "active": true}],
            "tls": {"insecure_skip_verify": true},
            "tenants": [{"name": "plain", "weight": 1, "api_key": "k", "encryption_mode": "none"}]
        }),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{env}");
    let env_id = env["id"].as_str().expect("env id").to_string();

    let (st, col) = post_json(
        client,
        &format!("{base}/api/v1/collections"),
        &json!({"name": format!("col-{name}")}),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{col}");
    let col_id = col["id"].as_str().expect("col id").to_string();

    let (method, endpoint) = if fail_endpoint {
        ("POST", "<<baseUrl>>/fail")
    } else {
        ("GET", "<<baseUrl>>/ok")
    };
    let (st, req) = post_json(
        client,
        &format!("{base}/api/v1/requests"),
        &json!({
            "collection_id": col_id,
            "name": format!("req-{name}"),
            "protocol": "http",
            "method": method,
            "endpoint": endpoint,
            "mazbri": {"product_id": "P001"},
            "weight": 1
        }),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{req}");
    let req_id = req["id"].as_str().expect("req id").to_string();

    let (st, prof) = post_json(
        client,
        &format!("{base}/api/v1/load-profiles"),
        profile_payload,
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{prof}");
    let prof_id = prof["id"].as_str().expect("profile id").to_string();

    (env_id, req_id, prof_id)
}

fn once_profile(name: &str, qps: u64, requests: u64) -> Value {
    json!({
        "name": name,
        "model": "open_loop",
        "qps": qps,
        "concurrency": 2,
        "requests": requests,
        "warmup_secs": 0,
        "ramp_secs": 0,
        "timeout_secs": 5,
        "processes": 1,
        "loop_enabled": false
    })
}

fn loop_profile(name: &str, interval: u64) -> Value {
    json!({
        "name": name,
        "model": "open_loop",
        "qps": 20,
        "concurrency": 2,
        "requests": 8,
        "warmup_secs": 0,
        "ramp_secs": 0,
        "timeout_secs": 5,
        "processes": 1,
        "loop_enabled": true,
        "rounds": 3,
        "loop_interval_secs": interval
    })
}

#[tokio::test]
async fn interactive_probe_executes_http_and_encrypted_mazbri_with_wire_debug() {
    const TENANT_KEY: &str = "eC1sb2FkZ2VuLXRlc3QtdGVuYW50LWtleS0zMmJ5dGU=";
    const TENANT_UUID: &str = "9a8b7c6d-0000-4000-8000-00000000d004";

    let tmp = TempDir::new("probe");
    let (mock, _mock_shutdown) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path, None).await;
    let client = client();

    // 1. HTTP probe with <<baseUrl>>, {{seq}}, detailed assertions, and unresolved var detection.
    let (st, env) = post_json(
        &client,
        &format!("{base}/api/v1/environments"),
        &json!({
            "name": "dev-http",
            "variables": [{"key": "baseUrl", "value": mock, "secret": false, "active": true}]
        }),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{env}");
    let env_id = env["id"].as_str().expect("env id");

    let (st, probe_http) = post_json(
        &client,
        &format!("{base}/api/v1/probe"),
        &json!({
            "environment_id": env_id,
            "seq": 42,
            "request": {
                "name": "Echo JSON",
                "protocol": "http",
                "method": "GET",
                "endpoint": "<<baseUrl>>/json-echo",
                "params": [
                    {"key": "seq", "value": "{{seq}}", "active": true},
                    {"key": "missing", "value": "<<notDefined>>", "active": true}
                ],
                "assertions": {
                    "expect_transport": "ok",
                    "checks": [
                        {"path": "code", "eq": 0},
                        {"path": "seq", "eq": "42"}
                    ]
                }
            }
        }),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::OK, "{probe_http}");
    assert_eq!(probe_http["protocol"], "http");
    assert_eq!(probe_http["transport_ok"], true);
    assert_eq!(probe_http["passed_assertions"], true);
    assert_eq!(probe_http["http_status"], 200);
    assert_eq!(probe_http["body_json"]["seq"], "42");
    assert_eq!(probe_http["assertions"].as_array().map(Vec::len), Some(2));
    assert!(
        probe_http["request_preview"]["unresolved_variables"]
            .as_array()
            .expect("unresolved")
            .iter()
            .any(|v| v == "notDefined"),
        "{probe_http}"
    );

    // 2. Mazbri probe (full envelope encryption + SM4/SM3 suite + SM2 signature) via mockgw.
    let gateway = x_loadgen_mockgw::MockGateway::start(
        "127.0.0.1:0",
        &x_loadgen_mockgw::MockConfig::default(),
    )
    .await
    .expect("mockgw");
    let (st, kp) = post_json(
        &client,
        &format!("{base}/api/v1/crypto/binding-keypair"),
        &json!({"alg": "sm2-sign", "kid": "kid-probe"}),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::OK, "{kp}");
    gateway
        .set_config(&x_loadgen_mockgw::MockConfig {
            tenants: vec![x_loadgen_mockgw::MockTenant {
                api_key: "ak-probe".into(),
                encryption_mode: "full".into(),
                tenant_id: Some(TENANT_UUID.into()),
                tenant_key_base64: Some(TENANT_KEY.into()),
                key_version: Some("1".into()),
                suite: Some("suite/gm-sm4-hmac-sm3".into()),
                binding_kid: Some("kid-probe".into()),
                binding_alg: Some("sm2-sign".into()),
                binding_verify_key: Some(kp["public_key_pem"].as_str().expect("pub").to_string()),
                ..x_loadgen_mockgw::MockTenant::default()
            }],
            ..x_loadgen_mockgw::MockConfig::default()
        })
        .expect("set mockgw config");

    let (st, probe_mazbri) = post_json(
        &client,
        &format!("{base}/api/v1/probe"),
        &json!({
            "seq": 7,
            "request": {
                "name": "Mazbri Echo",
                "protocol": "mazbri",
                "method": "POST",
                "endpoint": "/upstream/echo",
                "body": {
                    "content_type": "application/json",
                    "body": "{\"hello\":\"world\",\"seq\":\"{{seq}}\"}"
                },
                "mazbri": {
                    "product_id": "P-PROBE",
                    "gateway_base_url": gateway.base_url(),
                    "tenant_source": "inline",
                    "inline_tenant": {
                        "name": "inline-gm",
                        "api_key": "ak-probe",
                        "encryption_mode": "full",
                        "tenant_id": TENANT_UUID,
                        "tenant_key_base64": TENANT_KEY,
                        "key_version": "1",
                        "suite": "suite/gm-sm4-hmac-sm3",
                        "binding_kid": "kid-probe",
                        "binding_alg": "sm2-sign",
                        "binding_key": kp["private_key_pem"]
                    }
                },
                "assertions": {
                    "expect_transport": "ok",
                    "checks": [{"path": "echo.product_id", "eq": "P-PROBE"}]
                }
            }
        }),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::OK, "{probe_mazbri}");
    assert_eq!(probe_mazbri["protocol"], "mazbri");
    assert_eq!(probe_mazbri["transport_ok"], true, "{probe_mazbri}");
    assert_eq!(probe_mazbri["passed_assertions"], true, "{probe_mazbri}");
    assert_eq!(probe_mazbri["body_json"]["echo"]["product_id"], "P-PROBE");
    let wire = &probe_mazbri["mazbri_wire"];
    assert_eq!(wire["encryption_mode"], "full");
    assert_eq!(wire["suite"], "suite/gm-sm4-hmac-sm3");
    assert_eq!(wire["binding_alg"], "sm2-sign");
    assert!(wire["binding_headers"]["x-mazbri-signature"].is_string());
    assert!(wire["decoded_envelope"].is_object());

    gateway.stop().await;
    let _ = shutdown.send(());
}

#[tokio::test]
async fn two_once_runs_can_overlap_and_third_over_cap_is_rejected() {
    let tmp = TempDir::new("cap");
    let (mock, _mock_shutdown) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path, Some(10)).await;
    let client = client();

    let (env_a, req_a, prof_a) = seed_workspace_item(
        &client,
        &base,
        &mock,
        "a",
        false,
        &once_profile("prof-a", 4, 40),
    )
    .await;
    let (env_b, req_b, prof_b) = seed_workspace_item(
        &client,
        &base,
        &mock,
        "b",
        false,
        &once_profile("prof-b", 4, 40),
    )
    .await;
    let (env_c, req_c, prof_c) = seed_workspace_item(
        &client,
        &base,
        &mock,
        "c",
        false,
        &once_profile("prof-c", 4, 40),
    )
    .await;

    let (status, run_a) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"request_id": req_a, "environment_id": env_a, "load_profile_id": prof_a, "mode": "once"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run_a}");
    let (status, run_b) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"request_id": req_b, "environment_id": env_b, "load_profile_id": prof_b, "mode": "once"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run_b}");
    assert_eq!(run_a["status"], "running");
    assert_eq!(run_b["status"], "running");
    assert_ne!(run_a["id"], run_b["id"]);

    let dir_a = tmp.path.join("runs").join(run_a["id"].as_str().unwrap());
    let dir_b = tmp.path.join("runs").join(run_b["id"].as_str().unwrap());
    assert!(dir_a.is_dir(), "{}", dir_a.display());
    assert!(dir_b.is_dir(), "{}", dir_b.display());

    let (status, running) = get_json(&client, &format!("{base}/api/v1/runs?status=running")).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(running.as_array().map(Vec::len), Some(2), "{running}");

    let (status, rejected) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"request_id": req_c, "environment_id": env_c, "load_profile_id": prof_c, "mode": "once"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{rejected}");
    assert_eq!(rejected["error"]["code"], "invalid_request");

    let a_id = run_a["id"].as_str().unwrap();
    let b_id = run_b["id"].as_str().unwrap();
    wait_status(&client, &base, a_id, &["finished", "stopped"]).await;
    wait_status(&client, &base, b_id, &["finished", "stopped"]).await;
    let _ = shutdown.send(());
}

#[tokio::test]
async fn loop_rounds_write_three_windows_and_survive_injected_failures() {
    let tmp = TempDir::new("loop");
    let (mock, _mock_shutdown) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path, None).await;
    let client = client();

    let (env_id, req_id, prof_id) = seed_workspace_item(
        &client,
        &base,
        &mock,
        "loop-fail",
        true,
        &loop_profile("prof-loop-fail", 1),
    )
    .await;
    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req_id,
            "environment_id": env_id,
            "load_profile_id": prof_id,
            "mode": "loop"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap();
    wait_status(&client, &base, id, &["finished"]).await;

    let (status, windows) = get_json(&client, &format!("{base}/api/v1/runs/{id}/windows")).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{windows}");
    let rows = windows["windows"].as_array().expect("windows array");
    assert_eq!(rows.len(), 3, "{windows}");
    let start0 = rows[0]["started_at"].as_i64().unwrap();
    let start1 = rows[1]["started_at"].as_i64().unwrap();
    let start2 = rows[2]["started_at"].as_i64().unwrap();
    assert!(
        start1 - start0 >= 900,
        "window interval too small: {start0} -> {start1}"
    );
    assert!(
        start2 - start1 >= 900,
        "window interval too small: {start1} -> {start2}"
    );
    let (code, live) = get_json(&client, &format!("{base}/api/v1/runs/{id}/stats")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{live}");
    assert!(live["failure"].as_u64().unwrap_or(0) > 0, "{live}");
    let _ = shutdown.send(());
}

#[tokio::test]
async fn patch_qps_is_recorded_and_over_cap_is_rejected() {
    let tmp = TempDir::new("patch");
    let (mock, _mock_shutdown) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path, Some(10)).await;
    let client = client();

    let (env_id, req_id, prof_id) = seed_workspace_item(
        &client,
        &base,
        &mock,
        "patch",
        false,
        &once_profile("patch-once", 4, 200),
    )
    .await;
    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req_id,
            "environment_id": env_id,
            "load_profile_id": prof_id,
            "mode": "once"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap();

    let response = client
        .patch(format!("{base}/api/v1/runs/{id}/traffic"))
        .json(&json!({"qps": 6}))
        .send()
        .await
        .expect("patch");
    let status = response.status();
    let patched = body(response).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{patched}");
    assert_eq!(patched["current_qps"], 6);

    let (code, live) = get_json(&client, &format!("{base}/api/v1/runs/{id}/stats")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{live}");
    assert_eq!(live["current_qps"], 6);

    let (status, timeline) = get_json(&client, &format!("{base}/api/v1/runs/{id}/timeline")).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{timeline}");
    let events = timeline["events"].as_array().expect("events");
    assert!(
        events.iter().any(|event| event["event"] == "traffic"),
        "{timeline}"
    );

    let response = client
        .patch(format!("{base}/api/v1/runs/{id}/traffic"))
        .json(&json!({"qps": 20}))
        .send()
        .await
        .expect("patch over cap");
    let status = response.status();
    let rejected = body(response).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{rejected}");

    let (status, again) = get_json(&client, &format!("{base}/api/v1/runs/{id}/stats")).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{again}");
    assert_eq!(again["current_qps"], 6);

    let _ = client
        .post(format!("{base}/api/v1/runs/{id}/stop"))
        .send()
        .await;
    wait_status(&client, &base, id, &["stopped", "finished"]).await;
    let _ = shutdown.send(());
}

fn dumped_sample() -> Sample {
    Sample {
        seq: None,
        start_unix_ms: 1_700_000_000_000,
        end_unix_ms: 1_700_000_000_100,
        latency_us: 100,
        product_id: "P001".into(),
        product_name: Some("probe".into()),
        request_index: Some(0),
        http_status: Some(200),
        success: true,
        expected: false,
        reason: None,
        error_class: None,
        request_id: Some("dump-1".into()),
        error: None,
        response_bytes: Some(2),
        customer_name: None,
    }
}

#[tokio::test]
async fn resume_imports_pending_dump_and_collection_and_adhoc_runs_succeed() {
    let tmp = TempDir::new("resume");
    let (mock, _mock_shutdown) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path, None).await;
    let client = client();

    let (env_id, req_id, prof_id) = seed_workspace_item(
        &client,
        &base,
        &mock,
        "resume",
        false,
        &once_profile("resume-once", 8, 80),
    )
    .await;
    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req_id,
            "environment_id": env_id,
            "load_profile_id": prof_id,
            "mode": "once"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_string();

    tokio::time::sleep(Duration::from_millis(200)).await;
    let response = client
        .post(format!("{base}/api/v1/runs/{id}/stop"))
        .send()
        .await
        .expect("stop");
    assert!(response.status().is_success(), "{}", response.status());
    wait_status(&client, &base, &id, &["stopped"]).await;

    let (code, before) = get_json(&client, &format!("{base}/api/v1/runs/{id}/stats")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{before}");
    let samples_before = before["samples"].as_u64().unwrap_or(0);

    let paths = DataPaths::new(&tmp.path);
    let dumped = write_pending(&paths, &id, 0, &[dumped_sample()]).expect("write pending");
    assert!(dumped.exists());

    let (status, resumed) = post_json(
        &client,
        &format!("{base}/api/v1/runs/{id}/resume"),
        &json!({}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{resumed}");
    assert_eq!(resumed["status"], "running");
    let _ = wait_status(&client, &base, &id, &["running", "finished", "stopped"]).await;
    let (status, detail) = get_json(&client, &format!("{base}/api/v1/runs/{id}")).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{detail}");
    let ranges = detail["interrupt_ranges"]
        .as_array()
        .expect("interrupt_ranges");
    assert!(!ranges.is_empty(), "{detail}");

    wait_status(&client, &base, &id, &["finished"]).await;
    let (code, after) = get_json(&client, &format!("{base}/api/v1/runs/{id}/stats")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{after}");
    assert!(
        after["samples"].as_u64().unwrap_or(0) >= samples_before,
        "counters should continue from {samples_before}: {after}"
    );

    let worker = paths.worker_db(&id, 0);
    let conn = Connection::open(&worker).expect("open worker");
    let dumped_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM samples WHERE request_id = 'dump-1'",
            [],
            |row| row.get(0),
        )
        .expect("count dumped");
    assert_eq!(dumped_rows, 1, "pending dump was not imported");

    let (status, rejected) = post_json(
        &client,
        &format!("{base}/api/v1/runs/{id}/resume"),
        &json!({}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{rejected}");

    // Ad-hoc unsaved Hoppscotch tab load test works without creating a SavedRequest or LoadProfile.
    let (status, adhoc_run) = post_json(
        &client,
        &format!("{base}/api/v1/runs?wait=true&timeout=10"),
        &json!({
            "adhoc_request": {
                "name": "Quick Tab Stress",
                "protocol": "http",
                "method": "POST",
                "endpoint": format!("{mock}/v1/pay")
            },
            "model": "open_loop",
            "qps": 20,
            "concurrency": 2,
            "requests": 6,
            "mode": "once"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{adhoc_run}");
    assert_eq!(adhoc_run["status"], "finished", "{adhoc_run}");

    let _ = shutdown.send(());
}
