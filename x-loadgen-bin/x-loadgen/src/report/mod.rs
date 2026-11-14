//! Redacted per-run reports, HTML export, and merged sample pagination.

pub mod html;
pub mod redact;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::db::DataPaths;
use crate::db::open_existing_worker;
use crate::db::profiles::MODEL_OPEN_LOOP;
use crate::db::runs::{
    RunRecord, latest_checkpoint_blobs, latest_checkpoint_timeseries, list_timeline, list_windows,
};
use crate::engine::RunSnapshot;
use crate::engine::{
    CounterSnapshot, HistogramPercentiles, RING_CAPACITY, RingSlot, merge_percentiles,
    merge_timeseries_slots,
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
    let title = snapshot.get("title").cloned().unwrap_or(Value::Null);
    let source = snapshot.get("source").cloned().unwrap_or(Value::Null);
    let db_windows = list_windows(input.conn, &input.record.id)?;
    let windows = effective_windows(input, &db_windows);
    let timeline = list_timeline(input.conn, &input.record.id)?;
    let (counters, percentiles) = counters_and_percentiles(input, &windows)?;
    let timeseries = timeseries_points(input, &windows);
    let mut report = json!({
        "id": input.record.id,
        "title": title,
        "source": source,
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
    let verdict = verdict_from_totals(input.record, &windows, &counters, &percentiles);
    report["verdict"] = serde_json::to_value(&verdict)?;
    Ok(report)
}

/// Evaluate the run's frozen `assert` against current counters and percentiles.
///
/// # Errors
///
/// Returns an error if window or histogram data cannot be read.
pub fn compute_verdict(input: &ReportInput<'_>) -> Result<Verdict> {
    let db_windows = list_windows(input.conn, &input.record.id)?;
    let windows = effective_windows(input, &db_windows);
    let (counters, percentiles) = counters_and_percentiles(input, &windows)?;
    Ok(verdict_from_totals(
        input.record,
        &windows,
        &counters,
        &percentiles,
    ))
}

/// Windows used to report and time the run. While a multiprocess run is active
/// (including after `resume`, when `run_windows` only holds pre-resume rows) or
/// before consolidation, read the workers' window files so every completed and
/// in-progress window is visible and loop/downtime gaps never count towards `achieved_qps`.
fn effective_windows(input: &ReportInput<'_>, db_windows: &[Value]) -> Vec<Value> {
    let processes = u32::try_from(input.record.processes).unwrap_or(1);
    if processes > 1 && (status_is_running(&input.record.status) || db_windows.is_empty()) {
        let merged = crate::engine::merge_worker_windows(
            input.paths,
            &input.record.id,
            processes,
            input.record.ended_at,
        );
        if !merged.is_empty() {
            return merged;
        }
    }
    db_windows.to_vec()
}

fn verdict_from_totals(
    record: &RunRecord,
    windows: &[Value],
    counters: &Value,
    percentiles: &Value,
) -> Verdict {
    let snapshot = serde_json::from_str::<RunSnapshot>(&record.snapshot_json).ok();
    let spec: Option<AssertSpec> = snapshot.as_ref().and_then(|s| s.assert.clone());
    let configured = record
        .current_qps
        .and_then(|v| u64::try_from(v).ok())
        .or_else(|| {
            snapshot
                .as_ref()
                .and_then(|s| s.template.effective_total_qps())
                .and_then(|v| u64::try_from(v).ok())
        });
    let shape = snapshot
        .as_ref()
        .map_or_else(LoadShape::default, |s| LoadShape {
            warmup_ms: s.template.warmup_secs.saturating_mul(1000).max(0),
            ramp_ms: if s.template.model == MODEL_OPEN_LOOP {
                s.template.ramp_secs.saturating_mul(1000).max(0)
            } else {
                0
            },
        });
    let elapsed_ms = effective_elapsed_ms(record, windows, shape);
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

/// Per-window load shape used to turn wall-clock time into "full-rate" time.
#[derive(Clone, Copy, Default)]
struct LoadShape {
    warmup_ms: i64,
    /// Linear ramp length; 0 for closed-loop runs (ramp does not pace them).
    ramp_ms: i64,
}

impl LoadShape {
    /// Full-rate-equivalent milliseconds contained in the first `t` ms of a
    /// window: during the ramp the target rate is `t / ramp` of full, so the
    /// ramp contributes `t² / (2·ramp)`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    fn full_rate_ms(self, t: i64) -> i64 {
        let t = t.max(0);
        if self.ramp_ms <= 0 {
            return t;
        }
        if t <= self.ramp_ms {
            let tf = t as f64;
            return (tf * tf / (2.0 * self.ramp_ms as f64)).round() as i64;
        }
        self.ramp_ms / 2 + (t - self.ramp_ms)
    }

    /// Full-rate milliseconds of a window of `duration` ms that count towards
    /// `samples` (warmup samples are not counted, so warmup is excluded).
    fn counted_ms(self, duration: i64) -> i64 {
        let duration = duration.max(0);
        let warmup = self.warmup_ms.min(duration);
        self.full_rate_ms(duration) - self.full_rate_ms(warmup)
    }
}

/// Denominator for `achieved_qps`: sum of counted full-rate time over all
/// windows. Loop gaps between windows, warmup and the ramp deficit are
/// excluded so a perfect generator scores a ratio of ~1.0.
fn effective_elapsed_ms(record: &RunRecord, windows: &[Value], shape: LoadShape) -> Option<i64> {
    let now = i64::try_from(crate::db::unix_now_ms().unwrap_or(0)).unwrap_or(0);
    // An open window of a run that already ended (interrupted) closes at the
    // run's end, not at "now".
    let now = record.ended_at.unwrap_or(now);
    if windows.is_empty() {
        return elapsed_ms(record).map(|ms| shape.counted_ms(ms));
    }
    Some(windows_counted_ms(windows, shape, now))
}

/// Sum of counted full-rate time over window rows; open windows end at `now`.
///
/// Consecutive incomplete (`completed == false`) window segments produced by
/// mid-window stop/resume belong to the same logical round, so their active
/// durations are summed before subtracting per-round `warmup` / `ramp`.
fn windows_counted_ms(windows: &[Value], shape: LoadShape, now: i64) -> i64 {
    let mut total = 0_i64;
    let mut round_dur = 0_i64;
    let mut in_round = false;
    for w in windows {
        let Some(start) = w["started_at"].as_i64() else {
            continue;
        };
        let end = w["ended_at"].as_i64().unwrap_or(now);
        round_dur = round_dur.saturating_add(end.saturating_sub(start).max(0));
        in_round = true;
        let completed = w
            .get("completed")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| w["ended_at"].is_number());
        if completed {
            total = total.saturating_add(shape.counted_ms(round_dur));
            round_dur = 0;
            in_round = false;
        }
    }
    if in_round {
        total = total.saturating_add(shape.counted_ms(round_dur));
    }
    total
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
    let worker_fallback = if windows.is_empty() && input.live_counters.is_none() {
        Some(load_worker_checkpoint_totals(input.paths, &input.record.id))
    } else {
        None
    };

    let counters = if let Some(live) = input.live_counters {
        json!({
            "samples": live.samples,
            "success": live.success,
            "failure": live.failure,
            "expected": live.expected,
            "bytes": live.bytes,
            "dropped_details": live.dropped_details,
            "missed_sends": live.missed_sends,
        })
    } else if let Some((fallback_counters, _)) = worker_fallback.as_ref() {
        fallback_counters.clone()
    } else {
        sum_windows(windows)
    };

    let percentiles = if let Some((p50, p95, p99, max)) = input.live_percentiles {
        json!({ "p50_us": p50, "p95_us": p95, "p99_us": p99, "max_us": max })
    } else {
        let mut blobs = latest_checkpoint_blobs(input.conn, &input.record.id)?;
        if blobs.is_empty() {
            if let Some((_, fb_blobs)) = worker_fallback {
                blobs = fb_blobs;
            } else {
                let (_, fb_blobs) = load_worker_checkpoint_totals(input.paths, &input.record.id);
                blobs = fb_blobs;
            }
        }
        let (p50, p95, p99, max) =
            merge_percentiles(&blobs, 60_000_000).unwrap_or((None, None, None, None));
        json!({ "p50_us": p50, "p95_us": p99_or(p95), "p99_us": p99, "max_us": max })
    };
    Ok((counters, percentiles))
}

const fn p99_or(v: Option<u64>) -> Option<u64> {
    v
}

/// Sum latest checkpoints across worker SQLite shards (`wNN.sqlite`).
#[must_use]
pub fn load_worker_checkpoint_totals(paths: &DataPaths, run_id: &str) -> (Value, Vec<Vec<u8>>) {
    let mut samples = 0_u64;
    let mut success = 0_u64;
    let mut failure = 0_u64;
    let mut expected = 0_u64;
    let mut bytes = 0_u64;
    let mut dropped = 0_u64;
    let mut missed = 0_u64;
    let mut blobs = Vec::new();
    for idx in paths.worker_indexes(run_id) {
        let Ok(Some(conn)) = open_existing_worker(paths, run_id, idx, true) else {
            continue;
        };
        if let Ok(Some(cp)) = crate::db::runs::latest_worker_checkpoint(&conn) {
            if let Ok(cnt) = serde_json::from_str::<CounterSnapshot>(&cp.counters_json) {
                samples = samples.saturating_add(cnt.samples);
                success = success.saturating_add(cnt.success);
                failure = failure.saturating_add(cnt.failure);
                expected = expected.saturating_add(cnt.expected);
                bytes = bytes.saturating_add(cnt.bytes);
                dropped = dropped.saturating_add(cnt.dropped_details);
                missed = missed.saturating_add(cnt.missed_sends);
            }
            if let Some(blob) = cp.histogram_blob.filter(|b| !b.is_empty()) {
                blobs.push(blob);
            }
        }
    }
    (
        json!({
            "samples": samples,
            "success": success,
            "failure": failure,
            "expected": expected,
            "bytes": bytes,
            "dropped_details": dropped,
            "missed_sends": missed,
        }),
        blobs,
    )
}

fn sum_windows(windows: &[Value]) -> Value {
    let mut samples = 0_u64;
    let mut success = 0;
    let mut failure = 0;
    let mut expected = 0;
    let mut bytes = 0;
    let mut dropped = 0;
    let mut missed = 0;
    for window in windows {
        samples += window["samples"].as_u64().unwrap_or(0);
        success += window["success"].as_u64().unwrap_or(0);
        failure += window["failure"].as_u64().unwrap_or(0);
        expected += window["expected"].as_u64().unwrap_or(0);
        bytes += window["bytes"].as_u64().unwrap_or(0);
        dropped += window["dropped_details"].as_u64().unwrap_or(0);
        missed += window["missed_sends"].as_u64().unwrap_or(0);
    }
    json!({
        "samples": samples,
        "success": success,
        "failure": failure,
        "expected": expected,
        "bytes": bytes,
        "dropped_details": dropped,
        "missed_sends": missed,
    })
}

/// Load and merge persisted second-level [`RingSlot`] buckets across workers
/// from `run_checkpoints`, falling back to `wNN.sqlite` `worker_checkpoints`.
#[must_use]
pub fn load_persisted_timeseries(
    paths: &DataPaths,
    conn: &Connection,
    run_id: &str,
) -> Vec<RingSlot> {
    let mut raw_series = latest_checkpoint_timeseries(conn, run_id).unwrap_or_default();
    if raw_series.is_empty() {
        for idx in paths.worker_indexes(run_id) {
            let Ok(Some(wconn)) = open_existing_worker(paths, run_id, idx, true) else {
                continue;
            };
            if let Ok(Some(cp)) = crate::db::runs::latest_worker_checkpoint(&wconn)
                && let Some(ts_json) = cp.timeseries_json.filter(|s| !s.is_empty())
            {
                raw_series.push(ts_json);
            }
        }
    }
    merge_timeseries_slots(
        raw_series
            .into_iter()
            .filter_map(|raw| serde_json::from_str::<Vec<RingSlot>>(&raw).ok())
            .flatten(),
    )
}

#[must_use]
pub fn ring_slots_to_json(slots: &[RingSlot]) -> Vec<Value> {
    slots
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
        .collect()
}

fn timeseries_points(input: &ReportInput<'_>, windows: &[Value]) -> Vec<Value> {
    if let Some(slots) = input.live_ring.as_ref() {
        let sorted = merge_timeseries_slots(slots.iter().cloned());
        if !sorted.is_empty() {
            return ring_slots_to_json(&sorted);
        }
    }
    let persisted = load_persisted_timeseries(input.paths, input.conn, &input.record.id);
    if !persisted.is_empty() {
        return ring_slots_to_json(&persisted);
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
///
/// Unknown parameters are rejected so a client filter the server does not
/// implement fails loudly instead of being silently ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamplesQuery {
    pub product_id: Option<String>,
    pub success: Option<bool>,
    /// Exact match on `error_class` (e.g. `http`, `transport`, `assertion`).
    pub error_class: Option<String>,
    /// Only rows with `latency_us >= min_latency_us`.
    pub min_latency_us: Option<i64>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub after_end_unix_ms: Option<i64>,
    pub after_worker: Option<u32>,
    pub after_seq: Option<i64>,
    pub limit: Option<u32>,
}

/// Keyset cursor `(end_unix_ms, worker, seq)`; all three parts or none.
#[derive(Debug, Clone, Copy)]
struct SampleCursor {
    end_unix_ms: i64,
    worker: u32,
    seq: i64,
}

impl SamplesQuery {
    /// Validate the cursor and filter values.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message when the cursor is partial or a
    /// filter value is out of range.
    pub fn validate(&self) -> std::result::Result<(), String> {
        self.cursor()?;
        if self.min_latency_us.is_some_and(|v| v < 0) {
            return Err("min_latency_us must be >= 0".into());
        }
        Ok(())
    }

    fn cursor(&self) -> std::result::Result<Option<SampleCursor>, String> {
        match (self.after_end_unix_ms, self.after_worker, self.after_seq) {
            (None, None, None) => Ok(None),
            (Some(end_unix_ms), Some(worker), Some(seq)) => Ok(Some(SampleCursor {
                end_unix_ms,
                worker,
                seq,
            })),
            _ => {
                Err("cursor requires after_end_unix_ms, after_worker and after_seq together".into())
            }
        }
    }
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
/// Returns an error if the query is invalid (see [`SamplesQuery::validate`])
/// or a worker database cannot be read.
pub fn query_samples(
    paths: &DataPaths,
    run_id: &str,
    snapshot_json: &str,
    query: &SamplesQuery,
) -> Result<SamplesPage> {
    let cursor = query.cursor().map_err(anyhow::Error::msg)?;
    let secrets =
        collect_secrets(&serde_json::from_str::<Value>(snapshot_json).unwrap_or(Value::Null));
    let limit = query.limit.unwrap_or(100).clamp(1, 1000) as usize;
    let indexes = paths.worker_indexes(run_id);
    let mut merged = Vec::new();
    for index in indexes {
        let mut rows = load_worker_samples(paths, run_id, index, query, cursor, limit)?;
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
    cursor: Option<SampleCursor>,
    limit: usize,
) -> Result<Vec<SampleRow>> {
    let Some(conn) = open_existing_worker(paths, run_id, worker, true)? else {
        return Ok(Vec::new());
    };
    // Global order is (end_unix_ms, worker, seq). Within one worker DB the
    // worker is fixed, so the keyset predicate reduces to
    // `end > E OR (end = E AND seq > tie_seq)` where tie_seq depends on how
    // this worker compares to the cursor's worker.
    let (after_end, tie_seq) = match cursor {
        None => (None, i64::MIN),
        Some(c) => {
            let tie = match worker.cmp(&c.worker) {
                std::cmp::Ordering::Greater => i64::MIN,
                std::cmp::Ordering::Equal => c.seq,
                std::cmp::Ordering::Less => i64::MAX,
            };
            (Some(c.end_unix_ms), tie)
        }
    };
    let sql = "SELECT seq, start_unix_ms, end_unix_ms, latency_us, product_id, product_name,
                      request_index, http_status, success, expected, reason, error_class,
                      request_id, error, response_bytes, customer_name
               FROM samples
               WHERE (?1 IS NULL OR product_id = ?1)
                 AND (?2 IS NULL OR success = ?2)
                 AND (?3 IS NULL OR end_unix_ms >= ?3)
                 AND (?4 IS NULL OR end_unix_ms <= ?4)
                 AND (?5 IS NULL OR error_class = ?5)
                 AND (?6 IS NULL OR latency_us >= ?6)
                 AND (?7 IS NULL OR end_unix_ms > ?7 OR (end_unix_ms = ?7 AND seq > ?8))
               ORDER BY end_unix_ms, seq
               LIMIT ?9";
    let success = query.success.map(i64::from);
    let error_class = query
        .error_class
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let fetch = i64::try_from(limit + 1).unwrap_or(i64::MAX);
    let mut stmt = conn.prepare(sql).context("prepare samples query")?;
    let rows = stmt
        .query_map(
            params![
                query.product_id.as_deref(),
                success,
                query.from_ms,
                query.to_ms,
                error_class,
                query.min_latency_us,
                after_end,
                tie_seq,
                fetch,
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("read samples")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::redact::{contains_any_secret, redact_value};

    #[test]
    fn effective_elapsed_excludes_warmup_ramp_deficit_and_loop_gaps() {
        let shape = LoadShape {
            warmup_ms: 5_000,
            ramp_ms: 10_000,
        };
        // 60s window, 10s ramp => 5s ramp-equivalent + 50s = 55s full-rate,
        // minus warmup (first 5s of the ramp = 5²/(2·10) = 1.25s) => 53.75s.
        assert_eq!(shape.counted_ms(60_000), 53_750);
        // Window shorter than warmup counts nothing.
        assert_eq!(shape.counted_ms(3_000), 0);
        let closed = LoadShape {
            warmup_ms: 2_000,
            ramp_ms: 0,
        };
        assert_eq!(closed.counted_ms(10_000), 8_000);
        // Two 10s windows separated by a long loop gap: only window time counts.
        let windows = vec![
            json!({"started_at": 1_000, "ended_at": 11_000}),
            json!({"started_at": 600_000, "ended_at": 610_000}),
        ];
        assert_eq!(windows_counted_ms(&windows, closed, 0), 16_000);
        // An open window is measured up to `now`.
        let open = vec![json!({"started_at": 1_000, "ended_at": null})];
        assert_eq!(windows_counted_ms(&open, closed, 6_000), 3_000);
    }

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
