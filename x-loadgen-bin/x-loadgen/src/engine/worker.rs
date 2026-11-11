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
    LiveSnapshot, apply_worker_split, histogram_to_b64, write_atomic_json,
};
use crate::engine::supervisor::RunSnapshot;
use crate::engine::{
    EngineConfig, EngineHandle, EngineModel, ExecutePlan, HttpExecutor, LiveTraffic, ProductChoice,
    TenantChoice, Weighted,
};
use crate::log;

/// Entry point for `x-loadgen worker`.
///
/// # Errors
///
/// Returns an error if the data directory, snapshot, or engine cannot be started.
pub fn run(
    data_dir: std::path::PathBuf,
    run_id: &str,
    worker_index: u32,
    worker_count: u32,
) -> Result<ExitCode> {
    let paths = DataPaths::new(data_dir);
    let _guard = log::init(&paths, &log::worker_prefix(run_id, worker_index))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("start tokio runtime")?;
    runtime.block_on(run_async(&paths, run_id, worker_index, worker_count.max(1)))?;
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
    writer: std::sync::Mutex<Option<DetailWriter>>,
    traffic: Arc<LiveTraffic>,
    cancel: CancellationToken,
    window_index: Arc<AtomicI64>,
}

async fn run_async(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
    worker_count: u32,
) -> Result<()> {
    let mut ctx = prepare_worker(paths, run_id, worker_index, worker_count)?;
    spawn_background(&ctx);
    run_worker_windows(&ctx).await?;
    finish_worker(&mut ctx);
    Ok(())
}

fn prepare_worker(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
    worker_count: u32,
) -> Result<WorkerWindow> {
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

    let executor = Arc::new(HttpExecutor::from_snapshot(
        &snapshot.target,
        &snapshot.template,
    )?);
    let plan = Arc::new(build_plan(&snapshot));
    let timeout_us = secs_to_micros(snapshot.template.timeout_secs);
    let metrics = Arc::new(crate::engine::Metrics::new(timeout_us)?);
    if let Some(checkpoint) = load_checkpoint(paths, run_id, worker_index)? {
        restore_checkpoint(&metrics, &checkpoint);
    }

    let writer = DetailWriter::start(paths, run_id, worker_index, WriterConfig::default())?;
    let sink = Some(Arc::new(writer.handle()) as Arc<dyn crate::engine::SampleSink>);
    let (total_qps, total_conc) = traffic_from_record(&snapshot, &record_now(paths, run_id)?);
    let qps = split_value(
        total_qps,
        worker_index,
        worker_count,
        snapshot.template.qps_mode == "total",
    );
    let concurrency = split_value(
        total_conc.max(1),
        worker_index,
        worker_count,
        snapshot.template.qps_mode == "total",
    )
    .max(1);
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
        writer: std::sync::Mutex::new(Some(writer)),
        traffic: Arc::new(LiveTraffic {
            qps: AtomicU64::new(qps),
            concurrency: AtomicU64::new(concurrency),
        }),
        cancel: CancellationToken::new(),
        window_index: Arc::new(AtomicI64::new(0)),
    })
}

fn spawn_background(ctx: &WorkerWindow) {
    let control_cancel = ctx.cancel.clone();
    let control_traffic = Arc::clone(&ctx.traffic);
    let control_paths = ctx.paths.clone();
    let control_run = ctx.run_id.clone();
    let worker_index = ctx.worker_index;
    let worker_count = ctx.worker_count;
    let qps_mode_total = ctx.snapshot.template.qps_mode == "total";
    tokio::spawn(async move {
        poll_control(
            control_paths,
            control_run,
            worker_index,
            worker_count,
            qps_mode_total,
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
    tokio::spawn(async move {
        loop {
            publish_live(
                &live_paths,
                &live_run,
                worker_index,
                &live_metrics,
                &live_traffic,
                live_window.load(Ordering::Relaxed),
            );
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
    let rounds = if mode_loop {
        ctx.snapshot.template.rounds.map(as_u64)
    } else {
        Some(1)
    };
    let requests = ctx.snapshot.template.requests.map(|total| {
        split_value(
            as_u64(total),
            ctx.worker_index,
            ctx.worker_count,
            ctx.snapshot.template.requests_mode == "total",
        )
    });
    let interval = Duration::from_secs(as_u64(ctx.snapshot.template.loop_interval_secs));
    let mut index = 0_i64;

    loop {
        if ctx.cancel.is_cancelled() {
            break;
        }
        if let Some(limit) = rounds
            && as_u64(index) >= limit
        {
            break;
        }
        ctx.window_index.store(index, Ordering::Relaxed);
        let started_at = crate::engine::unix_now_ms();
        let before = ctx.metrics.counters();
        let config = worker_window_config(&ctx.snapshot, &ctx.plan, &ctx.traffic, requests);
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
        if let Err(err) = handle.join().await {
            warn!(error = %err, "worker window join failed");
            break;
        }
        let after = ctx.metrics.counters();
        let ended_at = crate::engine::unix_now_ms();
        let _ = (started_at, ended_at, before, after);
        publish_checkpoint(&ctx.writer, &ctx.metrics, index);
        ctx.metrics.reset_window_histogram();
        index = index.saturating_add(1);
        if ctx.cancel.is_cancelled() {
            break;
        }
        if let Some(limit) = rounds
            && as_u64(index) >= limit
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

fn record_now(paths: &DataPaths, run_id: &str) -> Result<runs::RunRecord> {
    let conn = open_central(paths)?;
    Ok(get_run(&conn, run_id)?)
}

fn traffic_from_record(snapshot: &RunSnapshot, record: &runs::RunRecord) -> (u64, u64) {
    let qps = record
        .current_qps
        .map_or_else(|| opt_u64(snapshot.template.qps), as_u64);
    let concurrency = record
        .current_concurrency
        .map_or_else(|| as_u64(snapshot.template.concurrency), as_u64)
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
    qps_mode_total: bool,
    traffic: Arc<LiveTraffic>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
        let Ok(conn) = open_central(&paths) else {
            continue;
        };
        let Ok(control) = get_run_control(&conn, &run_id) else {
            continue;
        };
        if control.stop_flag {
            cancel.cancel();
            return;
        }
        if let Some(qps) = control.qps {
            let share = split_value(as_u64(qps), worker_index, worker_count, qps_mode_total);
            traffic.qps.store(share, Ordering::Relaxed);
        }
        if let Some(conc) = control.concurrency {
            let share =
                split_value(as_u64(conc), worker_index, worker_count, qps_mode_total).max(1);
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
    if let Some(writer) = writer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        writer.publish_checkpoint(CheckpointUpdate {
            window_index: Some(window_index),
            counters_json,
            histogram_blob: blob,
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
}

fn build_plan(snapshot: &RunSnapshot) -> ExecutePlan {
    let products = snapshot
        .target
        .products
        .iter()
        .filter(|product| match snapshot.template.product_ids.as_ref() {
            Some(ids) if !ids.is_empty() => ids.iter().any(|id| id == &product.id),
            _ => true,
        })
        .map(|product| {
            let weight = snapshot
                .template
                .weight_overrides
                .as_ref()
                .and_then(|map| map.get(&product.id).copied())
                .unwrap_or(product.weight);
            Weighted {
                weight: as_u64(weight).max(1),
                value: ProductChoice {
                    product_id: product.product_id.clone(),
                    product_name: Some(product.name.clone()),
                },
            }
        })
        .collect();
    let tenants = snapshot
        .target
        .tenants
        .iter()
        .map(|tenant| Weighted {
            weight: as_u64(tenant.weight).max(1),
            value: TenantChoice {
                name: tenant.name.clone(),
            },
        })
        .collect();
    ExecutePlan {
        products,
        tenants,
        expected_reasons: snapshot
            .template
            .expected_reasons
            .clone()
            .unwrap_or_default(),
    }
}

fn worker_window_config(
    snapshot: &RunSnapshot,
    plan: &Arc<ExecutePlan>,
    traffic: &LiveTraffic,
    requests: Option<u64>,
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
        requests,
        duration: template
            .duration_secs
            .filter(|secs| *secs > 0)
            .map(|secs| Duration::from_secs(as_u64(secs))),
        warmup: Duration::from_secs(as_u64(template.warmup_secs)),
        ramp: Duration::from_secs(as_u64(template.ramp_secs)),
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
