//! One-request execution: pick tenant/product, refresh token, call the executor.

use std::sync::Arc;

use crate::db::Sample;
use crate::engine::metrics::{OutcomeClass, unix_now_ms};
use crate::engine::token::{TokenBroker, TokenError};
use crate::engine::transport::{ExecOutcome, RequestCtx, RequestExecutor, Weighted, pick_weighted};

/// Static description of a product used when recording a sample.
#[derive(Debug, Clone)]
pub struct ProductChoice {
    pub product_id: String,
    pub product_name: Option<String>,
}

/// Static description of a tenant; `needs_token` enables the plaintext broker.
#[derive(Debug, Clone)]
pub struct TenantChoice {
    pub name: String,
    pub needs_token: bool,
}

/// Inputs for a single execution (shared across a window).
pub struct ExecutePlan {
    pub products: Vec<Weighted<ProductChoice>>,
    pub tenants: Vec<Weighted<TenantChoice>>,
    pub expected_reasons: Vec<String>,
    pub token: Option<Arc<TokenBroker>>,
}

/// Outcome of [`execute_one`] ready to be recorded.
pub struct Executed {
    pub sample: Sample,
    pub class: OutcomeClass,
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
            product_id: "default".into(),
            product_name: None,
        });
    let tenant = pick_weighted(index.wrapping_mul(0x9E37_79B9_7F4A_7C15), &plan.tenants);
    let needs_token = tenant.is_some_and(|t| t.needs_token);
    let tenant_name = tenant.map(|t| t.name.clone());

    if needs_token && let Some(broker) = plan.token.as_ref() {
        match broker.get().await {
            Ok(token) => {
                let ctx = RequestCtx {
                    index,
                    product_id: product.product_id.clone(),
                    product_name: product.product_name.clone(),
                    tenant_name: tenant_name.clone(),
                    token: Some(token),
                };
                return classify(
                    plan,
                    product,
                    tenant_name,
                    executor.execute(&ctx).await,
                    index,
                );
            }
            Err(TokenError(message)) => {
                let now = unix_now_ms();
                let sample = Sample {
                    seq: None,
                    start_unix_ms: now,
                    end_unix_ms: now,
                    latency_us: 0,
                    product_id: product.product_id,
                    product_name: product.product_name,
                    request_index: i64::try_from(index).ok(),
                    http_status: None,
                    success: false,
                    expected: false,
                    reason: Some("token_refresh".into()),
                    error_class: Some("token_refresh".into()),
                    request_id: None,
                    error: Some(message),
                    response_bytes: None,
                    customer_name: tenant_name,
                };
                return Executed {
                    sample,
                    class: OutcomeClass::Failure,
                };
            }
        }
    }

    let ctx = RequestCtx {
        index,
        product_id: product.product_id.clone(),
        product_name: product.product_name.clone(),
        tenant_name: tenant_name.clone(),
        token: None,
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
    } else if outcome.transport_ok {
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
    let sample = Sample {
        seq: None,
        start_unix_ms: outcome.start_unix_ms,
        end_unix_ms: outcome.end_unix_ms,
        latency_us: outcome.latency_us,
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
        response_bytes: outcome.response_bytes,
        customer_name: tenant_name,
    };
    let _ = outcome.body;
    Executed { sample, class }
}
