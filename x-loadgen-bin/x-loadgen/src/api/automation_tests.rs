//! HTTP tests for template-name start, overrides, wait, assertions, and `OpenAPI`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use super::server::{prepare, spawn};
use crate::cli::ServeArgs;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "x-loadgen-auto-{}-{}-{n}-{tag}",
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

fn serve_args(dir: &Path) -> ServeArgs {
    ServeArgs {
        listen: "127.0.0.1:0".to_string(),
        data_dir: dir.to_path_buf(),
        no_auth: true,
        max_qps: None,
        max_concurrency: None,
    }
}

async fn start_api(dir: &Path) -> (String, tokio::sync::oneshot::Sender<()>) {
    let prepared = prepare(serve_args(dir)).expect("prepare");
    let (addr, shutdown, _join) = spawn(prepared).await.expect("spawn");
    (format!("http://{addr}"), shutdown)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("client")
}

async fn body(response: reqwest::Response) -> Value {
    response.json::<Value>().await.expect("json")
}

async fn start_mock(fail: bool) -> (String, tokio::sync::oneshot::Sender<()>) {
    use axum::Router;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    let app = if fail {
        Router::new().route(
            "/ok",
            post(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
        )
    } else {
        Router::new().route("/ok", get(|| async { "ok" }))
    };
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

fn target_payload(gateway: &str, name: &str) -> Value {
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

fn once_template(name: &str, target_id: &str, qps: u64, requests: u64) -> Value {
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
        "processes": 1,
        "loop_enabled": false
    })
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

async fn seed_template(
    client: &reqwest::Client,
    base: &str,
    gateway: &str,
    name: &str,
    qps: u64,
    requests: u64,
) -> (String, String) {
    let (status, target) = post_json(
        client,
        &format!("{base}/api/v1/targets"),
        &target_payload(gateway, name),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{target}");
    let target_id = target["id"].as_str().unwrap().to_string();
    let (status, template) = post_json(
        client,
        &format!("{base}/api/v1/templates"),
        &once_template(name, &target_id, qps, requests),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{template}");
    (
        template["id"].as_str().unwrap().to_string(),
        name.to_string(),
    )
}

async fn wait_status(client: &reqwest::Client, base: &str, id: &str, want: &[&str]) -> Value {
    let url = format!("{base}/api/v1/runs/{id}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
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

#[tokio::test]
async fn openapi_is_public_and_parses() {
    let tmp = TempDir::new("openapi");
    let (base, shutdown) = start_api(&tmp.path).await;
    let (status, value) = get_json(&client(), &format!("{base}/api/v1/openapi.json")).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{value}");
    assert!(
        value["openapi"].as_str().unwrap_or("").starts_with("3."),
        "{value}"
    );
    assert!(value["paths"]["/api/v1/runs/{id}/verdict"].is_object());
    shutdown.send(()).ok();
}

#[tokio::test]
async fn start_by_name_with_overrides_leaves_saved_template_unchanged() {
    let tmp = TempDir::new("override");
    let (mock, _ms) = start_mock(false).await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let client = client();
    let (id, name) = seed_template(&client, &base, &mock, "nightly-sma", 20, 200).await;

    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({
            "template": name,
            "mode": "once",
            "qps": 8,
            "requests": 6
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    assert_eq!(run["template_id"], id);
    assert_eq!(run["current_qps"], 8);
    let run_id = run["id"].as_str().expect("run id");
    let (st, detail) = get_json(&client, &format!("{base}/api/v1/runs/{run_id}")).await;
    assert_eq!(st, reqwest::StatusCode::OK, "{detail}");
    assert_eq!(detail["snapshot"]["template"]["qps"], 8);
    assert_eq!(detail["snapshot"]["template"]["requests"], 6);

    let (st, saved) = get_json(&client, &format!("{base}/api/v1/templates/{id}")).await;
    assert_eq!(st, reqwest::StatusCode::OK);
    assert_eq!(saved["qps"], 20);
    assert_eq!(saved["requests"], 200);
    wait_status(
        &client,
        &base,
        run["id"].as_str().unwrap(),
        &["finished", "failed", "stopped"],
    )
    .await;
    shutdown.send(()).ok();
}

#[tokio::test]
async fn wait_returns_finished_run_and_timeout_is_409() {
    let tmp = TempDir::new("wait");
    let (mock, _ms) = start_mock(false).await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let client = client();
    let (_id, name) = seed_template(&client, &base, &mock, "wait-once", 20, 8).await;

    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs?wait=true&timeout=15"),
        &json!({"template": name, "mode": "once"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let status_name = run["status"].as_str().unwrap_or("");
    assert!(["finished", "failed"].contains(&status_name), "{run}");

    // A loop run will not finish within 1 second.
    let (st, tpl) = post_json(
        &client,
        &format!("{base}/api/v1/templates"),
        &json!({
            "name": "wait-loop",
            "target_id": run["target_id"],
            "model": "open_loop",
            "qps": 5,
            "concurrency": 1,
            "requests": 50,
            "timeout_secs": 5,
            "loop_enabled": true,
            "rounds": 20,
            "loop_interval_secs": 2
        }),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{tpl}");
    let response = client
        .post(format!("{base}/api/v1/runs?wait=true&timeout=1"))
        .json(&json!({"template": "wait-loop", "mode": "loop"}))
        .send()
        .await
        .expect("wait loop");
    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);
    let body = body(response).await;
    assert_eq!(body["error"]["code"], "wait_timeout");
    assert!(body["run"]["id"].as_str().is_some());
    let id = body["run"]["id"].as_str().unwrap();
    let _ = client
        .post(format!("{base}/api/v1/runs/{id}/stop"))
        .send()
        .await;
    shutdown.send(()).ok();
}

async fn seed_failing_template(
    client: &reqwest::Client,
    base: &str,
    gateway: &str,
    name: &str,
) -> String {
    let payload = json!({
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
            "method": "POST",
            "weight": 1
        }]
    });
    let (status, target) = post_json(client, &format!("{base}/api/v1/targets"), &payload).await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{target}");
    let target_id = target["id"].as_str().unwrap();
    let (status, template) = post_json(
        client,
        &format!("{base}/api/v1/templates"),
        &once_template(name, target_id, 10, 6),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{template}");
    name.to_string()
}

#[tokio::test]
async fn verdict_pass_and_report_include_result() {
    let tmp = TempDir::new("verdict-pass");
    let (mock, _ms) = start_mock(false).await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let client = client();
    let (_id, name) = seed_template(&client, &base, &mock, "assert-ok", 20, 8).await;

    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs?wait=true&timeout=15"),
        &json!({
            "template": name,
            "mode": "once",
            "assert": { "p99_ms": 5000.0, "max_error_rate": 1.0 }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    assert_eq!(run["verdict"]["result"], "pass", "{run}");
    let id = run["id"].as_str().unwrap();
    let (st, verdict) = get_json(&client, &format!("{base}/api/v1/runs/{id}/verdict")).await;
    assert_eq!(st, reqwest::StatusCode::OK, "{verdict}");
    assert_eq!(verdict["result"], "pass");
    assert!(verdict["actual"]["p99_ms"].as_f64().is_some(), "{verdict}");
    let (st, report) = get_json(&client, &format!("{base}/api/v1/runs/{id}/report")).await;
    assert_eq!(st, reqwest::StatusCode::OK, "{report}");
    assert_eq!(report["verdict"]["result"], "pass");
    assert!(
        report["percentiles"]["p99_us"].as_u64().is_some(),
        "{report}"
    );
    shutdown.send(()).ok();
}

#[tokio::test]
async fn verdict_fail_is_422_and_running_is_409() {
    let tmp = TempDir::new("verdict-fail");
    let (fail_gw, _fms) = start_mock(true).await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let client = client();
    let name = seed_failing_template(&client, &base, &fail_gw, "assert-fail").await;

    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs?wait=true&timeout=15"),
        &json!({
            "template": name,
            "mode": "once",
            "assert": { "max_error_rate": 0.0 }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap();
    let (st, verdict) = get_json(&client, &format!("{base}/api/v1/runs/{id}/verdict")).await;
    assert_eq!(st, reqwest::StatusCode::UNPROCESSABLE_ENTITY, "{verdict}");
    assert_eq!(verdict["result"], "fail");
    assert!(
        verdict["failed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["check"] == "max_error_rate"),
        "{verdict}"
    );

    let (ok_gw, _oms) = start_mock(false).await;
    let (_oid, oname) = seed_template(&client, &base, &ok_gw, "pending-loop", 5, 40).await;
    let (st, running) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({
            "template": oname,
            "mode": "loop",
            "loop_enabled": true,
            "rounds": 50,
            "loop_interval_secs": 5,
            "assert": { "p99_ms": 5000.0 }
        }),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::CREATED, "{running}");
    let rid = running["id"].as_str().unwrap();
    let (st, pending) = get_json(&client, &format!("{base}/api/v1/runs/{rid}/verdict")).await;
    assert_eq!(st, reqwest::StatusCode::CONFLICT, "{pending}");
    assert_eq!(pending["result"], "pending");
    let _ = client
        .post(format!("{base}/api/v1/runs/{rid}/stop"))
        .send()
        .await;
    shutdown.send(()).ok();
}

#[tokio::test]
async fn invalid_assert_and_unknown_template_are_rejected() {
    let tmp = TempDir::new("validate");
    let (mock, _ms) = start_mock(false).await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let client = client();
    let (_id, name) = seed_template(&client, &base, &mock, "valid-tpl", 10, 8).await;

    let (st, body) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"template": name, "assert": {"p99_ms": 0}}),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_request");

    let (st, body) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"template": "no-such-template"}),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::NOT_FOUND, "{body}");

    let (st, body) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"assert": {"p99_ms": 10}}),
    )
    .await;
    assert_eq!(st, reqwest::StatusCode::BAD_REQUEST, "{body}");
    shutdown.send(()).ok();
}

#[tokio::test]
async fn no_assert_verdict_is_200_none() {
    let tmp = TempDir::new("none");
    let (mock, _ms) = start_mock(false).await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let client = client();
    let (_id, name) = seed_template(&client, &base, &mock, "no-assert", 20, 8).await;
    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs?wait=true&timeout=15"),
        &json!({"template": name, "mode": "once"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap();
    let (st, verdict) = get_json(&client, &format!("{base}/api/v1/runs/{id}/verdict")).await;
    assert_eq!(st, reqwest::StatusCode::OK, "{verdict}");
    assert_eq!(verdict["result"], "none");
    shutdown.send(()).ok();
}
