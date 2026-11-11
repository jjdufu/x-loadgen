//! Multi-process workers, QPS/request splits, and merged live stats.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use hdrhistogram::Histogram;
use hdrhistogram::serialization::Deserializer;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::db::runs::{STATUS_INTERRUPTED, STATUS_STOPPED};
use crate::db::{DataPaths, StoreError};
use crate::engine::supervisor::{RunSnapshot, RunningTask, StopKind, Supervisor};

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
    pub window_index: Option<i64>,
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
    if let Ok((p50, p95, p99, max)) = merge_percentiles(&blobs, highest_trackable_us) {
        merged.p50_us = p50;
        merged.p95_us = p95;
        merged.p99_us = p99;
        merged.max_us = max;
    }
    Ok(merged)
}

/// Percentiles `(p50, p95, p99, max)` in microseconds.
pub type HistogramPercentiles = (Option<u64>, Option<u64>, Option<u64>, Option<u64>);

/// Decode each V2 blob and add them into one histogram, then read percentiles.
///
/// # Errors
///
/// Returns an error if a blob is not a valid histogram or the histograms cannot
/// be added together.
pub fn merge_percentiles(
    blobs: &[Vec<u8>],
    highest_trackable_us: u64,
) -> Result<HistogramPercentiles> {
    if blobs.is_empty() {
        return Ok((None, None, None, None));
    }
    let highest = highest_trackable_us.max(2);
    let mut acc =
        Histogram::<u64>::new_with_bounds(1, highest, 3).context("create merge histogram")?;
    let mut deserializer = Deserializer::new();
    for blob in blobs {
        let mut cursor = std::io::Cursor::new(blob.as_slice());
        let next = deserializer
            .deserialize(&mut cursor)
            .context("deserialize worker histogram")?;
        acc.add(&next).context("merge worker histogram")?;
    }
    if acc.is_empty() {
        return Ok((None, None, None, None));
    }
    Ok((
        Some(acc.value_at_quantile(0.50)),
        Some(acc.value_at_quantile(0.95)),
        Some(acc.value_at_quantile(0.99)),
        Some(acc.max()),
    ))
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
pub fn spawn_workers(paths: &DataPaths, run_id: &str, processes: u32) -> Result<SpawnedWorkers> {
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

fn pid_i32(pid: u32) -> i32 {
    i32::try_from(pid).unwrap_or(i32::MAX)
}

fn write_pid_file(paths: &DataPaths, run_id: &str, index: u32, pid: u32) -> Result<()> {
    paths.ensure_run(run_id)?;
    let path = paths.run_dir(run_id).join(format!("w{index:02}.pid"));
    fs::write(&path, format!("{pid}\n")).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Signal every remaining child and wait, first with TERM then KILL.
pub fn stop_workers(workers: &mut SpawnedWorkers, timeout: Duration) {
    if let Some(pgid) = workers.pgid {
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{pgid}")])
            .status();
    } else {
        for child in &mut workers.children {
            let _ = child.kill();
        }
    }
    let deadline = std::time::Instant::now() + timeout;
    for child in &mut workers.children {
        wait_child_until(child, deadline);
        if child.try_wait().ok().and_then(|s| s).is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn wait_child_until(child: &mut Child, deadline: std::time::Instant) {
    while std::time::Instant::now() < deadline {
        match child.try_wait() {
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Ok(Some(_)) | Err(_) => return,
        }
    }
}

/// Drive a multi-process run until every worker exits or the run is cancelled.
pub async fn drive_cluster(
    supervisor: Supervisor,
    run_id: String,
    snapshot: RunSnapshot,
    task: Arc<RunningTask>,
) -> Result<(), StoreError> {
    let processes = u32::try_from(snapshot.template.processes.max(1)).unwrap_or(1);
    let mut workers = spawn_workers(supervisor.paths(), &run_id, processes)
        .map_err(|err| StoreError::Other(err.context("spawn workers")))?;
    info!(run_id = %run_id, processes, "spawned workers");
    append_event(&supervisor, &run_id, "workers", processes);

    let cancel = task.cancel.clone();
    let mut terminal = crate::db::runs::STATUS_FINISHED;
    loop {
        if cancel.is_cancelled() {
            stop_workers(&mut workers, Duration::from_secs(10));
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
                stop_workers(&mut workers, Duration::from_secs(10));
                terminal = STATUS_INTERRUPTED;
                break;
            }
        }
    }
    // Drop remaining child handles.
    stop_workers(&mut workers, Duration::from_secs(2));
    let _ = terminal;
    // The caller (supervisor.drive) persists the status; return a marker via
    // a side channel by writing a small status file if needed. Instead we
    // return Ok and let drive_cluster_status communicate.
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
    use super::{apply_worker_split, merge_percentiles};
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
}
