//! One-request execution: pick tenant/request, call the executor, classify.

use crate::capture;
use crate::db::Sample;
use crate::engine::metrics::OutcomeClass;
use crate::engine::transport::{ExecOutcome, RequestCtx, RequestExecutor, Weighted, pick_weighted};

/// Static description of a request item used when recording a sample.
#[derive(Debug, Clone)]
pub struct ProductChoice {
    pub request_key: String,
    pub product_id: String,
    pub product_name: Option<String>,
}

/// Static description of a tenant (credentials live in the executor).
#[derive(Debug, Clone)]
pub struct TenantChoice {
    pub name: String,
}

/// Inputs for a single execution (shared across a window).
pub struct ExecutePlan {
    pub products: Vec<Weighted<ProductChoice>>,
    pub tenants: Vec<Weighted<TenantChoice>>,
    pub expected_reasons: Vec<String>,
}

/// Outcome of [`execute_one`] ready to be recorded.
pub struct Executed {
    pub sample: Sample,
    pub class: OutcomeClass,
    /// Pre-send wait reported by the executor (see [`ExecOutcome::pre_send_wait_us`]).
    pub pre_send_wait_us: u64,
}

/// Run a single request and classify it.
pub async fn execute_one(
    plan: &ExecutePlan,
    executor: &dyn RequestExecutor,
    index: u64,
) -> Executed {
    let product = pick_weighted(index, &plan.products)
        .cloned()
        .unwrap_or(ProductChoice {
            request_key: "default".into(),
            product_id: "default".into(),
            product_name: None,
        });
    let tenant_name = pick_weighted(index.wrapping_mul(0x9E37_79B9_7F4A_7C15), &plan.tenants)
        .map(|tenant| tenant.name.clone());
    let ctx = RequestCtx {
        index,
        request_key: product.request_key.clone(),
        product_id: product.product_id.clone(),
        product_name: product.product_name.clone(),
        tenant_name: tenant_name.clone(),
    };
    classify(
        plan,
        product,
        tenant_name,
        executor.execute(&ctx).await,
        index,
    )
}

fn classify(
    plan: &ExecutePlan,
    product: ProductChoice,
    tenant_name: Option<String>,
    outcome: ExecOutcome,
    index: u64,
) -> Executed {
    let mut expected = outcome.expected;
    let mut class = if expected {
        OutcomeClass::Expected
    } else if outcome.transport_ok && outcome.error_class.is_none() {
        OutcomeClass::Success
    } else {
        OutcomeClass::Failure
    };
    if let Some(reason) = outcome.reason.as_deref()
        && plan
            .expected_reasons
            .iter()
            .any(|allowed| allowed == reason)
    {
        expected = true;
        class = OutcomeClass::Expected;
    }
    let success = matches!(class, OutcomeClass::Success);
    let customer_name = outcome.customer_name.or(tenant_name);
    let pre_send_wait_us = outcome.pre_send_wait_us;
    let protocol = if outcome.protocol.is_empty() {
        "http".to_string()
    } else {
        outcome.protocol
    };
    let sample = Sample {
        seq: None,
        start_unix_ms: outcome.start_unix_ms,
        end_unix_ms: outcome.end_unix_ms,
        latency_us: outcome.latency_us,
        protocol,
        http_method: outcome.http_method,
        product_id: product.product_id,
        product_name: product.product_name,
        request_index: i64::try_from(index).ok(),
        http_status: outcome.http_status,
        success,
        expected,
        reason: outcome.reason,
        error_class: outcome.error_class,
        request_id: outcome.request_id,
        error: outcome.error,
        request_bytes: outcome.request_bytes,
        response_bytes: outcome.response_bytes,
        queue_us: None,
        ttfb_us: outcome.ttfb_us,
        transfer_us: outcome.transfer_us,
        crypto_seal_us: outcome.crypto_seal_us,
        crypto_unseal_us: outcome.crypto_unseal_us,
        token_wait_us: outcome.token_wait_us,
        customer_name,
        trace_id: outcome.trace_id,
        trace_echo: outcome.trace_echo,
        warmup: false,
        captures: capture::to_json(&outcome.captures),
        response_body: if matches!(class, OutcomeClass::Failure) {
            outcome.failure_body
        } else {
            None
        },
    };
    let _ = outcome.body;
    Executed {
        sample,
        class,
        pre_send_wait_us,
    }
}
