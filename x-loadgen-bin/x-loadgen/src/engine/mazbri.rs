//! mazbri gateway protocol: token exchange, sealed gateway calls, decoded judging.
//!
//! Per tenant the executor keeps one [`TokenBroker`] (`POST {base}/auth/token`,
//! single-flight, refreshed `token_refresh_skew_secs` before expiry) and one
//! [`GatewaySession`] built from the page-entered settings. Each request:
//!
//! 1. renders the request into a [`GatewayRequest`] (with Hoppscotch `<<var>>` / `{{seq}}` interpolation);
//! 2. seals it (`request` / `full`) and adds `x-mazbri-*` binding headers;
//! 3. sends `POST {base}/api/v1/gateway` with `Authorization: Bearer`;
//! 4. decodes the response (opening the envelope under `full`) and judges the
//!    decrypted `data`. Latency covers sealing, the call, and decoding.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde_json::Value;
use x_loadgen_core::crypto::request_binding::unix_now_secs;
use x_loadgen_sdk::{
    Error as SdkError, GATEWAY_INVOKE_PATH, GatewayRequest, GatewayResponseEnvelope,
    GatewaySession, TOKEN_PATH, parse_token_response, token_request_body,
};

use crate::capture::Captures;
use crate::db::workspace::TenantSpec;
use crate::engine::metrics::unix_now_ms;
use crate::engine::token::{CachedToken, RefreshFn, TokenBroker, TokenError, box_refresh};
use crate::engine::transport::{
    ExecOutcome, PreparedRequest, RequestIds, active_kv_pairs, apply_judge, elapsed_us,
    failed_outcome, join_url, response_request_id, urlencoding_lite,
};
use crate::expect::judge;
use crate::vars::{RuntimeVars, render_str};

/// Shortest token lifetime used after subtracting the refresh skew.
const MIN_TOKEN_LIFETIME: Duration = Duration::from_secs(1);
/// Upper bound on a cached token lifetime (guards `Instant` overflow).
const MAX_TOKEN_LIFETIME: Duration = Duration::from_hours(24);

/// Credentials and codec for one mazbri tenant against one gateway URL.
pub struct MazbriTenant {
    invoke_url: String,
    session: GatewaySession,
    broker: TokenBroker,
}

impl MazbriTenant {
    /// Build the session (validating every field) and the token broker.
    ///
    /// # Errors
    ///
    /// Returns an error naming the tenant when its settings are invalid or the
    /// `api_key` is missing; key material never appears in the message.
    pub fn new(
        client: &Client,
        base_url: &str,
        tenant: &TenantSpec,
        skew: Duration,
    ) -> Result<Self> {
        let session = tenant
            .settings()
            .session()
            .map_err(|error| anyhow!("tenant {}: {error}", tenant.name))?;
        let api_key = tenant
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .with_context(|| format!("tenant {} has no api_key", tenant.name))?
            .to_string();
        let refresh = token_refresher(
            client.clone(),
            join_url(base_url, TOKEN_PATH),
            api_key,
            skew,
        );
        Ok(Self {
            invoke_url: join_url(base_url, GATEWAY_INVOKE_PATH),
            session,
            broker: TokenBroker::new(refresh),
        })
    }

    /// Execute one request call through the gateway.
    pub async fn execute(
        &self,
        client: &Client,
        req: &PreparedRequest,
        ids: &RequestIds,
        runtime: &RuntimeVars,
        customer_name: Option<String>,
    ) -> ExecOutcome {
        let token_wait = Instant::now();
        let token = match self.broker.get().await {
            Ok(token) => token,
            Err(TokenError(message)) => {
                let now = unix_now_ms();
                return failed_outcome(
                    now,
                    now,
                    0,
                    ids,
                    Some("token_refresh".into()),
                    Some(message),
                    None,
                    customer_name,
                );
            }
        };
        // A single-flight refresh can stall every caller; charge that wait as
        // queueing delay (it is not part of the wire latency itself).
        let pre_send_wait_us = u64::try_from(token_wait.elapsed().as_micros()).unwrap_or(u64::MAX);
        let mut outcome = self
            .execute_with_token(client, req, &token, ids, runtime, customer_name)
            .await;
        outcome.pre_send_wait_us = pre_send_wait_us;
        outcome
    }

    async fn execute_with_token(
        &self,
        client: &Client,
        req: &PreparedRequest,
        token: &str,
        ids: &RequestIds,
        runtime: &RuntimeVars,
        customer_name: Option<String>,
    ) -> ExecOutcome {
        let started = unix_now_ms();
        let request = match self.build_call(client, req, token, ids, runtime) {
            Ok(request) => request,
            // Never reached the wire: latency 0 keeps it out of the histogram.
            Err((class, message)) => {
                return failed_outcome(
                    started,
                    unix_now_ms(),
                    0,
                    ids,
                    Some(class.into()),
                    Some(message),
                    None,
                    customer_name,
                );
            }
        };
        // Start the clock after sealing/signing (client-side CPU work).
        let started = unix_now_ms();
        let sent = Instant::now();
        let fail = |class: &str, message: String, status: Option<i64>| {
            failed_outcome(
                started,
                unix_now_ms(),
                elapsed_us(sent),
                ids,
                Some(class.into()),
                Some(message),
                status,
                customer_name.clone(),
            )
        };
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => return fail("transport", error.to_string(), None),
        };
        let http_status = response.status().as_u16();
        if http_status == 401 {
            self.broker.invalidate(token).await;
        }
        let trace_echo = ids.trace_echo(response.headers());
        let request_id = ids
            .request_id
            .clone()
            .or_else(|| response_request_id(response.headers()));
        let raw = match response.bytes().await {
            Ok(raw) => raw,
            Err(error) => {
                let mut failed = fail("transport", error.to_string(), Some(i64::from(http_status)));
                failed.request_id = request_id;
                failed.trace_echo = trace_echo;
                return failed;
            }
        };
        // Stop the clock before decrypting/decoding the response.
        let latency_us = elapsed_us(sent);
        let end_unix_ms = unix_now_ms();
        let decoded = self.session.decode_response(http_status, &raw);
        let mut outcome = ExecOutcome {
            start_unix_ms: started,
            end_unix_ms,
            latency_us,
            http_status: Some(i64::from(http_status)),
            transport_ok: false,
            expected: false,
            reason: None,
            error_class: None,
            error: None,
            response_bytes: i64::try_from(raw.len()).ok(),
            request_id,
            trace_id: Some(ids.trace_id()),
            trace_echo,
            captures: Captures::new(),
            body: None,
            customer_name,
            pre_send_wait_us: 0,
        };
        judge_decoded(&mut outcome, decoded, http_status, req);
        outcome
    }

    /// Render, seal, and sign one gateway call.
    fn build_call(
        &self,
        client: &Client,
        req: &PreparedRequest,
        token: &str,
        ids: &RequestIds,
        runtime: &RuntimeVars,
    ) -> Result<reqwest::RequestBuilder, (&'static str, String)> {
        let gw_req = build_gateway_request(req, runtime);
        let plaintext = serde_json::to_vec(&gw_req)
            .map_err(|error| ("config", format!("gateway request: {error}")))?;
        let encoded = self
            .session
            .encode_request(&plaintext, unix_now_secs())
            .map_err(|error| ("crypto", format!("seal request: {error}")))?;
        let mut request = client
            .post(&self.invoke_url)
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(encoded.body);
        for (name, value) in encoded.headers {
            request = request.header(name, value);
        }
        for (name, value) in ids.headers() {
            request = request.header(name, value);
        }
        if let Some(timeout) = req.timeout {
            request = request.timeout(timeout);
        }
        Ok(request)
    }
}

/// Render a [`PreparedRequest`] into a [`GatewayRequest`] for `runtime`.
pub(crate) fn build_gateway_request(
    req: &PreparedRequest,
    runtime: &RuntimeVars,
) -> GatewayRequest {
    let render_map = |pairs: &[(String, String)]| {
        (!pairs.is_empty()).then(|| {
            pairs
                .iter()
                .map(|(key, value)| {
                    (
                        render_str(key, &req.variables, runtime),
                        render_str(value, &req.variables, runtime),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
    };

    let ct = req
        .body
        .content_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"));

    let body = match ct {
        Some("application/x-www-form-urlencoded") => {
            let active_form = active_kv_pairs(&req.body.form);
            if active_form.is_empty() {
                req.body
                    .body
                    .as_deref()
                    .map(|raw| Value::String(render_str(raw, &req.variables, runtime)))
            } else {
                let encoded = active_form
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}={}",
                            urlencoding_lite(&render_str(k, &req.variables, runtime)),
                            urlencoding_lite(&render_str(v, &req.variables, runtime))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("&");
                Some(Value::String(encoded))
            }
        }
        Some("application/json") | None => {
            req.body
                .body
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|text| {
                    let rendered = render_str(text, &req.variables, runtime);
                    serde_json::from_str(&rendered).unwrap_or(Value::String(rendered))
                })
        }
        Some(_) => req
            .body
            .body
            .as_deref()
            .map(|text| Value::String(render_str(text, &req.variables, runtime))),
    };

    let raw_ep = render_str(&req.endpoint_template, &req.variables, runtime);
    let path = if raw_ep.is_empty() {
        "/".to_string()
    } else {
        raw_ep
    };
    let product_id = render_str(&req.product_id_template, &req.variables, runtime);

    GatewayRequest {
        product_id,
        path,
        method: req.method.as_str().to_string(),
        query: render_map(&req.params),
        headers: render_map(&req.headers),
        body,
    }
}

/// Fill `outcome` from the decoded response and run the request expectations.
fn judge_decoded(
    outcome: &mut ExecOutcome,
    decoded: Result<GatewayResponseEnvelope, SdkError>,
    http_status: u16,
    req: &PreparedRequest,
) {
    let http_ok = (200..300).contains(&http_status);
    let rejection = match decoded {
        Ok(envelope) => {
            outcome.transport_ok = http_ok && envelope.success;
            if outcome.request_id.is_none() {
                outcome.request_id.clone_from(&envelope.request_id);
            }
            let message = rejection_message(&envelope, http_status);
            outcome.body = envelope.data;
            message
        }
        Err(error) if http_ok => {
            outcome.error_class = Some(decode_error_class(&error).into());
            outcome.error = Some(format!("decode response: {error}"));
            return;
        }
        Err(_) => format!("gateway rejected the call with HTTP {http_status}"),
    };
    let judgement = judge(
        outcome.transport_ok,
        outcome.body.as_ref(),
        req.transport,
        &req.checks,
    );
    apply_judge(outcome, judgement);
    if !outcome.transport_ok && outcome.error_class.as_deref() == Some("transport") {
        outcome.error = Some(rejection);
    }
}

fn token_refresher(client: Client, url: String, api_key: String, skew: Duration) -> RefreshFn {
    box_refresh(move || {
        let client = client.clone();
        let url = url.clone();
        let body = token_request_body(&api_key);
        async move {
            let response = client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|error| TokenError(format!("token request failed: {error}")))?;
            let status = response.status().as_u16();
            let raw = response
                .bytes()
                .await
                .map_err(|error| TokenError(format!("token response failed: {error}")))?;
            let issued = parse_token_response(status, &raw)
                .map_err(|error| TokenError(error.to_string()))?;
            let lifetime = Duration::from_secs(issued.expires_in)
                .saturating_sub(skew)
                .max(MIN_TOKEN_LIFETIME)
                // A huge `expires_in` would overflow `Instant + Duration` and panic.
                .min(MAX_TOKEN_LIFETIME);
            Ok(CachedToken {
                value: issued.access_token,
                expires_at: Instant::now() + lifetime,
            })
        }
    })
}

fn rejection_message(envelope: &GatewayResponseEnvelope, http_status: u16) -> String {
    envelope.error.clone().unwrap_or_else(|| {
        format!(
            "gateway call failed (HTTP {http_status}, upstream status {})",
            envelope.status
        )
    })
}

pub(crate) fn decode_error_class(error: &SdkError) -> &'static str {
    match error {
        SdkError::Crypto(_) | SdkError::LegacyDenied(_) | SdkError::UnsupportedAlgorithm(_) => {
            "crypto"
        }
        _ => "protocol",
    }
}
