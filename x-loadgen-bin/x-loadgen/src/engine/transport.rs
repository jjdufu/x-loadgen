//! Tenant/product mixing and the request executor interface.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

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
    pub token: Option<String>,
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

/// Executes one HTTP (or mock) request.
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
