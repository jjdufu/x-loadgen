//! Tenant/product mixing and the request executor interface.

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use reqwest::{Client, Method};
use serde_json::Value;

use crate::db::targets::{PROTOCOL_HTTP, PROTOCOL_MAZBRI, Product, TargetDetail};
use crate::db::templates::Template;
use crate::engine::mazbri::MazbriTenant;
use crate::engine::metrics::unix_now_ms;
use crate::expect::{FieldCheck, Judge, TransportExpect, judge};
use crate::http::{HttpClientOptions, build_http_client};

/// Deterministic weighted pick. `seed` is the request index (`next_index`).
#[must_use]
pub fn mix_request_seed(seed: u64, weights: &[u64]) -> usize {
    let total: u64 = weights.iter().copied().sum();
    if total == 0 || weights.is_empty() {
        return 0;
    }
    let mut x = seed.wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 32;
    let r = x % total;
    let mut acc = 0_u64;
    for (index, weight) in weights.iter().enumerate() {
        acc = acc.saturating_add(*weight);
        if r < acc {
            return index;
        }
    }
    weights.len() - 1
}

/// Context passed to a [`RequestExecutor`] for a single request.
#[derive(Debug, Clone)]
pub struct RequestCtx {
    pub index: u64,
    pub product_id: String,
    pub product_name: Option<String>,
    pub tenant_name: Option<String>,
}

/// Result of a single executed request (no scheduling or metrics).
#[derive(Debug, Clone)]
pub struct ExecOutcome {
    pub start_unix_ms: i64,
    pub end_unix_ms: i64,
    pub latency_us: i64,
    pub http_status: Option<i64>,
    pub transport_ok: bool,
    pub expected: bool,
    pub reason: Option<String>,
    pub error_class: Option<String>,
    pub error: Option<String>,
    pub response_bytes: Option<i64>,
    pub request_id: Option<String>,
    pub body: Option<Value>,
}

/// Executes one request (HTTP, mazbri gateway, or a test mock).
pub trait RequestExecutor: Send + Sync {
    /// Run a single request.
    fn execute<'a>(
        &'a self,
        ctx: &'a RequestCtx,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + Send + 'a>>;
}

/// A product or tenant entry with a weight used by [`mix_request_seed`].
#[derive(Debug, Clone)]
pub struct Weighted<T> {
    pub weight: u64,
    pub value: T,
}

/// Pick an item using [`mix_request_seed`].
#[must_use]
pub fn pick_weighted<T>(seed: u64, items: &[Weighted<T>]) -> Option<&T> {
    if items.is_empty() {
        return None;
    }
    let weights: Vec<u64> = items.iter().map(|item| item.weight.max(1)).collect();
    let index = mix_request_seed(seed, &weights);
    items.get(index).map(|item| &item.value)
}

/// Executor built from a run snapshot. The client (and, for mazbri targets,
/// each tenant's token cache and gateway session) is reused across windows.
///
/// - `http` targets call `{base}{product.path}` directly, with the tenant's
///   optional `api_key` as `X-API-KEY`.
/// - `mazbri` targets exchange the tenant `api_key` for a bearer token and send
///   every product as a [`x_loadgen_sdk::GatewayRequest`] to `/api/v1/gateway`,
///   sealed and signed per the tenant settings; the response is decoded before
///   it is timed and judged.
pub struct HttpExecutor {
    pub(super) client: Client,
    pub(super) base_url: String,
    pub(super) include_request_id: bool,
    mazbri: bool,
    products: HashMap<String, PreparedProduct>,
    tenants: HashMap<String, PreparedTenant>,
}

pub(super) struct PreparedProduct {
    pub(super) product_id: String,
    pub(super) method: Method,
    pub(super) path: String,
    pub(super) query: Vec<(String, String)>,
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: Option<String>,
    pub(super) body_is_json: bool,
    pub(super) timeout: Option<Duration>,
    pub(super) transport: TransportExpect,
    pub(super) checks: Vec<FieldCheck>,
}

enum PreparedTenant {
    Http { api_key: Option<String> },
    Mazbri(Box<MazbriTenant>),
}

impl HttpExecutor {
    /// Build a client and per-product request templates from a snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be constructed or a product
    /// method / expectation is invalid.
    pub fn from_snapshot(target: &TargetDetail, template: &Template) -> Result<Self> {
        let options = HttpClientOptions {
            connect_timeout_secs: u64::try_from(template.connect_timeout_secs.max(0))
                .unwrap_or(u64::MAX)
                .max(1),
            timeout_secs: u64::try_from(template.timeout_secs.max(0))
                .unwrap_or(u64::MAX)
                .max(1),
            user_agent: format!("x-loadgen/{}", env!("CARGO_PKG_VERSION")),
            client_cert: target.client_cert.clone(),
            client_key: target.client_key.clone(),
            ca_cert: target.ca_cert.clone(),
            insecure_skip_verify: target.insecure_skip_verify,
            ..HttpClientOptions::new()
        };
        let client = build_http_client(&options)?;
        let selected = select_products(target, template);
        if selected.is_empty() {
            bail!("the snapshot has no products to execute");
        }
        let mut products = HashMap::new();
        for product in selected {
            let prepared = prepare_product(product, template.timeout_secs)?;
            products.insert(product.product_id.clone(), prepared);
        }
        let mut tenants = HashMap::new();
        match target.protocol.as_str() {
            PROTOCOL_HTTP => {
                for tenant in &target.tenants {
                    let api_key = tenant.api_key.clone().filter(|key| !key.trim().is_empty());
                    tenants.insert(tenant.name.clone(), PreparedTenant::Http { api_key });
                }
            }
            PROTOCOL_MAZBRI => {
                if target.tenants.is_empty() {
                    bail!("a mazbri target needs at least one tenant");
                }
                let skew = Duration::from_secs(
                    u64::try_from(template.token_refresh_skew_secs.max(0)).unwrap_or(0),
                );
                for tenant in &target.tenants {
                    let prepared =
                        MazbriTenant::new(&client, &target.gateway_base_url, tenant, skew)?;
                    tenants.insert(
                        tenant.name.clone(),
                        PreparedTenant::Mazbri(Box::new(prepared)),
                    );
                }
            }
            other => bail!("unsupported target protocol {other:?}"),
        }
        Ok(Self {
            client,
            base_url: target.gateway_base_url.clone(),
            include_request_id: template.include_request_id,
            mazbri: target.protocol == PROTOCOL_MAZBRI,
            products,
            tenants,
        })
    }
}

fn select_products<'a>(target: &'a TargetDetail, template: &Template) -> Vec<&'a Product> {
    match template.product_ids.as_ref() {
        Some(ids) if !ids.is_empty() => target
            .products
            .iter()
            .filter(|product| ids.iter().any(|id| id == &product.id))
            .collect(),
        _ => target.products.iter().collect(),
    }
}

fn prepare_product(product: &Product, default_timeout_secs: i64) -> Result<PreparedProduct> {
    let method = Method::from_bytes(product.method.trim().as_bytes())
        .with_context(|| format!("invalid HTTP method {}", product.method))?;
    let timeout_secs = product.timeout_secs.unwrap_or(default_timeout_secs);
    let timeout = if timeout_secs > 0 {
        Some(Duration::from_secs(
            u64::try_from(timeout_secs).unwrap_or(u64::MAX),
        ))
    } else {
        None
    };
    let transport = product
        .expect_transport
        .as_deref()
        .and_then(TransportExpect::parse)
        .unwrap_or(TransportExpect::Ok);
    let checks = match &product.expect_checks_json {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| serde_json::from_value::<FieldCheck>(item.clone()).ok())
            .collect(),
        _ => Vec::new(),
    };
    Ok(PreparedProduct {
        product_id: product.product_id.clone(),
        method,
        path: product.path.clone(),
        query: flatten_pairs(product.query_json.as_ref()),
        headers: flatten_pairs(product.headers_json.as_ref()),
        body: product.body_json.as_ref().map(render_body_template),
        body_is_json: product
            .body_json
            .as_ref()
            .is_some_and(|value| !value.is_string()),
        timeout,
        transport,
        checks,
    })
}

pub(super) fn flatten_pairs(value: Option<&Value>) -> Vec<(String, String)> {
    let Some(Value::Object(map)) = value else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(key, value)| {
            let rendered = match value {
                Value::String(s) => s.clone(),
                Value::Null => return None,
                other => other.to_string(),
            };
            Some((key.clone(), rendered))
        })
        .collect()
}

fn render_body_template(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub(super) fn apply_seq(template: &str, index: u64) -> String {
    template.replace("{{seq}}", &index.to_string())
}

fn urlencoding_lite(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            _ => {
                for byte in ch.to_string().as_bytes() {
                    let _ = write!(out, "%{byte:02X}");
                }
            }
        }
    }
    out
}

pub(super) fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    if path.is_empty() {
        return base.to_string();
    }
    if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

impl RequestExecutor for HttpExecutor {
    fn execute<'a>(
        &'a self,
        ctx: &'a RequestCtx,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + Send + 'a>> {
        Box::pin(async move { self.execute_one(ctx).await })
    }
}

impl HttpExecutor {
    async fn execute_one(&self, ctx: &RequestCtx) -> ExecOutcome {
        let started = unix_now_ms();
        let Some(product) = self.products.get(&ctx.product_id) else {
            return failed_outcome(
                started,
                started,
                0,
                None,
                Some("config".into()),
                Some(format!("unknown product_id {}", ctx.product_id)),
                None,
            );
        };
        let tenant = ctx
            .tenant_name
            .as_ref()
            .and_then(|name| self.tenants.get(name));
        let request_id = request_id_for(self.include_request_id, ctx);
        let api_key = match tenant {
            Some(PreparedTenant::Mazbri(mazbri)) => {
                return mazbri.execute(self, ctx, product, request_id).await;
            }
            Some(PreparedTenant::Http { api_key }) => api_key.as_deref(),
            None if self.mazbri => {
                return failed_outcome(
                    started,
                    started,
                    0,
                    request_id,
                    Some("config".into()),
                    Some("no mazbri tenant selected for this request".into()),
                    None,
                );
            }
            None => None,
        };
        let request = self.build_request(ctx, product);
        let request = attach_identity(request, api_key, request_id.as_deref());
        let sent = Instant::now();
        match request.send().await {
            Ok(response) => classify_response(started, sent, request_id, product, response).await,
            Err(err) => failed_outcome(
                started,
                unix_now_ms(),
                elapsed_us(sent),
                request_id,
                Some("transport".into()),
                Some(err.to_string()),
                None,
            ),
        }
    }

    fn build_request(
        &self,
        ctx: &RequestCtx,
        product: &PreparedProduct,
    ) -> reqwest::RequestBuilder {
        let mut request = self.client.request(
            product.method.clone(),
            build_url(&self.base_url, product, ctx.index),
        );
        if let Some(timeout) = product.timeout {
            request = request.timeout(timeout);
        }
        for (key, value) in &product.headers {
            request = request.header(key, apply_seq(value, ctx.index));
        }
        if let Some(body) = product.body.as_ref() {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(apply_seq(body, ctx.index));
        }
        request
    }
}

fn request_id_for(include: bool, ctx: &RequestCtx) -> Option<String> {
    include.then(|| format!("{}-{}", ctx.product_id, ctx.index))
}

fn attach_identity(
    mut request: reqwest::RequestBuilder,
    api_key: Option<&str>,
    request_id: Option<&str>,
) -> reqwest::RequestBuilder {
    if let Some(api_key) = api_key {
        request = request.header("X-API-KEY", api_key);
    }
    if let Some(id) = request_id {
        request = request.header("X-Request-Id", id);
    }
    request
}

fn build_url(base: &str, product: &PreparedProduct, index: u64) -> String {
    let mut url = join_url(base, &product.path);
    if product.query.is_empty() {
        return url;
    }
    let query: String = product
        .query
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding_lite(key),
                urlencoding_lite(&apply_seq(value, index))
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    if url.contains('?') {
        url.push('&');
    } else {
        url.push('?');
    }
    url.push_str(&query);
    url
}

async fn classify_response(
    started: i64,
    sent: Instant,
    request_id: Option<String>,
    product: &PreparedProduct,
    response: reqwest::Response,
) -> ExecOutcome {
    let status = i64::from(response.status().as_u16());
    let transport_ok = response.status().is_success();
    let bytes = response
        .content_length()
        .and_then(|n| i64::try_from(n).ok());
    let body = response.json::<Value>().await.ok();
    let mut outcome = ExecOutcome {
        start_unix_ms: started,
        end_unix_ms: unix_now_ms(),
        latency_us: elapsed_us(sent),
        http_status: Some(status),
        transport_ok,
        expected: false,
        reason: None,
        error_class: None,
        error: None,
        response_bytes: bytes,
        request_id,
        body,
    };
    let judgement = judge(
        transport_ok,
        outcome.body.as_ref(),
        product.transport,
        &product.checks,
    );
    apply_judge(&mut outcome, judgement);
    outcome
}

pub(super) fn failed_outcome(
    start_unix_ms: i64,
    end_unix_ms: i64,
    latency_us: i64,
    request_id: Option<String>,
    error_class: Option<String>,
    error: Option<String>,
    http_status: Option<i64>,
) -> ExecOutcome {
    ExecOutcome {
        start_unix_ms,
        end_unix_ms,
        latency_us,
        http_status,
        transport_ok: false,
        expected: false,
        reason: None,
        error_class,
        error,
        response_bytes: None,
        request_id,
        body: None,
    }
}

pub(super) fn elapsed_us(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_micros()).unwrap_or(i64::MAX)
}

pub(super) fn apply_judge(outcome: &mut ExecOutcome, judgement: Judge) {
    match judgement {
        Judge::Met => {
            outcome.expected = !outcome.transport_ok;
            outcome.reason = Some(if outcome.transport_ok {
                "ok".into()
            } else {
                "reject".into()
            });
        }
        Judge::TransportFailed => {
            outcome.transport_ok = false;
            outcome.expected = false;
            outcome.error_class = Some("transport".into());
            outcome.error = Some("transport expectation was not met".into());
        }
        Judge::Failed { class, message } => {
            outcome.expected = false;
            outcome.error_class = Some(class.to_string());
            outcome.error = Some(message);
        }
    }
}
