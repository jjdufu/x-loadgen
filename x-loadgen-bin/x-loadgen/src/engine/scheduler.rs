//! Streaming open-loop / closed-loop dispatcher.
//!
//! # Open-loop pacing
//!
//! Open-loop windows follow an *absolute* send schedule: the intended send
//! time of request `n + 1` is derived from the intended time of request `n`,
//! never from "now". A late timer wake-up therefore does not slow the run
//! down; the overdue sends are dispatched back to back until the schedule is
//! caught up. This keeps the achieved rate on target even when the target
//! interval is shorter than the runtime's timer resolution (~1 ms).
//!
//! When the concurrency cap or a slow target keeps the dispatcher behind
//! schedule, the time a request spent waiting past its intended send time is
//! added to the latency recorded in the histogram (coordinated-omission
//! correction). Sends that fall more than [`MAX_SCHEDULE_LAG`] behind are
//! dropped and counted as `missed_sends` instead of being fired in a burst.
//!
//! During a linear ramp the next send time is obtained by integrating the
//! ramped rate, so the very first sends of a ramp are spaced correctly instead
//! of being separated by one huge `1 / rate(≈0)` interval.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::execute::{ExecutePlan, execute_one};
use super::metrics::Metrics;
use super::transport::RequestExecutor;
use super::{EngineModel, SampleSink};

/// Longest a scheduled open-loop send may lag behind its intended time before
/// it is dropped and counted as missed.
pub const MAX_SCHEDULE_LAG: Duration = Duration::from_secs(1);

/// Timer lateness below this bound is treated as scheduling jitter of the
/// load generator itself and is not charged to the target's latency.
const TIMER_SLACK: Duration = Duration::from_millis(2);

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
    let mut pacer = Pacer::new(started, plan.ramp);
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

        // 1. Open loop: wait for the next slot of the absolute schedule.
        let mut intended: Option<Instant> = None;
        let mut paced_lag = Duration::ZERO;
        if plan.model == EngineModel::OpenLoop {
            let qps = traffic.qps.load(Ordering::Relaxed);
            if qps > 0 {
                let missed = pacer.skip_stale(Instant::now(), qps);
                if missed > 0 {
                    metrics.add_missed_sends(missed);
                }
                let due = pacer.due(Instant::now());
                if let Some(end) = deadline
                    && due >= end
                {
                    break;
                }
                if Instant::now() < due {
                    tokio::select! {
                        () = cancel.cancelled() => break,
                        () = tokio::time::sleep_until(due.into()) => {}
                    }
                    if cancel.is_cancelled() {
                        break;
                    }
                }
                pacer.advance(qps);
                intended = Some(due);
                // Lag that already existed when the slot became due (catch-up
                // after back-pressure). Small timer lateness is ignored.
                let lag = Instant::now().saturating_duration_since(due);
                if lag >= TIMER_SLACK {
                    paced_lag = lag;
                }
            } else {
                // Paused (qps = 0): restart the schedule when traffic resumes.
                pacer.reset();
            }
        }

        // 2. Respect the concurrency cap.
        let concurrency = traffic.concurrency.load(Ordering::Relaxed).max(1);
        let slot_wait_started = Instant::now();
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
        let queue_delay = if intended.is_some() {
            paced_lag + Instant::now().saturating_duration_since(slot_wait_started)
        } else {
            Duration::ZERO
        };
        let queue_us = u64::try_from(queue_delay.as_micros()).unwrap_or(u64::MAX);

        let index = next_index;
        next_index = next_index.saturating_add(1);
        let warmup = intended
            .unwrap_or_else(Instant::now)
            .saturating_duration_since(started)
            < plan.warmup;
        metrics.begin_in_flight();
        let guard = InFlightGuard(Arc::clone(&metrics));
        let exec = Arc::clone(&executor);
        let exec_plan = Arc::clone(&plan.execute);
        let metrics_task = Arc::clone(&metrics);
        let sink_task = Arc::clone(&sink);
        tasks.spawn(async move {
            // Dropped on completion *and* on panic, so a panicking request
            // can never leave `in_flight` permanently elevated.
            let _guard = guard;
            let mut executed = execute_one(&exec_plan, exec.as_ref(), index).await;
            executed.sample.queue_us = i64::try_from(queue_us).ok();
            metrics_task.record_with_queue(
                executed.sample,
                executed.class,
                warmup,
                queue_us.saturating_add(executed.pre_send_wait_us),
                sink_task.as_ref(),
            );
        });
    }

    while tasks.join_next().await.is_some() {}
}

/// Decrements `in_flight` when dropped.
struct InFlightGuard(Arc<Metrics>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.end_in_flight();
    }
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

/// Absolute open-loop send schedule.
struct Pacer {
    started: Instant,
    ramp: Duration,
    /// Intended send time of the next request; `None` until the first send
    /// (or after a pause), in which case the schedule restarts at "now".
    next: Option<Instant>,
}

impl Pacer {
    const fn new(started: Instant, ramp: Duration) -> Self {
        Self {
            started,
            ramp,
            next: None,
        }
    }

    fn reset(&mut self) {
        self.next = None;
    }

    /// Intended send time of the next request.
    fn due(&mut self, now: Instant) -> Instant {
        *self.next.get_or_insert(now)
    }

    /// Move the schedule one send forward.
    fn advance(&mut self, qps: u64) {
        if let Some(at) = self.next {
            self.next = Some(self.after(at, qps));
        }
    }

    /// Drop every scheduled send older than `now - MAX_SCHEDULE_LAG`.
    /// Returns how many sends were dropped.
    fn skip_stale(&mut self, now: Instant, qps: u64) -> u64 {
        let Some(mut next) = self.next else {
            return 0;
        };
        let Some(horizon) = now.checked_sub(MAX_SCHEDULE_LAG) else {
            return 0;
        };
        let mut missed = 0_u64;
        // Inside the ramp the interval changes every step: walk it.
        while next < horizon && next.saturating_duration_since(self.started) < self.ramp {
            next = self.after(next, qps);
            missed = missed.saturating_add(1);
        }
        // Past the ramp the interval is constant: skip in bulk.
        if next < horizon {
            let interval = 1.0 / rate_of(qps);
            let behind = horizon.saturating_duration_since(next).as_secs_f64();
            let steps = (behind / interval).ceil().max(1.0);
            // Saturating float -> int conversion is the intended behaviour.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let steps_u64 = steps as u64;
            next = add_secs(next, steps * interval);
            missed = missed.saturating_add(steps_u64);
        }
        self.next = Some(next);
        missed
    }

    fn after(&self, at: Instant, qps: u64) -> Instant {
        let t0 = at.saturating_duration_since(self.started).as_secs_f64();
        let t1 = next_send_offset(t0, self.ramp.as_secs_f64(), rate_of(qps));
        add_secs(self.started, t1)
    }
}

/// Offset (seconds since window start) of the send following one at `t0`.
///
/// The rate ramps linearly from 0 to `rate` over `ramp` seconds, so the next
/// send happens when the integral of the rate from `t0` reaches exactly one
/// request.
fn next_send_offset(t0: f64, ramp: f64, rate: f64) -> f64 {
    if ramp <= 0.0 || t0 >= ramp {
        return t0 + 1.0 / rate;
    }
    // Inside the ramp: rate(t) = rate * t / ramp, so
    // ∫_{t0}^{t1} rate(t) dt = rate / (2 ramp) * (t1² - t0²) = 1.
    let candidate = t0.mul_add(t0, 2.0 * ramp / rate).sqrt();
    if candidate <= ramp {
        return candidate;
    }
    // The request completes after the ramp ends: finish the remainder at full rate.
    let used = rate / (2.0 * ramp) * t0.mul_add(-t0, ramp * ramp);
    ramp + (1.0 - used).max(0.0) / rate
}

fn rate_of(qps: u64) -> f64 {
    f64::from(u32::try_from(qps.clamp(1, u64::from(u32::MAX))).unwrap_or(u32::MAX))
}

fn add_secs(at: Instant, secs: f64) -> Instant {
    let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    Duration::try_from_secs_f64(secs)
        .ok()
        .and_then(|d| at.checked_add(d))
        .unwrap_or(at + Duration::from_hours(24))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_rate_spacing() {
        let t = next_send_offset(1.0, 0.0, 1000.0);
        assert!((t - 1.001).abs() < 1e-12);
    }

    #[test]
    fn ramp_start_is_not_a_huge_gap() {
        // qps=100, ramp=60s: the old code waited ~120s after the 2nd send.
        let mut t = 0.0;
        let mut sends = 0;
        while t < 60.0 {
            t = next_send_offset(t, 60.0, 100.0);
            sends += 1;
        }
        // Expected integral over the ramp: 100 * 60 / 2 = 3000 sends.
        assert!((2995..=3005).contains(&sends), "sends during ramp: {sends}");
        // The first gap is sqrt(2 * 60 / 100) ≈ 1.095 s.
        let first = next_send_offset(0.0, 60.0, 100.0);
        assert!((first - 1.095_445).abs() < 1e-3, "first gap {first}");
    }

    #[test]
    fn ramp_crossing_is_continuous() {
        let t = next_send_offset(9.999, 10.0, 10.0);
        assert!(t > 9.999 && t < 10.2, "{t}");
        let after = next_send_offset(10.0, 10.0, 10.0);
        assert!((after - 10.1).abs() < 1e-9);
    }

    #[test]
    fn skip_stale_counts_missed_sends() {
        let start = Instant::now();
        let mut pacer = Pacer::new(start, Duration::ZERO);
        pacer.due(start);
        // 3 s later at 100 qps: everything older than now-1s (≈200 sends) is dropped.
        let missed = pacer.skip_stale(start + Duration::from_secs(3), 100);
        assert!((199..=201).contains(&missed), "missed {missed}");
        let next = pacer.due(start);
        let lag = (start + Duration::from_secs(3)).saturating_duration_since(next);
        assert!(lag <= MAX_SCHEDULE_LAG);
    }

    #[test]
    fn schedule_is_absolute() {
        let start = Instant::now();
        let mut pacer = Pacer::new(start, Duration::ZERO);
        pacer.due(start);
        for _ in 0..5000 {
            pacer.advance(5000);
        }
        let next = pacer.due(start);
        let offset = next.saturating_duration_since(start).as_secs_f64();
        assert!((offset - 1.0).abs() < 1e-6, "offset {offset}");
    }
}
