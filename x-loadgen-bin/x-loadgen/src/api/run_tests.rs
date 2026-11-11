//! HTTP tests for run lifecycle: multi-task, loop windows, traffic, stop, resume.

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
    use axum::Router;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    let app = Router::new()
        .route("/ok", get(|| async { "ok" }))
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

fn target_payload(gateway: &str, fail_product: bool) -> Value {
    let path = if fail_product { "/fail" } else { "/ok" };
    let method = if fail_product { "POST" } else { "GET" };
    json!({
        "name": format!("gw-{}", if fail_product { "fail" } else { "ok" }),
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
            "path": path,
            "method": method,
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

fn loop_template(name: &str, target_id: &str, interval: u64, fail: bool) -> Value {
    let _ = fail;
    json!({
        "name": name,
        "target_id": target_id,
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

async fn post_named_template(
    client: &reqwest::Client,
    base: &str,
    gateway: &str,
    name: &str,
    qps: u64,
    requests: u64,
) -> String {
    let mut payload = target_payload(gateway, false);
    payload["name"] = json!(name);
    let (status, target) = post_json(client, &format!("{base}/api/v1/targets"), &payload).await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{target}");
    let target_id = target["id"].as_str().unwrap();
    let (status, template) = post_json(
        client,
        &format!("{base}/api/v1/templates"),
        &once_template(&format!("tmpl-{name}"), target_id, qps, requests),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{template}");
    template["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn two_once_runs_can_overlap_and_third_over_cap_is_rejected() {
    let tmp = TempDir::new("cap");
    let (mock, _mock_shutdown) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path, Some(10)).await;
    let client = client();

    let id_a = post_named_template(&client, &base, &mock, "a", 4, 40).await;
    let id_b = post_named_template(&client, &base, &mock, "b", 4, 40).await;
    let id_c = post_named_template(&client, &base, &mock, "c", 4, 40).await;

    let (status, run_a) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"template_id": id_a, "mode": "once"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run_a}");
    let (status, run_b) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"template_id": id_b, "mode": "once"}),
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
        &json!({"template_id": id_c, "mode": "once"}),
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

    let (status, target) = post_json(
        &client,
        &format!("{base}/api/v1/targets"),
        &target_payload(&mock, true),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{target}");
    let target_id = target["id"].as_str().unwrap();
    let (status, template) = post_json(
        &client,
        &format!("{base}/api/v1/templates"),
        &loop_template("loop-fail", target_id, 1, true),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{template}");
    let template_id = template["id"].as_str().unwrap();
    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"template_id": template_id, "mode": "loop"}),
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

    let (status, target) = post_json(
        &client,
        &format!("{base}/api/v1/targets"),
        &target_payload(&mock, false),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{target}");
    let target_id = target["id"].as_str().unwrap();
    let (status, template) = post_json(
        &client,
        &format!("{base}/api/v1/templates"),
        &once_template("patch-once", target_id, 4, 200),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{template}");
    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"template_id": template["id"], "mode": "once"}),
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
async fn resume_imports_pending_dump_and_rejects_finished() {
    let tmp = TempDir::new("resume");
    let (mock, _mock_shutdown) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path, None).await;
    let client = client();

    let (status, target) = post_json(
        &client,
        &format!("{base}/api/v1/targets"),
        &target_payload(&mock, false),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{target}");
    let target_id = target["id"].as_str().unwrap();
    let (status, template) = post_json(
        &client,
        &format!("{base}/api/v1/templates"),
        &once_template("resume-once", target_id, 8, 80),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{template}");
    let (status, run) = post_json(
        &client,
        &format!("{base}/api/v1/runs"),
        &json!({"template_id": template["id"], "mode": "once"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_string();

    // Give the run a moment to produce some samples, then stop it.
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

    // Imported dump should appear in the worker database.
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

    let _ = shutdown.send(());
}
