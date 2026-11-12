//! HTTP tests for redacted reports, sample pagination, SSE, and Mazbri E2E secret scrubbing.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use super::server::{prepare, spawn};
use crate::cli::ServeArgs;
use crate::db::{DataPaths, Sample, insert_samples, open_worker_connection};

const API_KEY: &str = "SECRET-API-KEY-AAAA-REPORT";
const BEARER_TOKEN: &str = "SECRET-BEARER-BBBB-REPORT";
const SECRET_VAR: &str = "SECRET-ENV-VAR-CCCC-REPORT";

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "x-loadgen-report-{}-{}-{n}-{tag}",
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
        .timeout(Duration::from_secs(15))
        .build()
        .expect("client")
}

async fn body(response: reqwest::Response) -> Value {
    response.json::<Value>().await.expect("json")
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

fn assert_no_secrets(text: &str) {
    for secret in [API_KEY, BEARER_TOKEN, SECRET_VAR] {
        assert!(!text.contains(secret), "secret leaked in output: {secret}");
    }
}

async fn start_once_run(
    http: &reqwest::Client,
    base: &str,
    gateway: &str,
    name: &str,
    requests: u64,
) -> (String, String) {
    let (status, env) = post_json(
        http,
        &format!("{base}/api/v1/environments"),
        &json!({
            "name": format!("env-{name}"),
            "gateway_base_url": gateway,
            "variables": [
                {"key": "baseUrl", "value": gateway, "secret": false, "active": true},
                {"key": "secretToken", "value": SECRET_VAR, "secret": true, "active": true}
            ],
            "http_auth": {
                "auth_type": "bearer",
                "auth_active": true,
                "token": BEARER_TOKEN
            },
            "tls": {"insecure_skip_verify": true},
            "tenants": [{
                "name": "plain",
                "weight": 1,
                "api_key": API_KEY,
                "encryption_mode": "none"
            }]
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{env}");
    let env_id = env["id"].as_str().unwrap().to_string();

    let (status, col) = post_json(
        http,
        &format!("{base}/api/v1/collections"),
        &json!({"name": format!("col-{name}")}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{col}");
    let col_id = col["id"].as_str().unwrap();

    let (status, req) = post_json(
        http,
        &format!("{base}/api/v1/requests"),
        &json!({
            "collection_id": col_id,
            "name": "probe",
            "protocol": "http",
            "method": "GET",
            "endpoint": "<<baseUrl>>/ok",
            "headers": [{"key": "X-Api-Key", "value": API_KEY, "active": true}],
            "mazbri": {"product_id": "P001"},
            "weight": 1
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{req}");

    let (status, profile) = post_json(
        http,
        &format!("{base}/api/v1/load-profiles"),
        &json!({
            "name": format!("tpl-{name}"),
            "model": "open_loop",
            "qps": 20,
            "concurrency": 2,
            "requests": requests,
            "warmup_secs": 0,
            "ramp_secs": 0,
            "timeout_secs": 5,
            "processes": 1,
            "loop_enabled": false
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{profile}");

    let (status, run) = post_json(
        http,
        &format!("{base}/api/v1/runs"),
        &json!({
            "request_id": req["id"],
            "environment_id": env_id,
            "load_profile_id": profile["id"],
            "mode": "once"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_string();
    (id, env_id)
}

#[tokio::test]
async fn report_and_run_detail_redact_secrets_but_environment_detail_does_not() {
    let tmp = TempDir::new("redact");
    let (gateway, _mock) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let http = client();
    let (id, env_id) = start_once_run(&http, &base, &gateway, "redact", 4).await;
    let _ = wait_status(&http, &base, &id, &["finished", "stopped"]).await;

    let (code, detail) = get_json(&http, &format!("{base}/api/v1/runs/{id}")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{detail}");
    assert_no_secrets(&detail.to_string());
    assert!(detail.get("snapshot_json").is_none());

    let (code, report) = get_json(&http, &format!("{base}/api/v1/runs/{id}/report")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{report}");
    assert_no_secrets(&report.to_string());
    assert!(report.get("snapshot_json").is_none());
    assert_eq!(report["id"], id);

    let response = http
        .get(format!("{base}/api/v1/runs/{id}/report.json"))
        .send()
        .await
        .expect("report.json");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let disp = response
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(disp.contains("attachment"), "{disp}");
    let json_text = response.text().await.expect("json body");
    assert_no_secrets(&json_text);

    let response = http
        .get(format!("{base}/api/v1/runs/{id}/report.html"))
        .send()
        .await
        .expect("report.html");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let html = response.text().await.expect("html");
    assert_no_secrets(&html);
    assert!(html.contains("压测报告"), "{html:.200}");

    let (code, env) = get_json(&http, &format!("{base}/api/v1/environments/{env_id}")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{env}");
    assert_eq!(env["http_auth"]["token"], BEARER_TOKEN);
    assert_eq!(env["tenants"][0]["api_key"], API_KEY);

    let forbidden = [
        "summary.json",
        "cluster-summary.json",
        "platform-enrichment.json",
    ];
    let found = walk_named(&tmp.path, &forbidden);
    assert_eq!(found.len(), 0, "unexpected summary files: {found:?}");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn report_includes_interrupt_ranges_after_resume() {
    let tmp = TempDir::new("interrupt");
    let (gateway, _mock) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let http = client();
    let (id, _) = start_once_run(&http, &base, &gateway, "interrupt", 80).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let (code, stopped) =
        post_json(&http, &format!("{base}/api/v1/runs/{id}/stop"), &json!({})).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{stopped}");
    let _ = wait_status(&http, &base, &id, &["stopped"]).await;
    let (code, resumed) = post_json(
        &http,
        &format!("{base}/api/v1/runs/{id}/resume"),
        &json!({}),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{resumed}");
    let _ = wait_status(&http, &base, &id, &["finished", "stopped"]).await;
    let (code, report) = get_json(&http, &format!("{base}/api/v1/runs/{id}/report")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{report}");
    let ranges = report["interrupt_ranges"].as_array().expect("ranges");
    assert_ne!(
        ranges.len(),
        0,
        "expected interrupt ranges in report: {report}"
    );
    assert!(ranges[0].get("start").is_some());
    assert!(ranges[0].get("end").is_some());
    let _ = shutdown.send(());
}

#[tokio::test]
async fn samples_filter_and_keyset_pagination_are_stable() {
    let tmp = TempDir::new("samples");
    let (gateway, _mock) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let http = client();
    let (id, _) = start_once_run(&http, &base, &gateway, "samples", 2).await;
    let _ = wait_status(&http, &base, &id, &["finished", "stopped"]).await;

    let paths = DataPaths::new(&tmp.path);
    seed_pages(&paths, &id);

    let (code, page1) = get_json(&http, &format!("{base}/api/v1/runs/{id}/samples?limit=10")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{page1}");
    let items = page1["items"].as_array().expect("items");
    assert_eq!(items.len(), 10);
    let next = page1["next"].as_object().expect("next cursor");
    let cursor = format!(
        "after_end_unix_ms={}&after_worker={}&after_seq={}&limit=10",
        next["after_end_unix_ms"], next["after_worker"], next["after_seq"]
    );
    let (code, page2) = get_json(&http, &format!("{base}/api/v1/runs/{id}/samples?{cursor}")).await;
    assert_eq!(code, reqwest::StatusCode::OK, "{page2}");
    let items2 = page2["items"].as_array().expect("items2");
    assert_ne!(items2.len(), 0);
    let last1 = (
        items.last().unwrap()["end_unix_ms"].as_i64().unwrap(),
        items.last().unwrap()["worker"].as_u64().unwrap(),
        items.last().unwrap()["seq"].as_i64().unwrap(),
    );
    let first2 = (
        items2[0]["end_unix_ms"].as_i64().unwrap(),
        items2[0]["worker"].as_u64().unwrap(),
        items2[0]["seq"].as_i64().unwrap(),
    );
    assert!(
        first2 > last1,
        "pages should be strictly increasing: {last1:?} vs {first2:?}"
    );

    let (code, only_a) = get_json(
        &http,
        &format!("{base}/api/v1/runs/{id}/samples?product_id=PA&limit=1000"),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{only_a}");
    let items_a = only_a["items"].as_array().expect("A");
    assert_ne!(items_a.len(), 0);
    assert!(items_a.iter().all(|row| row["product_id"] == "PA"));

    let (code, fails) = get_json(
        &http,
        &format!("{base}/api/v1/runs/{id}/samples?success=false&limit=1000"),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{fails}");
    let fail_items = fails["items"].as_array().expect("fails");
    assert_ne!(fail_items.len(), 0);
    assert!(fail_items.iter().all(|row| row["success"] == false));
    for row in fail_items {
        if let Some(err) = row["error"].as_str() {
            assert_no_secrets(err);
        }
    }

    let (code, ranged) = get_json(
        &http,
        &format!("{base}/api/v1/runs/{id}/samples?from_ms=10000&to_ms=10050&limit=1000"),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{ranged}");
    for row in ranged["items"].as_array().expect("range") {
        let end = row["end_unix_ms"].as_i64().unwrap();
        assert!((10_000..=10_050).contains(&end), "{end}");
    }

    let _ = shutdown.send(());
}

#[tokio::test]
async fn sse_sends_a_stats_event() {
    let tmp = TempDir::new("sse");
    let (gateway, _mock) = start_mock().await;
    let (base, shutdown) = start_api(&tmp.path).await;
    let http = client();
    let (id, _) = start_once_run(&http, &base, &gateway, "sse", 8).await;
    let response = http
        .get(format!("{base}/api/v1/runs/{id}/events"))
        .send()
        .await
        .expect("sse");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let ctype = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ctype.contains("text/event-stream"), "{ctype}");
    let mut response = response;
    let chunk = tokio::time::timeout(Duration::from_secs(3), response.chunk())
        .await
        .expect("sse timeout")
        .expect("chunk")
        .expect("bytes");
    let text = String::from_utf8_lossy(&chunk);
    assert!(
        text.contains("event: stats") || text.contains("data:"),
        "{text}"
    );
    assert_no_secrets(&text);
    let _ = wait_status(&http, &base, &id, &["finished", "stopped"]).await;
    let _ = shutdown.send(());
}

fn seed_pages(paths: &DataPaths, run_id: &str) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for i in 0..60 {
        let sample = Sample {
            seq: Some(i + 1),
            start_unix_ms: 10_000 + i,
            end_unix_ms: 10_000 + i,
            latency_us: 50 + i,
            product_id: if i % 2 == 0 { "PA" } else { "PB" }.into(),
            product_name: None,
            request_index: Some(i),
            http_status: Some(if i % 5 == 0 { 500 } else { 200 }),
            success: i % 5 != 0,
            expected: false,
            reason: None,
            error_class: if i % 5 == 0 {
                Some("http".into())
            } else {
                None
            },
            request_id: None,
            error: if i % 5 == 0 {
                Some(format!("failed for {API_KEY}"))
            } else {
                None
            },
            response_bytes: Some(8),
            customer_name: None,
        };
        if i % 2 == 0 {
            left.push(sample);
        } else {
            right.push(sample);
        }
    }
    let conn0 = open_worker_connection(paths, run_id, 0).expect("w00");
    conn0.execute("DELETE FROM samples", []).expect("clear w00");
    insert_samples(&conn0, &left).expect("seed 0");
    drop(conn0);
    let conn1 = open_worker_connection(paths, run_id, 1).expect("w01");
    conn1.execute("DELETE FROM samples", []).expect("clear w01");
    insert_samples(&conn1, &right).expect("seed 1");
}

fn walk_named(root: &Path, names: &[&str]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if names.iter().any(|name| entry.file_name() == *name) {
                found.push(path);
            }
        }
    }
    found
}

const MAZBRI_TENANT_KEY: &str = "eC1sb2FkZ2VuLXRlc3QtdGVuYW50LWtleS0zMmJ5dGU=";
const MAZBRI_API_KEY: &str = "SECRET-MAZBRI-API-KEY-EEEE";
const MAZBRI_TENANT_UUID: &str = "7d1c2b3a-0000-4000-8000-00000000b002";

async fn mazbri_workspace_with_generated_sm2_key(
    http: &reqwest::Client,
    base: &str,
    gateway: &x_loadgen_mockgw::MockGateway,
) -> (String, String, String) {
    use x_loadgen_mockgw::{MockConfig, MockTenant};

    let (status, env) = post_json(
        http,
        &format!("{base}/api/v1/environments"),
        &json!({
            "name": "mazbri-mock",
            "gateway_base_url": gateway.base_url(),
            "tenants": [{
                "name": "gm",
                "api_key": MAZBRI_API_KEY,
                "encryption_mode": "full",
                "tenant_id": MAZBRI_TENANT_UUID,
                "tenant_key_base64": MAZBRI_TENANT_KEY,
                "key_version": "3",
                "suite": "suite/gm-sm4-hmac-sm3"
            }]
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{env}");
    let env_id = env["id"].as_str().expect("env id").to_string();
    let tid = env["tenants"][0]["id"].as_str().expect("tid").to_string();

    let (status, keypair) = post_json(
        http,
        &format!("{base}/api/v1/environments/{env_id}/tenants/{tid}/binding-keypair"),
        &json!({"alg": "sm2-sign", "kid": "kid-gm"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{keypair}");
    let public_pem = keypair["public_key_pem"]
        .as_str()
        .expect("public")
        .to_string();

    gateway
        .set_config(&MockConfig {
            tenants: vec![MockTenant {
                api_key: MAZBRI_API_KEY.into(),
                encryption_mode: "full".into(),
                tenant_id: Some(MAZBRI_TENANT_UUID.into()),
                tenant_key_base64: Some(MAZBRI_TENANT_KEY.into()),
                key_version: Some("3".into()),
                suite: Some("suite/gm-sm4-hmac-sm3".into()),
                binding_kid: Some("kid-gm".into()),
                binding_alg: Some("sm2-sign".into()),
                binding_verify_key: Some(public_pem),
                ..MockTenant::default()
            }],
            ..MockConfig::default()
        })
        .expect("register tenant");

    let (status, col) = post_json(
        http,
        &format!("{base}/api/v1/collections"),
        &json!({"name": "mazbri-suite", "default_protocol": "mazbri"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{col}");
    let (status, req) = post_json(
        http,
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
    assert_eq!(status, reqwest::StatusCode::CREATED, "{req}");
    let req_id = req["id"].as_str().expect("req id").to_string();

    let (_, stored) = get_json(http, &format!("{base}/api/v1/environments/{env_id}")).await;
    let private_pem = stored["tenants"][0]["binding_key"]
        .as_str()
        .expect("private key")
        .to_string();
    (env_id, req_id, private_pem)
}

async fn assert_run_outputs_hide(
    http: &reqwest::Client,
    base: &str,
    id: &str,
    detail: &Value,
    secrets: &[&str],
) {
    let mut outputs = vec![detail.to_string()];
    outputs.push(
        get_json(http, &format!("{base}/api/v1/runs/{id}/report"))
            .await
            .1
            .to_string(),
    );
    for suffix in ["report.json", "report.html", "samples?limit=100"] {
        let text = http
            .get(format!("{base}/api/v1/runs/{id}/{suffix}"))
            .send()
            .await
            .expect("export")
            .text()
            .await
            .expect("text");
        outputs.push(text);
    }
    for text in &outputs {
        for secret in secrets {
            assert!(
                !text.contains(secret),
                "a credential leaked into a run output"
            );
        }
        assert!(!text.contains("PRIVATE KEY"));
    }
}

#[tokio::test]
async fn mazbri_full_sm2_run_succeeds_and_reports_hide_every_credential() {
    let tmp = TempDir::new("mazbri");
    let gateway = x_loadgen_mockgw::MockGateway::start(
        "127.0.0.1:0",
        &x_loadgen_mockgw::MockConfig::default(),
    )
    .await
    .expect("mock gateway");
    let (base, shutdown) = start_api(&tmp.path).await;
    let http = client();
    let (env_id, req_id, private_pem) =
        mazbri_workspace_with_generated_sm2_key(&http, &base, &gateway).await;

    let requests = 6_u64;
    let (status, profile) = post_json(
        &http,
        &format!("{base}/api/v1/load-profiles"),
        &json!({
            "name": "tpl-mazbri",
            "model": "open_loop",
            "qps": 20,
            "concurrency": 2,
            "requests": requests,
            "timeout_secs": 5,
            "processes": 1,
            "loop_enabled": false
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
            "mode": "once"
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().expect("run id").to_string();
    let detail = wait_status(&http, &base, &id, &["finished", "stopped", "failed"]).await;
    assert_eq!(detail["status"], "finished", "{detail}");
    let (_, counters) = get_json(&http, &format!("{base}/api/v1/runs/{id}/stats")).await;
    assert_eq!(counters["samples"], requests, "{counters}");
    assert_eq!(counters["success"], requests, "{counters}");
    assert_eq!(counters["failure"], 0, "{counters}");
    let mock = gateway.stats();
    assert_eq!(mock.tokens_issued, 1);
    assert_eq!(mock.bindings_verified, requests);
    assert_eq!(mock.envelopes_opened, requests);
    assert_eq!(mock.responses_sealed, requests);

    let secrets = [MAZBRI_API_KEY, MAZBRI_TENANT_KEY, private_pem.as_str()];
    assert_run_outputs_hide(&http, &base, &id, &detail, &secrets).await;
    gateway.stop().await;
    let _ = shutdown.send(());
}
