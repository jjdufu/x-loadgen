//! mazbri gateway protocol: token exchange, sealed gateway calls, decoded judging.
//!
//! Per tenant the executor keeps one [`TokenBroker`] (`POST {base}/auth/token`,
//! single-flight, refreshed `token_refresh_skew_secs` before expiry) and one
//! [`GatewaySession`] built from the page-entered settings. Each request:
//!
//! 1. renders the product into a [`GatewayRequest`];
//! 2. seals it (`request` / `full`) and adds `x-mazbri-*` binding headers;
//! 3. sends `POST {base}/api/v1/gateway` with `Authorization: Bearer`;
//! 4. decodes the response (opening the envelope under `full`) and judges the
//!    decrypted `data`. Latency covers sealing, the call, and decoding.
//!
//! Error classes: `token_refresh` (no token), `crypto` (seal / open / signature
//! failure), `protocol` (response does not match the contract), `transport`
//! (network or gateway rejection).

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

use crate::db::targets::Tenant;
use crate::engine::metrics::unix_now_ms;
use crate::engine::token::{CachedToken, RefreshFn, TokenBroker, TokenError, box_refresh};
use crate::engine::transport::{
    ExecOutcome, HttpExecutor, PreparedProduct, RequestCtx, apply_judge, apply_seq, elapsed_us,
    failed_outcome, join_url,
};
use crate::expect::judge;

/// Shortest token lifetime used after subtracting the refresh skew.
const MIN_TOKEN_LIFETIME: Duration = Duration::from_secs(1);

/// Credentials and codec for one mazbri tenant.
pub struct MazbriTenant {
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
    pub fn new(client: &Client, base_url: &str, tenant: &Tenant, skew: Duration) -> Result<Self> {
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
            session,
            broker: TokenBroker::new(refresh),
        })
    }

    /// Execute one product call through the gateway.
    pub async fn execute(
        &self,
        executor: &HttpExecutor,
        ctx: &RequestCtx,
        product: &PreparedProduct,
        request_id: Option<String>,
    ) -> ExecOutcome {
        let token = match self.broker.get().await {
            Ok(token) => token,
            Err(TokenError(message)) => {
                let now = unix_now_ms();
                return failed_outcome(
                    now,
                    now,
                    0,
                    request_id,
                    Some("token_refresh".into()),
                    Some(message),
                    None,
                );
            }
        };
        let started = unix_now_ms();
        let sent = Instant::now();
        let fail = |class: &str, message: String, status: Option<i64>| {
            failed_outcome(
                started,
                unix_now_ms(),
                elapsed_us(sent),
                request_id.clone(),
                Some(class.into()),
                Some(message),
                status,
            )
        };
        let request = match self.build_call(executor, ctx, product, &token, request_id.as_deref()) {
            Ok(request) => request,
            Err((class, message)) => return fail(class, message, None),
        };
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => return fail("transport", error.to_string(), None),
        };
        let http_status = response.status().as_u16();
        if http_status == 401 {
            self.broker.invalidate(&token).await;
        }
        let raw = match response.bytes().await {
            Ok(raw) => raw,
            Err(error) => {
                return fail("transport", error.to_string(), Some(i64::from(http_status)));
            }
        };
        let decoded = self.session.decode_response(http_status, &raw);
        let mut outcome = ExecOutcome {
            start_unix_ms: started,
            end_unix_ms: unix_now_ms(),
            latency_us: elapsed_us(sent),
            http_status: Some(i64::from(http_status)),
            transport_ok: false,
            expected: false,
            reason: None,
            error_class: None,
            error: None,
            response_bytes: i64::try_from(raw.len()).ok(),
            request_id,
            body: None,
        };
        judge_decoded(&mut outcome, decoded, http_status, product);
        outcome
    }
}

impl MazbriTenant {
    /// Render, seal, and sign one gateway call.
    fn build_call(
        &self,
        executor: &HttpExecutor,
        ctx: &RequestCtx,
        product: &PreparedProduct,
        token: &str,
        request_id: Option<&str>,
    ) -> Result<reqwest::RequestBuilder, (&'static str, String)> {
        let plaintext = serde_json::to_vec(&gateway_request(product, ctx.index))
            .map_err(|error| ("config", format!("gateway request: {error}")))?;
        let encoded = self
            .session
            .encode_request(&plaintext, unix_now_secs())
            .map_err(|error| ("crypto", format!("seal request: {error}")))?;
        let mut request = executor
            .client
            .post(join_url(&executor.base_url, GATEWAY_INVOKE_PATH))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(encoded.body);
        for (name, value) in encoded.headers {
            request = request.header(name, value);
        }
        if let Some(id) = request_id {
            request = request.header("X-Request-Id", id);
        }
        if let Some(timeout) = product.timeout {
            request = request.timeout(timeout);
        }
        Ok(request)
    }
}

/// Fill `outcome` from the decoded response and run the product expectations.
fn judge_decoded(
    outcome: &mut ExecOutcome,
    decoded: Result<GatewayResponseEnvelope, SdkError>,
    http_status: u16,
    product: &PreparedProduct,
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
        product.transport,
        &product.checks,
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
                .max(MIN_TOKEN_LIFETIME);
            Ok(CachedToken {
                value: issued.access_token,
                expires_at: Instant::now() + lifetime,
            })
        }
    })
}

/// Render a product into the gateway request body for request `index`.
fn gateway_request(product: &PreparedProduct, index: u64) -> GatewayRequest {
    let render_map = |pairs: &[(String, String)]| {
        (!pairs.is_empty()).then(|| {
            pairs
                .iter()
                .map(|(key, value)| (key.clone(), apply_seq(value, index)))
                .collect::<BTreeMap<_, _>>()
        })
    };
    let body = product.body.as_deref().map(|text| {
        let rendered = apply_seq(text, index);
        if product.body_is_json {
            serde_json::from_str(&rendered).unwrap_or(Value::String(rendered))
        } else {
            Value::String(rendered)
        }
    });
    GatewayRequest {
        product_id: product.product_id.clone(),
        path: product.path.clone(),
        method: product.method.as_str().to_string(),
        query: render_map(&product.query),
        headers: render_map(&product.headers),
        body,
    }
}

fn rejection_message(envelope: &GatewayResponseEnvelope, http_status: u16) -> String {
    envelope.error.clone().unwrap_or_else(|| {
        format!(
            "gateway call failed (HTTP {http_status}, upstream status {})",
            envelope.status
        )
    })
}

fn decode_error_class(error: &SdkError) -> &'static str {
    match error {
        SdkError::Crypto(_) | SdkError::LegacyDenied(_) | SdkError::UnsupportedAlgorithm(_) => {
            "crypto"
        }
        _ => "protocol",
    }
}
