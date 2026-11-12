//! Tenant/request mixing and the dual-protocol (`http` + `mazbri`) request executor.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as FmtWrite;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use reqwest::{Client, Method};
use serde_json::Value;

use crate::db::profiles::LoadConfig;
use crate::db::workspace::{
    HttpAuthSpec, KeyValueRow, PROTOCOL_HTTP, PROTOCOL_MAZBRI, RequestBodySpec, TlsSpec,
    non_empty_owned, non_empty_str,
};
use crate::engine::mazbri::MazbriTenant;
use crate::engine::metrics::unix_now_ms;
use crate::engine::snapshot::{ExecutionRequest, ExecutionTarget};
use crate::expect::{FieldCheck, Judge, TransportExpect, judge};
use crate::http::{HttpClientOptions, build_http_client};
use crate::vars::{RuntimeVars, render_str};

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
    pub request_key: String,
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
    pub customer_name: Option<String>,
}

/// Executes one request (HTTP, mazbri gateway, or a test mock).
pub trait RequestExecutor: Send + Sync {
    /// Run a single request.
    fn execute<'a>(
        &'a self,
        ctx: &'a RequestCtx,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + Send + 'a>>;
}

/// A request or tenant entry with a weight used by [`mix_request_seed`].
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

/// Executor built from a [`crate::engine::RunSnapshot`].
///
/// Reuses connection pools and per-tenant [`MazbriTenant`] token brokers / sessions
/// across windows, supporting both `http` and `mazbri` requests inside the same
/// workspace or collection.
pub struct HttpExecutor {
    pub(crate) client: Client,
    pub(crate) base_url: Option<String>,
    pub(crate) include_request_id: bool,
    requests: HashMap<String, PreparedRequest>,
    http_tenants: HashMap<String, Option<String>>,
    mazbri_tenants: HashMap<(String, String), Arc<MazbriTenant>>,
}

pub(crate) enum PreparedMazbriMode {
    Inline {
        tenant_name: String,
        tenant: Arc<MazbriTenant>,
    },
    Environment {
        gateway_base_url: String,
        pinned_tenant: Option<String>,
    },
}

pub(crate) struct PreparedRequest {
    pub(crate) product_id_template: String,
    pub(crate) method: Method,
    pub(crate) endpoint_template: String,
    pub(crate) params: Vec<(String, String)>,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: RequestBodySpec,
    pub(crate) auth: HttpAuthSpec,
    pub(crate) variables: BTreeMap<String, String>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) transport: TransportExpect,
    pub(crate) checks: Vec<FieldCheck>,
    pub(crate) custom_client: Option<Client>,
    pub(crate) mazbri_mode: Option<PreparedMazbriMode>,
}

/// Rendered HTTP wire components shared between load-test execution and Probe (`Send`).
#[derive(Debug, Clone)]
pub(crate) struct RenderedHttpCall {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub basic_auth: Option<(String, String)>,
    pub body_bytes: Option<Vec<u8>>,
    pub content_type: Option<String>,
    pub body_preview: Option<String>,
}

impl HttpExecutor {
    /// Build a client and per-request execution templates from a frozen snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP/TLS client cannot be constructed or any
    /// request / Mazbri session is invalid.
    pub fn from_snapshot(target: &ExecutionTarget, template: &LoadConfig) -> Result<Self> {
        if target.requests.is_empty() {
            bail!("the snapshot has no requests to execute");
        }
        let client = build_client_for_tls(&target.tls, template)?;
        let skew = Duration::from_secs(
            u64::try_from(template.token_refresh_skew_secs.max(0)).unwrap_or(0),
        );

        let mut http_tenants = HashMap::new();
        for tenant in &target.tenants {
            http_tenants.insert(
                tenant.name.trim().to_string(),
                non_empty_owned(tenant.api_key.as_deref()),
            );
        }

        let mut mazbri_tenants: HashMap<(String, String), Arc<MazbriTenant>> = HashMap::new();
        let mut requests = HashMap::with_capacity(target.requests.len());

        for req in &target.requests {
            let prepared =
                prepare_request(req, target, template, &client, skew, &mut mazbri_tenants)?;
            requests.insert(req.id.clone(), prepared);
        }

        Ok(Self {
            client,
            base_url: target.gateway_base_url.clone(),
            include_request_id: template.include_request_id,
            requests,
            http_tenants,
            mazbri_tenants,
        })
    }
}

pub(crate) fn build_client_for_tls(tls: &TlsSpec, template: &LoadConfig) -> Result<Client> {
    let options = HttpClientOptions {
        connect_timeout_secs: u64::try_from(template.connect_timeout_secs.max(0))
            .unwrap_or(u64::MAX)
            .max(1),
        timeout_secs: u64::try_from(template.timeout_secs.max(0))
            .unwrap_or(u64::MAX)
            .max(1),
        user_agent: format!("x-loadgen/{}", env!("CARGO_PKG_VERSION")),
        client_cert: non_empty_owned(tls.client_cert.as_deref()),
        client_key: non_empty_owned(tls.client_key.as_deref()),
        ca_cert: non_empty_owned(tls.ca_cert.as_deref()),
        insecure_skip_verify: tls.insecure_skip_verify,
        ..HttpClientOptions::new()
    };
    build_http_client(&options)
}

fn prepare_request(
    req: &ExecutionRequest,
    target: &ExecutionTarget,
    template: &LoadConfig,
    default_client: &Client,
    skew: Duration,
    mazbri_tenants: &mut HashMap<(String, String), Arc<MazbriTenant>>,
) -> Result<PreparedRequest> {
    let method = Method::from_bytes(req.method.trim().as_bytes())
        .with_context(|| format!("invalid HTTP method {}", req.method))?;
    let timeout_secs = req.timeout_secs.unwrap_or(template.timeout_secs);
    let timeout = (timeout_secs > 0)
        .then(|| Duration::from_secs(u64::try_from(timeout_secs).unwrap_or(u64::MAX)));
    let transport =
        TransportExpect::parse(&req.assertions.expect_transport).unwrap_or(TransportExpect::Ok);

    let merged_tls = req.tls.overlay_on(&target.tls);
    let custom_client = if merged_tls == target.tls {
        None
    } else {
        Some(build_client_for_tls(&merged_tls, template)?)
    };
    let effective_client = custom_client.as_ref().unwrap_or(default_client);

    let params = active_kv_pairs(&req.params);
    let headers = active_kv_pairs(&req.headers);

    let mazbri_mode = match req.protocol.as_str() {
        PROTOCOL_HTTP => None,
        PROTOCOL_MAZBRI => Some(prepare_mazbri_mode(
            req,
            target,
            effective_client,
            skew,
            mazbri_tenants,
        )?),
        other => bail!("unsupported request protocol {other:?}"),
    };

    Ok(PreparedRequest {
        product_id_template: req.mazbri.product_id.clone(),
        method,
        endpoint_template: req.endpoint.clone(),
        params,
        headers,
        body: req.body.clone(),
        auth: req.auth.clone(),
        variables: req.variables.clone(),
        timeout,
        transport,
        checks: req.assertions.checks.clone(),
        custom_client,
        mazbri_mode,
    })
}

fn prepare_mazbri_mode(
    req: &ExecutionRequest,
    target: &ExecutionTarget,
    effective_client: &Client,
    skew: Duration,
    mazbri_tenants: &mut HashMap<(String, String), Arc<MazbriTenant>>,
) -> Result<PreparedMazbriMode> {
    let sample_rt = RuntimeVars::for_index(0);
    let raw_gw = non_empty_str(req.mazbri.gateway_base_url.as_deref())
        .or_else(|| non_empty_str(target.gateway_base_url.as_deref()))
        .or_else(|| req.variables.get("gateway_base_url").map(String::as_str))
        .or_else(|| req.variables.get("baseUrl").map(String::as_str))
        .with_context(|| format!("mazbri request '{}' has no gateway_base_url", req.name))?;
    let gw_url = render_str(raw_gw, &req.variables, &sample_rt);

    match req.mazbri.tenant_source.trim() {
        "inline" => {
            let spec =
                req.mazbri.inline_tenant.as_ref().with_context(|| {
                    format!("mazbri request '{}' missing inline_tenant", req.name)
                })?;
            let prepared = MazbriTenant::new(effective_client, &gw_url, spec, skew)?;
            Ok(PreparedMazbriMode::Inline {
                tenant_name: spec.name.trim().to_string(),
                tenant: Arc::new(prepared),
            })
        }
        "environment" => {
            if target.tenants.is_empty() {
                bail!(
                    "mazbri request '{}' needs at least one tenant in environment",
                    req.name
                );
            }
            let pinned = non_empty_owned(req.mazbri.tenant_name.as_deref());
            if let Some(pin_name) = pinned.as_deref() {
                let spec = target
                    .tenants
                    .iter()
                    .find(|t| t.name.trim() == pin_name)
                    .with_context(|| {
                        format!("pinned mazbri tenant '{pin_name}' not found in environment")
                    })?;
                let key = (gw_url.clone(), pin_name.to_string());
                if let std::collections::hash_map::Entry::Vacant(e) = mazbri_tenants.entry(key) {
                    let built = MazbriTenant::new(effective_client, &gw_url, spec, skew)?;
                    e.insert(Arc::new(built));
                }
            } else {
                for spec in &target.tenants {
                    let t_name = spec.name.trim().to_string();
                    let key = (gw_url.clone(), t_name);
                    if let std::collections::hash_map::Entry::Vacant(e) = mazbri_tenants.entry(key)
                    {
                        let built = MazbriTenant::new(effective_client, &gw_url, spec, skew)?;
                        e.insert(Arc::new(built));
                    }
                }
            }
            Ok(PreparedMazbriMode::Environment {
                gateway_base_url: gw_url,
                pinned_tenant: pinned,
            })
        }
        other => bail!("unsupported mazbri tenant_source {other:?}"),
    }
}

pub(crate) fn active_kv_pairs(rows: &[KeyValueRow]) -> Vec<(String, String)> {
    rows.iter()
        .filter(|row| row.active && !row.key.trim().is_empty())
        .map(|row| (row.key.trim().to_string(), row.value.clone()))
        .collect()
}

pub(crate) fn urlencoding_lite(value: &str) -> String {
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

pub(crate) fn join_url(base: &str, path: &str) -> String {
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

/// Render URL, query string, headers, HTTP auth, and body for one HTTP execution.
pub(crate) fn render_http_call(
    base_url: Option<&str>,
    req: &PreparedRequest,
    runtime: &RuntimeVars,
    tenant_api_key: Option<&str>,
    request_id: Option<&str>,
) -> RenderedHttpCall {
    let rendered_ep = render_str(&req.endpoint_template, &req.variables, runtime);
    let mut url = if rendered_ep.starts_with("http://") || rendered_ep.starts_with("https://") {
        rendered_ep
    } else {
        let fallback_base = base_url
            .or_else(|| req.variables.get("baseUrl").map(String::as_str))
            .unwrap_or("");
        let rendered_base = render_str(fallback_base, &req.variables, runtime);
        join_url(&rendered_base, &rendered_ep)
    };

    let mut query_items: Vec<(String, String)> = req
        .params
        .iter()
        .map(|(k, v)| {
            (
                render_str(k, &req.variables, runtime),
                render_str(v, &req.variables, runtime),
            )
        })
        .collect();

    let mut headers: Vec<(String, String)> = req
        .headers
        .iter()
        .map(|(k, v)| {
            (
                render_str(k, &req.variables, runtime),
                render_str(v, &req.variables, runtime),
            )
        })
        .collect();

    let basic_auth = apply_http_auth(
        &req.auth,
        &req.variables,
        runtime,
        tenant_api_key,
        &mut headers,
        &mut query_items,
    );

    if let Some(rid) = request_id {
        headers.push(("X-Request-Id".into(), rid.to_string()));
    }

    if !query_items.is_empty() {
        let encoded = query_items
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding_lite(k), urlencoding_lite(v)))
            .collect::<Vec<_>>()
            .join("&");
        if url.contains('?') {
            url.push('&');
        } else {
            url.push('?');
        }
        url.push_str(&encoded);
    }

    let has_explicit_ct = headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));

    let (body_bytes, content_type, body_preview) = render_body(&req.body, &req.variables, runtime);
    let effective_ct = if has_explicit_ct { None } else { content_type };

    RenderedHttpCall {
        url,
        headers,
        basic_auth,
        body_bytes,
        content_type: effective_ct,
        body_preview,
    }
}

fn apply_http_auth(
    auth: &HttpAuthSpec,
    variables: &BTreeMap<String, String>,
    runtime: &RuntimeVars,
    tenant_api_key: Option<&str>,
    headers: &mut Vec<(String, String)>,
    query_items: &mut Vec<(String, String)>,
) -> Option<(String, String)> {
    if !auth.auth_active {
        if let Some(api_key) = tenant_api_key {
            headers.push(("X-API-KEY".into(), api_key.to_string()));
        }
        return None;
    }
    match auth.auth_type.trim() {
        "bearer" => {
            if let Some(tok) = non_empty_str(auth.token.as_deref()) {
                let rendered = render_str(tok, variables, runtime);
                headers.push(("Authorization".into(), format!("Bearer {rendered}")));
            }
            None
        }
        "basic" => {
            let user = render_str(auth.username.as_deref().unwrap_or(""), variables, runtime);
            let pass = render_str(auth.password.as_deref().unwrap_or(""), variables, runtime);
            Some((user, pass))
        }
        "api-key" => {
            if let Some(k) = non_empty_str(auth.key.as_deref()) {
                let rk = render_str(k, variables, runtime);
                let rv = render_str(auth.value.as_deref().unwrap_or(""), variables, runtime);
                if auth.add_to.as_deref() == Some("QUERY_PARAMS") {
                    query_items.push((rk, rv));
                } else {
                    headers.push((rk, rv));
                }
            }
            None
        }
        _ => {
            if let Some(api_key) = tenant_api_key {
                headers.push(("X-API-KEY".into(), api_key.to_string()));
            }
            None
        }
    }
}

fn render_body(
    spec: &RequestBodySpec,
    static_vars: &BTreeMap<String, String>,
    runtime: &RuntimeVars,
) -> (Option<Vec<u8>>, Option<String>, Option<String>) {
    let ct = spec
        .content_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"));

    match ct {
        None => {
            if let Some(raw) = spec.body.as_deref().filter(|s| !s.is_empty()) {
                let rendered = render_str(raw, static_vars, runtime);
                (
                    Some(rendered.clone().into_bytes()),
                    Some("application/json".to_string()),
                    Some(rendered),
                )
            } else {
                (None, None, None)
            }
        }
        Some("application/x-www-form-urlencoded") => {
            let active_form = active_kv_pairs(&spec.form);
            let encoded = if !active_form.is_empty() {
                active_form
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}={}",
                            urlencoding_lite(&render_str(k, static_vars, runtime)),
                            urlencoding_lite(&render_str(v, static_vars, runtime))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("&")
            } else if let Some(raw) = spec.body.as_deref() {
                render_str(raw, static_vars, runtime)
            } else {
                String::new()
            };
            (
                Some(encoded.clone().into_bytes()),
                Some("application/x-www-form-urlencoded".to_string()),
                Some(encoded),
            )
        }
        Some("multipart/form-data") => {
            let boundary = format!("----xloadgen{}", runtime.uuid.replace('-', ""));
            let mut buf = String::new();
            for (k, v) in active_kv_pairs(&spec.form) {
                let rk = render_str(&k, static_vars, runtime).replace('"', "%22");
                let rv = render_str(&v, static_vars, runtime);
                let _ = write!(
                    buf,
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"{rk}\"\r\n\r\n{rv}\r\n"
                );
            }
            let _ = write!(buf, "--{boundary}--\r\n");
            (
                Some(buf.clone().into_bytes()),
                Some(format!("multipart/form-data; boundary={boundary}")),
                Some(buf),
            )
        }
        Some(other_ct) => {
            let rendered = render_str(spec.body.as_deref().unwrap_or(""), static_vars, runtime);
            (
                Some(rendered.clone().into_bytes()),
                Some(other_ct.to_string()),
                Some(rendered),
            )
        }
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
        let lookup_key = if self.requests.contains_key(&ctx.request_key) {
            &ctx.request_key
        } else {
            &ctx.product_id
        };
        let Some(req) = self.requests.get(lookup_key) else {
            return failed_outcome(
                started,
                started,
                0,
                None,
                Some("config".into()),
                Some(format!("unknown request_key {}", ctx.request_key)),
                None,
                ctx.tenant_name.clone(),
            );
        };

        let request_id = request_id_for(self.include_request_id, ctx);
        let client = req.custom_client.as_ref().unwrap_or(&self.client);

        if let Some(mazbri_mode) = req.mazbri_mode.as_ref() {
            return self
                .execute_mazbri_mode(started, ctx, req, mazbri_mode, client, request_id)
                .await;
        }

        let tenant_api_key = ctx
            .tenant_name
            .as_deref()
            .and_then(|name| self.http_tenants.get(name))
            .and_then(|opt| opt.as_deref());

        let runtime = RuntimeVars::for_index(ctx.index);
        let call = render_http_call(
            self.base_url.as_deref(),
            req,
            &runtime,
            tenant_api_key,
            request_id.as_deref(),
        );

        let mut builder = client.request(req.method.clone(), &call.url);
        if let Some(timeout) = req.timeout {
            builder = builder.timeout(timeout);
        }
        if let Some((user, pass)) = call.basic_auth {
            builder = builder.basic_auth(user, Some(pass));
        }
        for (k, v) in call.headers {
            builder = builder.header(k, v);
        }
        if let Some(ct) = call.content_type {
            builder = builder.header(reqwest::header::CONTENT_TYPE, ct);
        }
        if let Some(bytes) = call.body_bytes {
            builder = builder.body(bytes);
        }

        let sent = Instant::now();
        match builder.send().await {
            Ok(response) => {
                classify_response(
                    started,
                    sent,
                    request_id,
                    req,
                    response,
                    ctx.tenant_name.clone(),
                )
                .await
            }
            Err(err) => failed_outcome(
                started,
                unix_now_ms(),
                elapsed_us(sent),
                request_id,
                Some("transport".into()),
                Some(err.to_string()),
                None,
                ctx.tenant_name.clone(),
            ),
        }
    }

    async fn execute_mazbri_mode(
        &self,
        started: i64,
        ctx: &RequestCtx,
        req: &PreparedRequest,
        mazbri_mode: &PreparedMazbriMode,
        client: &reqwest::Client,
        request_id: Option<String>,
    ) -> ExecOutcome {
        let (actual_tenant_name, mazbri_tenant) = match mazbri_mode {
            PreparedMazbriMode::Inline {
                tenant_name,
                tenant,
            } => (tenant_name.clone(), Arc::clone(tenant)),
            PreparedMazbriMode::Environment {
                gateway_base_url,
                pinned_tenant,
            } => {
                let chosen = pinned_tenant.as_deref().or(ctx.tenant_name.as_deref());
                let Some(name) = chosen else {
                    return failed_outcome(
                        started,
                        started,
                        0,
                        request_id,
                        Some("config".into()),
                        Some("no mazbri tenant selected for this request".into()),
                        None,
                        None,
                    );
                };
                let key = (gateway_base_url.clone(), name.to_string());
                let Some(found) = self.mazbri_tenants.get(&key) else {
                    return failed_outcome(
                        started,
                        started,
                        0,
                        request_id,
                        Some("config".into()),
                        Some(format!("mazbri tenant '{name}' not prepared")),
                        None,
                        Some(name.to_string()),
                    );
                };
                (name.to_string(), Arc::clone(found))
            }
        };
        mazbri_tenant
            .execute(client, ctx, req, request_id, Some(actual_tenant_name))
            .await
    }
}

fn request_id_for(include: bool, ctx: &RequestCtx) -> Option<String> {
    include.then(|| {
        let slug: String = ctx
            .product_id
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                    ch
                } else {
                    '-'
                }
            })
            .collect();
        format!("{slug}-{}", ctx.index)
    })
}

async fn classify_response(
    started: i64,
    sent: Instant,
    request_id: Option<String>,
    req: &PreparedRequest,
    response: reqwest::Response,
    customer_name: Option<String>,
) -> ExecOutcome {
    let status = i64::from(response.status().as_u16());
    let transport_ok = response.status().is_success();
    let raw_bytes = match response.bytes().await {
        Ok(b) => b,
        Err(err) => {
            return failed_outcome(
                started,
                unix_now_ms(),
                elapsed_us(sent),
                request_id,
                Some("transport".into()),
                Some(err.to_string()),
                Some(status),
                customer_name,
            );
        }
    };
    let bytes = i64::try_from(raw_bytes.len()).ok();
    let body = serde_json::from_slice::<Value>(&raw_bytes).ok();
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
        customer_name,
    };
    let judgement = judge(
        transport_ok,
        outcome.body.as_ref(),
        req.transport,
        &req.checks,
    );
    apply_judge(&mut outcome, judgement);
    outcome
}

#[allow(clippy::too_many_arguments)]
pub(super) fn failed_outcome(
    start_unix_ms: i64,
    end_unix_ms: i64,
    latency_us: i64,
    request_id: Option<String>,
    error_class: Option<String>,
    error: Option<String>,
    http_status: Option<i64>,
    customer_name: Option<String>,
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
        customer_name,
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
