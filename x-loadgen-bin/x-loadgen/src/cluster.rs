//! Multi-process launcher: spawn N worker processes and merge their reports.

use anyhow::{Context, Result, bail};
use mazbri_core::num::u64_to_f64;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ClusterLaunch {
    pub processes: usize,
    pub config_path: PathBuf,
    pub worker_extra_args: Vec<String>,
    pub aggregate_dir: PathBuf,
    pub run_id: String,
}

#[derive(Debug)]
pub struct WorkerResult {
    pub index: usize,
    pub exit_code: i32,
    pub report_dir: PathBuf,
}

/// Re-exec this binary as workers with `--worker-index` / `--worker-count`.
pub fn run_cluster(launch: &ClusterLaunch) -> Result<Vec<WorkerResult>> {
    if launch.processes == 0 {
        bail!("cluster.processes must be >= 1");
    }
    let exe = std::env::current_exe().context("current_exe")?;
    fs::create_dir_all(&launch.aggregate_dir)?;
    let started = Instant::now();
    let mut children = Vec::with_capacity(launch.processes);
    for index in 0..launch.processes {
        let worker_run_id = format!("{}-w{index:02}", launch.run_id);
        let report_dir = launch.aggregate_dir.join(&worker_run_id);
        fs::create_dir_all(&report_dir)?;
        let mut cmd = Command::new(&exe);
        cmd.arg("--config")
            .arg(&launch.config_path)
            .arg("--worker-index")
            .arg(index.to_string())
            .arg("--worker-count")
            .arg(launch.processes.to_string())
            .arg("--run-id")
            .arg(&worker_run_id)
            .arg("--report-dir")
            .arg(launch.aggregate_dir.as_os_str());
        for a in &launch.worker_extra_args {
            cmd.arg(a);
        }
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        info!(index, "spawning loadgen worker");
        let child = cmd
            .spawn()
            .with_context(|| format!("spawn worker {index}"))?;
        children.push((index, report_dir, child));
    }
    let mut results = Vec::with_capacity(children.len());
    for (index, report_dir, mut child) in children {
        let status = child
            .wait()
            .with_context(|| format!("wait worker {index}"))?;
        results.push(WorkerResult {
            index,
            exit_code: status.code().unwrap_or(-1),
            report_dir,
        });
    }
    info!(
        workers = results.len(),
        elapsed_s = started.elapsed().as_secs_f64(),
        "cluster workers finished"
    );
    Ok(results)
}

/// Merge worker `data/summary.json` files into one aggregate summary.
pub fn aggregate_summaries(
    results: &[WorkerResult],
    out_dir: &Path,
    run_id: &str,
) -> Result<PathBuf> {
    let data_dir = out_dir.join(run_id).join("data");
    fs::create_dir_all(&data_dir)?;
    let summaries = collect_worker_summaries(results, out_dir, run_id)?;
    let aggregate = merge_summaries(&summaries, results, run_id);
    let out = data_dir.join("cluster-summary.json");
    fs::write(&out, serde_json::to_string_pretty(&aggregate)?)?;
    let overview = aggregate.get("overview").cloned().unwrap_or(Value::Null);
    println!(
        "cluster aggregate samples={} success={} failure={} avg_rps={:.2} duration_s={:.2} -> {}",
        overview.get("samples").and_then(Value::as_u64).unwrap_or(0),
        overview.get("success").and_then(Value::as_u64).unwrap_or(0),
        overview.get("failure").and_then(Value::as_u64).unwrap_or(0),
        overview
            .get("avg_rps")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        aggregate
            .get("duration_s")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        out.display()
    );
    Ok(out)
}

fn collect_worker_summaries(
    results: &[WorkerResult],
    out_dir: &Path,
    run_id: &str,
) -> Result<Vec<Value>> {
    let mut summaries = Vec::new();
    for w in results {
        let path = w.report_dir.join("data").join("summary.json");
        if path.is_file() {
            let text =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            summaries.push(serde_json::from_str(&text)?);
        }
    }
    if summaries.is_empty() {
        for w in results {
            let path = out_dir
                .join(format!("{}-w{:02}", run_id, w.index))
                .join("data/summary.json");
            if path.is_file() {
                let text = fs::read_to_string(&path)?;
                summaries.push(serde_json::from_str(&text)?);
            }
        }
    }
    if summaries.is_empty() {
        bail!("no worker summary.json found to aggregate");
    }
    Ok(summaries)
}

fn merge_summaries(summaries: &[Value], results: &[WorkerResult], run_id: &str) -> Value {
    let mut acc = OverviewAccum::new();
    for s in summaries {
        acc.absorb(s);
    }

    let duration_s = if acc.max_end > acc.min_start {
        u64_to_f64(acc.max_end - acc.min_start) / 1000.0
    } else {
        0.001
    };
    let avg_rps = u64_to_f64(acc.total_samples) / duration_s.max(0.001);
    let mean = |xs: &[u64]| -> f64 {
        if xs.is_empty() {
            0.0
        } else {
            let n = u64::try_from(xs.len()).unwrap_or(1);
            u64_to_f64(xs.iter().sum::<u64>()) / u64_to_f64(n)
        }
    };

    let ok_rate = if acc.total_samples == 0 {
        0.0
    } else {
        u64_to_f64(acc.total_ok) / u64_to_f64(acc.total_samples)
    };

    serde_json::json!({
        "schema_version": 2,
        "run_id": run_id,
        "title": format!("cluster aggregate ({})", summaries.len()),
        "workers": results.iter().map(|w| serde_json::json!({
            "index": w.index,
            "exit_code": w.exit_code,
            "report_dir": w.report_dir,
        })).collect::<Vec<_>>(),
        "started_unix_ms": acc.min_start,
        "ended_unix_ms": acc.max_end,
        "duration_s": duration_s,
        "overview": {
            "samples": acc.total_samples,
            "success": acc.total_ok,
            "failure": acc.total_fail,
            "expected": acc.total_exp,
            "business_ok_rate": ok_rate,
            "avg_rps": avg_rps,
        },
        "latency": {
            "note": "mean of per-worker percentiles (each worker has full latency distribution in its own summary)",
            "p50_mean_of_workers": mean(&acc.lat_p50),
            "p95_mean_of_workers": mean(&acc.lat_p95),
            "p99_mean_of_workers": mean(&acc.lat_p99),
        },
        "status_codes": acc.status_merged,
    })
}

struct OverviewAccum {
    total_samples: u64,
    total_ok: u64,
    total_fail: u64,
    total_exp: u64,
    min_start: u64,
    max_end: u64,
    lat_p50: Vec<u64>,
    lat_p95: Vec<u64>,
    lat_p99: Vec<u64>,
    status_merged: serde_json::Map<String, Value>,
}

impl OverviewAccum {
    fn new() -> Self {
        Self {
            total_samples: 0,
            total_ok: 0,
            total_fail: 0,
            total_exp: 0,
            min_start: u64::MAX,
            max_end: 0,
            lat_p50: Vec::new(),
            lat_p95: Vec::new(),
            lat_p99: Vec::new(),
            status_merged: serde_json::Map::new(),
        }
    }

    fn absorb(&mut self, s: &Value) {
        if let Some(o) = s.get("overview") {
            self.total_samples += o.get("samples").and_then(Value::as_u64).unwrap_or(0);
            self.total_ok += o.get("success").and_then(Value::as_u64).unwrap_or(0);
            self.total_fail += o.get("failure").and_then(Value::as_u64).unwrap_or(0);
            self.total_exp += o.get("expected").and_then(Value::as_u64).unwrap_or(0);
        }
        if let Some(v) = s.get("started_unix_ms").and_then(Value::as_u64) {
            self.min_start = self.min_start.min(v);
        }
        if let Some(v) = s.get("ended_unix_ms").and_then(Value::as_u64) {
            self.max_end = self.max_end.max(v);
        }
        if let Some(lat) = s.get("latency") {
            if let Some(v) = lat.get("p50").and_then(Value::as_u64) {
                self.lat_p50.push(v);
            }
            if let Some(v) = lat.get("p95").and_then(Value::as_u64) {
                self.lat_p95.push(v);
            }
            if let Some(v) = lat.get("p99").and_then(Value::as_u64) {
                self.lat_p99.push(v);
            }
        }
        merge_status_codes(s, &mut self.status_merged);
    }
}

/// worker `summary.json` 的 `status_codes` 为 `[{"name": "200", "count": N}, ...]`，按 name 累加。
fn merge_status_codes(s: &Value, status_merged: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(arr)) = s.get("status_codes") else {
        return;
    };
    for item in arr {
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let n = item.get("count").and_then(Value::as_u64).unwrap_or(0);
        let cur = status_merged.get(name).and_then(Value::as_u64).unwrap_or(0);
        status_merged.insert(name.to_string(), Value::from(cur + n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_status_codes_sums_by_name() {
        let mut merged = serde_json::Map::new();
        for s in [
            serde_json::json!({"status_codes": [{"name": "200", "count": 3}, {"name": "202", "count": 1}]}),
            serde_json::json!({"status_codes": [{"name": "200", "count": 2}]}),
        ] {
            merge_status_codes(&s, &mut merged);
        }
        assert_eq!(merged.get("200").and_then(Value::as_u64), Some(5));
        assert_eq!(merged.get("202").and_then(Value::as_u64), Some(1));
        assert_eq!(merged.len(), 2);
    }
}
