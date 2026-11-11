//! The mock enforces the client contract: grant type, bearer, binding, replay, envelopes.

use serde_json::{Value, json};
use x_loadgen_core::crypto::request_binding::unix_now_secs;
use x_loadgen_mockgw::{MockConfig, MockGateway, MockTenant};
use x_loadgen_sdk::{GatewayRequest, TenantSettings, parse_token_response, token_request_body};

const TENANT_KEY: &str = "eC1sb2FkZ2VuLXRlc3QtdGVuYW50LWtleS0zMmJ5dGU=";
const HMAC_KEY: &str = "eC1sb2FkZ2VuLWJpbmRpbmcta2V5";
const TENANT_ID: &str = "9e8d7c6b-0000-4000-8000-00000000d004";

fn tenant() -> MockTenant {
    MockTenant {
        api_key: "ak-contract".into(),
        encryption_mode: "full".into(),
        tenant_id: Some(TENANT_ID.into()),
        tenant_key_base64: Some(TENANT_KEY.into()),
        key_version: Some("1".into()),
        suite: Some("suite/tenant-default-2026-11".into()),
        binding_kid: Some("kid-c".into()),
        binding_alg: Some("hmac-sha384".into()),
        binding_verify_key: Some(HMAC_KEY.into()),
        ..MockTenant::default()
    }
}

fn settings() -> TenantSettings<'static> {
    TenantSettings {
        encryption_mode: "full",
        tenant_id: Some(TENANT_ID),
        tenant_key_base64: Some(TENANT_KEY),
        key_version: Some("1"),
        suite: Some("suite/tenant-default-2026-11"),
        binding_kid: Some("kid-c"),
        binding_alg: Some("hmac-sha384"),
        binding_key: Some(HMAC_KEY),
        ..TenantSettings::default()
    }
}

async fn token(http: &reqwest::Client, base: &str) -> String {
    let response = http
        .post(format!("{base}/auth/token"))
        .json(&token_request_body("ak-contract"))
        .send()
        .await
        .expect("token");
    let status = response.status().as_u16();
    let raw = response.bytes().await.expect("bytes");
    parse_token_response(status, &raw)
        .expect("issued")
        .access_token
}

#[tokio::test]
async fn token_endpoint_checks_grant_type_and_api_key() {
    let gateway = MockGateway::start(
        "127.0.0.1:0",
        &MockConfig {
            tenants: vec![tenant()],
            ..MockConfig::default()
        },
    )
    .await
    .expect("start");
    let base = gateway.base_url();
    let http = reqwest::Client::new();
    let wrong_grant = http
        .post(format!("{base}/auth/token"))
        .json(&json!({"api_key": "ak-contract", "grant_type": "password"}))
        .send()
        .await
        .expect("send");
    assert_eq!(wrong_grant.status(), reqwest::StatusCode::BAD_REQUEST);
    let unknown = http
        .post(format!("{base}/auth/token"))
        .json(&token_request_body("nope"))
        .send()
        .await
        .expect("send");
    assert_eq!(unknown.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_ne!(token(&http, &base).await, "");
    let stats = gateway.stats();
    assert_eq!((stats.tokens_issued, stats.token_rejected), (1, 2));
    gateway.stop().await;
}

#[tokio::test]
async fn gateway_requires_bearer_binding_and_rejects_replays() {
    let gateway = MockGateway::start(
        "127.0.0.1:0",
        &MockConfig {
            tenants: vec![tenant()],
            ..MockConfig::default()
        },
    )
    .await
    .expect("start");
    let base = gateway.base_url();
    let http = reqwest::Client::new();
    let session = settings().session().expect("session");
    let request = GatewayRequest {
        product_id: "P1".into(),
        path: "/x".into(),
        method: "POST".into(),
        query: None,
        headers: None,
        body: Some(json!({"k": "v"})),
    };
    let plaintext = serde_json::to_vec(&request).expect("json");
    let encoded = session
        .encode_request(&plaintext, unix_now_secs())
        .expect("encode");

    let no_bearer = http
        .post(format!("{base}/api/v1/gateway"))
        .body(encoded.body.clone())
        .send()
        .await
        .expect("send");
    assert_eq!(no_bearer.status(), reqwest::StatusCode::UNAUTHORIZED);

    let bearer = token(&http, &base).await;
    let send = |with_binding: bool| {
        let mut builder = http
            .post(format!("{base}/api/v1/gateway"))
            .bearer_auth(&bearer)
            .body(encoded.body.clone());
        if with_binding {
            for (name, value) in &encoded.headers {
                builder = builder.header(*name, value);
            }
        }
        builder.send()
    };
    let unsigned = send(false).await.expect("send");
    assert_eq!(unsigned.status(), reqwest::StatusCode::UNAUTHORIZED);

    let accepted = send(true).await.expect("send");
    assert_eq!(accepted.status(), reqwest::StatusCode::OK);
    let raw = accepted.bytes().await.expect("bytes");
    let sealed: Value = serde_json::from_slice(&raw).expect("json");
    assert_eq!(
        sealed["dataAlg"], "aes-256-gcm",
        "full mode answers with an envelope"
    );
    let decoded = session.decode_response(200, &raw).expect("decode");
    assert!(decoded.success);
    assert_eq!(decoded.data.expect("data")["echo"]["body"]["k"], "v");

    let replay = send(true).await.expect("send");
    assert_eq!(replay.status(), reqwest::StatusCode::UNAUTHORIZED);
    let stats = gateway.stats();
    assert_eq!(stats.accepted, 1);
    assert_eq!(stats.rejected, 3);
    assert_eq!(stats.bindings_verified, 1);
    gateway.stop().await;
}

#[tokio::test]
async fn tenant_table_can_be_replaced_and_rejects_invalid_tenants() {
    let gateway = MockGateway::start("127.0.0.1:0", &MockConfig::default())
        .await
        .expect("start");
    let base = gateway.base_url();
    let http = reqwest::Client::new();
    let mut bad = tenant();
    bad.suite = Some("suite/unknown".into());
    let rejected = http
        .put(format!("{base}/__mock/tenants"))
        .json(&json!({"tenants": [bad]}))
        .send()
        .await
        .expect("send");
    assert_eq!(rejected.status(), reqwest::StatusCode::BAD_REQUEST);
    let replaced = http
        .put(format!("{base}/__mock/tenants"))
        .json(&json!({"tenants": [tenant()]}))
        .send()
        .await
        .expect("send");
    assert_eq!(replaced.status(), reqwest::StatusCode::OK);
    assert_ne!(token(&http, &base).await, "");
    let debug = format!("{:?}", tenant());
    assert!(!debug.contains(TENANT_KEY) && !debug.contains(HMAC_KEY));
    gateway.stop().await;
}
