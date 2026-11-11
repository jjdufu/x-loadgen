//! Local mock of the mazbri gateway client contract.
//!
//! It implements the two endpoints a client talks to and checks every request
//! with the vendored `x-loadgen-core` primitives, independently of the client
//! code path:
//!
//! - `POST /auth/token`: `{api_key, grant_type: client_credentials}` →
//!   `{success, access_token, token_type, expires_in}`.
//! - `POST /api/v1/gateway`: requires `Authorization: Bearer`; verifies the
//!   `x-mazbri-*` request binding (signature over the final body, timestamp
//!   window, nonce replay); opens the transport envelope under `request` /
//!   `full`; answers with a `GatewayResponseEnvelope` echoing the
//!   `GatewayRequest`, sealed under `full`.
//!
//! Test helpers: `GET /__mock/stats` (counters), `PUT /__mock/tenants`
//! (replace the tenant table), `GET /__mock/health`. Per-tenant `fault`
//! injects a tampered or plaintext response so clients can prove they count
//! decrypt / verify failures as errors.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use x_loadgen_core::crypto::request_binding::{
    HEADER_KEY_ID, HEADER_NONCE, HEADER_SIGNATURE, HEADER_SIGNATURE_INPUT, HEADER_TIMESTAMP,
    binding_key_ref, canonical_http_binding, parse_binding_alg, parse_signature_input,
    parse_unix_timestamp, unix_now_secs, validate_binding_nonce, verify_binding,
};
use x_loadgen_core::crypto::transport::{
    SealEnvelopeParams, TransportEnvelope, open_envelope, seal_envelope,
};
use x_loadgen_core::crypto::{
    AlgorithmId, AlgorithmKind, AlgorithmRegistry, KeyRef, TenantTransportProfile, TransportMode,
};
use x_loadgen_sdk::{
    GATEWAY_INVOKE_PATH, GRANT_TYPE_CLIENT_CREDENTIALS, GatewayRequest, GatewayResponseEnvelope,
    TOKEN_PATH, TenantSettings, transport_profile,
};

/// Allowed distance between a binding timestamp and the mock's clock.
const BINDING_CLOCK_SKEW_SECS: i64 = 300;
/// Nonces remembered for replay detection before the set is reset.
const NONCE_CAPACITY: usize = 200_000;

/// Injected misbehaviour for one tenant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    /// Behave like the real gateway.
    #[default]
    None,
    /// Under `full`, corrupt the sealed response so the client cannot open it.
    TamperResponse,
    /// Under `full`, answer 2xx in plaintext (a downgrade the client must reject).
    PlaintextResponse,
}

/// One tenant as the gateway sees it. Field names follow the x-loadgen tenant form.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct MockTenant {
    /// API key exchanged for a token.
    pub api_key: String,
    /// `none` / `request` / `full`.
    #[serde(default = "default_mode")]
    pub encryption_mode: String,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub tenant_key_base64: Option<String>,
    #[serde(default)]
    pub key_version: Option<String>,
    #[serde(default)]
    pub suite: Option<String>,
    #[serde(default)]
    pub data_alg: Option<String>,
    #[serde(default)]
    pub mac_alg: Option<String>,
    #[serde(default)]
    pub require_signature: Option<bool>,
    #[serde(default)]
    pub allow_insecure: bool,
    /// Expected `x-mazbri-key-id`; binding is required when set.
    #[serde(default)]
    pub binding_kid: Option<String>,
    #[serde(default)]
    pub binding_alg: Option<String>,
    /// HMAC key (standard base64) or signature public key PEM.
    #[serde(default)]
    pub binding_verify_key: Option<String>,
    #[serde(default)]
    pub fault: Fault,
}

impl std::fmt::Debug for MockTenant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockTenant")
            .field("encryption_mode", &self.encryption_mode)
            .field("tenant_id", &self.tenant_id)
            .field("suite", &self.suite)
            .field("data_alg", &self.data_alg)
            .field("mac_alg", &self.mac_alg)
            .field("binding_kid", &self.binding_kid)
            .field("binding_alg", &self.binding_alg)
            .field("fault", &self.fault)
            .finish_non_exhaustive()
    }
}

fn default_mode() -> String {
    "none".to_string()
}

const fn default_token_ttl() -> u64 {
    1800
}

/// Mock configuration (also the body of `PUT /__mock/tenants`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockConfig {
    /// `expires_in` returned by the token endpoint.
    #[serde(default = "default_token_ttl")]
    pub token_ttl_secs: u64,
    #[serde(default)]
    pub tenants: Vec<MockTenant>,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            token_ttl_secs: default_token_ttl(),
            tenants: Vec::new(),
        }
    }
}

/// Counter snapshot returned by `GET /__mock/stats`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockStats {
    pub tokens_issued: u64,
    pub token_rejected: u64,
    pub calls: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub bindings_verified: u64,
    pub envelopes_opened: u64,
    pub responses_sealed: u64,
}

#[derive(Default)]
struct Counters {
    tokens_issued: AtomicU64,
    token_rejected: AtomicU64,
    calls: AtomicU64,
    accepted: AtomicU64,
    rejected: AtomicU64,
    bindings_verified: AtomicU64,
    envelopes_opened: AtomicU64,
    responses_sealed: AtomicU64,
}

impl Counters {
    fn snapshot(&self) -> MockStats {
        let read = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        MockStats {
            tokens_issued: read(&self.tokens_issued),
            token_rejected: read(&self.token_rejected),
            calls: read(&self.calls),
            accepted: read(&self.accepted),
            rejected: read(&self.rejected),
            bindings_verified: read(&self.bindings_verified),
            envelopes_opened: read(&self.envelopes_opened),
            responses_sealed: read(&self.responses_sealed),
        }
    }
}

fn bump(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

struct Binding {
    kid: String,
    alg: AlgorithmId,
    key: KeyRef,
}

struct PreparedTenant {
    config: MockTenant,
    profile: TenantTransportProfile,
    tenant_id: String,
    tenant_key: Vec<u8>,
    key_version: String,
    binding: Option<Binding>,
}

impl PreparedTenant {
    fn new(config: MockTenant) -> Result<Self> {
        let settings = TenantSettings {
            encryption_mode: &config.encryption_mode,
            tenant_id: config.tenant_id.as_deref(),
            tenant_key_base64: config.tenant_key_base64.as_deref(),
            key_version: config.key_version.as_deref(),
            suite: config.suite.as_deref(),
            data_alg: config.data_alg.as_deref(),
            mac_alg: config.mac_alg.as_deref(),
            require_signature: config.require_signature,
            allow_insecure: config.allow_insecure,
            binding_kid: None,
            binding_alg: None,
            binding_key: None,
        };
        let profile = transport_profile(&settings).map_err(|error| anyhow!("{error}"))?;
        let tenant_key = match config.tenant_key_base64.as_deref() {
            Some(text) if !text.trim().is_empty() => STANDARD
                .decode(text.trim())
                .context("tenant_key_base64 must be standard base64")?,
            _ => Vec::new(),
        };
        let binding = match (
            filled(config.binding_kid.as_deref()),
            filled(config.binding_alg.as_deref()),
            filled(config.binding_verify_key.as_deref()),
        ) {
            (None, None, None) => None,
            (Some(kid), Some(alg), Some(key)) => Some(prepare_binding(kid, alg, key)?),
            _ => {
                return Err(anyhow!(
                    "binding_kid, binding_alg, and binding_verify_key go together"
                ));
            }
        };
        Ok(Self {
            tenant_id: config.tenant_id.clone().unwrap_or_default(),
            key_version: config.key_version.clone().unwrap_or_default(),
            profile,
            tenant_key,
            binding,
            config,
        })
    }
}

fn filled(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

fn prepare_binding(kid: &str, alg: &str, key: &str) -> Result<Binding> {
    let alg = parse_binding_alg(alg).map_err(|_| anyhow!("unsupported binding_alg {alg:?}"))?;
    let material = if alg.kind() == AlgorithmKind::Mac {
        STANDARD
            .decode(key)
            .context("HMAC binding_verify_key must be standard base64")?
    } else {
        key.as_bytes().to_vec()
    };
    let key = binding_key_ref(alg, &material).map_err(|error| anyhow!("{error}"))?;
    Ok(Binding {
        kid: kid.to_string(),
        alg,
        key,
    })
}

struct Tables {
    token_ttl: Duration,
    tenants: HashMap<String, Arc<PreparedTenant>>,
    tokens: HashMap<String, (String, Instant)>,
    nonces: HashSet<String>,
    issued: u64,
}

struct Shared {
    tables: Mutex<Tables>,
    counters: Counters,
    registry: AlgorithmRegistry,
}

impl Shared {
    fn tables(&self) -> MutexGuard<'_, Tables> {
        self.tables.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn prepare_tenants(config: &MockConfig) -> Result<HashMap<String, Arc<PreparedTenant>>> {
    let mut tenants = HashMap::new();
    for (index, tenant) in config.tenants.iter().enumerate() {
        let prepared =
            PreparedTenant::new(tenant.clone()).with_context(|| format!("tenant #{index}"))?;
        tenants.insert(tenant.api_key.clone(), Arc::new(prepared));
    }
    Ok(tenants)
}

/// A running mock gateway.
pub struct MockGateway {
    addr: SocketAddr,
    shared: Arc<Shared>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl MockGateway {
    /// Bind `listen` (e.g. `127.0.0.1:0`) and serve until dropped or [`MockGateway::stop`].
    ///
    /// # Errors
    ///
    /// Returns an error when a tenant is invalid or the address cannot be bound.
    pub async fn start(listen: &str, config: &MockConfig) -> Result<Self> {
        let shared = Arc::new(Shared {
            tables: Mutex::new(Tables {
                token_ttl: Duration::from_secs(config.token_ttl_secs),
                tenants: prepare_tenants(config)?,
                tokens: HashMap::new(),
                nonces: HashSet::new(),
                issued: 0,
            }),
            counters: Counters::default(),
            registry: AlgorithmRegistry::default_registry(),
        });
        let listener = tokio::net::TcpListener::bind(listen)
            .await
            .with_context(|| format!("bind {listen}"))?;
        let addr = listener.local_addr()?;
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let app = router(Arc::clone(&shared));
        let join = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });
        Ok(Self {
            addr,
            shared,
            shutdown: Some(tx),
            join: Some(join),
        })
    }

    /// Bound socket address.
    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `http://{addr}` for use as `gateway_base_url`.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Current counters.
    #[must_use]
    pub fn stats(&self) -> MockStats {
        self.shared.counters.snapshot()
    }

    /// Replace the tenant table (issued tokens are dropped).
    ///
    /// # Errors
    ///
    /// Returns an error when a tenant is invalid; the old table is kept.
    pub fn set_config(&self, config: &MockConfig) -> Result<()> {
        let tenants = prepare_tenants(config)?;
        let mut tables = self.shared.tables();
        tables.tenants = tenants;
        tables.tokens.clear();
        tables.token_ttl = Duration::from_secs(config.token_ttl_secs);
        Ok(())
    }

    /// Stop serving and wait for the server task.
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }
}

impl Drop for MockGateway {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route(TOKEN_PATH, post(issue_token))
        .route(GATEWAY_INVOKE_PATH, post(invoke))
        .route("/__mock/stats", get(stats))
        .route("/__mock/health", get(|| async { "ok" }))
        .route("/__mock/tenants", axum::routing::put(replace_tenants))
        .with_state(shared)
}

async fn stats(State(shared): State<Arc<Shared>>) -> Json<MockStats> {
    Json(shared.counters.snapshot())
}

async fn replace_tenants(
    State(shared): State<Arc<Shared>>,
    Json(config): Json<MockConfig>,
) -> Response {
    match prepare_tenants(&config) {
        Ok(tenants) => {
            let mut tables = shared.tables();
            tables.tenants = tenants;
            tables.tokens.clear();
            tables.token_ttl = Duration::from_secs(config.token_ttl_secs);
            (
                StatusCode::OK,
                Json(json!({"tenants": config.tenants.len()})),
            )
                .into_response()
        }
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{error:#}")})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct TokenRequest {
    api_key: String,
    grant_type: String,
}

async fn issue_token(State(shared): State<Arc<Shared>>, body: Bytes) -> Response {
    let reject = |status: StatusCode, error: &str| {
        bump(&shared.counters.token_rejected);
        (status, Json(json!({"success": false, "error": error}))).into_response()
    };
    let Ok(request) = serde_json::from_slice::<TokenRequest>(&body) else {
        return reject(StatusCode::BAD_REQUEST, "malformed token request");
    };
    if request.grant_type != GRANT_TYPE_CLIENT_CREDENTIALS {
        return reject(StatusCode::BAD_REQUEST, "unsupported grant_type");
    }
    let mut tables = shared.tables();
    if !tables.tenants.contains_key(&request.api_key) {
        drop(tables);
        return reject(StatusCode::UNAUTHORIZED, "invalid api key");
    }
    tables.issued = tables.issued.saturating_add(1);
    let token = format!("mock-token-{}-{}", tables.issued, unix_now_secs());
    let ttl = tables.token_ttl;
    tables
        .tokens
        .insert(token.clone(), (request.api_key, Instant::now() + ttl));
    drop(tables);
    bump(&shared.counters.tokens_issued);
    Json(json!({
        "success": true,
        "access_token": token,
        "token_type": "Bearer",
        "expires_in": ttl.as_secs(),
    }))
    .into_response()
}

fn rejection(status: StatusCode, error: &str, request_id: Option<String>) -> Response {
    let envelope = GatewayResponseEnvelope {
        success: false,
        data: None,
        error: Some(error.to_string()),
        status: status.as_u16(),
        request_id,
        content_type: None,
        body_encoding: None,
    };
    (status, Json(envelope)).into_response()
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

async fn invoke(State(shared): State<Arc<Shared>>, headers: HeaderMap, body: Bytes) -> Response {
    bump(&shared.counters.calls);
    let request_id = header(&headers, "x-request-id").map(str::to_string);
    match handle_invoke(&shared, &headers, &body, request_id.clone()) {
        Ok(response) => {
            bump(&shared.counters.accepted);
            response
        }
        Err((status, error)) => {
            bump(&shared.counters.rejected);
            rejection(status, error, request_id)
        }
    }
}

type Rejection = (StatusCode, &'static str);

fn handle_invoke(
    shared: &Shared,
    headers: &HeaderMap,
    body: &[u8],
    request_id: Option<String>,
) -> Result<Response, Rejection> {
    let tenant = authenticate(shared, headers)?;
    if let Some(binding) = tenant.binding.as_ref() {
        verify_request_binding(shared, binding, headers, body)?;
        bump(&shared.counters.bindings_verified);
    }
    let plaintext = match tenant.profile.mode {
        TransportMode::None => body.to_vec(),
        TransportMode::Request | TransportMode::Full => {
            let envelope: TransportEnvelope = serde_json::from_slice(body)
                .map_err(|_| (StatusCode::BAD_REQUEST, "expected a transport envelope"))?;
            let opened = open_envelope(
                &tenant.profile,
                &shared.registry,
                &tenant.tenant_key,
                envelope
                    .mac_alg
                    .as_ref()
                    .map(|_| tenant.tenant_key.as_slice()),
                &tenant.tenant_id,
                GATEWAY_INVOKE_PATH,
                &envelope,
            )
            .map_err(|_| (StatusCode::BAD_REQUEST, "request envelope failed to open"))?;
            bump(&shared.counters.envelopes_opened);
            opened
        }
    };
    let request: GatewayRequest = serde_json::from_slice(&plaintext)
        .map_err(|_| (StatusCode::BAD_REQUEST, "malformed gateway request"))?;
    let reply = GatewayResponseEnvelope {
        success: true,
        data: Some(json!({
            "echo": {
                "product_id": request.product_id,
                "path": request.path,
                "method": request.method,
                "query": request.query,
                "headers": request.headers,
                "body": request.body,
            },
            "tenant_id": tenant.tenant_id,
        })),
        error: None,
        status: 200,
        request_id,
        content_type: Some("application/json".into()),
        body_encoding: None,
    };
    let plain_reply =
        serde_json::to_vec(&reply).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "encode"))?;
    if tenant.profile.mode != TransportMode::Full || tenant.config.fault == Fault::PlaintextResponse
    {
        return Ok((StatusCode::OK, Json(reply)).into_response());
    }
    let mut sealed = seal_reply(shared, &tenant, &plain_reply)?;
    if tenant.config.fault == Fault::TamperResponse {
        tamper(&mut sealed);
    }
    bump(&shared.counters.responses_sealed);
    Ok((StatusCode::OK, Json(sealed)).into_response())
}

fn authenticate(shared: &Shared, headers: &HeaderMap) -> Result<Arc<PreparedTenant>, Rejection> {
    let token = header(headers, "authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token"))?;
    let tables = shared.tables();
    let (api_key, expires_at) = tables
        .tokens
        .get(token)
        .ok_or((StatusCode::UNAUTHORIZED, "unknown access token"))?;
    if Instant::now() >= *expires_at {
        return Err((StatusCode::UNAUTHORIZED, "access token expired"));
    }
    tables
        .tenants
        .get(api_key)
        .cloned()
        .ok_or((StatusCode::UNAUTHORIZED, "tenant no longer exists"))
}

fn verify_request_binding(
    shared: &Shared,
    binding: &Binding,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), Rejection> {
    let missing = (StatusCode::UNAUTHORIZED, "request binding headers missing");
    let alg = parse_signature_input(header(headers, HEADER_SIGNATURE_INPUT).ok_or(missing)?)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "unsupported signature input"))?;
    if alg != binding.alg {
        return Err((StatusCode::UNAUTHORIZED, "binding algorithm mismatch"));
    }
    if header(headers, HEADER_KEY_ID).ok_or(missing)? != binding.kid {
        return Err((StatusCode::UNAUTHORIZED, "unknown binding key id"));
    }
    let timestamp = parse_unix_timestamp(header(headers, HEADER_TIMESTAMP).ok_or(missing)?)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid binding timestamp"))?;
    if (unix_now_secs() - timestamp).abs() > BINDING_CLOCK_SKEW_SECS {
        return Err((StatusCode::UNAUTHORIZED, "binding timestamp outside window"));
    }
    let nonce = header(headers, HEADER_NONCE).ok_or(missing)?;
    validate_binding_nonce(nonce).map_err(|_| (StatusCode::UNAUTHORIZED, "invalid nonce"))?;
    let signature = header(headers, HEADER_SIGNATURE).ok_or(missing)?;
    let payload = canonical_http_binding("POST", GATEWAY_INVOKE_PATH, timestamp, nonce, body);
    verify_binding(
        &shared.registry,
        binding.alg,
        &binding.key,
        &payload,
        signature,
    )
    .map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            "request binding signature mismatch",
        )
    })?;
    let mut tables = shared.tables();
    if tables.nonces.len() >= NONCE_CAPACITY {
        tables.nonces.clear();
    }
    if !tables.nonces.insert(nonce.to_string()) {
        return Err((StatusCode::UNAUTHORIZED, "binding nonce replayed"));
    }
    Ok(())
}

fn seal_reply(
    shared: &Shared,
    tenant: &PreparedTenant,
    plaintext: &[u8],
) -> Result<TransportEnvelope, Rejection> {
    let (_, mac_alg) = tenant
        .profile
        .effective_algs()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "profile"))?;
    seal_envelope(SealEnvelopeParams {
        profile: &tenant.profile,
        registry: &shared.registry,
        data_key: &tenant.tenant_key,
        mac_key: mac_alg.map(|_| tenant.tenant_key.as_slice()),
        key_version: &tenant.key_version,
        tenant_id: &tenant.tenant_id,
        http_path: GATEWAY_INVOKE_PATH,
        plaintext,
        timestamp: unix_now_secs(),
    })
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "seal response"))
}

/// Flip one bit of the ciphertext (or of the signature when the ciphertext is empty).
fn tamper(envelope: &mut TransportEnvelope) {
    let mut bytes = STANDARD.decode(&envelope.ciphertext).unwrap_or_default();
    if let Some(first) = bytes.first_mut() {
        *first ^= 0x01;
        envelope.ciphertext = STANDARD.encode(&bytes);
    } else {
        envelope.signature = format!("00{}", envelope.signature);
    }
}

/// Parse a JSON config document.
///
/// # Errors
///
/// Returns an error when the text is not a valid [`MockConfig`].
pub fn parse_config(text: &str) -> Result<MockConfig> {
    serde_json::from_str(text).context("parse mock gateway config")
}

/// Build a sample config value (documentation helper for the binary's `--example`).
#[must_use]
pub fn example_config() -> Value {
    json!({
        "token_ttl_secs": 1800,
        "tenants": [{
            "api_key": "<api key>",
            "encryption_mode": "full",
            "tenant_id": "<tenant uuid>",
            "tenant_key_base64": "<32-byte key, base64>",
            "key_version": "1",
            "suite": "suite/gm-sm4-hmac-sm3",
            "binding_kid": "<kid>",
            "binding_alg": "sm2-sign",
            "binding_verify_key": "<public key PEM or HMAC key base64>"
        }]
    })
}
