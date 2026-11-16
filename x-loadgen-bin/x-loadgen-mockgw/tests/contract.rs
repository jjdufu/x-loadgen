//! The mock enforces the client contract: grant type, bearer, binding, replay, envelopes,
//! and request identity (`X-Request-Id`, W3C `traceparent`).

use serde_json::{Value, json};
use x_loadgen_core::crypto::request_binding::unix_now_secs;
use x_loadgen_mockgw::{MockConfig, MockGateway, MockTenant};
use x_loadgen_sdk::{
    GatewayRequest, REQUEST_ID_HEADER, TRACEPARENT_HEADER, TenantSettings, TraceParent,
    is_valid_request_id, new_request_id, parse_token_response, token_request_body,
};

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

#[tokio::test]
async fn gateway_keeps_request_id_and_continues_traceparent() {
    let gateway = MockGateway::start(
        "127.0.0.1:0",
        &MockConfig {
            tenants: vec![MockTenant {
                api_key: "ak-contract".into(),
                encryption_mode: "none".into(),
                ..MockTenant::default()
            }],
            ..MockConfig::default()
        },
    )
    .await
    .expect("start");
    let base = gateway.base_url();
    let http = reqwest::Client::new();
    let bearer = token(&http, &base).await;
    let body = serde_json::to_vec(&GatewayRequest {
        product_id: "P1".into(),
        path: "/x".into(),
        method: "GET".into(),
        query: None,
        headers: None,
        body: None,
    })
    .expect("json");
    let call = |rid: Option<&str>, tp: Option<&str>| {
        let mut builder = http
            .post(format!("{base}/api/v1/gateway"))
            .bearer_auth(&bearer)
            .body(body.clone());
        if let Some(rid) = rid {
            builder = builder.header(REQUEST_ID_HEADER, rid);
        }
        if let Some(tp) = tp {
            builder = builder.header(TRACEPARENT_HEADER, tp);
        }
        builder.send()
    };
    let header = |resp: &reqwest::Response, name: &str| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };

    let rid = new_request_id();
    let inbound = TraceParent::generate(false);
    let resp = call(Some(&rid), Some(&inbound.to_string()))
        .await
        .expect("send");
    assert_eq!(
        header(&resp, REQUEST_ID_HEADER).as_deref(),
        Some(rid.as_str())
    );
    let echoed = TraceParent::parse(&header(&resp, TRACEPARENT_HEADER).expect("traceparent"))
        .expect("valid traceparent");
    assert!(echoed.same_trace(&inbound));
    assert_ne!(echoed.parent_id(), inbound.parent_id());
    assert!(!echoed.sampled(), "flags follow the caller");
    let envelope: Value = resp.json().await.expect("json");
    assert_eq!(envelope["request_id"], rid.as_str());

    let replaced = call(Some("bad id with spaces"), Some("garbage"))
        .await
        .expect("send");
    let new_id = header(&replaced, REQUEST_ID_HEADER).expect("request id");
    assert_ne!(new_id, "bad id with spaces");
    assert!(is_valid_request_id(&new_id));
    let fresh = TraceParent::parse(&header(&replaced, TRACEPARENT_HEADER).expect("traceparent"))
        .expect("valid traceparent");
    assert!(fresh.sampled());

    let rejected = http
        .post(format!("{base}/api/v1/gateway"))
        .header(REQUEST_ID_HEADER, rid.as_str())
        .body(body.clone())
        .send()
        .await
        .expect("send");
    assert_eq!(rejected.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(
        header(&rejected, REQUEST_ID_HEADER).as_deref(),
        Some(rid.as_str())
    );
    assert!(header(&rejected, TRACEPARENT_HEADER).is_some());
    gateway.stop().await;
}
