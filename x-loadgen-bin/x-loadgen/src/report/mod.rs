//! Redacted per-run reports, HTML export, and merged sample pagination.

pub mod html;
pub mod redact;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::db::DataPaths;
use crate::db::open_existing_worker;
use crate::db::runs::{RunRecord, latest_checkpoint_blobs, list_timeline, list_windows};
use crate::engine::RunSnapshot;
use crate::engine::{
    CounterSnapshot, HistogramPercentiles, RING_CAPACITY, RingSlot, merge_percentiles,
};
use crate::report::redact::{apply_secrets, collect_secrets, redact_snapshot_json, scrub_text};
use crate::run_assert::{
    ActualMetrics, ActualParts, AssertSpec, Verdict, evaluate, status_is_running,
};

/// Inputs needed to assemble a report without loading sample rows.
pub struct ReportInput<'a> {
    pub paths: &'a DataPaths,
    pub conn: &'a Connection,
    pub record: &'a RunRecord,
    pub live_counters: Option<CounterSnapshot>,
    pub live_percentiles: Option<HistogramPercentiles>,
    pub live_ring: Option<Vec<RingSlot>>,
}

/// Build a redacted report JSON for one run. Window rows, checkpoints, the
/// timeline and interrupt ranges are included; sample rows are not loaded.
///
/// # Errors
///
/// Returns an error if window, timeline, or histogram data cannot be read.
pub fn build_report(input: &ReportInput<'_>) -> Result<Value> {
    let secrets = collect_secrets(
        &serde_json::from_str::<Value>(&input.record.snapshot_json).unwrap_or(Value::Null),
    );
    let snapshot = redact_snapshot_json(&input.record.snapshot_json);
    let windows = list_windows(input.conn, &input.record.id)?;
    let timeline = list_timeline(input.conn, &input.record.id)?;
    let (counters, percentiles) = counters_and_percentiles(input, &windows)?;
    let timeseries = timeseries_points(input, &windows);
    let mut report = json!({
        "id": input.record.id,
        "template_id": input.record.template_id,
        "target_id": input.record.target_id,
        "mode": input.record.mode,
        "status": input.record.status,
        "started_at": input.record.started_at,
        "ended_at": input.record.ended_at,
        "current_qps": input.record.current_qps,
        "current_concurrency": input.record.current_concurrency,
        "processes": input.record.processes,
        "interrupt_ranges": input.record.interrupt_ranges,
        "error": input.record.error,
        "created_at": input.record.created_at,
        "counters": counters,
        "percentiles": percentiles,
        "windows": windows,
        "timeline": timeline,
        "timeseries": timeseries,
        "timeseries_capacity": RING_CAPACITY,
        "snapshot": snapshot,
    });
    apply_secrets(&mut report, &secrets);
    let verdict = verdict_from_totals(input.record, &counters, &percentiles);
    report["verdict"] = serde_json::to_value(&verdict)?;
    Ok(report)
}

/// Evaluate the run's frozen `assert` against current counters and percentiles.
///
/// # Errors
///
/// Returns an error if window or histogram data cannot be read.
pub fn compute_verdict(input: &ReportInput<'_>) -> Result<Verdict> {
    let windows = list_windows(input.conn, &input.record.id)?;
    let (counters, percentiles) = counters_and_percentiles(input, &windows)?;
    Ok(verdict_from_totals(input.record, &counters, &percentiles))
}

fn verdict_from_totals(record: &RunRecord, counters: &Value, percentiles: &Value) -> Verdict {
    let snapshot = serde_json::from_str::<RunSnapshot>(&record.snapshot_json).ok();
    let spec: Option<AssertSpec> = snapshot.as_ref().and_then(|s| s.assert.clone());
    let configured = record
        .current_qps
        .and_then(|v| u64::try_from(v).ok())
        .or_else(|| {
            snapshot
                .as_ref()
                .and_then(|s| s.template.qps)
                .and_then(|v| u64::try_from(v).ok())
        });
    let elapsed_ms = elapsed_ms(record);
    let actual = ActualMetrics::from_parts(&ActualParts {
        samples: json_u64(&counters["samples"]),
        success: json_u64(&counters["success"]),
        failure: json_u64(&counters["failure"]),
        p50_us: json_opt_u64(&percentiles["p50_us"]),
        p95_us: json_opt_u64(&percentiles["p95_us"]),
        p99_us: json_opt_u64(&percentiles["p99_us"]),
        max_us: json_opt_u64(&percentiles["max_us"]),
        configured_qps: configured,
        elapsed_ms,
    });
    evaluate(spec.as_ref(), &actual, status_is_running(&record.status))
}

fn elapsed_ms(record: &RunRecord) -> Option<i64> {
    let start = record.started_at?;
    let end = record
        .ended_at
        .unwrap_or_else(|| i64::try_from(crate::db::unix_now_ms().unwrap_or(0)).unwrap_or(0));
    Some(end.saturating_sub(start).max(0))
}

fn json_u64(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .unwrap_or(0)
}

fn json_opt_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
}

fn counters_and_percentiles(input: &ReportInput<'_>, windows: &[Value]) -> Result<(Value, Value)> {
    let from_windows = sum_windows(windows);
    let counters = if let Some(live) = input.live_counters {
        json!({
            "samples": live.samples,
            "success": live.success,
            "failure": live.failure,
            "expected": live.expected,
            "bytes": live.bytes,
            "dropped_details": live.dropped_details,
        })
    } else {
        from_windows
    };
    let percentiles = if let Some((p50, p95, p99, max)) = input.live_percentiles {
        json!({ "p50_us": p50, "p95_us": p95, "p99_us": p99, "max_us": max })
    } else {
        let blobs = latest_checkpoint_blobs(input.conn, &input.record.id)?;
        let (p50, p95, p99, max) =
            merge_percentiles(&blobs, 60_000_000).unwrap_or((None, None, None, None));
        json!({ "p50_us": p50, "p95_us": p95, "p99_us": p99, "max_us": max })
    };
    Ok((counters, percentiles))
}

fn sum_windows(windows: &[Value]) -> Value {
    let mut samples = 0_u64;
    let mut success = 0;
    let mut failure = 0;
    let mut expected = 0;
    let mut bytes = 0;
    let mut dropped = 0;
    for window in windows {
        samples += window["samples"].as_u64().unwrap_or(0);
        success += window["success"].as_u64().unwrap_or(0);
        failure += window["failure"].as_u64().unwrap_or(0);
        expected += window["expected"].as_u64().unwrap_or(0);
        bytes += window["bytes"].as_u64().unwrap_or(0);
        dropped += window["dropped_details"].as_u64().unwrap_or(0);
    }
    json!({
        "samples": samples,
        "success": success,
        "failure": failure,
        "expected": expected,
        "bytes": bytes,
        "dropped_details": dropped,
    })
}

fn timeseries_points(input: &ReportInput<'_>, windows: &[Value]) -> Vec<Value> {
    if let Some(slots) = input.live_ring.as_ref() {
        return slots
            .iter()
            .filter(|slot| slot.unix_sec != 0)
            .map(|slot| {
                json!({
                    "unix_sec": slot.unix_sec,
                    "samples": slot.samples,
                    "success": slot.success,
                    "failure": slot.failure,
                    "expected": slot.expected,
                    "bytes": slot.bytes,
                    "warmup": slot.warmup,
                })
            })
            .collect();
    }
    windows
        .iter()
        .map(|window| {
            json!({
                "started_at": window["started_at"],
                "ended_at": window["ended_at"],
                "samples": window["samples"],
                "success": window["success"],
                "failure": window["failure"],
                "expected": window["expected"],
                "bytes": window["bytes"],
            })
        })
        .collect()
}

/// Query string for `GET /api/v1/runs/:id/samples`.
#[derive(Debug, Default, Deserialize)]
pub struct SamplesQuery {
    pub product_id: Option<String>,
    pub success: Option<bool>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub after_end_unix_ms: Option<i64>,
    pub after_worker: Option<u32>,
    pub after_seq: Option<i64>,
    pub limit: Option<u32>,
}

/// One page of redacted sample rows plus an optional keyset cursor.
#[derive(Debug)]
pub struct SamplesPage {
    pub items: Vec<Value>,
    pub next: Option<Value>,
}

/// Merge-paginated sample rows from every worker database of a run.
///
/// # Errors
///
/// Returns an error if a worker database cannot be read.
pub fn query_samples(
    paths: &DataPaths,
    run_id: &str,
    snapshot_json: &str,
    query: &SamplesQuery,
) -> Result<SamplesPage> {
    let secrets =
        collect_secrets(&serde_json::from_str::<Value>(snapshot_json).unwrap_or(Value::Null));
    let limit = query.limit.unwrap_or(100).clamp(1, 1000) as usize;
    let indexes = paths.worker_indexes(run_id);
    let mut merged = Vec::new();
    for index in indexes {
        let mut rows = load_worker_samples(paths, run_id, index, query, limit)?;
        merged.append(&mut rows);
    }
    merged.sort_by(|a, b| {
        a.end_unix_ms
            .cmp(&b.end_unix_ms)
            .then(a.worker.cmp(&b.worker))
            .then(a.seq.cmp(&b.seq))
    });
    let more = merged.len() > limit;
    merged.truncate(limit);
    let next = if more {
        merged.last().map(|row| {
            json!({
                "after_end_unix_ms": row.end_unix_ms,
                "after_worker": row.worker,
                "after_seq": row.seq,
            })
        })
    } else {
        None
    };
    let items = merged
        .into_iter()
        .map(|row| row.into_json(&secrets))
        .collect();
    Ok(SamplesPage { items, next })
}

struct SampleRow {
    worker: u32,
    seq: i64,
    start_unix_ms: i64,
    end_unix_ms: i64,
    latency_us: i64,
    product_id: String,
    product_name: Option<String>,
    request_index: Option<i64>,
    http_status: Option<i64>,
    success: bool,
    expected: bool,
    reason: Option<String>,
    error_class: Option<String>,
    request_id: Option<String>,
    error: Option<String>,
    response_bytes: Option<i64>,
    customer_name: Option<String>,
}

impl SampleRow {
    fn into_json(self, secrets: &[String]) -> Value {
        json!({
            "worker": self.worker,
            "seq": self.seq,
            "start_unix_ms": self.start_unix_ms,
            "end_unix_ms": self.end_unix_ms,
            "latency_us": self.latency_us,
            "product_id": self.product_id,
            "product_name": self.product_name,
            "request_index": self.request_index,
            "http_status": self.http_status,
            "success": self.success,
            "expected": self.expected,
            "reason": self.reason,
            "error_class": self.error_class,
            "request_id": self.request_id,
            "error": self.error.map(|text| scrub_text(&text, secrets)),
            "response_bytes": self.response_bytes,
            "customer_name": self.customer_name,
        })
    }
}

fn load_worker_samples(
    paths: &DataPaths,
    run_id: &str,
    worker: u32,
    query: &SamplesQuery,
    limit: usize,
) -> Result<Vec<SampleRow>> {
    let Some(conn) = open_existing_worker(paths, run_id, worker, true)? else {
        return Ok(Vec::new());
    };
    let sql = "SELECT seq, start_unix_ms, end_unix_ms, latency_us, product_id, product_name,
                      request_index, http_status, success, expected, reason, error_class,
                      request_id, error, response_bytes, customer_name
               FROM samples
               WHERE (?1 IS NULL OR product_id = ?1)
                 AND (?2 IS NULL OR success = ?2)
                 AND (?3 IS NULL OR end_unix_ms >= ?3)
                 AND (?4 IS NULL OR end_unix_ms <= ?4)
               ORDER BY end_unix_ms, seq";
    let success = query.success.map(i64::from);
    let mut stmt = conn.prepare(sql).context("prepare samples query")?;
    let rows = stmt
        .query_map(
            params![
                query.product_id.as_deref(),
                success,
                query.from_ms,
                query.to_ms,
            ],
            |row| {
                Ok(SampleRow {
                    worker,
                    seq: row.get(0)?,
                    start_unix_ms: row.get(1)?,
                    end_unix_ms: row.get(2)?,
                    latency_us: row.get(3)?,
                    product_id: row.get(4)?,
                    product_name: row.get(5)?,
                    request_index: row.get(6)?,
                    http_status: row.get(7)?,
                    success: row.get::<_, i64>(8)? != 0,
                    expected: row.get::<_, i64>(9)? != 0,
                    reason: row.get(10)?,
                    error_class: row.get(11)?,
                    request_id: row.get(12)?,
                    error: row.get(13)?,
                    response_bytes: row.get(14)?,
                    customer_name: row.get(15)?,
                })
            },
        )
        .context("query samples")?;
    let mut out = Vec::new();
    for row in rows {
        let row = row?;
        if after_cursor(query, &row) {
            out.push(row);
            if out.len() > limit {
                break;
            }
        }
    }
    Ok(out)
}

fn after_cursor(query: &SamplesQuery, row: &SampleRow) -> bool {
    let Some(after_end) = query.after_end_unix_ms else {
        return true;
    };
    let after_worker = query.after_worker.unwrap_or(0);
    let after_seq = query.after_seq.unwrap_or(0);
    (row.end_unix_ms, row.worker, row.seq) > (after_end, after_worker, after_seq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::redact::{contains_any_secret, redact_value};

    #[test]
    fn report_json_and_html_do_not_contain_snapshot_secrets() {
        let snapshot = json!({
            "target": {
                "name": "gw",
                "client_key": "SECRET-CLIENT-KEY-ZZ11",
                "tenants": [{
                    "name": "t1",
                    "api_key": "SECRET-API-KEY-AAAA",
                    "tenant_key_base64": "SECRET-TENANT-KEY-BBBB",
                    "binding_key": "SECRET-BINDING-KEY-CCCC"
                }]
            },
            "template": { "name": "once" }
        });
        let secrets = collect_secrets(&snapshot);
        assert_eq!(secrets.len(), 4);
        let redacted = redact_value(snapshot.clone());
        let html = html::render(&json!({
            "id": "run-1",
            "status": "finished",
            "snapshot": redacted,
            "counters": { "samples": 1, "success": 1, "failure": 0, "expected": 0 },
            "percentiles": {},
            "windows": [],
            "timeline": [],
            "interrupt_ranges": [],
            "timeseries": []
        }));
        let json_text = redacted.to_string();
        assert!(!contains_any_secret(&json_text, &secrets), "{json_text}");
        assert!(!contains_any_secret(&html, &secrets));
        assert!(!json_text.contains("api_key"));
        assert!(!json_text.contains("client_key"));
        assert!(!json_text.contains("binding_key"));
        assert!(!json_text.contains("tenant_key_base64"));
    }

    #[test]
    fn generated_binding_private_key_is_redacted_but_public_key_and_algorithms_stay() {
        let private_pem =
            "-----BEGIN PRIVATE KEY-----\nTUMwMDAxUFJJVkFURQ==\n-----END PRIVATE KEY-----\n";
        let public_pem = "-----BEGIN PUBLIC KEY-----\nTUMwMDAxUFVCTElD\n-----END PUBLIC KEY-----\n";
        let snapshot = json!({
            "target": {
                "name": "gw",
                "protocol": "mazbri",
                "tenants": [{
                    "name": "t1",
                    "api_key": "SECRET-API-KEY-EEEE",
                    "encryption_mode": "full",
                    "suite": "suite/gm-sm4-hmac-sm3",
                    "tenant_key_base64": "U0VDUkVULVRFTkFOVC1LRVktRkZGRg==",
                    "binding_kid": "kid-1",
                    "binding_alg": "sm2-sign",
                    "binding_key": private_pem,
                    "binding_public_key": public_pem
                }]
            }
        });
        let secrets = collect_secrets(&snapshot);
        let redacted = redact_value(snapshot).to_string();
        assert!(!contains_any_secret(&redacted, &secrets), "{redacted}");
        assert!(!redacted.contains("PRIVATE KEY"));
        assert!(redacted.contains("BEGIN PUBLIC KEY"));
        assert!(redacted.contains("sm2-sign"));
        assert!(redacted.contains("suite/gm-sm4-hmac-sm3"));
    }
}
