//! Side-by-side A/B comparison between two load-test reports (`base` vs `target`).

use std::collections::BTreeMap;

use serde_json::{Value, json};

/// Build a comparison payload from two already-built redacted reports.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn build_comparison(base: &Value, target: &Value) -> Value {
    let metrics = vec![
        diff_metric(
            "achieved_qps",
            "平均 QPS",
            "req/s",
            num(&base["summary"]["achieved_qps"]),
            num(&target["summary"]["achieved_qps"]),
            Some(true),
        ),
        diff_metric(
            "peak_qps",
            "峰值 QPS",
            "req/s",
            num(&base["summary"]["peak_qps"]),
            num(&target["summary"]["peak_qps"]),
            Some(true),
        ),
        diff_metric(
            "success_rate",
            "成功率",
            "%",
            num(&base["summary"]["success_rate"]).map(|v| v * 100.0),
            num(&target["summary"]["success_rate"]).map(|v| v * 100.0),
            Some(true),
        ),
        diff_metric(
            "error_rate",
            "错误率",
            "%",
            num(&base["summary"]["error_rate"]).map(|v| v * 100.0),
            num(&target["summary"]["error_rate"]).map(|v| v * 100.0),
            Some(false),
        ),
        diff_metric(
            "apdex",
            "Apdex 评分",
            "",
            num(&base["summary"]["apdex"]["score"]),
            num(&target["summary"]["apdex"]["score"]),
            Some(true),
        ),
        diff_metric(
            "avg_us",
            "平均延迟",
            "us",
            num(&base["percentiles"]["avg_us"]),
            num(&target["percentiles"]["avg_us"]),
            Some(false),
        ),
        diff_metric(
            "p50_us",
            "P50 延迟",
            "us",
            num(&base["percentiles"]["p50_us"]),
            num(&target["percentiles"]["p50_us"]),
            Some(false),
        ),
        diff_metric(
            "p90_us",
            "P90 延迟",
            "us",
            num(&base["percentiles"]["p90_us"]),
            num(&target["percentiles"]["p90_us"]),
            Some(false),
        ),
        diff_metric(
            "p95_us",
            "P95 延迟",
            "us",
            num(&base["percentiles"]["p95_us"]),
            num(&target["percentiles"]["p95_us"]),
            Some(false),
        ),
        diff_metric(
            "p99_us",
            "P99 延迟",
            "us",
            num(&base["percentiles"]["p99_us"]),
            num(&target["percentiles"]["p99_us"]),
            Some(false),
        ),
        diff_metric(
            "max_us",
            "最大延迟",
            "us",
            num(&base["percentiles"]["max_us"]),
            num(&target["percentiles"]["max_us"]),
            Some(false),
        ),
        diff_metric(
            "avg_bandwidth_bps",
            "平均下行带宽",
            "B/s",
            num(&base["summary"]["avg_bandwidth_bps"]),
            num(&target["summary"]["avg_bandwidth_bps"]),
            None,
        ),
    ];

    let config_diff = json!({
        "base": extract_config_summary(base),
        "target": extract_config_summary(target),
    });

    let products_diff = build_products_diff(base, target);
    let aligned_series = build_aligned_series(base, target);

    json!({
        "base": compact_report_head(base),
        "target": compact_report_head(target),
        "config_diff": config_diff,
        "metrics": metrics,
        "products_diff": products_diff,
        "aligned_series": aligned_series,
    })
}

fn compact_report_head(r: &Value) -> Value {
    json!({
        "id": r["id"],
        "title": r["title"],
        "status": r["status"],
        "mode": r["mode"],
        "started_at": r["started_at"],
        "ended_at": r["ended_at"],
        "processes": r["processes"],
        "source": r["source"],
        "verdict": r["verdict"],
        "counters": r["counters"],
        "percentiles": r["percentiles"],
        "summary": r["summary"],
    })
}

fn extract_config_summary(r: &Value) -> Value {
    let tpl = &r["snapshot"]["template"];
    json!({
        "model": tpl["model"],
        "qps": r["current_qps"],
        "concurrency": r["current_concurrency"],
        "processes": r["processes"],
        "mode": r["mode"],
        "duration_secs": tpl["duration_secs"],
        "requests": tpl["requests"],
        "warmup_secs": tpl["warmup_secs"],
        "ramp_secs": tpl["ramp_secs"],
        "environment_name": r["source"]["environment_name"],
        "protocols": r["summary"]["protocols"],
    })
}

fn diff_metric(
    key: &str,
    label: &str,
    unit: &str,
    base: Option<f64>,
    target: Option<f64>,
    higher_is_better: Option<bool>,
) -> Value {
    let delta = match (base, target) {
        (Some(b), Some(t)) => Some(round4(t - b)),
        _ => None,
    };
    let delta_pct = match (base, target) {
        (Some(b), Some(t)) if b.abs() > 1e-9 => Some(round2(((t - b) / b.abs()) * 100.0)),
        _ => None,
    };
    let status = match (delta, higher_is_better) {
        (Some(d), Some(true)) if d > 1e-6 => "improved",
        (Some(d), Some(true)) if d < -1e-6 => "regressed",
        (Some(d), Some(false)) if d < -1e-6 => "improved",
        (Some(d), Some(false)) if d > 1e-6 => "regressed",
        _ => "neutral",
    };
    json!({
        "key": key,
        "label": label,
        "unit": unit,
        "base": base.map(round4),
        "target": target.map(round4),
        "delta": delta,
        "delta_pct": delta_pct,
        "higher_is_better": higher_is_better,
        "status": status,
    })
}

fn build_products_diff(base: &Value, target: &Value) -> Vec<Value> {
    let mut base_map: BTreeMap<String, &Value> = BTreeMap::new();
    let mut target_map: BTreeMap<String, &Value> = BTreeMap::new();
    let mut keys: Vec<String> = Vec::new();
    if let Some(arr) = base["breakdown"]["products"].as_array() {
        for item in arr {
            if let Some(id) = item["product_id"].as_str() {
                base_map.insert(id.to_string(), item);
                keys.push(id.to_string());
            }
        }
    }
    if let Some(arr) = target["breakdown"]["products"].as_array() {
        for item in arr {
            if let Some(id) = item["product_id"].as_str() {
                target_map.insert(id.to_string(), item);
                if !base_map.contains_key(id) {
                    keys.push(id.to_string());
                }
            }
        }
    }
    keys.into_iter()
        .map(|k| {
            let b = base_map.get(&k).copied();
            let t = target_map.get(&k).copied();
            let bp95 = b.and_then(|v| num(&v["p95_us"]));
            let tp95 = t.and_then(|v| num(&v["p95_us"]));
            let p95_delta_pct = match (bp95, tp95) {
                (Some(bv), Some(tv)) if bv > 0.0 => Some(round2(((tv - bv) / bv) * 100.0)),
                _ => None,
            };
            json!({
                "product_id": k,
                "base_samples": b.and_then(|v| v["samples"].as_u64()),
                "target_samples": t.and_then(|v| v["samples"].as_u64()),
                "base_qps": b.and_then(|v| num(&v["qps"])),
                "target_qps": t.and_then(|v| num(&v["qps"])),
                "base_p50_us": b.and_then(|v| num(&v["p50_us"])),
                "target_p50_us": t.and_then(|v| num(&v["p50_us"])),
                "base_p95_us": bp95,
                "target_p95_us": tp95,
                "p95_delta_pct": p95_delta_pct,
                "base_p99_us": b.and_then(|v| num(&v["p99_us"])),
                "target_p99_us": t.and_then(|v| num(&v["p99_us"])),
                "base_error_rate": b.and_then(|v| num(&v["error_rate"])),
                "target_error_rate": t.and_then(|v| num(&v["error_rate"])),
            })
        })
        .collect()
}

fn build_aligned_series(base: &Value, target: &Value) -> Vec<Value> {
    let b_pts = base["timeseries"].as_array();
    let t_pts = target["timeseries"].as_array();
    let b_len = b_pts.map_or(0, Vec::len);
    let t_len = t_pts.map_or(0, Vec::len);
    let len = b_len.max(t_len);
    let mut out = Vec::with_capacity(len);
    for idx in 0..len {
        let bp = b_pts.and_then(|a| a.get(idx));
        let tp = t_pts.and_then(|a| a.get(idx));
        out.push(json!({
            "offset_sec": idx,
            "base_qps": bp.and_then(|v| v["samples"].as_u64()),
            "target_qps": tp.and_then(|v| v["samples"].as_u64()),
            "base_p95_us": bp.and_then(|v| num(&v["p95_us"])),
            "target_p95_us": tp.and_then(|v| num(&v["p95_us"])),
            "base_p99_us": bp.and_then(|v| num(&v["p99_us"])),
            "target_p99_us": tp.and_then(|v| num(&v["p99_us"])),
            "base_error_rate": bp.and_then(|v| num(&v["error_rate"])),
            "target_error_rate": tp.and_then(|v| num(&v["error_rate"])),
        }));
    }
    out
}

#[allow(clippy::cast_precision_loss)]
fn num(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|n| n as f64))
        .or_else(|| v.as_u64().map(|n| n as f64))
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}
