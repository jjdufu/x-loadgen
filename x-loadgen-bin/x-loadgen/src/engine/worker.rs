//! Hidden `worker` process: one shard of a multi-process run.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::db::runs::{self, get_run, get_run_control};
use crate::db::{
    CheckpointUpdate, DataPaths, DetailWriter, WriterConfig, import_pending_dir, open_central,
    open_worker_connection,
};
use crate::engine::cluster::{
    LiveSnapshot, WorkerWindowRecord, apply_worker_split, histogram_to_b64,
    load_worker_window_records, seal_single_worker_windows, upsert_worker_window_record,
    write_atomic_json,
};
use crate::engine::snapshot::RunSnapshot;
use crate::engine::supervisor::build_plan_from_snapshot;
use crate::engine::{
    CounterSnapshot, EngineConfig, EngineHandle, EngineModel, ExecutePlan, HttpExecutor,
    LiveTraffic,
};
use crate::log;

/// Hidden CLI arguments for `x-loadgen worker`.
#[derive(Debug, Clone, clap::Args)]
pub struct WorkerArgs {
    #[arg(long)]
    pub data_dir: std::path::PathBuf,
    #[arg(long)]
    pub run_id: String,
    #[arg(long)]
    pub worker_index: u32,
    #[arg(long)]
    pub worker_count: u32,
    #[arg(long, default_value_t = 0)]
    pub start_window: i64,
    #[arg(long, default_value_t = 0)]
    pub completed_rounds: u64,
    #[arg(long, default_value_t = 0)]
    pub partial_elapsed_ms: u64,
    #[arg(long, default_value_t = -1, allow_hyphen_values = true)]
    pub last_completed_window: i64,
}

/// Entry point for `x-loadgen worker`.
///
/// # Errors
///
/// Returns an error if the data directory, snapshot, or engine cannot be started.
pub fn run(mut args: WorkerArgs) -> Result<ExitCode> {
    args.worker_count = args.worker_count.max(1);
    let paths = DataPaths::new(args.data_dir.clone());
    let _guard = log::init(&paths, &log::worker_prefix(&args.run_id, args.worker_index))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("start tokio runtime")?;
    runtime.block_on(run_async(&paths, &args))?;
    Ok(ExitCode::SUCCESS)
}

struct WorkerWindow {
    paths: DataPaths,
    run_id: String,
    worker_index: u32,
    worker_count: u32,
    snapshot: RunSnapshot,
    executor: Arc<HttpExecutor>,
    plan: Arc<ExecutePlan>,
    metrics: Arc<crate::engine::Metrics>,
    sink: Option<Arc<dyn crate::engine::SampleSink>>,
    writer: Arc<std::sync::Mutex<Option<DetailWriter>>>,
    traffic: Arc<LiveTraffic>,
    cancel: CancellationToken,
    window_index: Arc<AtomicI64>,
    start_window: i64,
    completed_rounds: u64,
    partial_elapsed_ms: u64,
    partial_samples: u64,
}

async fn run_async(paths: &DataPaths, args: &WorkerArgs) -> Result<()> {
    let mut ctx = prepare_worker(paths, args)?;
    // The parent stops a cluster with SIGTERM. Treat it like the stop flag:
    // cancel, let the window drain, and write the final checkpoint instead of
    // dying with the counters and histogram still in memory.
    let signal_cancel = ctx.cancel.clone();
    tokio::spawn(async move {
        crate::signal::wait_exit_signal().await;
        info!("stop signal received; draining");
        signal_cancel.cancel();
    });
    spawn_background(&ctx);
    run_worker_windows(&ctx).await?;
    finish_worker(&mut ctx);
    Ok(())
}

fn prepare_worker(paths: &DataPaths, args: &WorkerArgs) -> Result<WorkerWindow> {
    let run_id = args.run_id.as_str();
    let worker_index = args.worker_index;
    let worker_count = args.worker_count;
    let central = open_central(paths)?;
    let record = get_run(&central, run_id)?;
    let snapshot: RunSnapshot =
        serde_json::from_str(&record.snapshot_json).context("parse run snapshot")?;
    drop(central);

    let conn = open_worker_connection(paths, run_id, worker_index)?;
    let imported = import_pending_dir(&conn, paths, run_id, worker_index)?;
    if imported > 0 {
        info!(imported, "imported pending dump samples");
    }
    drop(conn);

    seal_single_worker_windows(paths, run_id, worker_index, crate::engine::unix_now_ms());
    let existing_records = load_worker_window_records(paths, run_id, worker_index);
    let (start_window, completed_rounds, partial_elapsed_ms, last_completed_window) =
        resolve_worker_resume(args, &existing_records);
    let partial_samples = existing_records
        .iter()
        .filter(|r| r.window_index > last_completed_window && r.window_index < start_window)
        .map(|r| r.samples)
        .fold(0_u64, u64::saturating_add);

    let executor = Arc::new(HttpExecutor::from_snapshot(
        &snapshot.target,
        &snapshot.template,
    )?);
    let plan = Arc::new(build_plan_from_snapshot(&snapshot));
    let timeout_us = secs_to_micros(snapshot.template.timeout_secs);
    let metrics = Arc::new(crate::engine::Metrics::new(timeout_us)?);
    if let Some(checkpoint) = load_checkpoint(paths, run_id, worker_index)? {
        restore_checkpoint(&metrics, &checkpoint);
    }

    let writer = DetailWriter::start(paths, run_id, worker_index, WriterConfig::default())?;
    let sink = Some(Arc::new(writer.handle()) as Arc<dyn crate::engine::SampleSink>);
    let (total_qps, total_conc) = traffic_from_record(&snapshot, &record_now(paths, run_id)?);
    // `current_qps` / `current_concurrency` are always run-wide totals
    // (`per_process` was already multiplied out by `effective_total_*`), so
    // every worker takes its share regardless of `qps_mode`. Splitting only in
    // `total` mode made `per_process` runs send `processes`× too much.
    let qps = apply_worker_split(total_qps, worker_index, worker_count);
    let concurrency = apply_worker_split(total_conc.max(1), worker_index, worker_count).max(1);
    Ok(WorkerWindow {
        paths: paths.clone(),
        run_id: run_id.to_string(),
        worker_index,
        worker_count,
        snapshot,
        executor,
        plan,
        metrics,
        sink,
        writer: Arc::new(std::sync::Mutex::new(Some(writer))),
        traffic: Arc::new(LiveTraffic {
            qps: AtomicU64::new(qps),
            concurrency: AtomicU64::new(concurrency),
        }),
        cancel: CancellationToken::new(),
        window_index: Arc::new(AtomicI64::new(start_window)),
        start_window,
        completed_rounds,
        partial_elapsed_ms,
        partial_samples,
    })
}

fn resolve_worker_resume(
    args: &WorkerArgs,
    records: &[WorkerWindowRecord],
) -> (i64, u64, u64, i64) {
    if args.start_window > 0 || args.completed_rounds > 0 || args.last_completed_window >= 0 {
        return (
            args.start_window,
            args.completed_rounds,
            args.partial_elapsed_ms,
            args.last_completed_window,
        );
    }
    let Some(max_index) = records.iter().map(|r| r.window_index).max() else {
        return (0, 0, 0, -1);
    };
    let mut completed_rounds = 0_u64;
    let mut last_completed = -1_i64;
    for r in records {
        if r.ended_at.is_some() && r.completed {
            completed_rounds = completed_rounds.saturating_add(1);
            last_completed = last_completed.max(r.window_index);
        }
    }
    let now = crate::engine::unix_now_ms();
    let partial_elapsed_ms = records
        .iter()
        .filter(|r| r.window_index > last_completed)
        .map(|r| {
            let end = r.ended_at.unwrap_or(now);
            u64::try_from(end.saturating_sub(r.started_at).max(0)).unwrap_or(0)
        })
        .fold(0_u64, u64::saturating_add);
    (
        max_index.saturating_add(1),
        completed_rounds,
        partial_elapsed_ms,
        last_completed,
    )
}

fn spawn_background(ctx: &WorkerWindow) {
    let control_cancel = ctx.cancel.clone();
    let control_traffic = Arc::clone(&ctx.traffic);
    let control_paths = ctx.paths.clone();
    let control_run = ctx.run_id.clone();
    let worker_index = ctx.worker_index;
    let worker_count = ctx.worker_count;
    tokio::spawn(async move {
        poll_control(
            control_paths,
            control_run,
            worker_index,
            worker_count,
            control_traffic,
            control_cancel,
        )
        .await;
    });

    let live_cancel = ctx.cancel.clone();
    let live_metrics = Arc::clone(&ctx.metrics);
    let live_traffic = Arc::clone(&ctx.traffic);
    let live_paths = ctx.paths.clone();
    let live_run = ctx.run_id.clone();
    let live_window = Arc::clone(&ctx.window_index);
    let live_writer = Arc::clone(&ctx.writer);
    tokio::spawn(async move {
        loop {
            let window_index = live_window.load(Ordering::Relaxed);
            publish_live(
                &live_paths,
                &live_run,
                worker_index,
                &live_metrics,
                &live_traffic,
                window_index,
            );
            // Keep the persisted checkpoint fresh (the writer stores it once a
            // second) so a hard kill mid-window loses at most ~1 s of stats.
            publish_checkpoint(&live_writer, &live_metrics, window_index);
            tokio::select! {
                () = live_cancel.cancelled() => break,
                () = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    });
}

fn finish_worker(ctx: &mut WorkerWindow) {
    ctx.cancel.cancel();
    publish_live(
        &ctx.paths,
        &ctx.run_id,
        ctx.worker_index,
        &ctx.metrics,
        &ctx.traffic,
        ctx.window_index.load(Ordering::Relaxed),
    );
    publish_checkpoint(
        &ctx.writer,
        &ctx.metrics,
        ctx.window_index.load(Ordering::Relaxed),
    );
    // WriterHandle clones keep the write thread's channel open. Drop the
    // worker-owned sink before joining so close() can observe disconnect,
    // and close() itself also stops the thread if any handle is still held.
    drop(ctx.sink.take());
    if let Some(writer) = ctx
        .writer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        && let Err(err) = writer.close()
    {
        warn!(error = %err, "worker writer close reported an error");
    }
    info!("worker finished");
}

async fn run_worker_windows(ctx: &WorkerWindow) -> Result<()> {
    let mode_loop = record_now(&ctx.paths, &ctx.run_id)?.mode == "loop";
    let total_rounds = if mode_loop {
        ctx.snapshot.template.rounds.map(as_u64)
    } else {
        Some(1)
    };
    let remaining_rounds = total_rounds.map(|r| r.saturating_sub(ctx.completed_rounds));
    let base_requests = ctx.snapshot.template.requests.map(|total| {
        split_value(
            as_u64(total),
            ctx.worker_index,
            ctx.worker_count,
            ctx.snapshot.template.requests_mode == "total",
        )
    });
    let interval = Duration::from_secs(as_u64(ctx.snapshot.template.loop_interval_secs));
    let mut index = ctx.start_window;
    let mut session_round = 0_u64;

    loop {
        if ctx.cancel.is_cancelled() {
            break;
        }
        if let Some(limit) = remaining_rounds
            && session_round >= limit
        {
            break;
        }
        ctx.window_index.store(index, Ordering::Relaxed);
        let started_at = crate::engine::unix_now_ms();
        let before = ctx.metrics.counters();
        // Mark the window open so the server can time a running cluster too.
        record_window(ctx, index, started_at, None, false, &before, &before);
        let (partial_samples, partial_elapsed) = if session_round == 0 {
            (
                ctx.partial_samples,
                Duration::from_millis(ctx.partial_elapsed_ms),
            )
        } else {
            (0, Duration::ZERO)
        };
        let config = worker_window_config(
            &ctx.snapshot,
            &ctx.plan,
            &ctx.traffic,
            base_requests,
            partial_samples,
            partial_elapsed,
        );
        let sink = ctx
            .sink
            .as_ref()
            .context("worker sink is held until the window loop ends")?
            .clone();
        let handle = EngineHandle::start_with(
            config,
            Arc::clone(&ctx.executor) as Arc<dyn crate::engine::RequestExecutor>,
            sink,
            Arc::clone(&ctx.traffic),
            Arc::clone(&ctx.metrics),
            ctx.cancel.child_token(),
        );
        let join_res = handle.join().await;
        let completed = join_res.is_ok() && !ctx.cancel.is_cancelled();
        let after = ctx.metrics.counters();
        let ended_at = crate::engine::unix_now_ms();
        record_window(
            ctx,
            index,
            started_at,
            Some(ended_at),
            completed,
            &before,
            &after,
        );
        publish_checkpoint(&ctx.writer, &ctx.metrics, index);
        ctx.metrics.reset_window_histogram();
        if let Err(err) = join_res {
            warn!(error = %err, "worker window join failed");
            break;
        }
        index = index.saturating_add(1);
        session_round = session_round.saturating_add(1);
        if ctx.cancel.is_cancelled() {
            break;
        }
        if let Some(limit) = remaining_rounds
            && session_round >= limit
        {
            break;
        }
        if !interval.is_zero() {
            tokio::select! {
                () = ctx.cancel.cancelled() => break,
                () = tokio::time::sleep(interval) => {}
            }
        }
    }
    Ok(())
}

/// Persist this worker's part of window `index` (counter deltas plus the
/// window histogram) to `wNN-windows.json`.
fn record_window(
    ctx: &WorkerWindow,
    index: i64,
    started_at: i64,
    ended_at: Option<i64>,
    completed: bool,
    before: &CounterSnapshot,
    after: &CounterSnapshot,
) {
    let histogram_b64 = ended_at
        .and_then(|_| ctx.metrics.window_histogram_blob().ok())
        .map(|blob| histogram_to_b64(&blob));
    let record = WorkerWindowRecord {
        window_index: index,
        started_at,
        ended_at,
        samples: after.samples.saturating_sub(before.samples),
        success: after.success.saturating_sub(before.success),
        failure: after.failure.saturating_sub(before.failure),
        expected: after.expected.saturating_sub(before.expected),
        bytes: after.bytes.saturating_sub(before.bytes),
        dropped_details: after.dropped_details.saturating_sub(before.dropped_details),
        missed_sends: after.missed_sends.saturating_sub(before.missed_sends),
        histogram_b64,
        completed,
    };
    if let Err(err) = upsert_worker_window_record(&ctx.paths, &ctx.run_id, ctx.worker_index, record)
    {
        warn!(error = %err, "write window record failed");
    }
}

fn record_now(paths: &DataPaths, run_id: &str) -> Result<runs::RunRecord> {
    let conn = open_central(paths)?;
    Ok(get_run(&conn, run_id)?)
}

fn traffic_from_record(snapshot: &RunSnapshot, record: &runs::RunRecord) -> (u64, u64) {
    let qps = record
        .current_qps
        .map_or_else(|| opt_u64(snapshot.template.effective_total_qps()), as_u64);
    let concurrency = record
        .current_concurrency
        .map_or_else(
            || as_u64(snapshot.template.effective_total_concurrency()),
            as_u64,
        )
        .max(1);
    (qps, concurrency)
}

fn split_value(total: u64, index: u32, count: u32, total_mode: bool) -> u64 {
    if total_mode {
        apply_worker_split(total, index, count)
    } else {
        total
    }
}

async fn poll_control(
    paths: DataPaths,
    run_id: String,
    worker_index: u32,
    worker_count: u32,
    traffic: Arc<LiveTraffic>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
        if !paths.central_db().is_file() || !paths.run_dir(&run_id).is_dir() {
            cancel.cancel();
            return;
        }
        let Ok(conn) = open_central(&paths) else {
            continue;
        };
        let control = match get_run_control(&conn, &run_id) {
            Ok(control) => control,
            Err(crate::db::StoreError::NotFound(_)) => {
                cancel.cancel();
                return;
            }
            Err(_) => continue,
        };
        if control.stop_flag {
            cancel.cancel();
            return;
        }
        if let Some(qps) = control.qps {
            // Live patches are run-wide totals too: always split.
            let share = apply_worker_split(as_u64(qps), worker_index, worker_count);
            traffic.qps.store(share, Ordering::Relaxed);
        }
        if let Some(conc) = control.concurrency {
            let share = apply_worker_split(as_u64(conc), worker_index, worker_count).max(1);
            traffic.concurrency.store(share, Ordering::Relaxed);
        }
    }
}

fn publish_live(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
    metrics: &crate::engine::Metrics,
    traffic: &LiveTraffic,
    window_index: i64,
) {
    let counters = metrics.counters();
    let (p50, p95, p99, max) = metrics.percentiles_us();
    let histogram_b64 = metrics
        .histogram_blob()
        .ok()
        .map(|blob| histogram_to_b64(&blob));
    let snap = LiveSnapshot {
        ts: crate::engine::unix_now_ms(),
        window_index,
        samples: counters.samples,
        success: counters.success,
        failure: counters.failure,
        expected: counters.expected,
        bytes: counters.bytes,
        dropped_details: counters.dropped_details,
        in_flight: counters.in_flight,
        peak_in_flight: counters.peak_in_flight,
        current_qps: traffic.qps.load(Ordering::Relaxed),
        current_concurrency: traffic.concurrency.load(Ordering::Relaxed),
        p50_us: p50,
        p95_us: p95,
        p99_us: p99,
        max_us: max,
        histogram_b64,
        missed_sends: counters.missed_sends,
        timeseries: metrics.timeseries_slots(),
    };
    if let Err(err) = write_atomic_json(&paths.worker_live(run_id, worker_index), &snap) {
        warn!(error = %err, "write live snapshot failed");
    }
}

fn publish_checkpoint(
    writer: &std::sync::Mutex<Option<DetailWriter>>,
    metrics: &crate::engine::Metrics,
    window_index: i64,
) {
    let counters_json = serde_json::to_string(&metrics.counters()).unwrap_or_else(|_| "{}".into());
    let blob = metrics.histogram_blob().ok();
    let timeseries_json = serde_json::to_string(&metrics.timeseries_slots()).ok();
    if let Some(writer) = writer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        writer.publish_checkpoint(CheckpointUpdate {
            window_index: Some(window_index),
            counters_json,
            histogram_blob: blob,
            timeseries_json,
        });
    }
}

fn load_checkpoint(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
) -> Result<Option<runs::WorkerCheckpoint>> {
    let conn = open_worker_connection(paths, run_id, worker_index)?;
    Ok(runs::latest_worker_checkpoint(&conn)?)
}

fn restore_checkpoint(metrics: &crate::engine::Metrics, checkpoint: &runs::WorkerCheckpoint) {
    if let Ok(mut snapshot) =
        serde_json::from_str::<crate::engine::CounterSnapshot>(&checkpoint.counters_json)
    {
        snapshot.in_flight = 0;
        metrics.restore_counters(snapshot);
    }
    if let Some(blob) = checkpoint.histogram_blob.as_deref()
        && !blob.is_empty()
    {
        let _ = metrics.restore_histogram(blob);
    }
    if let Some(raw_ts) = checkpoint.timeseries_json.as_deref()
        && let Ok(slots) = serde_json::from_str::<Vec<crate::engine::RingSlot>>(raw_ts)
    {
        metrics.restore_timeseries(&slots);
    }
}

fn worker_window_config(
    snapshot: &RunSnapshot,
    plan: &Arc<ExecutePlan>,
    traffic: &LiveTraffic,
    base_requests: Option<u64>,
    partial_samples: u64,
    partial_elapsed: Duration,
) -> EngineConfig {
    let template = &snapshot.template;
    let model = if template.model == "closed_loop" {
        EngineModel::ClosedLoop
    } else {
        EngineModel::OpenLoop
    };
    EngineConfig {
        model,
        qps: Some(traffic.qps.load(Ordering::Relaxed)),
        concurrency: traffic.concurrency.load(Ordering::Relaxed).max(1),
        requests: base_requests.map(|r| r.saturating_sub(partial_samples)),
        duration: template
            .duration_secs
            .filter(|secs| *secs > 0)
            .map(|secs| Duration::from_secs(as_u64(secs)).saturating_sub(partial_elapsed)),
        warmup: Duration::from_secs(as_u64(template.warmup_secs)).saturating_sub(partial_elapsed),
        ramp: Duration::from_secs(as_u64(template.ramp_secs)).saturating_sub(partial_elapsed),
        timeout: Duration::from_secs(as_u64(template.timeout_secs).max(1)),
        plan: Arc::clone(plan),
    }
}

fn opt_u64(value: Option<i64>) -> u64 {
    value.map_or(0, as_u64)
}

fn as_u64(value: i64) -> u64 {
    u64::try_from(value.max(0)).unwrap_or(0)
}

fn secs_to_micros(secs: i64) -> u64 {
    as_u64(secs).saturating_mul(1_000_000).max(2)
}
