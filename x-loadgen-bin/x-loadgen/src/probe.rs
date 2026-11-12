//! Single-shot interactive request execution (`Send` button in the Hoppscotch UI).
//!
//! Executes either a `SavedRequest` or an unsaved `RequestDraft` against an
//! optional `Environment`, returning response headers, pretty/raw body, timing,
//! per-rule assertion outcomes, unresolved variable warnings, and (for `mazbri`
//! requests) a full `MazbriWireDebug` breakdown of `/auth/token` + `x-mazbri-*`
//! binding headers + sealed/unsealed envelopes.

use std::collections::BTreeMap;
use std::time::Instant;

use reqwest::Method;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use x_loadgen_core::crypto::request_binding::unix_now_secs;
use x_loadgen_sdk::{GATEWAY_INVOKE_PATH, TOKEN_PATH, parse_token_response, token_request_body};

use crate::db::StoreError;
use crate::db::collections::RequestDraft;
use crate::db::profiles::LoadConfig;
use crate::db::workspace::{PROTOCOL_MAZBRI, TenantSpec, non_empty_str};
use crate::engine::mazbri::{build_gateway_request, decode_error_class};
use crate::engine::snapshot::{ExecutionRequest, ExecutionTarget, compile_single_request_target};
use crate::engine::transport::{
    PreparedRequest, Weighted, active_kv_pairs, build_client_for_tls, join_url, pick_weighted,
    render_http_call,
};
use crate::expect::{DetailedCheckResult, Judge, TransportExpect, eval_checks_detailed, judge};
use crate::vars::{RuntimeVars, render_str, unresolved_tokens};

/// Body for `POST /api/v1/probe`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProbeRequest {
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub request: Option<RequestDraft>,
    #[serde(default)]
    pub collection_id: Option<String>,
    #[serde(default)]
    pub environment_id: Option<String>,
    #[serde(default)]
    pub variables_override: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub seq: Option<u64>,
}

/// Preview of the rendered outbound request and any unresolved `<<var>>` placeholders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRequestPreview {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_preview: Option<String>,
    pub unresolved_variables: Vec<String>,
}

/// Cryptographic wire trace returned when probing a `mazbri` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MazbriWireDebug {
    pub tenant_name: String,
    pub encryption_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_alg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_alg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_alg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_kid: Option<String>,
    pub token_url: String,
    pub token_exchange_us: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expires_in: Option<u64>,
    pub invoke_url: String,
    pub binding_headers: BTreeMap<String, String>,
    pub inner_gateway_request: Value,
    pub wire_request_body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wire_response_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wire_response_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoded_envelope: Option<Value>,
}

/// Response returned by `POST /api/v1/probe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResponse {
    pub protocol: String,
    pub transport_ok: bool,
    pub passed_assertions: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    pub latency_us: i64,
    pub response_bytes: usize,
    pub headers: Vec<(String, String)>,
    pub body_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_json: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub assertions: Vec<DetailedCheckResult>,
    pub request_preview: ProbeRequestPreview,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mazbri_wire: Option<MazbriWireDebug>,
}

/// Compile a single request with synchronous DB access, then execute it asynchronously.
///
/// # Errors
///
/// Returns `StoreError` if the referenced request, collection, environment, or TLS/URL
/// settings fail validation.
pub fn prepare_probe(conn: &Connection, req: &ProbeRequest) -> Result<ExecutionTarget, StoreError> {
    let (target, _, _) = compile_single_request_target(
        conn,
        req.request_id.as_deref(),
        req.request.as_ref(),
        req.collection_id.as_deref(),
        req.environment_id.as_deref(),
        req.variables_override.as_ref(),
    )?;
    Ok(target)
}

/// Execute a compiled single-request target once and collect full debug telemetry.
///
/// # Errors
///
/// Returns `StoreError` if the HTTP/TLS client cannot be constructed.
pub async fn run_probe(target: ExecutionTarget, seq: u64) -> Result<ProbeResponse, StoreError> {
    let exec_req = target
        .requests
        .first()
        .ok_or_else(|| StoreError::invalid("probe target has no request"))?;

    let mut load_cfg = LoadConfig::default();
    if let Some(t) = exec_req.timeout_secs.filter(|v| *v > 0) {
        load_cfg.timeout_secs = t;
    }
    let merged_tls = exec_req.tls.overlay_on(&target.tls);
    let client = build_client_for_tls(&merged_tls, &load_cfg)
        .map_err(|err| StoreError::invalid(format!("invalid TLS/HTTP configuration: {err}")))?;

    let method = Method::from_bytes(exec_req.method.trim().as_bytes())
        .map_err(|err| StoreError::invalid(format!("invalid HTTP method: {err}")))?;
    let transport_expect = TransportExpect::parse(&exec_req.assertions.expect_transport)
        .unwrap_or(TransportExpect::Ok);

    let prepared = PreparedRequest {
        product_id_template: exec_req.mazbri.product_id.clone(),
        method,
        endpoint_template: exec_req.endpoint.clone(),
        params: active_kv_pairs(&exec_req.params),
        headers: active_kv_pairs(&exec_req.headers),
        body: exec_req.body.clone(),
        auth: exec_req.auth.clone(),
        variables: exec_req.variables.clone(),
        timeout: Some(std::time::Duration::from_secs(
            u64::try_from(load_cfg.timeout_secs.max(1)).unwrap_or(30),
        )),
        transport: transport_expect,
        checks: exec_req.assertions.checks.clone(),
        custom_client: None,
        mazbri_mode: None,
    };

    let unresolved = collect_unresolved(exec_req, target.gateway_base_url.as_deref());
    let runtime = RuntimeVars::for_index(seq);

    if exec_req.protocol == PROTOCOL_MAZBRI {
        Ok(run_mazbri_probe(&client, &target, exec_req, &prepared, &runtime, unresolved).await)
    } else {
        Ok(run_http_probe(&client, &target, &prepared, &runtime, unresolved).await)
    }
}

async fn run_http_probe(
    client: &reqwest::Client,
    target: &ExecutionTarget,
    prepared: &PreparedRequest,
    runtime: &RuntimeVars,
    unresolved: Vec<String>,
) -> ProbeResponse {
    let weighted_tenants: Vec<Weighted<&TenantSpec>> = target
        .tenants
        .iter()
        .map(|t| Weighted {
            weight: u64::try_from(t.weight.max(1)).unwrap_or(1),
            value: t,
        })
        .collect();
    let picked_api_key = pick_weighted(runtime.seq, &weighted_tenants)
        .and_then(|t| non_empty_str(t.api_key.as_deref()));

    let call = render_http_call(
        target.gateway_base_url.as_deref(),
        prepared,
        runtime,
        picked_api_key,
        Some(&format!("probe-{}", runtime.uuid)),
    );

    let preview_headers = call
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), mask_header_if_secret(k, v)))
        .collect();

    let request_preview = ProbeRequestPreview {
        method: prepared.method.as_str().to_string(),
        url: call.url.clone(),
        headers: preview_headers,
        body_preview: call.body_preview.clone(),
        unresolved_variables: unresolved,
    };

    let mut builder = client.request(prepared.method.clone(), &call.url);
    if let Some(timeout) = prepared.timeout {
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

    let started = Instant::now();
    let response = match builder.send().await {
        Ok(resp) => resp,
        Err(err) => {
            let latency_us = elapsed_us(started);
            let judgement = judge(false, None, prepared.transport, &prepared.checks);
            let passed = matches!(judgement, Judge::Met);
            return ProbeResponse {
                protocol: "http".into(),
                transport_ok: false,
                passed_assertions: passed,
                http_status: None,
                status_text: None,
                latency_us,
                response_bytes: 0,
                headers: Vec::new(),
                body_text: String::new(),
                body_json: None,
                error_class: Some("transport".into()),
                error: Some(err.to_string()),
                assertions: Vec::new(),
                request_preview,
                mazbri_wire: None,
            };
        }
    };

    decode_http_probe_response(response, started, prepared, request_preview).await
}

async fn decode_http_probe_response(
    response: reqwest::Response,
    started: Instant,
    prepared: &PreparedRequest,
    request_preview: ProbeRequestPreview,
) -> ProbeResponse {
    let status_code = response.status().as_u16();
    let status_text = response.status().canonical_reason().map(str::to_string);
    let transport_ok = response.status().is_success();
    let resp_headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("<binary>").to_string(),
            )
        })
        .collect();

    let raw_bytes = match response.bytes().await {
        Ok(b) => b,
        Err(err) => {
            return ProbeResponse {
                protocol: "http".into(),
                transport_ok: false,
                passed_assertions: false,
                http_status: Some(status_code),
                status_text,
                latency_us: elapsed_us(started),
                response_bytes: 0,
                headers: resp_headers,
                body_text: String::new(),
                body_json: None,
                error_class: Some("transport".into()),
                error: Some(err.to_string()),
                assertions: Vec::new(),
                request_preview,
                mazbri_wire: None,
            };
        }
    };

    let latency_us = elapsed_us(started);
    let response_bytes = raw_bytes.len();
    let body_json = serde_json::from_slice::<Value>(&raw_bytes).ok();
    let body_text = if let Some(val) = body_json.as_ref() {
        serde_json::to_string_pretty(val)
            .unwrap_or_else(|_| String::from_utf8_lossy(&raw_bytes).into_owned())
    } else {
        String::from_utf8_lossy(&raw_bytes).into_owned()
    };

    let detailed_checks = eval_checks_detailed(body_json.as_ref(), &prepared.checks);
    let judgement = judge(
        transport_ok,
        body_json.as_ref(),
        prepared.transport,
        &prepared.checks,
    );
    let (passed_assertions, error_class, error) = match judgement {
        Judge::Met => (true, None, None),
        Judge::TransportFailed => (
            false,
            Some("transport".to_string()),
            Some(format!(
                "HTTP {status_code} failed transport expectation 'ok'"
            )),
        ),
        Judge::Failed { class, message } => (false, Some(class.to_string()), Some(message)),
    };

    ProbeResponse {
        protocol: "http".into(),
        transport_ok,
        passed_assertions,
        http_status: Some(status_code),
        status_text,
        latency_us,
        response_bytes,
        headers: resp_headers,
        body_text,
        body_json,
        error_class,
        error,
        assertions: detailed_checks,
        request_preview,
        mazbri_wire: None,
    }
}

#[allow(clippy::too_many_lines)]
async fn run_mazbri_probe(
    client: &reqwest::Client,
    target: &ExecutionTarget,
    exec_req: &ExecutionRequest,
    prepared: &PreparedRequest,
    runtime: &RuntimeVars,
    unresolved: Vec<String>,
) -> ProbeResponse {
    let raw_gw = non_empty_str(exec_req.mazbri.gateway_base_url.as_deref())
        .or_else(|| non_empty_str(target.gateway_base_url.as_deref()))
        .or_else(|| {
            exec_req
                .variables
                .get("gateway_base_url")
                .map(String::as_str)
        })
        .or_else(|| exec_req.variables.get("baseUrl").map(String::as_str))
        .unwrap_or("");
    let gw_base = render_str(raw_gw, &exec_req.variables, runtime);
    let token_url = join_url(&gw_base, TOKEN_PATH);
    let invoke_url = join_url(&gw_base, GATEWAY_INVOKE_PATH);

    let tenant_spec: Option<&TenantSpec> = match exec_req.mazbri.tenant_source.trim() {
        "inline" => exec_req.mazbri.inline_tenant.as_ref(),
        _ => {
            if let Some(pinned) = non_empty_str(exec_req.mazbri.tenant_name.as_deref()) {
                target.tenants.iter().find(|t| t.name.trim() == pinned)
            } else {
                let weighted: Vec<Weighted<&TenantSpec>> = target
                    .tenants
                    .iter()
                    .map(|t| Weighted {
                        weight: u64::try_from(t.weight.max(1)).unwrap_or(1),
                        value: t,
                    })
                    .collect();
                pick_weighted(runtime.seq, &weighted).copied()
            }
        }
    };

    let gw_req = build_gateway_request(prepared, runtime);
    let inner_json = serde_json::to_value(&gw_req).unwrap_or(Value::Null);
    let inner_pretty = serde_json::to_string_pretty(&inner_json).unwrap_or_default();

    let Some(tenant) = tenant_spec else {
        return ProbeResponse {
            protocol: "mazbri".into(),
            transport_ok: false,
            passed_assertions: false,
            http_status: None,
            status_text: None,
            latency_us: 0,
            response_bytes: 0,
            headers: Vec::new(),
            body_text: String::new(),
            body_json: None,
            error_class: Some("config".into()),
            error: Some("no Mazbri tenant available".into()),
            assertions: Vec::new(),
            request_preview: ProbeRequestPreview {
                method: "POST".into(),
                url: invoke_url,
                headers: Vec::new(),
                body_preview: Some(inner_pretty),
                unresolved_variables: unresolved,
            },
            mazbri_wire: None,
        };
    };

    let session = match tenant.settings().session() {
        Ok(s) => s,
        Err(err) => {
            return ProbeResponse {
                protocol: "mazbri".into(),
                transport_ok: false,
                passed_assertions: false,
                http_status: None,
                status_text: None,
                latency_us: 0,
                response_bytes: 0,
                headers: Vec::new(),
                body_text: String::new(),
                body_json: None,
                error_class: Some("config".into()),
                error: Some(format!("tenant '{}': {err}", tenant.name)),
                assertions: Vec::new(),
                request_preview: ProbeRequestPreview {
                    method: "POST".into(),
                    url: invoke_url,
                    headers: Vec::new(),
                    body_preview: Some(inner_pretty),
                    unresolved_variables: unresolved,
                },
                mazbri_wire: None,
            };
        }
    };

    // Step 1: Exchange API key for Bearer token at /auth/token
    let api_key = tenant.api_key.as_deref().unwrap_or("").trim();
    let tok_started = Instant::now();
    let tok_resp = client
        .post(&token_url)
        .json(&token_request_body(api_key))
        .send()
        .await;
    let tok_us = elapsed_us(tok_started);

    let issued = match tok_resp {
        Ok(resp) => {
            let st = resp.status().as_u16();
            match resp.bytes().await {
                Ok(raw) => parse_token_response(st, &raw).map_err(|e| e.to_string()),
                Err(e) => Err(format!("token response read failed: {e}")),
            }
        }
        Err(e) => Err(format!("token request failed: {e}")),
    };

    let issued = match issued {
        Ok(tok) => tok,
        Err(err_msg) => {
            let wire = MazbriWireDebug {
                tenant_name: tenant.name.clone(),
                encryption_mode: tenant.encryption_mode.clone(),
                suite: tenant.suite.clone(),
                data_alg: tenant.data_alg.clone(),
                mac_alg: tenant.mac_alg.clone(),
                binding_alg: tenant.binding_alg.clone(),
                binding_kid: tenant.binding_kid.clone(),
                token_url,
                token_exchange_us: tok_us,
                token_preview: None,
                token_expires_in: None,
                invoke_url: invoke_url.clone(),
                binding_headers: BTreeMap::new(),
                inner_gateway_request: inner_json,
                wire_request_body: String::new(),
                wire_response_status: None,
                wire_response_body: None,
                decoded_envelope: None,
            };
            return ProbeResponse {
                protocol: "mazbri".into(),
                transport_ok: false,
                passed_assertions: false,
                http_status: None,
                status_text: None,
                latency_us: tok_us,
                response_bytes: 0,
                headers: Vec::new(),
                body_text: String::new(),
                body_json: None,
                error_class: Some("token_refresh".into()),
                error: Some(err_msg),
                assertions: Vec::new(),
                request_preview: ProbeRequestPreview {
                    method: "POST".into(),
                    url: invoke_url,
                    headers: Vec::new(),
                    body_preview: Some(inner_pretty),
                    unresolved_variables: unresolved,
                },
                mazbri_wire: Some(wire),
            };
        }
    };

    // Step 2: Encode (seal + sign) GatewayRequest
    let call_started = Instant::now();
    let plaintext_bytes = serde_json::to_vec(&gw_req).unwrap_or_default();
    let encoded = match session.encode_request(&plaintext_bytes, unix_now_secs()) {
        Ok(enc) => enc,
        Err(err) => {
            return ProbeResponse {
                protocol: "mazbri".into(),
                transport_ok: false,
                passed_assertions: false,
                http_status: None,
                status_text: None,
                latency_us: elapsed_us(call_started),
                response_bytes: 0,
                headers: Vec::new(),
                body_text: String::new(),
                body_json: None,
                error_class: Some("crypto".into()),
                error: Some(format!("seal request: {err}")),
                assertions: Vec::new(),
                request_preview: ProbeRequestPreview {
                    method: "POST".into(),
                    url: invoke_url,
                    headers: Vec::new(),
                    body_preview: Some(inner_pretty),
                    unresolved_variables: unresolved,
                },
                mazbri_wire: None,
            };
        }
    };

    let wire_req_text = serde_json::from_slice::<Value>(&encoded.body)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| String::from_utf8_lossy(&encoded.body).into_owned());

    let mut binding_map = BTreeMap::new();
    for (k, v) in &encoded.headers {
        binding_map.insert((*k).to_string(), v.clone());
    }

    let probe_req_id = format!("probe-{}", runtime.uuid);
    let mut preview_headers = vec![
        (
            "Authorization".to_string(),
            format!("Bearer {}", mask_token(&issued.access_token)),
        ),
        ("Content-Type".to_string(), "application/json".to_string()),
        ("X-Request-Id".to_string(), probe_req_id.clone()),
    ];
    for (k, v) in &encoded.headers {
        preview_headers.push(((*k).to_string(), v.clone()));
    }

    let request_preview = ProbeRequestPreview {
        method: "POST".into(),
        url: invoke_url.clone(),
        headers: preview_headers,
        body_preview: Some(wire_req_text.clone()),
        unresolved_variables: unresolved,
    };

    let mut builder = client
        .post(&invoke_url)
        .bearer_auth(&issued.access_token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("X-Request-Id", &probe_req_id)
        .body(encoded.body.clone());
    if let Some(timeout) = prepared.timeout {
        builder = builder.timeout(timeout);
    }
    for (k, v) in &encoded.headers {
        builder = builder.header(*k, v);
    }

    let response = match builder.send().await {
        Ok(resp) => resp,
        Err(err) => {
            let wire = MazbriWireDebug {
                tenant_name: tenant.name.clone(),
                encryption_mode: tenant.encryption_mode.clone(),
                suite: tenant.suite.clone(),
                data_alg: tenant.data_alg.clone(),
                mac_alg: tenant.mac_alg.clone(),
                binding_alg: tenant.binding_alg.clone(),
                binding_kid: tenant.binding_kid.clone(),
                token_url,
                token_exchange_us: tok_us,
                token_preview: Some(mask_token(&issued.access_token)),
                token_expires_in: Some(issued.expires_in),
                invoke_url,
                binding_headers: binding_map,
                inner_gateway_request: inner_json,
                wire_request_body: wire_req_text,
                wire_response_status: None,
                wire_response_body: None,
                decoded_envelope: None,
            };
            return ProbeResponse {
                protocol: "mazbri".into(),
                transport_ok: false,
                passed_assertions: false,
                http_status: None,
                status_text: None,
                latency_us: elapsed_us(call_started),
                response_bytes: 0,
                headers: Vec::new(),
                body_text: String::new(),
                body_json: None,
                error_class: Some("transport".into()),
                error: Some(err.to_string()),
                assertions: Vec::new(),
                request_preview,
                mazbri_wire: Some(wire),
            };
        }
    };

    let http_status = response.status().as_u16();
    let status_text = response.status().canonical_reason().map(str::to_string);
    let resp_headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("<binary>").to_string(),
            )
        })
        .collect();

    let raw_bytes = match response.bytes().await {
        Ok(b) => b,
        Err(err) => {
            return ProbeResponse {
                protocol: "mazbri".into(),
                transport_ok: false,
                passed_assertions: false,
                http_status: Some(http_status),
                status_text,
                latency_us: elapsed_us(call_started),
                response_bytes: 0,
                headers: resp_headers,
                body_text: String::new(),
                body_json: None,
                error_class: Some("transport".into()),
                error: Some(err.to_string()),
                assertions: Vec::new(),
                request_preview,
                mazbri_wire: None,
            };
        }
    };

    let wire_resp_pretty = serde_json::from_slice::<Value>(&raw_bytes)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| String::from_utf8_lossy(&raw_bytes).into_owned());

    let decoded = session.decode_response(http_status, &raw_bytes);
    let latency_us = elapsed_us(call_started);
    let http_ok = (200..300).contains(&http_status);

    let (transport_ok, body_json, body_text, decoded_env_json, decode_err) = match decoded {
        Ok(envelope) => {
            let env_val = serde_json::to_value(&envelope).ok();
            let t_ok = http_ok && envelope.success;
            let data = envelope.data.clone();
            let text = if let Some(d) = data.as_ref() {
                serde_json::to_string_pretty(d).unwrap_or_default()
            } else if let Some(ev) = env_val.as_ref() {
                serde_json::to_string_pretty(ev).unwrap_or_default()
            } else {
                wire_resp_pretty.clone()
            };
            let rej = (!t_ok).then(|| {
                envelope.error.unwrap_or_else(|| {
                    format!(
                        "gateway call failed (HTTP {http_status}, upstream status {})",
                        envelope.status
                    )
                })
            });
            (t_ok, data, text, env_val, rej.map(|m| ("transport", m)))
        }
        Err(err) if http_ok => (
            false,
            None,
            wire_resp_pretty.clone(),
            None,
            Some((decode_error_class(&err), format!("decode response: {err}"))),
        ),
        Err(_) => (
            false,
            None,
            wire_resp_pretty.clone(),
            None,
            Some((
                "transport",
                format!("gateway rejected the call with HTTP {http_status}"),
            )),
        ),
    };

    let detailed_checks = eval_checks_detailed(body_json.as_ref(), &prepared.checks);
    let (passed_assertions, error_class, error) = if let Some((cls, msg)) = decode_err {
        if cls == "transport" {
            let j = judge(
                transport_ok,
                body_json.as_ref(),
                prepared.transport,
                &prepared.checks,
            );
            match j {
                Judge::Met => (true, None, None),
                Judge::TransportFailed => (false, Some("transport".into()), Some(msg)),
                Judge::Failed { class, message } => (false, Some(class.into()), Some(message)),
            }
        } else {
            (false, Some(cls.to_string()), Some(msg))
        }
    } else {
        let j = judge(
            transport_ok,
            body_json.as_ref(),
            prepared.transport,
            &prepared.checks,
        );
        match j {
            Judge::Met => (true, None, None),
            Judge::TransportFailed => (
                false,
                Some("transport".into()),
                Some("transport expectation was not met".into()),
            ),
            Judge::Failed { class, message } => (false, Some(class.into()), Some(message)),
        }
    };

    let wire = MazbriWireDebug {
        tenant_name: tenant.name.clone(),
        encryption_mode: tenant.encryption_mode.clone(),
        suite: tenant.suite.clone(),
        data_alg: tenant.data_alg.clone(),
        mac_alg: tenant.mac_alg.clone(),
        binding_alg: tenant.binding_alg.clone(),
        binding_kid: tenant.binding_kid.clone(),
        token_url,
        token_exchange_us: tok_us,
        token_preview: Some(mask_token(&issued.access_token)),
        token_expires_in: Some(issued.expires_in),
        invoke_url,
        binding_headers: binding_map,
        inner_gateway_request: inner_json,
        wire_request_body: wire_req_text,
        wire_response_status: Some(http_status),
        wire_response_body: Some(wire_resp_pretty),
        decoded_envelope: decoded_env_json,
    };

    ProbeResponse {
        protocol: "mazbri".into(),
        transport_ok,
        passed_assertions,
        http_status: Some(http_status),
        status_text,
        latency_us,
        response_bytes: raw_bytes.len(),
        headers: resp_headers,
        body_text,
        body_json,
        error_class,
        error,
        assertions: detailed_checks,
        request_preview,
        mazbri_wire: Some(wire),
    }
}

fn collect_unresolved(req: &ExecutionRequest, env_base_url: Option<&str>) -> Vec<String> {
    let mut missing = Vec::new();
    let mut scan = |s: &str| {
        for tok in unresolved_tokens(s, &req.variables) {
            if !missing.contains(&tok) {
                missing.push(tok);
            }
        }
    };
    if let Some(base) = env_base_url {
        scan(base);
    }
    scan(&req.endpoint);
    for p in &req.params {
        if p.active {
            scan(&p.key);
            scan(&p.value);
        }
    }
    for h in &req.headers {
        if h.active {
            scan(&h.key);
            scan(&h.value);
        }
    }
    if let Some(b) = req.body.body.as_deref() {
        scan(b);
    }
    for f in &req.body.form {
        if f.active {
            scan(&f.key);
            scan(&f.value);
        }
    }
    if req.protocol == PROTOCOL_MAZBRI {
        scan(&req.mazbri.product_id);
        if let Some(gw) = req.mazbri.gateway_base_url.as_deref() {
            scan(gw);
        }
    }
    missing
}

fn mask_token(token: &str) -> String {
    let trimmed = token.trim();
    if trimmed.len() <= 10 {
        "***".to_string()
    } else {
        format!("{}...{}", &trimmed[..6], &trimmed[trimmed.len() - 4..])
    }
}

fn mask_header_if_secret(key: &str, value: &str) -> String {
    if matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "authorization" | "proxy-authorization" | "x-api-key" | "api-key" | "cookie"
    ) {
        if let Some(rest) = value.strip_prefix("Bearer ") {
            format!("Bearer {}", mask_token(rest))
        } else {
            mask_token(value)
        }
    } else {
        value.to_string()
    }
}

fn elapsed_us(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_micros()).unwrap_or(i64::MAX)
}
