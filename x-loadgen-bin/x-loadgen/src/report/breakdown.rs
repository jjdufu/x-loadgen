//! Report breakdowns computed from stored samples:
//! - Per-endpoint (product) & per-tenant full latency percentiles, QPS, error rates, bytes
//! - HTTP status code & family distribution + Top N errors
//! - Latency distribution histogram with CDF & Apdex satisfaction buckets
//! - Per-second exact latency percentiles & error classes (for time-series enrichment)
//! - Protocol-specific phase timing breakdowns (HTTP generic vs Mazbri gateway)
//!
//! Warmup samples are excluded everywhere.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result};
use hdrhistogram::Histogram;
use serde_json::{Value, json};

use crate::db::{DataPaths, open_existing_worker};
use crate::engine::{LatencyStats, new_latency_histogram};
use crate::report::redact::scrub_text;

/// Most frequent errors listed in the report.
pub const TOP_ERRORS: usize = 10;
const NO_TENANT: &str = "(none)";
const NO_STATUS: &str = "none";

/// Fixed upper bounds (in microseconds) for the latency distribution histogram.
const HIST_EDGES_US: [u64; 18] = [
    1_000,      // 1ms
    2_000,      // 2ms
    5_000,      // 5ms
    10_000,     // 10ms
    20_000,     // 20ms
    35_000,     // 35ms
    50_000,     // 50ms
    75_000,     // 75ms
    100_000,    // 100ms
    150_000,    // 150ms
    250_000,    // 250ms
    500_000,    // 500ms
    1_000_000,  // 1s
    2_000_000,  // 2s
    5_000_000,  // 5s
    10_000_000, // 10s
    30_000_000, // 30s
    60_000_000, // 60s
];

const SIZE_EDGES: [(u64, &str); 6] = [
    (0, "0 B"),
    (1_024, "< 1 KB"),
    (10 * 1_024, "1–10 KB"),
    (100 * 1_024, "10–100 KB"),
    (1_024 * 1_024, "100 KB–1 MB"),
    (u64::MAX, "> 1 MB"),
];

/// Exact per-second stats aggregated from recorded samples.
#[derive(Debug, Clone, Default)]
pub struct PerSecondSampleStats {
    pub avg_us: Option<u64>,
    pub p50_us: Option<u64>,
    pub p90_us: Option<u64>,
    pub p95_us: Option<u64>,
    pub p99_us: Option<u64>,
    pub max_us: Option<u64>,
    pub error_classes: BTreeMap<String, u64>,
}

/// Complete breakdown bundle returned to `report::build_report`.
pub struct BreakdownOutput {
    pub breakdown_json: Value,
    pub per_second: BTreeMap<u64, PerSecondSampleStats>,
    pub apdex_json: Value,
    pub total_request_bytes: u64,
    pub total_response_bytes: u64,
    pub protocols: Vec<String>,
}

/// Inputs from the run snapshot needed for protocol/Apdex metadata.
pub struct BreakdownContext<'a> {
    pub effective_elapsed_secs: Option<f64>,
    pub apdex_satisfied_ms: f64,
    pub snapshot_redacted: &'a Value,
}

struct PhaseAcc {
    samples: u64,
    sum_us: u64,
    hist: Histogram<u64>,
}

impl PhaseAcc {
    fn new() -> Result<Self> {
        Ok(Self {
            samples: 0,
            sum_us: 0,
            hist: new_latency_histogram(60_000_000)?,
        })
    }

    fn record(&mut self, val: Option<i64>) {
        if let Some(raw) = val
            && raw >= 0
            && let Ok(us) = u64::try_from(raw)
        {
            self.samples += 1;
            self.sum_us = self.sum_us.saturating_add(us);
            let rec = us.max(1);
            if self.hist.record(rec).is_err() {
                self.hist.saturating_record(rec);
            }
        }
    }

    fn avg_us(&self) -> Option<u64> {
        (self.samples > 0).then(|| self.sum_us / self.samples)
    }

    fn to_json(&self, share_base_us: f64) -> Value {
        let stats = LatencyStats::from_histogram(&self.hist);
        let avg = self.avg_us();
        let share_pct = match (avg, share_base_us > 0.0) {
            (Some(a), true) => Some(round2((u64_to_f64(a) / share_base_us) * 100.0)),
            _ => None,
        };
        json!({
            "samples": self.samples,
            "avg_us": avg,
            "p50_us": stats.p50_us,
            "p95_us": stats.p95_us,
            "p99_us": stats.p99_us,
            "max_us": stats.max_us,
            "share_pct": share_pct,
        })
    }
}

struct Group {
    protocol: String,
    method: Option<String>,
    samples: u64,
    success: u64,
    failure: u64,
    expected: u64,
    total_bytes: u64,
    bytes_count: u64,
    hist: Histogram<u64>,
}

impl Group {
    fn new(protocol: String, method: Option<String>) -> Result<Self> {
        Ok(Self {
            protocol,
            method,
            samples: 0,
            success: 0,
            failure: 0,
            expected: 0,
            total_bytes: 0,
            bytes_count: 0,
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
        if let Some(b) = row
            .response_bytes
            .and_then(|v| u64::try_from(v.max(0)).ok())
        {
            self.total_bytes = self.total_bytes.saturating_add(b);
            self.bytes_count += 1;
        }
        if let Ok(us) = u64::try_from(row.latency_us)
            && us > 0
            && self.hist.record(us).is_err()
        {
            self.hist.saturating_record(us);
        }
    }

    fn to_json(
        &self,
        key_name: &str,
        key: &str,
        total_samples: u64,
        elapsed_secs: Option<f64>,
    ) -> Value {
        let stats = LatencyStats::from_histogram(&self.hist);
        let share_pct = if total_samples > 0 {
            round2((u64_to_f64(self.samples) / u64_to_f64(total_samples)) * 100.0)
        } else {
            0.0
        };
        let error_rate = if self.samples > 0 {
            round4(u64_to_f64(self.failure) / u64_to_f64(self.samples))
        } else {
            0.0
        };
        let qps = elapsed_secs
            .filter(|&s| s > 0.0)
            .map(|s| round2(u64_to_f64(self.samples) / s));
        let avg_bytes = (self.bytes_count > 0).then(|| self.total_bytes / self.bytes_count);
        let mut out = json!({
            "protocol": self.protocol,
            "method": self.method,
            "samples": self.samples,
            "share_pct": share_pct,
            "qps": qps,
            "success": self.success,
            "failure": self.failure,
            "expected": self.expected,
            "error_rate": error_rate,
            "min_us": stats.min_us,
            "avg_us": stats.avg_us,
            "stdev_us": stats.stdev_us,
            "p50_us": stats.p50_us,
            "p75_us": stats.p75_us,
            "p90_us": stats.p90_us,
            "p95_us": stats.p95_us,
            "p99_us": stats.p99_us,
            "p999_us": stats.p999_us,
            "max_us": stats.max_us,
            "total_bytes": self.total_bytes,
            "avg_bytes": avg_bytes,
        });
        out[key_name] = Value::String(key.to_string());
        out
    }
}

struct SecAcc {
    hist: Histogram<u64>,
    error_classes: BTreeMap<String, u64>,
}

impl SecAcc {
    fn new() -> Result<Self> {
        Ok(Self {
            hist: new_latency_histogram(60_000_000)?,
            error_classes: BTreeMap::new(),
        })
    }
}

struct Row {
    end_unix_ms: i64,
    protocol: String,
    http_method: Option<String>,
    product_id: String,
    tenant: Option<String>,
    latency_us: i64,
    success: bool,
    expected: bool,
    http_status: Option<i64>,
    error_class: Option<String>,
    error: Option<String>,
    request_bytes: Option<i64>,
    response_bytes: Option<i64>,
    queue_us: Option<i64>,
    ttfb_us: Option<i64>,
    transfer_us: Option<i64>,
    crypto_seal_us: Option<i64>,
    crypto_unseal_us: Option<i64>,
    token_wait_us: Option<i64>,
    trace_echo: String,
}

#[derive(Default)]
struct ErrorCount {
    count: u64,
    status: BTreeMap<String, u64>,
}

struct HttpAcc {
    samples: u64,
    methods: BTreeMap<String, u64>,
    status_families: BTreeMap<&'static str, u64>,
    queue: PhaseAcc,
    ttfb: PhaseAcc,
    transfer: PhaseAcc,
    total: PhaseAcc,
    req_bytes_total: u64,
    req_bytes_count: u64,
    resp_size_hist: Histogram<u64>,
    resp_bytes_total: u64,
    resp_bytes_count: u64,
    resp_size_buckets: [u64; SIZE_EDGES.len()],
}

impl HttpAcc {
    fn new() -> Result<Self> {
        Ok(Self {
            samples: 0,
            methods: BTreeMap::new(),
            status_families: BTreeMap::new(),
            queue: PhaseAcc::new()?,
            ttfb: PhaseAcc::new()?,
            transfer: PhaseAcc::new()?,
            total: PhaseAcc::new()?,
            req_bytes_total: 0,
            req_bytes_count: 0,
            resp_size_hist: new_latency_histogram(100_000_000)?,
            resp_bytes_total: 0,
            resp_bytes_count: 0,
            resp_size_buckets: [0; SIZE_EDGES.len()],
        })
    }

    fn record(&mut self, row: &Row) {
        self.samples += 1;
        if let Some(m) = row.http_method.as_deref().filter(|s| !s.is_empty()) {
            *self.methods.entry(m.to_string()).or_default() += 1;
        }
        *self
            .status_families
            .entry(status_family(row.http_status))
            .or_default() += 1;
        self.queue.record(row.queue_us);
        self.ttfb.record(row.ttfb_us);
        self.transfer.record(row.transfer_us);
        if row.latency_us > 0 {
            self.total.record(Some(row.latency_us));
        }
        if let Some(rb) = row.request_bytes.and_then(|v| u64::try_from(v.max(0)).ok()) {
            self.req_bytes_total = self.req_bytes_total.saturating_add(rb);
            self.req_bytes_count += 1;
        }
        if let Some(b) = row
            .response_bytes
            .and_then(|v| u64::try_from(v.max(0)).ok())
        {
            self.resp_bytes_total = self.resp_bytes_total.saturating_add(b);
            self.resp_bytes_count += 1;
            let rec = b.max(1);
            if self.resp_size_hist.record(rec).is_err() {
                self.resp_size_hist.saturating_record(rec);
            }
            for (idx, &(limit, _)) in SIZE_EDGES.iter().enumerate() {
                if (idx == 0 && b == 0) || (idx > 0 && b <= limit) {
                    self.resp_size_buckets[idx] += 1;
                    break;
                }
            }
        }
    }

    fn to_json(&self) -> Value {
        let base_us = u64_to_f64(
            self.queue
                .avg_us()
                .unwrap_or(0)
                .saturating_add(self.ttfb.avg_us().unwrap_or(0))
                .saturating_add(self.transfer.avg_us().unwrap_or(0)),
        );
        let size_buckets: Vec<Value> = SIZE_EDGES
            .iter()
            .enumerate()
            .filter(|(idx, _)| self.resp_size_buckets[*idx] > 0)
            .map(|(idx, &(_, label))| {
                let count = self.resp_size_buckets[idx];
                let pct = if self.resp_bytes_count > 0 {
                    round2((u64_to_f64(count) / u64_to_f64(self.resp_bytes_count)) * 100.0)
                } else {
                    0.0
                };
                json!({ "label": label, "count": count, "pct": pct })
            })
            .collect();
        let resp_stats = LatencyStats::from_histogram(&self.resp_size_hist);
        json!({
            "samples": self.samples,
            "methods": self.methods,
            "status_families": self.status_families,
            "phases": {
                "queue": self.queue.to_json(base_us),
                "ttfb": self.ttfb.to_json(base_us),
                "transfer": self.transfer.to_json(base_us),
                "total": self.total.to_json(u64_to_f64(self.total.avg_us().unwrap_or(0))),
            },
            "request_sizes": {
                "total_bytes": self.req_bytes_total,
                "avg_bytes": (self.req_bytes_count > 0).then(|| self.req_bytes_total / self.req_bytes_count),
            },
            "response_sizes": {
                "total_bytes": self.resp_bytes_total,
                "min_bytes": resp_stats.min_us,
                "avg_bytes": (self.resp_bytes_count > 0).then(|| self.resp_bytes_total / self.resp_bytes_count),
                "p95_bytes": resp_stats.p95_us,
                "max_bytes": resp_stats.max_us,
                "buckets": size_buckets,
            },
        })
    }
}

struct MazbriAcc {
    samples: u64,
    methods: BTreeMap<String, u64>,
    token_wait: PhaseAcc,
    crypto_seal: PhaseAcc,
    gateway_ttfb: PhaseAcc,
    transfer: PhaseAcc,
    crypto_unseal: PhaseAcc,
    queue: PhaseAcc,
    end_to_end: PhaseAcc,
    trace_match: u64,
    trace_mismatch: u64,
    trace_absent: u64,
}

impl MazbriAcc {
    fn new() -> Result<Self> {
        Ok(Self {
            samples: 0,
            methods: BTreeMap::new(),
            token_wait: PhaseAcc::new()?,
            crypto_seal: PhaseAcc::new()?,
            gateway_ttfb: PhaseAcc::new()?,
            transfer: PhaseAcc::new()?,
            crypto_unseal: PhaseAcc::new()?,
            queue: PhaseAcc::new()?,
            end_to_end: PhaseAcc::new()?,
            trace_match: 0,
            trace_mismatch: 0,
            trace_absent: 0,
        })
    }

    fn record(&mut self, row: &Row) {
        self.samples += 1;
        if let Some(m) = row.http_method.as_deref().filter(|s| !s.is_empty()) {
            *self.methods.entry(m.to_string()).or_default() += 1;
        }
        self.token_wait.record(row.token_wait_us);
        self.crypto_seal.record(row.crypto_seal_us);
        self.gateway_ttfb.record(row.ttfb_us);
        self.transfer.record(row.transfer_us);
        self.crypto_unseal.record(row.crypto_unseal_us);
        self.queue.record(row.queue_us);
        let e2e = row.crypto_seal_us.unwrap_or(0).max(0)
            + row.latency_us.max(0)
            + row.crypto_unseal_us.unwrap_or(0).max(0);
        if e2e > 0 {
            self.end_to_end.record(Some(e2e));
        }
        match row.trace_echo.as_str() {
            "match" => self.trace_match += 1,
            "mismatch" => self.trace_mismatch += 1,
            _ => self.trace_absent += 1,
        }
    }

    fn to_json(&self, snapshot_redacted: &Value) -> Value {
        let seal_avg = self.crypto_seal.avg_us().unwrap_or(0);
        let ttfb_avg = self.gateway_ttfb.avg_us().unwrap_or(0);
        let xfer_avg = self.transfer.avg_us().unwrap_or(0);
        let unseal_avg = self.crypto_unseal.avg_us().unwrap_or(0);
        let tok_avg = self.token_wait.avg_us().unwrap_or(0);
        let base_u64 = tok_avg
            .saturating_add(seal_avg)
            .saturating_add(ttfb_avg)
            .saturating_add(xfer_avg)
            .saturating_add(unseal_avg);
        let base_f = u64_to_f64(base_u64);
        let crypto_pct = if base_f > 0.0 {
            Some(round2(
                (u64_to_f64(seal_avg.saturating_add(unseal_avg)) / base_f) * 100.0,
            ))
        } else {
            None
        };
        let wire_pct = if base_f > 0.0 {
            Some(round2(
                (u64_to_f64(ttfb_avg.saturating_add(xfer_avg)) / base_f) * 100.0,
            ))
        } else {
            None
        };
        let (modes, suites) = extract_mazbri_meta(snapshot_redacted);
        json!({
            "samples": self.samples,
            "methods": self.methods,
            "encryption_modes": modes,
            "suites": suites,
            "crypto_overhead_pct": crypto_pct,
            "wire_pct": wire_pct,
            "trace_echo": {
                "match": self.trace_match,
                "mismatch": self.trace_mismatch,
                "absent": self.trace_absent,
            },
            "phases": {
                "token_wait": self.token_wait.to_json(base_f),
                "crypto_seal": self.crypto_seal.to_json(base_f),
                "gateway_ttfb": self.gateway_ttfb.to_json(base_f),
                "transfer": self.transfer.to_json(base_f),
                "crypto_unseal": self.crypto_unseal.to_json(base_f),
                "queue": self.queue.to_json(base_f),
                "end_to_end": self.end_to_end.to_json(base_f),
            },
        })
    }
}

/// Compute comprehensive breakdown JSON + per-second latency/error stats + Apdex.
///
/// # Errors
///
/// Returns an error when a worker database cannot be read.
#[allow(clippy::too_many_lines)]
pub fn build_breakdown(
    paths: &DataPaths,
    run_id: &str,
    secrets: &[String],
    ctx: &BreakdownContext<'_>,
) -> Result<BreakdownOutput> {
    let mut products: BTreeMap<String, Group> = BTreeMap::new();
    let mut tenants: BTreeMap<String, Group> = BTreeMap::new();
    let mut statuses: BTreeMap<String, u64> = BTreeMap::new();
    let mut status_families: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut errors: HashMap<(String, String), ErrorCount> = HashMap::new();
    let mut per_sec: BTreeMap<u64, SecAcc> = BTreeMap::new();
    let mut lat_buckets = [0_u64; HIST_EDGES_US.len() + 1];
    let mut lat_recorded = 0_u64;

    let satisfied_us = ctx.apdex_satisfied_ms.max(1.0) * 1000.0;
    let tolerated_us = satisfied_us * 4.0;
    let mut apdex_satisfied = 0_u64;
    let mut apdex_tolerated = 0_u64;
    let mut apdex_frustrated = 0_u64;
    let mut apdex_failed = 0_u64;
    let mut apdex_expected = 0_u64;

    let mut http_acc = HttpAcc::new()?;
    let mut mazbri_acc = MazbriAcc::new()?;
    let mut protocols_seen: BTreeSet<String> = BTreeSet::new();
    let mut total_request_bytes = 0_u64;
    let mut total_response_bytes = 0_u64;
    let mut total = 0_u64;

    for worker in paths.worker_indexes(run_id) {
        let Some(conn) = open_existing_worker(paths, run_id, worker, true)? else {
            continue;
        };
        let mut stmt = conn
            .prepare(
                "SELECT end_unix_ms, protocol, http_method, product_id, customer_name, latency_us,
                        success, expected, http_status, error_class, error,
                        request_bytes, response_bytes, queue_us, ttfb_us, transfer_us,
                        crypto_seal_us, crypto_unseal_us, token_wait_us, trace_echo
                 FROM samples WHERE warmup = 0",
            )
            .context("prepare breakdown scan")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Row {
                    end_unix_ms: row.get(0)?,
                    protocol: row.get(1)?,
                    http_method: row.get(2)?,
                    product_id: row.get(3)?,
                    tenant: row.get(4)?,
                    latency_us: row.get(5)?,
                    success: row.get::<_, i64>(6)? != 0,
                    expected: row.get::<_, i64>(7)? != 0,
                    http_status: row.get(8)?,
                    error_class: row.get(9)?,
                    error: row.get(10)?,
                    request_bytes: row.get(11)?,
                    response_bytes: row.get(12)?,
                    queue_us: row.get(13)?,
                    ttfb_us: row.get(14)?,
                    transfer_us: row.get(15)?,
                    crypto_seal_us: row.get(16)?,
                    crypto_unseal_us: row.get(17)?,
                    token_wait_us: row.get(18)?,
                    trace_echo: row.get(19)?,
                })
            })
            .context("scan samples for breakdown")?;
        for row in rows {
            let row = row.context("read sample for breakdown")?;
            total += 1;
            let proto = if row.protocol.is_empty() {
                "http"
            } else {
                row.protocol.as_str()
            };
            protocols_seen.insert(proto.to_string());
            if proto == "mazbri" {
                mazbri_acc.record(&row);
            } else {
                http_acc.record(&row);
            }
            if let Some(rb) = row.request_bytes.and_then(|v| u64::try_from(v.max(0)).ok()) {
                total_request_bytes = total_request_bytes.saturating_add(rb);
            }
            if let Some(b) = row
                .response_bytes
                .and_then(|v| u64::try_from(v.max(0)).ok())
            {
                total_response_bytes = total_response_bytes.saturating_add(b);
            }

            if !products.contains_key(&row.product_id) {
                products.insert(
                    row.product_id.clone(),
                    Group::new(proto.to_string(), row.http_method.clone())?,
                );
            }
            if let Some(group) = products.get_mut(&row.product_id) {
                group.add(&row);
            }

            let tenant = row.tenant.as_deref().unwrap_or(NO_TENANT);
            if !tenants.contains_key(tenant) {
                tenants.insert(tenant.to_string(), Group::new(proto.to_string(), None)?);
            }
            if let Some(group) = tenants.get_mut(tenant) {
                group.add(&row);
            }

            let status = row
                .http_status
                .map_or_else(|| NO_STATUS.to_string(), |s| s.to_string());
            *statuses.entry(status.clone()).or_default() += 1;
            *status_families
                .entry(status_family(row.http_status))
                .or_default() += 1;

            // Apdex classification
            if row.expected {
                apdex_expected += 1;
            } else if !row.success {
                apdex_failed += 1;
            } else {
                let lat_f = i64_to_f64(row.latency_us);
                if lat_f <= satisfied_us {
                    apdex_satisfied += 1;
                } else if lat_f <= tolerated_us {
                    apdex_tolerated += 1;
                } else {
                    apdex_frustrated += 1;
                }
            }

            // Latency histogram & per-second stats
            let sec = u64::try_from((row.end_unix_ms / 1000).max(0)).unwrap_or(0);
            if sec > 0 && !per_sec.contains_key(&sec) {
                per_sec.insert(sec, SecAcc::new()?);
            }
            if let Ok(us) = u64::try_from(row.latency_us)
                && us > 0
            {
                lat_recorded += 1;
                let idx = HIST_EDGES_US
                    .iter()
                    .position(|&edge| us <= edge)
                    .unwrap_or(HIST_EDGES_US.len());
                lat_buckets[idx] += 1;
                if let Some(acc) = per_sec.get_mut(&sec)
                    && acc.hist.record(us).is_err()
                {
                    acc.hist.saturating_record(us);
                }
            }

            if !row.success && !row.expected {
                let class = row.error_class.clone().unwrap_or_else(|| "unknown".into());
                if let Some(acc) = per_sec.get_mut(&sec) {
                    *acc.error_classes.entry(class.clone()).or_default() += 1;
                }
                let message = row
                    .error
                    .as_deref()
                    .map_or_default(|text| scrub_text(text, secrets));
                let entry = errors.entry((class, message)).or_default();
                entry.count += 1;
                *entry.status.entry(status).or_default() += 1;
            }
        }
    }

    let total_failures: u64 = errors.values().map(|e| e.count).sum();
    let mut top: Vec<_> = errors.into_iter().collect();
    top.sort_by(|a, b| b.1.count.cmp(&a.1.count).then_with(|| a.0.cmp(&b.0)));
    let top_errors: Vec<Value> = top
        .into_iter()
        .take(TOP_ERRORS)
        .map(|((class, message), counted)| {
            let share_pct = if total_failures > 0 {
                round2((u64_to_f64(counted.count) / u64_to_f64(total_failures)) * 100.0)
            } else {
                0.0
            };
            json!({
                "error_class": class,
                "error": message,
                "count": counted.count,
                "share_pct": share_pct,
                "http_status": counted.status,
            })
        })
        .collect();

    let latency_histogram = build_latency_histogram(&lat_buckets, lat_recorded);

    let apdex_denom = apdex_satisfied + apdex_tolerated + apdex_frustrated + apdex_failed;
    let apdex_score = (apdex_denom > 0).then(|| {
        round3(
            (u64_to_f64(apdex_satisfied) + 0.5 * u64_to_f64(apdex_tolerated))
                / u64_to_f64(apdex_denom),
        )
    });
    let apdex_json = json!({
        "score": apdex_score,
        "satisfied_ms": round2(ctx.apdex_satisfied_ms),
        "tolerated_ms": round2(ctx.apdex_satisfied_ms * 4.0),
        "satisfied": apdex_satisfied,
        "tolerated": apdex_tolerated,
        "frustrated": apdex_frustrated,
        "failed": apdex_failed,
        "expected": apdex_expected,
    });

    let per_second: BTreeMap<u64, PerSecondSampleStats> = per_sec
        .into_iter()
        .map(|(sec, acc)| {
            let s = LatencyStats::from_histogram(&acc.hist);
            (
                sec,
                PerSecondSampleStats {
                    avg_us: s.avg_us,
                    p50_us: s.p50_us,
                    p90_us: s.p90_us,
                    p95_us: s.p95_us,
                    p99_us: s.p99_us,
                    max_us: s.max_us,
                    error_classes: acc.error_classes,
                },
            )
        })
        .collect();

    let mut protocol_breakdown = serde_json::Map::new();
    if http_acc.samples > 0 {
        protocol_breakdown.insert("http".to_string(), http_acc.to_json());
    }
    if mazbri_acc.samples > 0 {
        protocol_breakdown.insert(
            "mazbri".to_string(),
            mazbri_acc.to_json(ctx.snapshot_redacted),
        );
    }

    let breakdown_json = json!({
        "basis": "recorded_samples",
        "samples": total,
        "products": products
            .iter()
            .map(|(k, g)| g.to_json("product_id", k, total, ctx.effective_elapsed_secs))
            .collect::<Vec<_>>(),
        "tenants": tenants
            .iter()
            .map(|(k, g)| g.to_json("tenant", k, total, ctx.effective_elapsed_secs))
            .collect::<Vec<_>>(),
        "status_codes": statuses,
        "status_families": status_families,
        "top_errors": top_errors,
        "latency_histogram": latency_histogram,
        "apdex": apdex_json,
        "protocol_breakdown": Value::Object(protocol_breakdown),
    });

    Ok(BreakdownOutput {
        breakdown_json,
        per_second,
        apdex_json,
        total_request_bytes,
        total_response_bytes,
        protocols: protocols_seen.into_iter().collect(),
    })
}

/// Lightweight per-second exact latency percentiles & error classes scan for
/// `GET /api/v1/runs/{id}/timeseries` so it matches `report.timeseries`.
#[must_use]
pub fn build_per_second_stats(
    paths: &DataPaths,
    run_id: &str,
) -> BTreeMap<u64, PerSecondSampleStats> {
    let mut per_sec: BTreeMap<u64, SecAcc> = BTreeMap::new();
    for worker in paths.worker_indexes(run_id) {
        let Ok(Some(conn)) = open_existing_worker(paths, run_id, worker, true) else {
            continue;
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT end_unix_ms, latency_us, success, expected, error_class
             FROM samples WHERE warmup = 0",
        ) else {
            continue;
        };
        let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)? != 0,
                row.get::<_, Option<String>>(4)?,
            ))
        }) else {
            continue;
        };
        for row in rows.flatten() {
            let (end_unix_ms, latency_us, success, expected, error_class) = row;
            let sec = u64::try_from((end_unix_ms / 1000).max(0)).unwrap_or(0);
            if sec == 0 {
                continue;
            }
            if !per_sec.contains_key(&sec)
                && let Ok(acc) = SecAcc::new()
            {
                per_sec.insert(sec, acc);
            }
            if let Some(acc) = per_sec.get_mut(&sec) {
                if let Ok(us) = u64::try_from(latency_us)
                    && us > 0
                    && acc.hist.record(us).is_err()
                {
                    acc.hist.saturating_record(us);
                }
                if !success && !expected {
                    let class = error_class.unwrap_or_else(|| "unknown".into());
                    *acc.error_classes.entry(class).or_default() += 1;
                }
            }
        }
    }
    per_sec
        .into_iter()
        .map(|(sec, acc)| {
            let s = LatencyStats::from_histogram(&acc.hist);
            (
                sec,
                PerSecondSampleStats {
                    avg_us: s.avg_us,
                    p50_us: s.p50_us,
                    p90_us: s.p90_us,
                    p95_us: s.p95_us,
                    p99_us: s.p99_us,
                    max_us: s.max_us,
                    error_classes: acc.error_classes,
                },
            )
        })
        .collect()
}

fn build_latency_histogram(buckets: &[u64; HIST_EDGES_US.len() + 1], total: u64) -> Vec<Value> {
    if total == 0 {
        return Vec::new();
    }
    let first = buckets.iter().position(|&c| c > 0).unwrap_or(0);
    let last = buckets
        .iter()
        .rposition(|&c| c > 0)
        .unwrap_or(buckets.len() - 1);
    let mut cum = 0_u64;
    let mut out = Vec::new();
    for (idx, &count) in buckets.iter().enumerate() {
        cum += count;
        if idx < first || idx > last {
            continue;
        }
        let from_ms = if idx == 0 {
            0.0
        } else {
            u64_to_f64(HIST_EDGES_US[idx - 1]) / 1000.0
        };
        let to_ms = (idx < HIST_EDGES_US.len()).then(|| u64_to_f64(HIST_EDGES_US[idx]) / 1000.0);
        let label = match to_ms {
            Some(hi) if idx == 0 => format!("≤{}ms", fmt_edge(hi)),
            Some(hi) => format!("{}–{}ms", fmt_edge(from_ms), fmt_edge(hi)),
            None => format!(">{}ms", fmt_edge(from_ms)),
        };
        let pct = round2((u64_to_f64(count) / u64_to_f64(total)) * 100.0);
        let cumulative_pct = round2((u64_to_f64(cum) / u64_to_f64(total)) * 100.0);
        out.push(json!({
            "label": label,
            "from_ms": from_ms,
            "to_ms": to_ms,
            "count": count,
            "pct": pct,
            "cumulative_pct": cumulative_pct,
        }));
    }
    out
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn fmt_edge(ms: f64) -> String {
    let clamped = ms.max(0.0);
    if clamped >= 1000.0 && (clamped % 1000.0).abs() < f64::EPSILON {
        format!("{}s", (clamped / 1000.0).round() as u64)
    } else {
        format!("{}", clamped.round() as u64)
    }
}

fn status_family(status: Option<i64>) -> &'static str {
    match status {
        Some(200..=299) => "2xx",
        Some(300..=399) => "3xx",
        Some(400..=499) => "4xx",
        Some(500..=599) => "5xx",
        Some(_) => "other",
        None => "none",
    }
}

fn extract_mazbri_meta(snapshot: &Value) -> (Vec<String>, Vec<String>) {
    let mut modes = BTreeSet::new();
    let mut suites = BTreeSet::new();
    let collect_tenant =
        |t: &Value, modes: &mut BTreeSet<String>, suites: &mut BTreeSet<String>| {
            if let Some(m) = t.get("encryption_mode").and_then(Value::as_str)
                && !m.trim().is_empty()
            {
                modes.insert(m.trim().to_string());
            }
            if let Some(s) = t.get("suite").and_then(Value::as_str)
                && !s.trim().is_empty()
            {
                suites.insert(s.trim().to_string());
            } else if let Some(d) = t.get("data_alg").and_then(Value::as_str)
                && !d.trim().is_empty()
            {
                suites.insert(d.trim().to_string());
            }
        };
    if let Some(tenants) = snapshot
        .get("target")
        .and_then(|t| t.get("tenants"))
        .and_then(Value::as_array)
    {
        for t in tenants {
            collect_tenant(t, &mut modes, &mut suites);
        }
    }
    if let Some(reqs) = snapshot
        .get("target")
        .and_then(|t| t.get("requests"))
        .and_then(Value::as_array)
    {
        for r in reqs {
            if let Some(inline) = r.get("mazbri").and_then(|m| m.get("inline_tenant")) {
                collect_tenant(inline, &mut modes, &mut suites);
            }
        }
    }
    (modes.into_iter().collect(), suites.into_iter().collect())
}

#[allow(clippy::cast_precision_loss)]
fn u64_to_f64(v: u64) -> f64 {
    v as f64
}

#[allow(clippy::cast_precision_loss)]
fn i64_to_f64(v: i64) -> f64 {
    v.max(0) as f64
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}
