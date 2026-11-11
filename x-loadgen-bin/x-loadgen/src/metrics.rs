//! Result collection and aggregation for load reports.
//! Modeled after common load-test stats: per-label rows, error catalog, time series.
use mazbri_core::num::SaturatingFrom;

use crate::config::{MetricId, ReportConfig};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct RequestSample {
    pub product_id: String,
    pub product_name: String,
    pub request_index: u32,
    pub start_unix_ms: u64,
    pub end_unix_ms: u64,
    pub latency_ms: u128,
    pub http_status: u16,
    pub success: bool,
    pub expected: bool,
    pub reason: Option<String>,
    pub error_class: Option<String>,
    pub response_bytes: usize,
    pub request_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
struct LabelAgg {
    name: String,
    requests: u64,
    success: u64,
    failure: u64,
    expected: u64,
    bytes: u64,
    latencies: Vec<u128>,
    status: HashMap<u16, u64>,
    reasons: HashMap<String, u64>,
}

#[derive(Debug, Default)]
struct SecBucket {
    completed: u64,
    success: u64,
    failure: u64,
    expected: u64,
    bytes: u64,
    latencies: Vec<u128>,
    status: HashMap<u16, u64>,
    reasons: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorExample {
    pub label: String,
    pub product_id: String,
    pub request_index: u32,
    pub status: u16,
    pub latency_ms: u128,
    pub start_unix_ms: u64,
    pub reason: Option<String>,
    pub error_class: Option<String>,
    pub request_id: Option<String>,
    pub message: Option<String>,
}

pub struct MetricsCollector {
    report: ReportConfig,
    started: Instant,
    started_unix_ms: u64,
    labels: HashMap<String, LabelAgg>,
    seconds: BTreeMap<u64, SecBucket>,
    all_latencies: Vec<u128>,
    error_examples: Vec<ErrorExample>,
    max_examples: usize,
    request_ids: Vec<String>,
}

impl MetricsCollector {
    pub fn new(report: ReportConfig) -> Self {
        Self {
            report,
            started: Instant::now(),
            started_unix_ms: unix_now_ms(),
            labels: HashMap::new(),
            seconds: BTreeMap::new(),
            all_latencies: Vec::new(),
            error_examples: Vec::new(),
            max_examples: 50,
            request_ids: Vec::new(),
        }
    }

    pub fn record(&mut self, s: &RequestSample) {
        if let Some(ref id) = s.request_id {
            self.request_ids.push(id.clone());
        }
        let want_lat = self.report.has_metric(MetricId::Latency);
        let want_ts = self.report.has_metric(MetricId::Throughput)
            || self.report.has_metric(MetricId::ReasonTimeseries)
            || self.report.has_metric(MetricId::Availability)
            || self.report.has_metric(MetricId::HttpStatus);
        let want_status = self.report.has_metric(MetricId::HttpStatus);
        let want_reason = self.report.has_metric(MetricId::ErrorReason)
            || self.report.has_metric(MetricId::ReasonTimeseries);
        let want_bytes = self.report.has_metric(MetricId::Bytes);
        let want_examples = self.report.has_metric(MetricId::FailuresSample);

        if want_lat {
            self.all_latencies.push(s.latency_ms);
        }

        if want_ts {
            let sec = s.end_unix_ms / 1000;
            let b = self.seconds.entry(sec).or_default();
            b.completed += 1;
            if s.success {
                b.success += 1;
            } else if s.expected {
                b.expected += 1;
            } else {
                b.failure += 1;
            }
            if want_bytes {
                b.bytes += s.response_bytes as u64;
            }
            if want_status {
                *b.status.entry(s.http_status).or_default() += 1;
            }
            if want_reason {
                if let Some(ref r) = s.reason {
                    *b.reasons.entry(r.clone()).or_default() += 1;
                } else if !s.success {
                    *b.reasons.entry("_none".into()).or_default() += 1;
                }
            }
            if want_lat && b.latencies.len() < 8192 {
                b.latencies.push(s.latency_ms);
            }
        }

        let agg = self
            .labels
            .entry(s.product_id.clone())
            .or_insert_with(|| LabelAgg {
                name: s.product_name.clone(),
                ..Default::default()
            });
        agg.name.clone_from(&s.product_name);
        agg.requests += 1;
        if s.success {
            agg.success += 1;
        } else if s.expected {
            agg.expected += 1;
        } else {
            agg.failure += 1;
        }
        if want_bytes {
            agg.bytes += s.response_bytes as u64;
        }
        if want_lat {
            agg.latencies.push(s.latency_ms);
        }
        if want_status {
            *agg.status.entry(s.http_status).or_default() += 1;
        }
        if want_reason && let Some(ref r) = s.reason {
            *agg.reasons.entry(r.clone()).or_default() += 1;
        }

        if want_examples
            && !s.success
            && !s.expected
            && self.error_examples.len() < self.max_examples
        {
            self.error_examples.push(ErrorExample {
                label: s.product_name.clone(),
                product_id: s.product_id.clone(),
                request_index: s.request_index,
                status: s.http_status,
                latency_ms: s.latency_ms,
                start_unix_ms: s.start_unix_ms,
                reason: s.reason.clone(),
                error_class: s.error_class.clone(),
                request_id: s.request_id.clone(),
                message: s.error.as_ref().map(|e| truncate(e, 160)),
            });
        }
    }

    pub fn request_ids(&self) -> Vec<String> {
        self.request_ids.clone()
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// 按秒聚合的时间序列；`t_rel` 相对首个桶归一化，便于绘图。
    fn time_series(&self) -> Vec<TimePoint> {
        let mut series: Vec<TimePoint> = self
            .seconds
            .iter()
            .map(|(sec, b)| {
                let lat = trend_stats(&b.latencies);
                TimePoint {
                    t: *sec,
                    t_rel: 0,
                    rps: mazbri_core::num::u64_to_f64(b.completed),
                    success: b.success,
                    failure: b.failure,
                    expected: b.expected,
                    bytes: b.bytes,
                    p50: lat.p50,
                    p95: lat.p95,
                    p99: lat.p99,
                    status: sorted_u16(&b.status),
                    reasons: sorted_str(&b.reasons),
                }
            })
            .collect();
        // normalize t to relative seconds from first bucket for charts
        if let Some(t0) = series.first().map(|p| p.t) {
            for p in &mut series {
                p.t_rel = p.t.saturating_sub(t0);
            }
        }
        series
    }

    pub fn build_summary(
        &self,
        run_id: &str,
        title: &str,
        run_context: RunContext,
    ) -> ReportSummary {
        let mut total_req = 0u64;
        let mut total_ok = 0u64;
        let mut total_fail = 0u64;
        let mut total_exp = 0u64;
        let mut total_bytes = 0u64;
        let mut status_all: HashMap<u16, u64> = HashMap::new();
        let mut reason_all: HashMap<String, u64> = HashMap::new();

        let mut labels: Vec<LabelStats> = self
            .labels
            .iter()
            .map(|(id, a)| {
                total_req += a.requests;
                total_ok += a.success;
                total_fail += a.failure;
                total_exp += a.expected;
                total_bytes += a.bytes;
                for (k, v) in &a.status {
                    *status_all.entry(*k).or_default() += *v;
                }
                for (k, v) in &a.reasons {
                    *reason_all.entry(k.clone()).or_default() += *v;
                }
                let lat = trend_stats(&a.latencies);
                let elapsed = self.elapsed().as_secs_f64().max(0.001);
                LabelStats {
                    label: a.name.clone(),
                    product_id: id.clone(),
                    samples: a.requests,
                    success: a.success,
                    failure: a.failure,
                    expected: a.expected,
                    error_pct: pct(a.failure, a.requests),
                    effective_error_pct: pct(a.failure, a.requests), // expected excluded from failure already
                    throughput_rps: mazbri_core::num::u64_to_f64(a.requests) / elapsed,
                    received_bytes: a.bytes,
                    latency: lat,
                }
            })
            .collect();
        labels.sort_by(|a, b| a.product_id.cmp(&b.product_id));

        let elapsed_s = self.elapsed().as_secs_f64().max(0.001);
        let business_ok_rate = ratio(total_ok, total_req);
        let effective_ok_rate = ratio(total_ok + total_exp, total_req);
        let fail_rate = ratio(total_fail, total_req);

        let series = self.time_series();

        let peak_rps = series.iter().map(|p| p.rps).fold(0.0_f64, f64::max);
        let latency = trend_stats(&self.all_latencies);
        let hist = latency_histogram(&self.all_latencies);
        let errors = build_error_catalog(&status_all, &reason_all, total_req, &self.error_examples);
        let verdict = evaluate(&self.report, business_ok_rate, effective_ok_rate, &latency);

        ReportSummary {
            schema_version: 2,
            run_id: run_id.to_string(),
            title: title.to_string(),
            profile: self.report.profile.clone(),
            metrics: self
                .report
                .resolved_metrics
                .iter()
                .map(|m| m.as_str().to_string())
                .collect(),
            started_unix_ms: self.started_unix_ms,
            ended_unix_ms: unix_now_ms(),
            duration_s: elapsed_s,
            overview: Overview {
                samples: total_req,
                success: total_ok,
                failure: total_fail,
                expected: total_exp,
                business_ok_rate,
                effective_ok_rate,
                error_rate: fail_rate,
                avg_rps: mazbri_core::num::u64_to_f64(total_req) / elapsed_s,
                peak_rps,
                received_kb: mazbri_core::num::u64_to_f64(total_bytes) / 1024.0,
            },
            latency,
            labels,
            timeseries: series,
            distribution: hist,
            errors,
            status_codes: sorted_u16(&status_all),
            reasons: sorted_str(&reason_all),
            verdict,
            run_context,
            expected_reasons: self.report.sla.expected_reasons.clone(),
        }
    }
}

fn build_error_catalog(
    status: &HashMap<u16, u64>,
    reasons: &HashMap<String, u64>,
    total: u64,
    examples: &[ErrorExample],
) -> ErrorCatalog {
    let mut rows: Vec<ErrorRow> = Vec::new();
    for (code, n) in status {
        if *code >= 400 || *code == 0 {
            rows.push(ErrorRow {
                key: format!("HTTP {code}"),
                kind: "status".into(),
                count: *n,
                pct_of_samples: pct(*n, total),
                pct_of_errors: 0.0,
            });
        }
    }
    for (r, n) in reasons {
        if r == "_none" {
            continue;
        }
        rows.push(ErrorRow {
            key: r.clone(),
            kind: "reason".into(),
            count: *n,
            pct_of_samples: pct(*n, total),
            pct_of_errors: 0.0,
        });
    }
    rows.sort_by(|a, b| b.count.cmp(&a.count).then(a.key.cmp(&b.key)));
    let err_total: u64 = rows.iter().map(|r| r.count).sum();
    for r in &mut rows {
        r.pct_of_errors = pct(r.count, err_total.max(1));
    }
    ErrorCatalog {
        rows,
        examples: examples.to_vec(),
    }
}

fn evaluate(report: &ReportConfig, business: f64, effective: f64, latency: &TrendStats) -> Verdict {
    let mut notes = Vec::new();
    let mut fail = false;
    let rate = if report.sla.expected_reasons.is_empty() {
        business
    } else {
        effective
    };
    if let Some(min) = report.sla.success_rate_min {
        if rate + 1e-12 < min {
            fail = true;
            notes.push(format!(
                "OK rate {:.2}% is below threshold {:.2}%",
                rate * 100.0,
                min * 100.0
            ));
        } else {
            notes.push(format!(
                "OK rate {:.2}% meets threshold {:.2}%",
                rate * 100.0,
                min * 100.0
            ));
        }
    }
    if let Some(max) = report.sla.p95_ms_max {
        if latency.count > 0 && latency.p95 > max {
            fail = true;
            notes.push(format!("p95 {} ms exceeds {} ms", latency.p95, max));
        } else if latency.count > 0 {
            notes.push(format!("p95 {} ms within {} ms", latency.p95, max));
        }
    }
    if let Some(max) = report.sla.p99_ms_max {
        if latency.count > 0 && latency.p99 > max {
            fail = true;
            notes.push(format!("p99 {} ms exceeds {} ms", latency.p99, max));
        } else if latency.count > 0 {
            notes.push(format!("p99 {} ms within {} ms", latency.p99, max));
        }
    }
    if !report.sla.expected_reasons.is_empty() {
        notes.push(format!(
            "Expected rejects: {}",
            report.sla.expected_reasons.join(", ")
        ));
    }
    if notes.is_empty() {
        notes.push("No SLA thresholds configured".into());
    }
    let status = if fail {
        "FAIL"
    } else if business < 0.99 && report.sla.expected_reasons.is_empty() {
        "WARN"
    } else {
        "PASS"
    };
    Verdict {
        status: status.into(),
        notes,
    }
}

pub fn is_expected_reason(report: &ReportConfig, reason: Option<&str>) -> bool {
    let Some(r) = reason else {
        return false;
    };
    report
        .sla
        .expected_reasons
        .iter()
        .any(|e| e.eq_ignore_ascii_case(r))
}

pub fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::saturating_from(d.as_millis()))
}

fn truncate(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

fn ratio(n: u64, d: u64) -> f64 {
    if d == 0 {
        0.0
    } else {
        mazbri_core::num::u64_to_f64(n) / mazbri_core::num::u64_to_f64(d)
    }
}

fn pct(n: u64, d: u64) -> f64 {
    ratio(n, d) * 100.0
}

fn sorted_u16(m: &HashMap<u16, u64>) -> Vec<CountRow> {
    let mut v: Vec<_> = m
        .iter()
        .map(|(k, n)| CountRow {
            name: k.to_string(),
            count: *n,
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn sorted_str(m: &HashMap<String, u64>) -> Vec<CountRow> {
    let mut v: Vec<_> = m
        .iter()
        .filter(|(k, _)| *k != "_none")
        .map(|(k, n)| CountRow {
            name: k.clone(),
            count: *n,
        })
        .collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    v
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct TrendStats {
    pub count: u64,
    pub min: u128,
    pub max: u128,
    pub mean: f64,
    pub p50: u128,
    pub p90: u128,
    pub p95: u128,
    pub p99: u128,
}

pub fn trend_stats(values: &[u128]) -> TrendStats {
    if values.is_empty() {
        return TrendStats::default();
    }
    let mut s = values.to_vec();
    s.sort_unstable();
    let n = s.len();
    let sum: u128 = s.iter().copied().sum();
    TrendStats {
        count: n as u64,
        min: s[0],
        max: s[n - 1],
        mean: crate::num::u128_to_f64(sum) / mazbri_core::num::usize_to_f64(n),
        p50: percentile(&s, 50.0),
        p90: percentile(&s, 90.0),
        p95: percentile(&s, 95.0),
        p99: percentile(&s, 99.0),
    }
}

fn percentile(sorted: &[u128], p: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = crate::num::f64_to_usize(
        ((p / 100.0) * (mazbri_core::num::usize_to_f64(sorted.len()) - 1.0)).round(),
    );
    sorted[rank.min(sorted.len() - 1)]
}

#[derive(Debug, Clone, Serialize)]
pub struct HistBin {
    pub le_ms: u64,
    pub count: u64,
    pub label: String,
}

fn latency_histogram(values: &[u128]) -> Vec<HistBin> {
    if values.is_empty() {
        return Vec::new();
    }
    let bounds: &[u64] = &[
        10, 20, 50, 100, 200, 300, 500, 750, 1000, 1500, 2000, 3000, 5000, 10000,
    ];
    let mut counts = vec![0u64; bounds.len() + 1];
    for &v in values {
        let mut placed = false;
        for (i, &b) in bounds.iter().enumerate() {
            if u64::saturating_from(v) <= b {
                counts[i] += 1;
                placed = true;
                break;
            }
        }
        if !placed {
            *counts.last_mut().unwrap() += 1;
        }
    }
    let mut out = Vec::new();
    for (i, &b) in bounds.iter().enumerate() {
        out.push(HistBin {
            le_ms: b,
            count: counts[i],
            label: format!("≤{b}"),
        });
    }
    out.push(HistBin {
        le_ms: u64::MAX,
        count: *counts.last().unwrap(),
        label: format!(">{}", bounds.last().unwrap()),
    });
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct CountRow {
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub business_ok_rate: f64,
    pub effective_ok_rate: f64,
    pub error_rate: f64,
    pub avg_rps: f64,
    pub peak_rps: f64,
    pub received_kb: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LabelStats {
    pub label: String,
    pub product_id: String,
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub error_pct: f64,
    pub effective_error_pct: f64,
    pub throughput_rps: f64,
    pub received_bytes: u64,
    pub latency: TrendStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimePoint {
    pub t: u64,
    #[serde(default)]
    pub t_rel: u64,
    pub rps: f64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub bytes: u64,
    pub p50: u128,
    pub p95: u128,
    pub p99: u128,
    pub status: Vec<CountRow>,
    pub reasons: Vec<CountRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorRow {
    pub key: String,
    pub kind: String,
    pub count: u64,
    pub pct_of_samples: f64,
    #[serde(default)]
    pub pct_of_errors: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorCatalog {
    pub rows: Vec<ErrorRow>,
    pub examples: Vec<ErrorExample>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    pub status: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunContext {
    pub tool: String,
    pub gateway: String,
    pub encryption: String,
    pub concurrency: usize,
    pub requests_per_product: u32,
    pub qps: Option<u32>,
    pub products: usize,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportSummary {
    pub schema_version: u32,
    pub run_id: String,
    pub title: String,
    pub profile: String,
    pub metrics: Vec<String>,
    pub started_unix_ms: u64,
    pub ended_unix_ms: u64,
    pub duration_s: f64,
    pub overview: Overview,
    pub latency: TrendStats,
    pub labels: Vec<LabelStats>,
    pub timeseries: Vec<TimePoint>,
    pub distribution: Vec<HistBin>,
    pub errors: ErrorCatalog,
    pub status_codes: Vec<CountRow>,
    pub reasons: Vec<CountRow>,
    pub verdict: Verdict,
    pub run_context: RunContext,
    pub expected_reasons: Vec<String>,
}
