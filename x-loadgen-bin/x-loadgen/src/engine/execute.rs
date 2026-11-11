//! One-request execution: pick tenant/product, call the executor, classify.

use crate::db::Sample;
use crate::engine::metrics::OutcomeClass;
use crate::engine::transport::{ExecOutcome, RequestCtx, RequestExecutor, Weighted, pick_weighted};

/// Static description of a product used when recording a sample.
#[derive(Debug, Clone)]
pub struct ProductChoice {
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
    let tenant_name = pick_weighted(index.wrapping_mul(0x9E37_79B9_7F4A_7C15), &plan.tenants)
        .map(|tenant| tenant.name.clone());
    let ctx = RequestCtx {
        index,
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
