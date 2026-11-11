//! HTTP tests for target and template CRUD.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use super::server::{prepare, spawn};
use crate::cli::ServeArgs;

const API_KEY_NONE: &str = "secret-api-key-alpha";
const API_KEY_FULL: &str = "secret-api-key-omega";
const TENANT_KEY: &str = "c2VjcmV0LXRlbmFudC1rZXktMTIzNA==";
const BINDING_KEY: &str = "secret-binding-key-xyz";
const CLIENT_KEY: &str = "-----BEGIN SECRET KEY-----";

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "x-loadgen-cat-{}-{}-{n}-{tag}",
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

fn serve_args(dir: &std::path::Path, no_auth: bool, max_qps: Option<u64>) -> ServeArgs {
    ServeArgs {
        listen: "127.0.0.1:0".to_string(),
        data_dir: dir.to_path_buf(),
        no_auth,
        max_qps,
        max_concurrency: None,
    }
}

async fn start(
    dir: &std::path::Path,
    no_auth: bool,
    max_qps: Option<u64>,
) -> (String, tokio::sync::oneshot::Sender<()>) {
    let prepared = prepare(serve_args(dir, no_auth, max_qps)).expect("prepare");
    let (addr, shutdown, _join) = spawn(prepared).await.expect("spawn");
    (format!("http://{addr}"), shutdown)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client")
}

async fn body(response: reqwest::Response) -> Value {
    response.json::<Value>().await.expect("json")
}

fn target_payload() -> Value {
    json!({
        "name": "lab-gateway",
        "gateway_base_url": "https://10.0.0.8:8443",
        "client_key": CLIENT_KEY,
        "insecure_skip_verify": true,
        "note": "box lab",
        "tenants": [
            {
                "name": "plain",
                "weight": 1,
                "api_key": API_KEY_NONE,
                "encryption_mode": "none"
            },
            {
                "name": "encrypted",
                "weight": 2,
                "api_key": API_KEY_FULL,
                "encryption_mode": "full",
                "tenant_id": "t-1001",
                "tenant_key_base64": TENANT_KEY,
                "key_version": "1",
                "binding_kid": "kid-1",
                "binding_alg": "hmac-sha256",
                "binding_key": BINDING_KEY
            }
        ],
        "products": [
            {
                "name": "pay",
                "product_id": "P001",
                "path": "/v1/pay",
                "method": "POST",
                "weight": 3,
                "body_json": {"amount": "{{seq}}"}
            },
            {
                "name": "query",
                "product_id": "P002",
                "path": "/v1/query",
                "method": "GET",
                "weight": 1
            }
        ]
    })
}

fn once_template(target_id: &str, product_ids: &[Value]) -> Value {
    json!({
        "name": "once-open",
        "target_id": target_id,
        "model": "open_loop",
        "qps": 8,
        "concurrency": 4,
        "requests": 100,
        "warmup_secs": 2,
        "ramp_secs": 3,
        "timeout_secs": 10,
        "connect_timeout_secs": 5,
        "token_refresh_skew_secs": 60,
        "include_request_id": true,
        "processes": 2,
        "qps_mode": "total",
        "requests_mode": "per_process",
        "loop_enabled": false,
        "product_ids": product_ids,
        "detail_retain_days": 7,
        "detail_retain_rows": 10_000,
        "expected_reasons": ["ok"],
        "note": "single window"
    })
}

fn loop_template(target_id: &str) -> Value {
    json!({
        "name": "loop-closed",
        "target_id": target_id,
        "model": "closed_loop",
        "concurrency": 6,
        "duration_secs": 30,
        "timeout_secs": 8,
        "loop_enabled": true,
        "loop_interval_secs": 15,
        "processes": 1,
        "note": "infinite loop"
    })
}

async fn create_target(client: &reqwest::Client, base: &str) -> Value {
    let response = client
        .post(format!("{base}/api/v1/targets"))
        .json(&target_payload())
        .send()
        .await
        .expect("create target");
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    body(response).await
}

#[tokio::test]
async fn create_target_shows_secrets_in_detail_but_not_in_lists() {
    let tmp = TempDir::new("target");
    let (base, shutdown) = start(&tmp.path, true, None).await;
    let client = client();
    let detail = create_target(&client, &base).await;
    assert_eq!(detail["name"], "lab-gateway");
    assert_eq!(detail["client_key"], CLIENT_KEY);
    let tenants = detail["tenants"].as_array().expect("tenants");
    assert_eq!(tenants.len(), 2);
    let encrypted = tenants
        .iter()
        .find(|tenant| tenant["name"] == "encrypted")
        .expect("encrypted tenant");
    assert_eq!(encrypted["api_key"], API_KEY_FULL);
    assert_eq!(encrypted["tenant_key_base64"], TENANT_KEY);
    assert_eq!(encrypted["binding_key"], BINDING_KEY);
    assert_eq!(detail["products"].as_array().expect("products").len(), 2);

    let list = body(
        client
            .get(format!("{base}/api/v1/targets"))
            .send()
            .await
            .expect("list"),
    )
    .await;
    let list_text = list.to_string();
    for secret in [
        API_KEY_NONE,
        API_KEY_FULL,
        TENANT_KEY,
        BINDING_KEY,
        CLIENT_KEY,
    ] {
        assert!(!list_text.contains(secret), "list leaked a secret field");
    }
    assert!(list[0].get("client_key").is_none());

    let target_id = detail["id"].as_str().expect("id");
    let tenant_list = body(
        client
            .get(format!("{base}/api/v1/targets/{target_id}/tenants"))
            .send()
            .await
            .expect("tenant list"),
    )
    .await;
    let tenant_text = tenant_list.to_string();
    for secret in [API_KEY_NONE, API_KEY_FULL, TENANT_KEY, BINDING_KEY] {
        assert!(!tenant_text.contains(secret), "tenant list leaked a secret");
    }
    let tid = encrypted["id"].as_str().expect("tid");
    let one = body(
        client
            .get(format!("{base}/api/v1/targets/{target_id}/tenants/{tid}"))
            .send()
            .await
            .expect("tenant detail"),
    )
    .await;
    assert_eq!(one["api_key"], API_KEY_FULL);
    shutdown.send(()).ok();
}

#[tokio::test]
async fn templates_cover_once_and_loop_and_reject_illegal_combinations() {
    let tmp = TempDir::new("tpl");
    let (base, shutdown) = start(&tmp.path, true, None).await;
    let client = client();
    let target = create_target(&client, &base).await;
    let target_id = target["id"].as_str().expect("tid").to_string();
    let product_ids: Vec<Value> = target["products"]
        .as_array()
        .expect("products")
        .iter()
        .map(|p| json!(p["id"]))
        .collect();
    let product_pk = product_ids[0].as_str().expect("pid").to_string();

    let once = client
        .post(format!("{base}/api/v1/templates"))
        .json(&once_template(&target_id, &product_ids))
        .send()
        .await
        .expect("once");
    assert_eq!(once.status(), reqwest::StatusCode::CREATED);
    let once_body = body(once).await;
    assert_eq!(once_body["loop_enabled"], false);
    assert_eq!(once_body["requests"], 100);
    assert_eq!(once_body["processes"], 2);
    assert_eq!(once_body["detail_retain_days"], 7);

    let mut loop_payload = loop_template(&target_id);
    let mut overrides = serde_json::Map::new();
    overrides.insert(product_pk.clone(), json!(5));
    loop_payload["weight_overrides"] = Value::Object(overrides);
    let looped = client
        .post(format!("{base}/api/v1/templates"))
        .json(&loop_payload)
        .send()
        .await
        .expect("loop");
    assert_eq!(looped.status(), reqwest::StatusCode::CREATED);
    let loop_body = body(looped).await;
    assert_eq!(loop_body["loop_enabled"], true);
    assert!(loop_body["rounds"].is_null());
    assert_eq!(loop_body["loop_interval_secs"], 15);
    assert_eq!(loop_body["duration_secs"], 30);
    assert_eq!(loop_body["weight_overrides"][&product_pk], 5);

    let bad_both = client
        .post(format!("{base}/api/v1/templates"))
        .json(&json!({
            "name": "bad-both",
            "target_id": target_id,
            "model": "open_loop",
            "concurrency": 1,
            "requests": 10,
            "duration_secs": 10,
            "timeout_secs": 5
        }))
        .send()
        .await
        .expect("both");
    assert_eq!(bad_both.status(), reqwest::StatusCode::BAD_REQUEST);

    let bad_url = client
        .post(format!("{base}/api/v1/targets"))
        .json(&json!({"name": "bad-url", "gateway_base_url": "ftp://example"}))
        .send()
        .await
        .expect("url");
    assert_eq!(bad_url.status(), reqwest::StatusCode::BAD_REQUEST);

    let bad_enc = client
        .post(format!("{base}/api/v1/targets/{target_id}/tenants"))
        .json(&json!({
            "name": "incomplete",
            "api_key": "k",
            "encryption_mode": "full"
        }))
        .send()
        .await
        .expect("enc");
    assert_eq!(bad_enc.status(), reqwest::StatusCode::BAD_REQUEST);

    let conflict = client
        .delete(format!("{base}/api/v1/targets/{target_id}"))
        .send()
        .await
        .expect("delete target");
    assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);
    shutdown.send(()).ok();
}

#[tokio::test]
async fn template_qps_above_global_cap_is_rejected() {
    let tmp = TempDir::new("cap");
    let (base, shutdown) = start(&tmp.path, true, Some(10)).await;
    let client = client();
    let target = create_target(&client, &base).await;
    let target_id = target["id"].as_str().expect("id");
    let response = client
        .post(format!("{base}/api/v1/templates"))
        .json(&json!({
            "name": "too-fast",
            "target_id": target_id,
            "model": "open_loop",
            "qps": 50,
            "concurrency": 2,
            "requests": 20,
            "timeout_secs": 5
        }))
        .send()
        .await
        .expect("cap");
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let err = body(response).await;
    assert_eq!(err["error"]["code"], "invalid_request");
    shutdown.send(()).ok();
}

#[tokio::test]
async fn unauthenticated_writes_and_detail_are_rejected() {
    let tmp = TempDir::new("authz");
    let (base, shutdown) = start(&tmp.path, false, None).await;
    let client = client();
    let write = client
        .post(format!("{base}/api/v1/targets"))
        .json(&target_payload())
        .send()
        .await
        .expect("write");
    assert_eq!(write.status(), reqwest::StatusCode::UNAUTHORIZED);
    let detail = client
        .get(format!("{base}/api/v1/targets/does-not-exist"))
        .send()
        .await
        .expect("detail");
    assert_eq!(detail.status(), reqwest::StatusCode::UNAUTHORIZED);
    shutdown.send(()).ok();
}

#[test]
fn catalog_code_does_not_read_legacy_secret_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    visit_rs(&root, &mut files);
    for path in files {
        let text = fs::read_to_string(&path).expect("read");
        assert!(
            !text.contains("test-tenants.json"),
            "{} still mentions test-tenants.json",
            path.display()
        );
        assert!(
            !text.contains(".secrets.toml"),
            "{} still mentions *.secrets.toml",
            path.display()
        );
    }
}

fn visit_rs(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            visit_rs(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.contains("tests") {
                out.push(path);
            }
        }
    }
}
