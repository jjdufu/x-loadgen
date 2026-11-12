//! Mazbri executor against the local mock gateway (token, binding, envelopes, failures).

use serde_json::{Value, json};
use x_loadgen_mockgw::{Fault, MockConfig, MockGateway, MockStats, MockTenant};
use x_loadgen_sdk::generate_binding_keypair;

use super::execute::execute_one;
use super::metrics::OutcomeClass;
use super::snapshot::ExecutionTarget;
use super::{
    ExecOutcome, ExecutePlan, HttpExecutor, ProductChoice, RequestCtx, RequestExecutor,
    TenantChoice, Weighted,
};
use crate::db::profiles::LoadConfig;

/// Base64 of the 32-byte key `x-loadgen-test-tenant-key-32byte`.
const TENANT_KEY: &str = "eC1sb2FkZ2VuLXRlc3QtdGVuYW50LWtleS0zMmJ5dGU=";
/// Base64 of `x-loadgen-binding-key`.
const HMAC_KEY: &str = "eC1sb2FkZ2VuLWJpbmRpbmcta2V5";
const API_KEY: &str = "ak-mazbri-test";
const TENANT_ID: &str = "3f0c7f9e-0000-4000-8000-00000000a001";
const REQUESTS: u64 = 3;

/// Client and gateway views of one tenant.
#[derive(Clone)]
struct Case {
    client: Value,
    gateway: MockTenant,
}

impl Case {
    fn plain() -> Self {
        Self {
            client: json!({"encryption_mode": "none"}),
            gateway: MockTenant {
                api_key: API_KEY.into(),
                encryption_mode: "none".into(),
                ..MockTenant::default()
            },
        }
    }

    fn encrypted(
        mode: &str,
        suite: Option<&str>,
        data_alg: Option<&str>,
        mac_alg: Option<&str>,
    ) -> Self {
        let mut case = Self::plain();
        case.client = json!({
            "encryption_mode": mode,
            "tenant_id": TENANT_ID,
            "tenant_key_base64": TENANT_KEY,
            "key_version": "7",
            "suite": suite,
            "data_alg": data_alg,
            "mac_alg": mac_alg,
        });
        case.gateway.encryption_mode = mode.into();
        case.gateway.tenant_id = Some(TENANT_ID.into());
        case.gateway.tenant_key_base64 = Some(TENANT_KEY.into());
        case.gateway.key_version = Some("7".into());
        case.gateway.suite = suite.map(str::to_string);
        case.gateway.data_alg = data_alg.map(str::to_string);
        case.gateway.mac_alg = mac_alg.map(str::to_string);
        case
    }

    fn insecure(mut self) -> Self {
        self.client["allow_insecure"] = json!(true);
        self.gateway.allow_insecure = true;
        self
    }

    /// Add request binding; asymmetric algorithms get a fresh key pair.
    fn bound(mut self, alg: &str) -> Self {
        let (client_key, verify_key) = if alg.starts_with("hmac-") {
            (HMAC_KEY.to_string(), HMAC_KEY.to_string())
        } else {
            generate_binding_keypair(alg).expect("key pair")
        };
        self.client["binding_kid"] = json!(format!("kid-{alg}"));
        self.client["binding_alg"] = json!(alg);
        self.client["binding_key"] = json!(client_key);
        self.gateway.binding_kid = Some(format!("kid-{alg}"));
        self.gateway.binding_alg = Some(alg.into());
        self.gateway.binding_verify_key = Some(verify_key);
        self
    }

    fn encrypted_mode(&self) -> bool {
        self.gateway.encryption_mode != "none"
    }
}

fn snapshot(base: &str, case: &Case) -> (ExecutionTarget, LoadConfig) {
    let mut tenant = json!({
        "id": "tenant-1",
        "name": "t",
        "weight": 1,
        "api_key": API_KEY,
        "allow_insecure": false,
    });
    for (key, value) in case.client.as_object().expect("object") {
        tenant[key] = value.clone();
    }
    let body_raw = serde_json::to_string(&json!({"seq": "{{seq}}", "note": "压测"})).unwrap();
    let target = json!({
        "id": "target-1",
        "name": "mock",
        "gateway_base_url": base,
        "tls": { "insecure_skip_verify": false },
        "tenants": [tenant],
        "requests": [{
            "id": "req-1",
            "name": "echo",
            "protocol": "mazbri",
            "method": "POST",
            "endpoint": "/upstream/echo",
            "params": [{"key": "q", "value": "{{seq}}", "active": true}],
            "headers": [{"key": "x-trace", "value": "trace-{{seq}}", "active": true}],
            "body": {
                "content_type": "application/json",
                "body": body_raw,
                "form": []
            },
            "auth": { "auth_type": "none", "auth_active": true },
            "mazbri": {
                "product_id": "P-ECHO",
                "tenant_source": "environment"
            },
            "tls": {},
            "assertions": {
                "expect_transport": "ok",
                "checks": [
                    {"path": "echo.product_id", "eq": "P-ECHO"},
                    {"path": "echo.body.note", "eq": "压测"},
                    {"path": "tenant_id", "exists": true}
                ]
            },
            "weight": 1,
            "variables": {}
        }]
    });
    let template = json!({
        "model": "closed_loop",
        "concurrency": 1,
        "requests": REQUESTS,
        "warmup_secs": 0,
        "ramp_secs": 0,
        "timeout_secs": 5,
        "connect_timeout_secs": 2,
        "token_refresh_skew_secs": 60,
        "include_request_id": true,
        "processes": 1,
        "qps_mode": "total",
        "requests_mode": "total",
        "loop_enabled": false,
        "loop_interval_secs": 0
    });
    (
        serde_json::from_value(target).expect("target"),
        serde_json::from_value(template).expect("template"),
    )
}

async fn run(case: &Case, gateway_view: Option<MockTenant>) -> (Vec<ExecOutcome>, MockStats) {
    run_with(case, gateway_view, |_| {}).await
}

async fn run_with(
    case: &Case,
    gateway_view: Option<MockTenant>,
    edit: impl FnOnce(&mut ExecutionTarget),
) -> (Vec<ExecOutcome>, MockStats) {
    let config = MockConfig {
        tenants: vec![gateway_view.unwrap_or_else(|| case.gateway.clone())],
        ..MockConfig::default()
    };
    let gateway = MockGateway::start("127.0.0.1:0", &config)
        .await
        .expect("mock gateway");
    let (mut target, template) = snapshot(&gateway.base_url(), case);
    edit(&mut target);
    let executor = HttpExecutor::from_snapshot(&target, &template).expect("executor");
    let mut outcomes = Vec::new();
    for index in 0..REQUESTS {
        let ctx = RequestCtx {
            index,
            request_key: "req-1".into(),
            product_id: "P-ECHO".into(),
            product_name: Some("echo".into()),
            tenant_name: Some("t".into()),
        };
        outcomes.push(executor.execute(&ctx).await);
    }
    let stats = gateway.stats();
    gateway.stop().await;
    (outcomes, stats)
}

async fn assert_all_succeed(label: &str, case: &Case) {
    let (outcomes, stats) = run(case, None).await;
    for (index, outcome) in outcomes.iter().enumerate() {
        assert!(
            outcome.transport_ok && outcome.error_class.is_none(),
            "{label}: request {index} failed: {:?} {:?} (HTTP {:?})",
            outcome.error_class,
            outcome.error,
            outcome.http_status
        );
        let body = outcome.body.as_ref().expect("decoded data");
        assert_eq!(body["echo"]["body"]["seq"], index.to_string(), "{label}");
        assert_eq!(body["echo"]["query"]["q"], index.to_string(), "{label}");
        assert_eq!(body["echo"]["path"], "/upstream/echo", "{label}");
        assert_eq!(outcome.reason.as_deref(), Some("ok"), "{label}");
    }
    assert_eq!(
        stats.tokens_issued, 1,
        "{label}: one token for all requests"
    );
    assert_eq!(stats.accepted, REQUESTS, "{label}");
    let bound = case.gateway.binding_alg.is_some();
    assert_eq!(
        stats.bindings_verified,
        if bound { REQUESTS } else { 0 },
        "{label}"
    );
    let opened = if case.encrypted_mode() { REQUESTS } else { 0 };
    assert_eq!(stats.envelopes_opened, opened, "{label}");
    let sealed = if case.gateway.encryption_mode == "full" {
        REQUESTS
    } else {
        0
    };
    assert_eq!(stats.responses_sealed, sealed, "{label}");
}

async fn assert_all_fail(
    label: &str,
    case: &Case,
    gateway_view: Option<MockTenant>,
    class: &str,
) -> MockStats {
    let (outcomes, stats) = run(case, gateway_view).await;
    for outcome in &outcomes {
        assert!(!outcome.transport_ok, "{label}: must fail");
        assert!(!outcome.expected, "{label}: must not count as expected");
        assert_eq!(
            outcome.error_class.as_deref(),
            Some(class),
            "{label}: {:?}",
            outcome.error
        );
    }
    stats
}

#[tokio::test]
async fn token_exchange_and_plaintext_gateway_call() {
    assert_all_succeed("plain", &Case::plain()).await;
}

#[tokio::test]
async fn every_request_binding_family_is_verified_by_the_gateway() {
    for alg in [
        "hmac-sha256",
        "hmac-sha384",
        "hmac-sha512",
        "hmac-sm3",
        "rsa-pkcs1v15-sign-sha256",
        "rsa-pkcs1v15-sign-sha384",
        "rsa-pkcs1v15-sign-sha512",
        "ecdsa-p256-sha256",
        "ecdsa-p384-sha384",
        "ed25519",
        "sm2-sign",
    ] {
        assert_all_succeed(alg, &Case::plain().bound(alg)).await;
    }
}

#[tokio::test]
async fn named_suites_seal_requests_and_open_full_responses() {
    for suite in [
        "suite/tenant-default-2026-11",
        "suite/gm-sm4-hmac-sm3",
        "suite/tenant-chacha20",
    ] {
        for mode in ["request", "full"] {
            let case = Case::encrypted(mode, Some(suite), None, None);
            assert_all_succeed(&format!("{suite} {mode}"), &case).await;
        }
    }
}

#[tokio::test]
async fn custom_algorithms_and_binding_cover_the_signed_envelope() {
    let custom =
        Case::encrypted("full", None, Some("sm4-cbc-pkcs7"), Some("hmac-sha256")).bound("sm2-sign");
    assert_all_succeed("custom sm4 + hmac-sha256 + sm2", &custom).await;
    let aead = Case::encrypted("request", None, Some("aes-128-gcm"), None).bound("hmac-sm3");
    assert_all_succeed("custom aes-128-gcm + hmac-sm3 binding", &aead).await;
}

#[tokio::test]
async fn legacy_algorithms_need_allow_insecure() {
    let legacy = Case::encrypted("full", None, Some("aes-128-ecb"), Some("hmac-md5"));
    let (target, template) = snapshot("http://127.0.0.1:9", &legacy);
    let refused = HttpExecutor::from_snapshot(&target, &template);
    assert!(
        refused.is_err(),
        "legacy without allow_insecure must be refused"
    );
    assert_all_succeed("aes-128-ecb + hmac-md5", &legacy.clone().insecure()).await;
    let des = Case::encrypted("full", None, Some("des-cbc-pkcs5"), Some("hmac-sha256")).insecure();
    assert_all_succeed("des-cbc-pkcs5", &des).await;
}

#[tokio::test]
async fn response_decrypt_or_verify_failures_are_errors() {
    for suite in ["suite/tenant-default-2026-11", "suite/gm-sm4-hmac-sm3"] {
        let case = Case::encrypted("full", Some(suite), None, None);
        let mut tampered = case.gateway.clone();
        tampered.fault = Fault::TamperResponse;
        let stats = assert_all_fail(suite, &case, Some(tampered), "crypto").await;
        assert_eq!(stats.responses_sealed, REQUESTS);
        let mut downgraded = case.gateway.clone();
        downgraded.fault = Fault::PlaintextResponse;
        assert_all_fail(suite, &case, Some(downgraded), "protocol").await;
    }
}

#[tokio::test]
async fn gateway_rejections_and_token_failures_are_counted() {
    let bound = Case::plain().bound("ed25519");
    let mut other_key = bound.gateway.clone();
    other_key.binding_verify_key = Some(generate_binding_keypair("ed25519").expect("pair").1);
    let stats = assert_all_fail("wrong binding key", &bound, Some(other_key), "transport").await;
    assert_eq!(stats.rejected, REQUESTS);
    assert_eq!(
        stats.tokens_issued, REQUESTS,
        "a 401 drops the cached token"
    );

    let sealed = Case::encrypted("request", Some("suite/tenant-chacha20"), None, None);
    let mut wrong_key = sealed.gateway.clone();
    wrong_key.tenant_key_base64 = Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into());
    let stats = assert_all_fail("gateway cannot open", &sealed, Some(wrong_key), "transport").await;
    assert_eq!(stats.envelopes_opened, 0);

    let mut unknown = Case::plain().gateway;
    unknown.api_key = "another-key".into();
    let stats = assert_all_fail(
        "unknown api key",
        &Case::plain(),
        Some(unknown),
        "token_refresh",
    )
    .await;
    assert_eq!(stats.calls, 0, "no gateway call without a token");
    assert!(stats.token_rejected >= 1);
}

#[tokio::test]
async fn business_checks_run_on_the_decrypted_data() {
    let case = Case::encrypted("full", Some("suite/gm-sm4-hmac-sm3"), None, None).bound("sm2-sign");
    let (outcomes, stats) = run_with(&case, None, |target| {
        target.requests[0].assertions.checks = serde_json::from_value(json!([
            {"path": "echo.body.note", "eq": "不是这个"}
        ]))
        .unwrap();
    })
    .await;
    assert_eq!(stats.responses_sealed, REQUESTS);
    for outcome in outcomes {
        assert!(outcome.transport_ok, "the call itself succeeded");
        assert_eq!(outcome.error_class.as_deref(), Some("business"));
        assert!(
            outcome
                .body
                .is_some_and(|body| body["echo"]["body"]["note"] == "压测")
        );
    }
}

#[tokio::test]
async fn crypto_and_business_failures_are_recorded_as_failed_samples() {
    let case = Case::encrypted("full", Some("suite/tenant-chacha20"), None, None);
    let mut tampered = case.gateway.clone();
    tampered.fault = Fault::TamperResponse;
    for (label, view, checks, class) in [
        ("tampered", Some(tampered), None, "crypto"),
        (
            "business",
            None,
            Some(json!([{"path": "echo.product_id", "eq": "P-OTHER"}])),
            "business",
        ),
    ] {
        let config = MockConfig {
            tenants: vec![view.unwrap_or_else(|| case.gateway.clone())],
            ..MockConfig::default()
        };
        let gateway = MockGateway::start("127.0.0.1:0", &config)
            .await
            .expect("mock gateway");
        let (mut target, template) = snapshot(&gateway.base_url(), &case);
        if let Some(checks) = checks {
            target.requests[0].assertions.checks = serde_json::from_value(checks).unwrap();
        }
        let executor = HttpExecutor::from_snapshot(&target, &template).expect("executor");
        let plan = ExecutePlan {
            products: vec![Weighted {
                weight: 1,
                value: ProductChoice {
                    request_key: "req-1".into(),
                    product_id: "P-ECHO".into(),
                    product_name: Some("echo".into()),
                },
            }],
            tenants: vec![Weighted {
                weight: 1,
                value: TenantChoice { name: "t".into() },
            }],
            expected_reasons: vec!["ok".into()],
        };
        for index in 0..REQUESTS {
            let executed = execute_one(&plan, &executor, index).await;
            assert_eq!(executed.class, OutcomeClass::Failure, "{label}");
            assert!(!executed.sample.success, "{label}");
            assert!(!executed.sample.expected, "{label}");
            assert_eq!(
                executed.sample.error_class.as_deref(),
                Some(class),
                "{label}"
            );
            assert_eq!(executed.sample.customer_name.as_deref(), Some("t"));
        }
        gateway.stop().await;
    }
}

#[tokio::test]
async fn every_custom_data_algorithm_round_trips_under_full_encryption() {
    for data_alg in [
        "aes-128-gcm",
        "aes-192-gcm",
        "aes-256-gcm",
        "chacha20-poly1305",
    ] {
        let case = Case::encrypted("full", None, Some(data_alg), None);
        assert_all_succeed(data_alg, &case).await;
    }
    for data_alg in [
        "aes-128-cbc",
        "aes-256-cbc",
        "aes-128-ctr",
        "aes-256-ctr",
        "aes-128-cfb",
        "aes-256-cfb",
        "sm4-cbc-pkcs7",
        "sm4-ctr",
    ] {
        for mac_alg in ["hmac-sha256", "hmac-sha384", "hmac-sha512", "hmac-sm3"] {
            let case = Case::encrypted("full", None, Some(data_alg), Some(mac_alg));
            assert_all_succeed(&format!("{data_alg} + {mac_alg}"), &case).await;
        }
    }
    for data_alg in [
        "aes-128-ecb",
        "aes-256-ecb",
        "sm4-ecb-pkcs7",
        "des-cbc-pkcs5",
    ] {
        let case = Case::encrypted("full", None, Some(data_alg), Some("hmac-md5")).insecure();
        assert_all_succeed(&format!("legacy {data_alg}"), &case).await;
    }
}
