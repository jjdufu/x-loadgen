//! Multi-process workers, QPS/request splits, and merged live stats.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use hdrhistogram::serialization::Deserializer;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::db::runs::{STATUS_INTERRUPTED, STATUS_STOPPED};
use crate::db::{DataPaths, StoreError};
use crate::engine::snapshot::RunSnapshot;
use crate::engine::supervisor::{RunningTask, StopKind, Supervisor};

/// Per-worker share of a *total* QPS or request budget.
///
/// The remainder is given to the first `total % worker_count` workers so the
/// shares always sum back to `total`.
#[must_use]
pub fn apply_worker_split(total: u64, worker_index: u32, worker_count: u32) -> u64 {
    let count = u64::from(worker_count.max(1));
    let index = u64::from(worker_index);
    let base = total / count;
    let remainder = total % count;
    if index < remainder {
        base.saturating_add(1)
    } else {
        base
    }
}

/// Live snapshot written atomically by each worker every second.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LiveSnapshot {
    pub ts: i64,
    pub window_index: i64,
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub bytes: u64,
    pub dropped_details: u64,
    pub in_flight: u64,
    pub peak_in_flight: u64,
    pub current_qps: u64,
    pub current_concurrency: u64,
    pub p50_us: Option<u64>,
    pub p95_us: Option<u64>,
    pub p99_us: Option<u64>,
    pub max_us: Option<u64>,
    /// V2-serialized `HdrHistogram`, base64-encoded. Empty when unused.
    #[serde(default)]
    pub histogram_b64: Option<String>,
    /// Open-loop sends dropped for falling too far behind schedule.
    pub missed_sends: u64,
    /// Chronologically ordered non-empty per-second throughput buckets.
    #[serde(default)]
    pub timeseries: Vec<crate::engine::RingSlot>,
}

/// Merged view of every worker's live snapshot for one run.
#[derive(Debug, Clone, Default)]
pub struct MergedStats {
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub bytes: u64,
    pub dropped_details: u64,
    pub in_flight: u64,
    pub peak_in_flight: u64,
    pub current_qps: u64,
    pub current_concurrency: u64,
    pub p50_us: Option<u64>,
    pub p95_us: Option<u64>,
    pub p99_us: Option<u64>,
    pub max_us: Option<u64>,
    pub latency_stats: super::metrics::LatencyStats,
    pub window_index: Option<i64>,
    pub missed_sends: u64,
    pub timeseries: Vec<crate::engine::RingSlot>,
}

/// Merge live snapshots: counters are summed, percentiles come from a histogram merge.
///
/// # Errors
///
/// Returns an error if a histogram blob cannot be decoded or merged.
pub fn merge_live(snaps: &[LiveSnapshot], highest_trackable_us: u64) -> Result<MergedStats> {
    let mut merged = MergedStats::default();
    if snaps.is_empty() {
        return Ok(merged);
    }
    let mut blobs = Vec::new();
    for snap in snaps {
        merged.samples = merged.samples.saturating_add(snap.samples);
        merged.success = merged.success.saturating_add(snap.success);
        merged.failure = merged.failure.saturating_add(snap.failure);
        merged.expected = merged.expected.saturating_add(snap.expected);
        merged.bytes = merged.bytes.saturating_add(snap.bytes);
        merged.dropped_details = merged.dropped_details.saturating_add(snap.dropped_details);
        merged.missed_sends = merged.missed_sends.saturating_add(snap.missed_sends);
        merged.in_flight = merged.in_flight.saturating_add(snap.in_flight);
        merged.peak_in_flight = merged.peak_in_flight.max(snap.peak_in_flight);
        merged.current_qps = merged.current_qps.saturating_add(snap.current_qps);
        merged.current_concurrency = merged
            .current_concurrency
            .saturating_add(snap.current_concurrency);
        merged.window_index = Some(
            merged
                .window_index
                .unwrap_or(snap.window_index)
                .max(snap.window_index),
        );
        if let Some(b64) = snap.histogram_b64.as_deref()
            && !b64.is_empty()
            && let Ok(bytes) = BASE64.decode(b64)
        {
            blobs.push(bytes);
        }
    }
    merged.timeseries = crate::engine::merge_timeseries_slots(
        snaps
            .iter()
            .flat_map(|snap| snap.timeseries.iter().cloned()),
    );
    if let Ok(stats) = merge_latency_stats(&blobs, highest_trackable_us) {
        merged.latency_stats = stats;
        merged.p50_us = stats.p50_us;
        merged.p95_us = stats.p95_us;
        merged.p99_us = stats.p99_us;
        merged.max_us = stats.max_us;
    }
    Ok(merged)
}

/// Percentiles `(p50, p95, p99, max)` in microseconds.
pub type HistogramPercentiles = (Option<u64>, Option<u64>, Option<u64>, Option<u64>);

/// Decode each V2 blob and add them into one histogram, then read full [`super::metrics::LatencyStats`].
///
/// # Errors
///
/// Returns an error if a blob is not a valid histogram or the histograms cannot
/// be added together.
pub fn merge_latency_stats(
    blobs: &[Vec<u8>],
    highest_trackable_us: u64,
) -> Result<super::metrics::LatencyStats> {
    if blobs.is_empty() {
        return Ok(super::metrics::LatencyStats::default());
    }
    let mut acc = super::metrics::new_latency_histogram(highest_trackable_us)
        .context("create merge histogram")?;
    let mut deserializer = Deserializer::new();
    for blob in blobs {
        let mut cursor = std::io::Cursor::new(blob.as_slice());
        let next = deserializer
            .deserialize(&mut cursor)
            .context("deserialize worker histogram")?;
        acc.add(&next).context("merge worker histogram")?;
    }
    Ok(super::metrics::LatencyStats::from_histogram(&acc))
}

/// Decode each V2 blob and add them into one histogram, then read percentiles.
///
/// `highest_trackable_us` is only the initial size of the accumulator: it
/// auto-resizes, so blobs containing samples above the ceiling (for example a
/// run with a timeout longer than 60 s) still merge instead of failing.
///
/// # Errors
///
/// Returns an error if a blob is not a valid histogram or the histograms cannot
/// be added together.
pub fn merge_percentiles(
    blobs: &[Vec<u8>],
    highest_trackable_us: u64,
) -> Result<HistogramPercentiles> {
    Ok(merge_latency_stats(blobs, highest_trackable_us)?.tuple4())
}

/// Encode a histogram blob as standard base64.
#[must_use]
pub fn histogram_to_b64(blob: &[u8]) -> String {
    BASE64.encode(blob)
}

const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Write `path` atomically (temp file in the same directory, then rename).
///
/// # Errors
///
/// Returns an error if the file cannot be written or renamed.
pub fn write_atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(value).context("serialize live snapshot")?;
    fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

/// Load every `wNN-live.json` under a run directory, newest-first per worker.
///
/// # Errors
///
/// Returns an I/O error if the directory cannot be read. Missing files are skipped.
#[must_use]
pub fn load_live_snapshots(paths: &DataPaths, run_id: &str, processes: u32) -> Vec<LiveSnapshot> {
    let mut snaps = Vec::new();
    for index in 0..processes {
        let path = paths.worker_live(run_id, index);
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        if let Ok(snap) = serde_json::from_slice::<LiveSnapshot>(&bytes) {
            snaps.push(snap);
        }
    }
    snaps
}

/// One worker's record of one window, persisted in `wNN-windows.json`.
///
/// Written with `ended_at = None` when the window starts and rewritten with
/// the final counters when it ends, so the server always knows which part of
/// the run's wall-clock time was actually spent sending (loop gaps excluded).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerWindowRecord {
    pub window_index: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub bytes: u64,
    pub dropped_details: u64,
    pub missed_sends: u64,
    /// V2-serialized window histogram, base64-encoded.
    pub histogram_b64: Option<String>,
    /// True when this worker finished the window naturally (not cancelled or crashed).
    #[serde(default)]
    pub completed: bool,
}

/// Shared resume parameters computed before spawning worker processes so every
/// shard starts at the exact same window index and remaining round budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterResumePlan {
    /// Zero-based index of the first window to open in this session.
    pub start_window: i64,
    /// Number of rounds that completed naturally across all workers prior to this session.
    pub completed_rounds: u64,
    /// Active wall-clock milliseconds already spent in the current incomplete round.
    pub partial_elapsed_ms: u64,
    /// Highest `window_index` with `completed == true`, or `-1` when none completed yet.
    pub last_completed_window: i64,
}

impl Default for ClusterResumePlan {
    fn default() -> Self {
        Self {
            start_window: 0,
            completed_rounds: 0,
            partial_elapsed_ms: 0,
            last_completed_window: -1,
        }
    }
}

impl ClusterResumePlan {
    /// Derive the resume plan from prior merged window rows.
    #[must_use]
    pub fn from_windows(windows: &[serde_json::Value], now_ms: i64) -> Self {
        if windows.is_empty() {
            return Self::default();
        }
        let max_index = windows
            .iter()
            .filter_map(|w| w["window_index"].as_i64())
            .max();
        let Some(max_index) = max_index else {
            return Self::default();
        };
        let mut completed_rounds = 0_u64;
        let mut last_completed_window = -1_i64;
        for w in windows {
            if w["completed"].as_bool() == Some(true) {
                completed_rounds = completed_rounds.saturating_add(1);
                if let Some(idx) = w["window_index"].as_i64() {
                    last_completed_window = last_completed_window.max(idx);
                }
            }
        }
        let mut partial_elapsed_ms = 0_u64;
        for w in windows {
            let Some(idx) = w["window_index"].as_i64() else {
                continue;
            };
            if idx > last_completed_window {
                let start = w["started_at"].as_i64().unwrap_or(now_ms);
                let end = w["ended_at"].as_i64().unwrap_or(now_ms);
                let dur = u64::try_from(end.saturating_sub(start).max(0)).unwrap_or(0);
                partial_elapsed_ms = partial_elapsed_ms.saturating_add(dur);
            }
        }
        Self {
            start_window: max_index.saturating_add(1),
            completed_rounds,
            partial_elapsed_ms,
            last_completed_window,
        }
    }
}

/// Read a worker's window records (missing or unreadable file → empty).
#[must_use]
pub fn load_worker_window_records(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
) -> Vec<WorkerWindowRecord> {
    fs::read(paths.worker_windows(run_id, worker_index))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Seal any unclosed (`ended_at == None`) window records across all workers of a
/// run, recovering counter deltas from each worker's latest SQLite checkpoint.
pub fn seal_worker_windows(paths: &DataPaths, run_id: &str, processes: u32, fallback_end_ms: i64) {
    for worker in 0..processes {
        seal_single_worker_windows(paths, run_id, worker, fallback_end_ms);
    }
}

/// Seal any unclosed (`ended_at == None`) window records for a single worker shard.
pub fn seal_single_worker_windows(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
    fallback_end_ms: i64,
) {
    let mut records = load_worker_window_records(paths, run_id, worker_index);
    if !records.iter().any(|r| r.ended_at.is_none()) {
        return;
    }
    let mut closed_samples = 0_u64;
    let mut closed_success = 0_u64;
    let mut closed_failure = 0_u64;
    let mut closed_expected = 0_u64;
    let mut closed_bytes = 0_u64;
    let mut closed_dropped = 0_u64;
    let mut closed_missed = 0_u64;
    for r in records.iter().filter(|r| r.ended_at.is_some()) {
        closed_samples = closed_samples.saturating_add(r.samples);
        closed_success = closed_success.saturating_add(r.success);
        closed_failure = closed_failure.saturating_add(r.failure);
        closed_expected = closed_expected.saturating_add(r.expected);
        closed_bytes = closed_bytes.saturating_add(r.bytes);
        closed_dropped = closed_dropped.saturating_add(r.dropped_details);
        closed_missed = closed_missed.saturating_add(r.missed_sends);
    }

    let checkpoint = crate::db::open_existing_worker(paths, run_id, worker_index, true)
        .ok()
        .flatten()
        .and_then(|conn| {
            crate::db::runs::latest_worker_checkpoint(&conn)
                .ok()
                .flatten()
        });
    let cp_ts = checkpoint.as_ref().map(|cp| cp.ts);
    let cp_counters = checkpoint
        .as_ref()
        .and_then(|cp| serde_json::from_str::<super::CounterSnapshot>(&cp.counters_json).ok());

    let last_open_pos = records.iter().rposition(|r| r.ended_at.is_none());
    for (pos, r) in records.iter_mut().enumerate() {
        if r.ended_at.is_none() {
            let end = cp_ts
                .filter(|&ts| ts >= r.started_at)
                .unwrap_or(fallback_end_ms)
                .max(r.started_at);
            r.ended_at = Some(end);
            r.completed = false;
            if Some(pos) == last_open_pos
                && r.samples == 0
                && let Some(cnt) = cp_counters.as_ref()
            {
                r.samples = cnt.samples.saturating_sub(closed_samples);
                r.success = cnt.success.saturating_sub(closed_success);
                r.failure = cnt.failure.saturating_sub(closed_failure);
                r.expected = cnt.expected.saturating_sub(closed_expected);
                r.bytes = cnt.bytes.saturating_sub(closed_bytes);
                r.dropped_details = cnt.dropped_details.saturating_sub(closed_dropped);
                r.missed_sends = cnt.missed_sends.saturating_sub(closed_missed);
            }
        }
    }
    if let Err(err) = write_atomic_json(&paths.worker_windows(run_id, worker_index), &records) {
        warn!(error = %err, worker = worker_index, "seal worker windows failed");
    }
}

/// Insert or replace (same `window_index` and `started_at`) one record and
/// rewrite the worker's window file atomically.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn upsert_worker_window_record(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
    record: WorkerWindowRecord,
) -> Result<()> {
    let mut records = load_worker_window_records(paths, run_id, worker_index);
    if let Some(existing) = records
        .iter_mut()
        .find(|r| r.window_index == record.window_index && r.started_at == record.started_at)
    {
        *existing = record;
    } else {
        records.push(record);
    }
    write_atomic_json(&paths.worker_windows(run_id, worker_index), &records)
}

/// Merge every worker's window records into one row per `window_index`, in
/// the same JSON shape as `run_windows` rows.
///
/// A merged window spans the earliest start to the latest end across workers;
/// it stays open (`ended_at = null`) while any worker's part is still open,
/// unless `close_open_at` is given (used when consolidating a finished run).
#[must_use]
pub fn merge_worker_windows(
    paths: &DataPaths,
    run_id: &str,
    processes: u32,
    close_open_at: Option<i64>,
) -> Vec<serde_json::Value> {
    let mut by_index: std::collections::BTreeMap<i64, Vec<WorkerWindowRecord>> =
        std::collections::BTreeMap::new();
    for worker in 0..processes {
        for record in load_worker_window_records(paths, run_id, worker) {
            by_index
                .entry(record.window_index)
                .or_default()
                .push(record);
        }
    }
    by_index
        .into_iter()
        .map(|(window_index, parts)| {
            merge_window_parts(window_index, &parts, processes, close_open_at)
        })
        .collect()
}

fn merge_window_parts(
    window_index: i64,
    parts: &[WorkerWindowRecord],
    processes: u32,
    close_open_at: Option<i64>,
) -> serde_json::Value {
    let started_at = parts.iter().map(|p| p.started_at).min().unwrap_or(0);
    let ended_at = if parts.iter().any(|p| p.ended_at.is_none()) {
        close_open_at
    } else {
        parts.iter().filter_map(|p| p.ended_at).max()
    };
    let completed = u32::try_from(parts.len()).ok() == Some(processes.max(1))
        && parts.iter().all(|p| p.ended_at.is_some() && p.completed);
    let sum =
        |f: fn(&WorkerWindowRecord) -> u64| parts.iter().map(f).fold(0_u64, u64::saturating_add);
    let blobs: Vec<Vec<u8>> = parts
        .iter()
        .filter_map(|p| p.histogram_b64.as_deref())
        .filter(|b64| !b64.is_empty())
        .filter_map(|b64| BASE64.decode(b64).ok())
        .collect();
    let (p50, p95, p99, max) =
        merge_percentiles(&blobs, 60_000_000).unwrap_or((None, None, None, None));
    serde_json::json!({
        "window_index": window_index,
        "started_at": started_at,
        "ended_at": ended_at,
        "samples": sum(|p| p.samples),
        "success": sum(|p| p.success),
        "failure": sum(|p| p.failure),
        "expected": sum(|p| p.expected),
        "dropped_details": sum(|p| p.dropped_details),
        "missed_sends": sum(|p| p.missed_sends),
        "p50_us": p50,
        "p95_us": p95,
        "p99_us": p99,
        "max_us": max,
        "bytes": sum(|p| p.bytes),
        "completed": completed,
    })
}

/// Resolve the `x-loadgen` binary used to spawn workers.
///
/// # Errors
///
/// Returns an error if no usable binary can be found.
pub fn worker_binary() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("X_LOADGEN_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let exe = std::env::current_exe().context("current executable")?;
    if is_loadgen_bin(&exe) {
        return Ok(exe);
    }
    if let Some(dir) = exe.parent() {
        let candidate = dir.join("x-loadgen");
        if candidate.is_file() {
            return Ok(candidate);
        }
        // cargo test places the package bin next to deps/ in target/debug.
        if let Some(parent) = dir.parent() {
            let candidate = parent.join("x-loadgen");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    bail!(
        "cannot locate the x-loadgen binary to spawn workers (current exe is {})",
        exe.display()
    )
}

fn is_loadgen_bin(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "x-loadgen")
}

/// Spawn `processes` worker children for one run, all in one process group.
///
/// # Errors
///
/// Returns an error if the binary cannot be located or a child cannot be started.
pub fn spawn_workers(
    paths: &DataPaths,
    run_id: &str,
    processes: u32,
    resume: ClusterResumePlan,
) -> Result<SpawnedWorkers> {
    let bin = worker_binary()?;
    let mut children: Vec<Child> = Vec::new();
    let logs = paths.logs_dir();
    let _ = fs::create_dir_all(&logs);
    for index in 0..processes {
        let log_path = logs.join(format!("run-{run_id}-w{index:02}.spawn.log"));
        let log = fs::File::create(&log_path)
            .with_context(|| format!("create {}", log_path.display()))?;
        let mut cmd = Command::new(&bin);
        cmd.arg("worker")
            .arg("--data-dir")
            .arg(paths.root())
            .arg("--run-id")
            .arg(run_id)
            .arg("--worker-index")
            .arg(index.to_string())
            .arg("--worker-count")
            .arg(processes.to_string())
            .arg("--start-window")
            .arg(resume.start_window.to_string())
            .arg("--completed-rounds")
            .arg(resume.completed_rounds.to_string())
            .arg("--partial-elapsed-ms")
            .arg(resume.partial_elapsed_ms.to_string())
            .arg("--last-completed-window")
            .arg(resume.last_completed_window.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().context("clone log")?))
            .stderr(Stdio::from(log));
        // The first child becomes the group leader; later children join that group.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            if index == 0 {
                cmd.process_group(0);
            } else if let Some(leader) = children.first() {
                cmd.process_group(pid_i32(leader.id()));
            }
        }
        let child = cmd
            .spawn()
            .with_context(|| format!("spawn worker {index} from {}", bin.display()))?;
        write_pid_file(paths, run_id, index, child.id())?;
        children.push(child);
    }
    let pgid = children.first().map(|c| pid_i32(c.id()));
    Ok(SpawnedWorkers { children, pgid })
}

/// Children of one run plus their process group id (Unix).
pub struct SpawnedWorkers {
    pub children: Vec<Child>,
    pub pgid: Option<i32>,
}

impl Drop for SpawnedWorkers {
    fn drop(&mut self) {
        for child in &mut self.children {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn pid_i32(pid: u32) -> i32 {
    i32::try_from(pid).unwrap_or(i32::MAX)
}

fn write_pid_file(paths: &DataPaths, run_id: &str, index: u32, pid: u32) -> Result<()> {
    paths.ensure_run(run_id)?;
    let path = paths.run_dir(run_id).join(format!("w{index:02}.pid"));
    fs::write(&path, format!("{pid}\n")).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Ask every remaining child to stop with SIGTERM, wait up to `grace` for a
/// clean exit, then SIGKILL whatever is left.
///
/// Workers handle SIGTERM by cancelling their window, draining in-flight
/// requests and writing a final checkpoint, so `grace` should cover one
/// request timeout. Waiting is async so the runtime thread is not blocked.
pub async fn stop_workers(workers: &mut SpawnedWorkers, grace: Duration) {
    let _ = workers.pgid;
    for child in &mut workers.children {
        if child.try_wait().ok().flatten().is_none() {
            let _ = Command::new("kill")
                .args(["-s", "TERM", &child.id().to_string()])
                .status();
        }
    }
    let deadline = tokio::time::Instant::now() + grace;
    while tokio::time::Instant::now() < deadline
        && workers
            .children
            .iter_mut()
            .any(|child| matches!(child.try_wait(), Ok(None)))
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    for child in &mut workers.children {
        if child.try_wait().ok().flatten().is_none() {
            warn!(
                pid = child.id(),
                "worker did not exit within grace period; killing"
            );
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Grace period for a worker to drain after SIGTERM: one request timeout plus
/// slack, capped so `stop_all` (30 s budget) can still finish.
fn stop_grace(snapshot: &RunSnapshot) -> Duration {
    let timeout = u64::try_from(snapshot.template.timeout_secs.max(1)).unwrap_or(1);
    Duration::from_secs(timeout.saturating_add(3)).min(Duration::from_secs(20))
}

/// Drive a multi-process run until every worker exits or the run is cancelled.
pub async fn drive_cluster(
    supervisor: Supervisor,
    run_id: String,
    snapshot: RunSnapshot,
    task: Arc<RunningTask>,
    resume: ClusterResumePlan,
) -> Result<(), StoreError> {
    let processes = u32::try_from(snapshot.template.processes.max(1)).unwrap_or(1);
    let mut workers = spawn_workers(supervisor.paths(), &run_id, processes, resume)
        .map_err(|err| StoreError::Other(err.context("spawn workers")))?;
    info!(
        run_id = %run_id,
        processes,
        start_window = resume.start_window,
        completed_rounds = resume.completed_rounds,
        "spawned workers"
    );
    append_event(&supervisor, &run_id, "workers", processes);

    let cancel = task.cancel.clone();
    let grace = stop_grace(&snapshot);
    let mut terminal = crate::db::runs::STATUS_FINISHED;
    loop {
        if cancel.is_cancelled() {
            stop_workers(&mut workers, grace).await;
            terminal = match task_stop_kind(&task) {
                StopKind::User => STATUS_STOPPED,
                StopKind::Signal | StopKind::None => STATUS_INTERRUPTED,
            };
            break;
        }
        match harvest(&mut workers) {
            Harvest::Running => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Harvest::AllExited { any_failed } => {
                if any_failed && !cancel.is_cancelled() {
                    terminal = STATUS_INTERRUPTED;
                    append_event(&supervisor, &run_id, "worker_exit", processes);
                } else if cancel.is_cancelled() {
                    terminal = match task_stop_kind(&task) {
                        StopKind::User => STATUS_STOPPED,
                        StopKind::Signal | StopKind::None => STATUS_INTERRUPTED,
                    };
                }
                break;
            }
            Harvest::OneFailed { index } => {
                warn!(run_id = %run_id, worker = index, "worker exited unexpectedly");
                append_event(&supervisor, &run_id, "worker_exit", index);
                stop_workers(&mut workers, grace).await;
                terminal = STATUS_INTERRUPTED;
                break;
            }
        }
    }
    // Drop remaining child handles.
    stop_workers(&mut workers, Duration::from_secs(2)).await;
    set_cluster_terminal(&task, terminal);
    Ok(())
}

fn set_cluster_terminal(task: &RunningTask, status: &str) {
    task.set_cluster_status(status);
}

fn task_stop_kind(task: &RunningTask) -> StopKind {
    task.stop_kind()
}

fn append_event(supervisor: &Supervisor, run_id: &str, event: &str, value: u32) {
    supervisor.append_cluster_event(run_id, event, value);
}

enum Harvest {
    Running,
    AllExited { any_failed: bool },
    OneFailed { index: u32 },
}

fn harvest(workers: &mut SpawnedWorkers) -> Harvest {
    let mut running = 0;
    let mut failed = false;
    let mut failed_index = None;
    for (index, child) in workers.children.iter_mut().enumerate() {
        match child.try_wait() {
            Ok(None) => running += 1,
            Ok(Some(status)) => {
                if !status.success() {
                    failed = true;
                    failed_index = Some(u32::try_from(index).unwrap_or(0));
                }
            }
            Err(_) => {
                failed = true;
                failed_index = Some(u32::try_from(index).unwrap_or(0));
            }
        }
    }
    if running > 0 {
        if failed && let Some(index) = failed_index {
            return Harvest::OneFailed { index };
        }
        Harvest::Running
    } else {
        Harvest::AllExited { any_failed: failed }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClusterResumePlan, apply_worker_split, merge_percentiles};
    use hdrhistogram::Histogram;
    use hdrhistogram::serialization::{Serializer, V2Serializer};

    fn blob(values: &[u64]) -> Vec<u8> {
        let mut hist = Histogram::<u64>::new_with_bounds(1, 1_000_000, 3).expect("hist");
        for value in values {
            hist.record(*value).expect("record");
        }
        let mut out = Vec::new();
        V2Serializer::new().serialize(&hist, &mut out).expect("ser");
        out
    }

    #[test]
    fn merged_window_spans_all_workers_and_sums_counters() {
        let part = |start: i64, end: Option<i64>, samples: u64, completed: bool| {
            super::WorkerWindowRecord {
                window_index: 0,
                started_at: start,
                ended_at: end,
                samples,
                success: samples,
                failure: 0,
                expected: 0,
                bytes: 0,
                dropped_details: 0,
                missed_sends: 1,
                histogram_b64: Some(super::histogram_to_b64(&blob(&[1_000, 2_000]))),
                completed,
            }
        };
        let closed = super::merge_window_parts(
            0,
            &[
                part(100, Some(2_100), 5, true),
                part(90, Some(2_050), 7, true),
            ],
            2,
            None,
        );
        assert_eq!(closed["started_at"], 90);
        assert_eq!(closed["ended_at"], 2_100);
        assert_eq!(closed["samples"], 12);
        assert_eq!(closed["missed_sends"], 2);
        assert_eq!(closed["completed"], true);
        assert!(closed["p99_us"].as_u64().is_some());

        let partial = super::merge_window_parts(
            0,
            &[
                part(100, Some(2_100), 5, true),
                part(90, Some(1_500), 3, false),
            ],
            2,
            None,
        );
        assert_eq!(partial["completed"], false);

        let open = super::merge_window_parts(
            0,
            &[part(100, Some(2_100), 5, true), part(90, None, 7, false)],
            2,
            None,
        );
        assert!(open["ended_at"].is_null(), "{open}");
        assert_eq!(open["completed"], false);

        let forced = super::merge_window_parts(0, &[part(100, None, 5, false)], 1, Some(9_000));
        assert_eq!(forced["ended_at"], 9_000);
        assert_eq!(forced["completed"], false);
    }

    #[test]
    fn resume_plan_advances_index_and_tracks_incomplete_round() {
        assert_eq!(
            ClusterResumePlan::from_windows(&[], 5_000),
            ClusterResumePlan::default()
        );
        let gap_stop = vec![serde_json::json!({
            "window_index": 0,
            "started_at": 1_000,
            "ended_at": 3_000,
            "completed": true,
        })];
        assert_eq!(
            ClusterResumePlan::from_windows(&gap_stop, 5_000),
            ClusterResumePlan {
                start_window: 1,
                completed_rounds: 1,
                partial_elapsed_ms: 0,
                last_completed_window: 0,
            }
        );

        let mid_window_twice = vec![
            serde_json::json!({
                "window_index": 0,
                "started_at": 1_000,
                "ended_at": 3_000,
                "completed": true,
            }),
            serde_json::json!({
                "window_index": 1,
                "started_at": 5_000,
                "ended_at": 5_800,
                "completed": false,
            }),
            serde_json::json!({
                "window_index": 2,
                "started_at": 8_000,
                "ended_at": 8_700,
                "completed": false,
            }),
        ];
        assert_eq!(
            ClusterResumePlan::from_windows(&mid_window_twice, 9_000),
            ClusterResumePlan {
                start_window: 3,
                completed_rounds: 1,
                partial_elapsed_ms: 1_500,
                last_completed_window: 0,
            }
        );
    }

    #[test]
    fn split_shares_sum_to_total() {
        let total = 50_u64;
        let shares: Vec<u64> = (0..2).map(|i| apply_worker_split(total, i, 2)).collect();
        assert_eq!(shares, vec![25, 25]);
        let shares: Vec<u64> = (0..3).map(|i| apply_worker_split(51, i, 3)).collect();
        assert_eq!(shares.iter().sum::<u64>(), 51);
        assert_eq!(shares, vec![17, 17, 17]);
        let shares: Vec<u64> = (0..3).map(|i| apply_worker_split(50, i, 3)).collect();
        assert_eq!(shares, vec![17, 17, 16]);
        assert_eq!(shares.iter().sum::<u64>(), 50);
    }

    #[test]
    fn merged_p99_is_not_the_average_of_p99s() {
        // Worker A is all 100µs; worker B is all 10_000µs. The average of the
        // two p99s is ~5050, but a merged histogram's p99 is near 10_000.
        let a = blob(&vec![100; 1000]);
        let b = blob(&vec![10_000; 1000]);
        let mut ha = Histogram::<u64>::new_with_bounds(1, 1_000_000, 3).unwrap();
        let mut hb = Histogram::<u64>::new_with_bounds(1, 1_000_000, 3).unwrap();
        for _ in 0..1000 {
            ha.record(100).unwrap();
            hb.record(10_000).unwrap();
        }
        let p99_a = ha.value_at_quantile(0.99);
        let p99_b = hb.value_at_quantile(0.99);
        let average = p99_a.midpoint(p99_b);
        let (_p50, _p95, p99, _max) = merge_percentiles(&[a, b], 1_000_000).expect("merge");
        let p99 = p99.expect("p99");
        assert!(
            p99.abs_diff(average) > 1_000,
            "merged p99 {p99} should not be the average {average} of {p99_a} and {p99_b}"
        );
        assert!(
            p99 >= 9_000,
            "merged p99 {p99} should sit near the slower worker"
        );
    }

    #[test]
    fn seal_worker_windows_recovers_crash_deltas_from_checkpoint() {
        let dir = std::env::temp_dir().join(format!(
            "x-loadgen-seal-test-{}-{}",
            std::process::id(),
            crate::engine::unix_now_ms()
        ));
        let paths = crate::db::DataPaths::new(&dir);
        let run_id = "run-crash";
        paths.ensure_run(run_id).expect("ensure run");
        let records = vec![
            super::WorkerWindowRecord {
                window_index: 0,
                started_at: 1_000,
                ended_at: Some(2_000),
                samples: 10,
                success: 9,
                failure: 1,
                expected: 0,
                bytes: 100,
                dropped_details: 0,
                missed_sends: 0,
                histogram_b64: None,
                completed: true,
            },
            super::WorkerWindowRecord {
                window_index: 1,
                started_at: 3_000,
                ended_at: None,
                samples: 0,
                success: 0,
                failure: 0,
                expected: 0,
                bytes: 0,
                dropped_details: 0,
                missed_sends: 0,
                histogram_b64: None,
                completed: false,
            },
        ];
        super::write_atomic_json(&paths.worker_windows(run_id, 0), &records).expect("write win");

        let conn = crate::db::open_worker_connection(&paths, run_id, 0).expect("open w00");
        let snap = crate::engine::CounterSnapshot {
            samples: 17,
            success: 15,
            failure: 2,
            expected: 0,
            bytes: 170,
            dropped_details: 1,
            in_flight: 3,
            peak_in_flight: 4,
            missed_sends: 2,
        };
        conn.execute(
            "INSERT INTO worker_checkpoints (ts, window_index, counters_json, histogram_blob)
             VALUES (?1, ?2, ?3, NULL)",
            rusqlite::params![3_650_i64, 1_i64, serde_json::to_string(&snap).unwrap()],
        )
        .expect("insert cp");
        drop(conn);

        super::seal_worker_windows(&paths, run_id, 1, 4_000);
        let sealed = super::load_worker_window_records(&paths, run_id, 0);
        assert_eq!(sealed.len(), 2);
        assert_eq!(sealed[0].samples, 10);
        assert!(sealed[0].completed);
        assert_eq!(sealed[1].ended_at, Some(3_650));
        assert_eq!(sealed[1].samples, 7);
        assert_eq!(sealed[1].success, 6);
        assert_eq!(sealed[1].failure, 1);
        assert_eq!(sealed[1].bytes, 70);
        assert_eq!(sealed[1].dropped_details, 1);
        assert_eq!(sealed[1].missed_sends, 2);
        assert!(!sealed[1].completed);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
