//! Report breakdowns computed from the stored samples: percentiles per product
//! and per tenant, the HTTP status distribution, and the most frequent errors.
//!
//! Warmup samples are left out like everywhere else. Percentiles use the raw
//! on-the-wire latency of the recorded samples (no coordinated-omission
//! correction, unlike the run-level percentiles), and samples dropped by the
//! bounded detail writer are not counted.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use hdrhistogram::Histogram;
use serde_json::{Value, json};

use crate::db::{DataPaths, open_existing_worker};
use crate::engine::new_latency_histogram;
use crate::report::redact::scrub_text;

/// Most frequent errors listed in the report.
pub const TOP_ERRORS: usize = 10;
/// Tenant key for samples without a tenant.
const NO_TENANT: &str = "(none)";
/// Status key for samples without an HTTP status (never reached the target).
const NO_STATUS: &str = "none";

struct Group {
    samples: u64,
    success: u64,
    failure: u64,
    expected: u64,
    hist: Histogram<u64>,
}

impl Group {
    fn new() -> Result<Self> {
        Ok(Self {
            samples: 0,
            success: 0,
            failure: 0,
            expected: 0,
            hist: new_latency_histogram(60_000_000)?,
        })
    }

    fn add(&mut self, row: &Row) {
        self.samples += 1;
        if row.expected {
            self.expected += 1;
        } else if row.success {
            self.success += 1;
        } else {
            self.failure += 1;
        }
        if let Ok(us) = u64::try_from(row.latency_us)
            && us > 0
            && self.hist.record(us).is_err()
        {
            self.hist.saturating_record(us);
        }
    }

    fn to_json(&self, key_name: &str, key: &str) -> Value {
        let pct = |q: f64| (!self.hist.is_empty()).then(|| self.hist.value_at_quantile(q));
        let mut out = json!({
            "samples": self.samples,
            "success": self.success,
            "failure": self.failure,
            "expected": self.expected,
            "p50_us": pct(0.50),
            "p95_us": pct(0.95),
            "p99_us": pct(0.99),
            "max_us": (!self.hist.is_empty()).then(|| self.hist.max()),
        });
        out[key_name] = Value::String(key.to_string());
        out
    }
}

struct Row {
    product_id: String,
    tenant: Option<String>,
    latency_us: i64,
    success: bool,
    expected: bool,
    http_status: Option<i64>,
    error_class: Option<String>,
    error: Option<String>,
}

#[derive(Default)]
struct ErrorCount {
    count: u64,
    status: BTreeMap<String, u64>,
}

/// Breakdown JSON for one run (`report.breakdown`).
///
/// # Errors
///
/// Returns an error when a worker database cannot be read.
pub fn build_breakdown(paths: &DataPaths, run_id: &str, secrets: &[String]) -> Result<Value> {
    let mut products: BTreeMap<String, Group> = BTreeMap::new();
    let mut tenants: BTreeMap<String, Group> = BTreeMap::new();
    let mut statuses: BTreeMap<String, u64> = BTreeMap::new();
    let mut errors: HashMap<(String, String), ErrorCount> = HashMap::new();
    let mut total = 0_u64;
    for worker in paths.worker_indexes(run_id) {
        let Some(conn) = open_existing_worker(paths, run_id, worker, true)? else {
            continue;
        };
        let mut stmt = conn
            .prepare(
                "SELECT product_id, customer_name, latency_us, success, expected, http_status,
                        error_class, error
                 FROM samples WHERE warmup = 0",
            )
            .context("prepare breakdown scan")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Row {
                    product_id: row.get(0)?,
                    tenant: row.get(1)?,
                    latency_us: row.get(2)?,
                    success: row.get::<_, i64>(3)? != 0,
                    expected: row.get::<_, i64>(4)? != 0,
                    http_status: row.get(5)?,
                    error_class: row.get(6)?,
                    error: row.get(7)?,
                })
            })
            .context("scan samples for breakdown")?;
        for row in rows {
            let row = row.context("read sample for breakdown")?;
            total += 1;
            if !products.contains_key(&row.product_id) {
                products.insert(row.product_id.clone(), Group::new()?);
            }
            if let Some(group) = products.get_mut(&row.product_id) {
                group.add(&row);
            }
            let tenant = row.tenant.as_deref().unwrap_or(NO_TENANT);
            if !tenants.contains_key(tenant) {
                tenants.insert(tenant.to_string(), Group::new()?);
            }
            if let Some(group) = tenants.get_mut(tenant) {
                group.add(&row);
            }
            let status = row
                .http_status
                .map_or_else(|| NO_STATUS.to_string(), |s| s.to_string());
            *statuses.entry(status.clone()).or_default() += 1;
            if !row.success && !row.expected {
                let class = row.error_class.clone().unwrap_or_else(|| "unknown".into());
                let message = row
                    .error
                    .as_deref()
                    .map(|text| scrub_text(text, secrets))
                    .unwrap_or_default();
                let entry = errors.entry((class, message)).or_default();
                entry.count += 1;
                *entry.status.entry(status).or_default() += 1;
            }
        }
    }
    let mut top: Vec<_> = errors.into_iter().collect();
    top.sort_by(|a, b| b.1.count.cmp(&a.1.count).then_with(|| a.0.cmp(&b.0)));
    let top_errors: Vec<Value> = top
        .into_iter()
        .take(TOP_ERRORS)
        .map(|((class, message), counted)| {
            json!({
                "error_class": class,
                "error": message,
                "count": counted.count,
                "http_status": counted.status,
            })
        })
        .collect();
    Ok(json!({
        "basis": "recorded_samples",
        "samples": total,
        "products": products.iter().map(|(k, g)| g.to_json("product_id", k)).collect::<Vec<_>>(),
        "tenants": tenants.iter().map(|(k, g)| g.to_json("tenant", k)).collect::<Vec<_>>(),
        "status_codes": statuses,
        "top_errors": top_errors,
    }))
}
