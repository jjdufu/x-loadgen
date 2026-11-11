//! Streaming load-test engine: scheduler, metrics, token refresh, and execution.

mod cluster;
mod execute;
mod limits;
mod metrics;
mod scheduler;
mod supervisor;
mod token;
mod transport;
mod windows;
pub mod worker;

#[cfg(test)]
mod tests;

pub use cluster::{
    HistogramPercentiles, LiveSnapshot, MergedStats, apply_worker_split, load_live_snapshots,
    merge_live, merge_percentiles,
};
pub use execute::{ExecutePlan, ProductChoice, TenantChoice};
pub use limits::{TrafficShare, check_caps, sum_traffic};
pub use metrics::{
    CounterSnapshot, Metrics, OutcomeClass, RING_CAPACITY, RingSlot, SampleSink, unix_now_ms,
};
pub use scheduler::{LiveTraffic, WindowPlan, run_window};
pub use supervisor::{StopKind, Supervisor};
pub use token::{CachedToken, RefreshFn, TokenBroker, TokenError, box_refresh};
pub use transport::{
    ExecOutcome, HttpExecutor, RequestCtx, RequestExecutor, Weighted, mix_request_seed,
    pick_weighted,
};

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

/// Open-loop (paced by QPS) or closed-loop (paced only by concurrency).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineModel {
    OpenLoop,
    ClosedLoop,
}

/// Inputs needed to start a window of load against an executor.
pub struct EngineConfig {
    pub model: EngineModel,
    pub qps: Option<u64>,
    pub concurrency: u64,
    pub requests: Option<u64>,
    pub duration: Option<Duration>,
    pub warmup: Duration,
    pub ramp: Duration,
    /// Used as the histogram's highest trackable value, in microseconds.
    pub timeout: Duration,
    pub plan: Arc<ExecutePlan>,
}

/// Handle to a running engine instance. Each handle has isolated metrics.
pub struct EngineHandle {
    cancel: CancellationToken,
    traffic: Arc<LiveTraffic>,
    metrics: Arc<Metrics>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl EngineHandle {
    /// Start dispatching on a background task.
    ///
    /// # Errors
    ///
    /// Returns an error if the histogram cannot be created.
    pub fn start(
        config: EngineConfig,
        executor: Arc<dyn RequestExecutor>,
        sink: Arc<dyn SampleSink>,
    ) -> Result<Self> {
        let timeout_us = u64::try_from(config.timeout.as_micros())
            .unwrap_or(u64::MAX)
            .max(2);
        let metrics = Arc::new(Metrics::new(timeout_us)?);
        let cancel = CancellationToken::new();
        let traffic = Arc::new(LiveTraffic {
            qps: std::sync::atomic::AtomicU64::new(config.qps.unwrap_or(0)),
            concurrency: std::sync::atomic::AtomicU64::new(config.concurrency.max(1)),
        });
        let window = WindowPlan {
            model: config.model,
            requests: config.requests,
            duration: config.duration,
            warmup: config.warmup,
            ramp: config.ramp,
            execute: config.plan,
        };
        let join = tokio::spawn(run_window(
            window,
            Arc::clone(&traffic),
            Arc::clone(&metrics),
            cancel.clone(),
            executor,
            sink,
        ));
        Ok(Self {
            cancel,
            traffic,
            metrics,
            join: Some(join),
        })
    }

    /// Start a window using caller-owned traffic, metrics, and cancellation.
    ///
    /// The histogram must already exist on `metrics`.
    #[must_use]
    pub fn start_with(
        config: EngineConfig,
        executor: Arc<dyn RequestExecutor>,
        sink: Arc<dyn SampleSink>,
        traffic: Arc<LiveTraffic>,
        metrics: Arc<Metrics>,
        cancel: CancellationToken,
    ) -> Self {
        let window = WindowPlan {
            model: config.model,
            requests: config.requests,
            duration: config.duration,
            warmup: config.warmup,
            ramp: config.ramp,
            execute: config.plan,
        };
        let join = tokio::spawn(run_window(
            window,
            Arc::clone(&traffic),
            Arc::clone(&metrics),
            cancel.clone(),
            executor,
            sink,
        ));
        Self {
            cancel,
            traffic,
            metrics,
            join: Some(join),
        }
    }

    /// Change the target QPS for subsequent deliveries. In-flight requests are not interrupted.
    pub fn set_qps(&self, qps: u64) {
        self.traffic.qps.store(qps, Ordering::Relaxed);
    }

    /// Change the concurrency cap for subsequent deliveries.
    pub fn set_concurrency(&self, concurrency: u64) {
        self.traffic
            .concurrency
            .store(concurrency.max(1), Ordering::Relaxed);
    }

    /// Ask the scheduler to stop issuing new requests and drain in-flight work.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Shared metrics for this engine instance.
    #[must_use]
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
    }

    /// Wait until the window finishes and in-flight requests drain.
    ///
    /// # Errors
    ///
    /// Returns an error if the dispatcher task panicked.
    pub async fn join(mut self) -> Result<CounterSnapshot> {
        if let Some(join) = self.join.take() {
            join.await
                .map_err(|err| anyhow::anyhow!("engine task panicked: {err}"))?;
        }
        Ok(self.metrics.counters())
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        // After `join` has been taken the window is finished; do not cancel a
        // token that may be shared with later loop windows.
        if self.join.is_some() {
            self.cancel.cancel();
        }
    }
}
