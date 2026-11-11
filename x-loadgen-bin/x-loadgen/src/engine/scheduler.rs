//! Streaming open-loop / closed-loop dispatcher.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::execute::{ExecutePlan, execute_one};
use super::metrics::Metrics;
use super::transport::RequestExecutor;
use super::{EngineModel, SampleSink};

/// Live knobs that [`EngineHandle`] can change without interrupting in-flight requests.
pub struct LiveTraffic {
    pub qps: AtomicU64,
    pub concurrency: AtomicU64,
}

/// Configuration for a single window of load.
pub struct WindowPlan {
    pub model: EngineModel,
    pub requests: Option<u64>,
    pub duration: Option<Duration>,
    pub warmup: Duration,
    pub ramp: Duration,
    pub execute: Arc<ExecutePlan>,
}

/// Dispatch requests until the window is cancelled or exhausted, then drain.
pub async fn run_window(
    plan: WindowPlan,
    traffic: Arc<LiveTraffic>,
    metrics: Arc<Metrics>,
    cancel: CancellationToken,
    executor: Arc<dyn RequestExecutor>,
    sink: Arc<dyn SampleSink>,
) {
    let started = Instant::now();
    let deadline = plan.duration.map(|d| started + d);
    let mut next_index = 0_u64;
    let mut next_emit = started;
    let mut tasks: JoinSet<()> = JoinSet::new();

    loop {
        if cancel.is_cancelled() {
            break;
        }
        if let Some(max) = plan.requests
            && next_index >= max
        {
            break;
        }
        if let Some(end) = deadline
            && Instant::now() >= end
        {
            break;
        }

        let concurrency = traffic.concurrency.load(Ordering::Relaxed).max(1);
        if !wait_for_slot(&metrics, concurrency, &cancel, &mut tasks).await {
            break;
        }
        // Re-check stop conditions after waiting for a slot.
        if cancel.is_cancelled() {
            break;
        }
        if let Some(end) = deadline
            && Instant::now() >= end
        {
            break;
        }

        if plan.model == EngineModel::OpenLoop {
            let qps = traffic.qps.load(Ordering::Relaxed);
            if qps > 0 {
                let now = Instant::now();
                if now < next_emit {
                    tokio::select! {
                        () = cancel.cancelled() => break,
                        () = tokio::time::sleep_until(next_emit.into()) => {}
                    }
                    if cancel.is_cancelled() {
                        break;
                    }
                }
                let elapsed = Instant::now().saturating_duration_since(started);
                let rate = ramped_qps(elapsed, plan.ramp, qps);
                if rate > 0.0 {
                    let interval = Duration::from_secs_f64((1.0 / rate).max(1e-9));
                    next_emit = next_emit.max(Instant::now()) + interval;
                } else {
                    next_emit = Instant::now() + Duration::from_millis(5);
                }
            }
        }

        let index = next_index;
        next_index = next_index.saturating_add(1);
        let warmup = started.elapsed() < plan.warmup;
        metrics.begin_in_flight();
        let exec = Arc::clone(&executor);
        let exec_plan = Arc::clone(&plan.execute);
        let metrics_task = Arc::clone(&metrics);
        let sink_task = Arc::clone(&sink);
        tasks.spawn(async move {
            let executed = execute_one(&exec_plan, exec.as_ref(), index).await;
            metrics_task.record(executed.sample, executed.class, warmup, sink_task.as_ref());
            metrics_task.end_in_flight();
        });
    }

    while tasks.join_next().await.is_some() {}
}

async fn wait_for_slot(
    metrics: &Metrics,
    concurrency: u64,
    cancel: &CancellationToken,
    tasks: &mut JoinSet<()>,
) -> bool {
    loop {
        if cancel.is_cancelled() {
            return false;
        }
        if metrics.in_flight() < concurrency {
            return true;
        }
        tokio::select! {
            () = cancel.cancelled() => return false,
            joined = tasks.join_next() => {
                if joined.is_none() {
                    // No tasks, but in_flight was high: yield and re-check.
                    tokio::task::yield_now().await;
                }
            }
        }
    }
}

fn ramped_qps(elapsed: Duration, ramp: Duration, target: u64) -> f64 {
    let target = f64::from(u32::try_from(target.min(u64::from(u32::MAX))).unwrap_or(u32::MAX));
    if ramp.is_zero() {
        return target;
    }
    let fraction = (elapsed.as_secs_f64() / ramp.as_secs_f64()).clamp(0.0, 1.0);
    target * fraction
}
