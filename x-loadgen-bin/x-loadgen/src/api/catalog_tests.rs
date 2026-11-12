//! HTTP tests for Hoppscotch workspace catalog CRUD: Collections, Requests,
//! Environments, Mazbri Tenant Pools, Load Profiles, and JSON Import/Export.
#![allow(clippy::too_many_lines)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use super::server::{prepare, spawn};
use crate::cli::ServeArgs;

const API_KEY_NONE: &str = "secret-api-key-alpha";
const API_KEY_FULL: &str = "secret-api-key-omega";
/// Base64 of the 32-byte key `x-loadgen-test-tenant-key-32byte`.
const TENANT_KEY: &str = "eC1sb2FkZ2VuLXRlc3QtdGVuYW50LWtleS0zMmJ5dGU=";
/// Base64 of `x-loadgen-binding-key` (HMAC request binding key).
const BINDING_KEY: &str = "eC1sb2FkZ2VuLWJpbmRpbmcta2V5";
const CLIENT_CERT: &str = "-----BEGIN CERTIFICATE-----\nTESTCERT\n-----END CERTIFICATE-----";
const CLIENT_KEY: &str = "-----BEGIN PRIVATE KEY-----\nSECRETCLIENTKEY\n-----END PRIVATE KEY-----";
const BEARER_SECRET: &str = "secret-bearer-token-xyz";
const ENV_SECRET_VAL: &str = "secret-env-var-value-999";

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

fn env_payload() -> Value {
    json!({
        "name": "lab-gateway",
        "gateway_base_url": "https://10.0.0.8:8443",
        "variables": [
            {"key": "baseUrl", "value": "https://10.0.0.8:8443", "secret": false, "active": true},
            {"key": "apiSecret", "value": ENV_SECRET_VAL, "secret": true, "active": true}
        ],
        "http_auth": {
            "auth_type": "bearer",
            "auth_active": true,
            "token": BEARER_SECRET
        },
        "tls": {
            "client_cert": CLIENT_CERT,
            "client_key": CLIENT_KEY,
            "insecure_skip_verify": true
        },
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
                "suite": "suite/tenant-default-2026-11",
                "binding_kid": "kid-1",
                "binding_alg": "hmac-sha256",
                "binding_key": BINDING_KEY
            }
        ]
    })
}

async fn create_env(client: &reqwest::Client, base: &str) -> Value {
    let response = client
        .post(format!("{base}/api/v1/environments"))
        .json(&env_payload())
        .send()
        .await
        .expect("create environment");
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    body(response).await
}

async fn post_status(client: &reqwest::Client, url: &str, payload: &Value) -> reqwest::StatusCode {
    client
        .post(url)
        .json(payload)
        .send()
        .await
        .expect("post")
        .status()
}

#[tokio::test]
async fn environments_preserve_global_and_redact_secrets_on_list_and_export() {
    let tmp = TempDir::new("env");
    let (base, shutdown) = start(&tmp.path, true, None).await;
    let client = client();

    // Built-in Global environment always exists and cannot be deleted.
    let del_global = client
        .delete(format!("{base}/api/v1/environments/global"))
        .send()
        .await
        .expect("delete global");
    assert_eq!(del_global.status(), reqwest::StatusCode::BAD_REQUEST);

    let detail = create_env(&client, &base).await;
    let env_id = detail["id"].as_str().expect("env id");
    assert_eq!(detail["name"], "lab-gateway");
    assert_eq!(detail["tls"]["client_key"], CLIENT_KEY);
    assert_eq!(detail["http_auth"]["token"], BEARER_SECRET);
    let tenants = detail["tenants"].as_array().expect("tenants");
    assert_eq!(tenants.len(), 2);
    let encrypted = tenants
        .iter()
        .find(|t| t["name"] == "encrypted")
        .expect("encrypted tenant");
    assert_eq!(encrypted["api_key"], API_KEY_FULL);
    assert_eq!(encrypted["tenant_key_base64"], TENANT_KEY);
    assert_eq!(encrypted["binding_key"], BINDING_KEY);

    // Summary list must never expose secret fields.
    let list = body(
        client
            .get(format!("{base}/api/v1/environments"))
            .send()
            .await
            .expect("list envs"),
    )
    .await;
    let list_arr = list.as_array().expect("env array");
    assert!(
        list_arr
            .iter()
            .any(|e| e["id"] == "global" && e["is_global"] == true)
    );
    let list_text = list.to_string();
    for secret in [
        API_KEY_NONE,
        API_KEY_FULL,
        TENANT_KEY,
        BINDING_KEY,
        CLIENT_KEY,
        BEARER_SECRET,
        ENV_SECRET_VAL,
    ] {
        assert!(!list_text.contains(secret), "summary list leaked {secret}");
    }

    // Export defaults to include_secrets=false and scrubs every secret.
    let redacted_export = body(
        client
            .get(format!(
                "{base}/api/v1/environments/export?environment_id={env_id}"
            ))
            .send()
            .await
            .expect("redacted export"),
    )
    .await;
    let redacted_text = redacted_export.to_string();
    for secret in [
        API_KEY_NONE,
        API_KEY_FULL,
        TENANT_KEY,
        BINDING_KEY,
        CLIENT_KEY,
        BEARER_SECRET,
        ENV_SECRET_VAL,
    ] {
        assert!(
            !redacted_text.contains(secret),
            "default export leaked {secret}"
        );
    }

    // Opt-in include_secrets=true preserves full credentials for secure backup.
    let full_export = body(
        client
            .get(format!(
                "{base}/api/v1/environments/export?environment_id={env_id}&include_secrets=true"
            ))
            .send()
            .await
            .expect("full export"),
    )
    .await;
    let full_text = full_export.to_string();
    assert!(full_text.contains(API_KEY_FULL));
    assert!(full_text.contains(ENV_SECRET_VAL));

    // Hoppscotch Environment JSON import works directly.
    let imported = client
        .post(format!("{base}/api/v1/environments/import"))
        .json(&json!({
            "name": "Hoppscotch Staging",
            "variables": [
                {"key": "baseUrl", "currentValue": "https://staging.example.com", "secret": false},
                {"key": "token", "currentValue": "hopp-secret-123", "secret": true}
            ]
        }))
        .send()
        .await
        .expect("import hoppscotch env");
    assert_eq!(imported.status(), reqwest::StatusCode::CREATED);
    let imported_body = body(imported).await;
    assert_eq!(imported_body[0]["name"], "Hoppscotch Staging");

    // Reject invalid URLs and mismatched mTLS cert/key pairs.
    assert_eq!(
        post_status(
            &client,
            &format!("{base}/api/v1/environments"),
            &json!({"name": "bad-url", "gateway_base_url": "ftp://example"}),
        )
        .await,
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post_status(
            &client,
            &format!("{base}/api/v1/environments"),
            &json!({
                "name": "half-mtls",
                "tls": {"client_key": CLIENT_KEY}
            }),
        )
        .await,
        reqwest::StatusCode::BAD_REQUEST
    );

    shutdown.send(()).ok();
}

#[tokio::test]
async fn collections_folders_requests_and_hoppscotch_import_export_roundtrip() {
    let tmp = TempDir::new("col");
    let (base, shutdown) = start(&tmp.path, true, None).await;
    let client = client();

    // Root collection with auth + headers + variables.
    let root_resp = client
        .post(format!("{base}/api/v1/collections"))
        .json(&json!({
            "name": "Payment Gateway",
            "default_protocol": "mazbri",
            "auth": {"auth_type": "bearer", "auth_active": true, "token": BEARER_SECRET},
            "headers": [{"key": "X-Trace-Source", "value": "hoppscotch-ui", "active": true}],
            "variables": [{"key": "servicePrefix", "value": "/v1", "secret": false, "active": true}]
        }))
        .send()
        .await
        .expect("create root collection");
    assert_eq!(root_resp.status(), reqwest::StatusCode::CREATED);
    let root = body(root_resp).await;
    let root_id = root["id"].as_str().expect("root id").to_string();

    // Child folder.
    let child_resp = client
        .post(format!("{base}/api/v1/collections"))
        .json(&json!({
            "parent_id": root_id,
            "name": "Orders"
        }))
        .send()
        .await
        .expect("create child folder");
    assert_eq!(child_resp.status(), reqwest::StatusCode::CREATED);
    let child = body(child_resp).await;
    let child_id = child["id"].as_str().expect("child id").to_string();

    // Cycle check: moving root under child must fail with 400.
    let cycle = client
        .put(format!("{base}/api/v1/collections/{root_id}"))
        .json(&json!({
            "parent_id": child_id,
            "name": "Payment Gateway"
        }))
        .send()
        .await
        .expect("cycle update");
    assert_eq!(cycle.status(), reqwest::StatusCode::BAD_REQUEST);

    // Create both an HTTP request and a Mazbri request inside the child folder.
    let http_req = client
        .post(format!("{base}/api/v1/requests"))
        .json(&json!({
            "collection_id": child_id,
            "name": "Health Check",
            "protocol": "http",
            "method": "GET",
            "endpoint": "<<baseUrl>><<servicePrefix>>/health",
            "params": [{"key": "probe", "value": "1", "active": true}],
            "weight": 2
        }))
        .send()
        .await
        .expect("create http request");
    assert_eq!(http_req.status(), reqwest::StatusCode::CREATED);
    let http_saved = body(http_req).await;
    let http_id = http_saved["id"].as_str().expect("http id").to_string();

    let mazbri_req = client
        .post(format!("{base}/api/v1/requests"))
        .json(&json!({
            "collection_id": child_id,
            "name": "Create Order",
            "protocol": "mazbri",
            "method": "POST",
            "endpoint": "/v1/orders",
            "body": {
                "content_type": "application/json",
                "body": "{\"order_no\":\"{{seq}}\"}"
            },
            "mazbri": {
                "product_id": "P-ORDER-CREATE",
                "tenant_source": "environment"
            },
            "assertions": {
                "expect_transport": "ok",
                "checks": [{"path": "code", "eq": 0}]
            },
            "weight": 5
        }))
        .send()
        .await
        .expect("create mazbri request");
    assert_eq!(mazbri_req.status(), reqwest::StatusCode::CREATED);

    // Reject unsupported protocol.
    assert_eq!(
        post_status(
            &client,
            &format!("{base}/api/v1/requests"),
            &json!({
                "collection_id": child_id,
                "name": "Bad",
                "protocol": "grpc",
                "method": "POST",
                "endpoint": "/x"
            }),
        )
        .await,
        reqwest::StatusCode::BAD_REQUEST
    );

    // Tree listing contains nested folder & 2 requests.
    let tree = body(
        client
            .get(format!("{base}/api/v1/collections"))
            .send()
            .await
            .expect("tree"),
    )
    .await;
    assert_eq!(
        tree[0]["folders"][0]["requests"].as_array().map(Vec::len),
        Some(2)
    );

    // Export redacts bearer token by default.
    let exported = body(
        client
            .get(format!(
                "{base}/api/v1/collections/export?collection_id={root_id}"
            ))
            .send()
            .await
            .expect("export col"),
    )
    .await;
    assert!(!exported.to_string().contains(BEARER_SECRET));

    // Import standard Hoppscotch v6 REST Collection JSON.
    let hopp_import = client
        .post(format!("{base}/api/v1/collections/import"))
        .json(&json!({
            "v": 6,
            "name": "Hoppscotch Petstore",
            "folders": [
                {
                    "v": 6,
                    "name": "Pets",
                    "folders": [],
                    "requests": [
                        {
                            "v": "11",
                            "name": "List Pets",
                            "method": "GET",
                            "endpoint": "<<baseUrl>>/pets",
                            "params": [{"key": "limit", "value": "20", "active": true}],
                            "headers": [],
                            "auth": {"authType": "inherit", "authActive": true},
                            "body": {"contentType": null, "body": null}
                        }
                    ]
                }
            ],
            "requests": []
        }))
        .send()
        .await
        .expect("import hoppscotch collection");
    assert_eq!(hopp_import.status(), reqwest::StatusCode::CREATED);
    let imported_nodes = body(hopp_import).await;
    assert_eq!(imported_nodes[0]["name"], "Hoppscotch Petstore");
    assert_eq!(
        imported_nodes[0]["folders"][0]["requests"][0]["name"],
        "List Pets"
    );

    // Cascade delete removes nested folders and saved requests.
    let del = client
        .delete(format!("{base}/api/v1/collections/{root_id}"))
        .send()
        .await
        .expect("delete root");
    assert_eq!(del.status(), reqwest::StatusCode::NO_CONTENT);
    let gone = client
        .get(format!("{base}/api/v1/requests/{http_id}"))
        .send()
        .await
        .expect("get deleted request");
    assert_eq!(gone.status(), reqwest::StatusCode::NOT_FOUND);

    shutdown.send(()).ok();
}

#[tokio::test]
async fn load_profiles_cover_once_and_loop_and_enforce_caps() {
    let tmp = TempDir::new("profiles");
    let (base, shutdown) = start(&tmp.path, true, Some(25)).await;
    let client = client();

    let once = client
        .post(format!("{base}/api/v1/load-profiles"))
        .json(&json!({
            "name": "once-open",
            "model": "open_loop",
            "qps": 12,
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
            "detail_retain_days": 7,
            "detail_retain_rows": 10_000,
            "expected_reasons": ["ok"],
            "note": "single window"
        }))
        .send()
        .await
        .expect("create once profile");
    assert_eq!(once.status(), reqwest::StatusCode::CREATED);
    let once_body = body(once).await;
    assert_eq!(once_body["loop_enabled"], false);
    assert_eq!(once_body["requests"], 100);
    assert_eq!(once_body["processes"], 2);

    let looped = client
        .post(format!("{base}/api/v1/load-profiles"))
        .json(&json!({
            "name": "loop-closed",
            "model": "closed_loop",
            "concurrency": 6,
            "duration_secs": 30,
            "timeout_secs": 8,
            "loop_enabled": true,
            "loop_interval_secs": 15,
            "processes": 1,
            "assert": {"p99_ms": 500.0, "max_error_rate": 0.05}
        }))
        .send()
        .await
        .expect("create loop profile");
    assert_eq!(looped.status(), reqwest::StatusCode::CREATED);
    let loop_body = body(looped).await;
    assert_eq!(loop_body["loop_enabled"], true);
    assert!(loop_body["rounds"].is_null());
    assert_eq!(loop_body["loop_interval_secs"], 15);
    assert_eq!(loop_body["duration_secs"], 30);

    // Reject mutually exclusive requests + duration_secs.
    assert_eq!(
        post_status(
            &client,
            &format!("{base}/api/v1/load-profiles"),
            &json!({
                "name": "bad-both",
                "model": "open_loop",
                "qps": 5,
                "concurrency": 1,
                "requests": 10,
                "duration_secs": 10,
                "timeout_secs": 5
            }),
        )
        .await,
        reqwest::StatusCode::BAD_REQUEST
    );

    // Reject QPS above global cap (25).
    let over_cap = client
        .post(format!("{base}/api/v1/load-profiles"))
        .json(&json!({
            "name": "too-fast",
            "model": "open_loop",
            "qps": 50,
            "concurrency": 2,
            "requests": 20,
            "timeout_secs": 5
        }))
        .send()
        .await
        .expect("over cap");
    assert_eq!(over_cap.status(), reqwest::StatusCode::BAD_REQUEST);
    let err = body(over_cap).await;
    assert_eq!(err["error"]["code"], "invalid_request");

    shutdown.send(()).ok();
}

#[tokio::test]
async fn unauthenticated_writes_and_detail_are_rejected() {
    let tmp = TempDir::new("authz");
    let (base, shutdown) = start(&tmp.path, false, None).await;
    let client = client();
    let write = client
        .post(format!("{base}/api/v1/environments"))
        .json(&env_payload())
        .send()
        .await
        .expect("write");
    assert_eq!(write.status(), reqwest::StatusCode::UNAUTHORIZED);
    let detail = client
        .get(format!("{base}/api/v1/collections"))
        .send()
        .await
        .expect("collections");
    assert_eq!(detail.status(), reqwest::StatusCode::UNAUTHORIZED);
    shutdown.send(()).ok();
}

#[tokio::test]
async fn mazbri_tenants_are_validated_against_the_client_contract() {
    let tmp = TempDir::new("validate");
    let (base, shutdown) = start(&tmp.path, true, None).await;
    let client = client();
    let env = create_env(&client, &base).await;
    let env_id = env["id"].as_str().expect("id");
    let url = format!("{base}/api/v1/environments/{env_id}/tenants");
    let tenant = |extra: Value| {
        let mut value = json!({
            "name": "t",
            "api_key": "k",
            "encryption_mode": "request",
            "tenant_id": "t-2002",
            "tenant_key_base64": TENANT_KEY,
            "key_version": "2"
        });
        for (key, field) in extra.as_object().expect("object") {
            value[key] = field.clone();
        }
        value
    };
    let rejected = [
        json!({}),
        json!({"suite": "suite/unknown"}),
        json!({"data_alg": "aes-128-ecb"}),
        json!({"suite": "suite/tenant-default-2026-11", "tenant_key_base64": "c2hvcnQ="}),
        json!({"suite": "suite/tenant-default-2026-11", "api_key": ""}),
        json!({"suite": "suite/tenant-chacha20", "binding_kid": "k1", "binding_alg": "hmac-sha1", "binding_key": BINDING_KEY}),
        json!({"suite": "suite/tenant-chacha20", "binding_kid": "k1", "binding_alg": "hmac-sha256", "binding_key": "not base64!"}),
        json!({"suite": "suite/tenant-chacha20", "binding_kid": "k1", "binding_alg": "ed25519", "binding_key": BINDING_KEY}),
        json!({"suite": "suite/tenant-chacha20", "binding_kid": "k1", "binding_alg": "ed25519"}),
        json!({"encryption_mode": "partial", "suite": "suite/tenant-chacha20"}),
    ];
    for (index, extra) in rejected.into_iter().enumerate() {
        let mut payload = tenant(extra);
        payload["name"] = json!(format!("bad-{index}"));
        assert_eq!(
            post_status(&client, &url, &payload).await,
            reqwest::StatusCode::BAD_REQUEST,
            "case {index} should be rejected"
        );
    }
    let accepted = [
        json!({"suite": "suite/gm-sm4-hmac-sm3"}),
        json!({"encryption_mode": "full", "suite": "suite/tenant-chacha20"}),
        json!({"data_alg": "aes-128-ecb", "mac_alg": "hmac-md5", "allow_insecure": true}),
        json!({"data_alg": "sm4-cbc-pkcs7", "mac_alg": "hmac-sha256", "binding_kid": "k1", "binding_alg": "hmac-sm3", "binding_key": BINDING_KEY}),
        json!({"encryption_mode": "none", "tenant_key_base64": null, "binding_kid": "k1", "binding_alg": "hmac-sha512", "binding_key": BINDING_KEY}),
    ];
    for (index, extra) in accepted.into_iter().enumerate() {
        let mut payload = tenant(extra);
        payload["name"] = json!(format!("good-{index}"));
        assert_eq!(
            post_status(&client, &url, &payload).await,
            reqwest::StatusCode::CREATED,
            "case {index} should be accepted"
        );
    }

    shutdown.send(()).ok();
}

#[tokio::test]
async fn binding_keypair_endpoints_support_environment_and_stateless_modes() {
    let tmp = TempDir::new("keypair");
    let (base, shutdown) = start(&tmp.path, true, None).await;
    let client = client();
    let env = create_env(&client, &base).await;
    let env_id = env["id"].as_str().expect("id");
    let tenant = env["tenants"]
        .as_array()
        .expect("tenants")
        .iter()
        .find(|t| t["name"] == "encrypted")
        .expect("encrypted");
    let tid = tenant["id"].as_str().expect("tid");
    let url = format!("{base}/api/v1/environments/{env_id}/tenants/{tid}/binding-keypair");

    assert_eq!(
        post_status(&client, &url, &json!({"alg": "hmac-sha256"})).await,
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post_status(&client, &url, &json!({"alg": "rsa-oaep-sha256"})).await,
        reqwest::StatusCode::BAD_REQUEST
    );
    for alg in ["ed25519", "ecdsa-p256-sha256", "sm2-sign"] {
        let response = client
            .post(&url)
            .json(&json!({"alg": alg, "kid": format!("kid-{alg}")}))
            .send()
            .await
            .expect("keypair");
        assert_eq!(response.status(), reqwest::StatusCode::OK, "{alg}");
        let text = response.text().await.expect("text");
        assert!(
            !text.contains("PRIVATE KEY"),
            "tenant keypair response must not leak private key"
        );
        let generated: Value = serde_json::from_str(&text).expect("json");
        assert_eq!(generated["binding_alg"], alg);
        assert_eq!(generated["binding_kid"], format!("kid-{alg}"));
        let public = generated["public_key_pem"].as_str().expect("public");
        assert!(public.starts_with("-----BEGIN PUBLIC KEY-----"), "{alg}");

        let stored = body(
            client
                .get(format!("{base}/api/v1/environments/{env_id}/tenants/{tid}"))
                .send()
                .await
                .expect("tenant"),
        )
        .await;
        assert_eq!(stored["binding_alg"], alg);
        assert_eq!(stored["binding_public_key"], public);
        assert!(
            stored["binding_key"]
                .as_str()
                .is_some_and(|pem| pem.starts_with("-----BEGIN PRIVATE KEY-----"))
        );
    }

    // Re-saving the environment preserves tenant IDs and generated public keys.
    assert_eq!(
        post_status(&client, &url, &json!({"alg": "rsa-pkcs1v15-sign-sha256"}),).await,
        reqwest::StatusCode::OK
    );
    let current_detail = body(
        client
            .get(format!("{base}/api/v1/environments/{env_id}"))
            .send()
            .await
            .expect("detail"),
    )
    .await;
    let saved = client
        .put(format!("{base}/api/v1/environments/{env_id}"))
        .json(&current_detail)
        .send()
        .await
        .expect("resave");
    assert_eq!(saved.status(), reqwest::StatusCode::OK);
    let resaved = body(saved).await;
    let kept = resaved["tenants"]
        .as_array()
        .expect("tenants")
        .iter()
        .find(|t| t["name"] == "encrypted")
        .expect("encrypted");
    assert_eq!(kept["id"], tid);
    assert!(kept["binding_public_key"].is_string());

    // Switching back to symmetric HMAC clears binding_public_key.
    let mut changed = kept.clone();
    changed["binding_alg"] = json!("hmac-sha256");
    changed["binding_key"] = json!(BINDING_KEY);
    let updated = client
        .put(format!("{base}/api/v1/environments/{env_id}/tenants/{tid}"))
        .json(&changed)
        .send()
        .await
        .expect("switch to hmac");
    assert_eq!(updated.status(), reqwest::StatusCode::OK);
    assert!(body(updated).await["binding_public_key"].is_null());

    // Stateless keypair helper returns both PEMs for inline ad-hoc Mazbri requests.
    let inline_kp = client
        .post(format!("{base}/api/v1/crypto/binding-keypair"))
        .json(&json!({"alg": "ed25519", "kid": "inline-kid"}))
        .send()
        .await
        .expect("stateless keypair");
    assert_eq!(inline_kp.status(), reqwest::StatusCode::OK);
    let kp_json = body(inline_kp).await;
    assert!(
        kp_json["private_key_pem"]
            .as_str()
            .is_some_and(|s| s.starts_with("-----BEGIN PRIVATE KEY-----"))
    );
    assert!(
        kp_json["public_key_pem"]
            .as_str()
            .is_some_and(|s| s.starts_with("-----BEGIN PUBLIC KEY-----"))
    );

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
