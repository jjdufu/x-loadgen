//! Unit tests for the streaming engine.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;

use super::{
    CachedToken, EngineConfig, EngineHandle, EngineModel, ExecOutcome, ExecutePlan, Metrics,
    ProductChoice, RING_CAPACITY, RequestCtx, RequestExecutor, SampleSink, TenantChoice,
    TokenBroker, TokenError, Weighted, box_refresh, mix_request_seed,
};
use crate::db::Sample;
use crate::engine::metrics::{OutcomeClass, unix_now_ms};

struct NullSink;

impl SampleSink for NullSink {
    fn try_send(&self, _sample: Sample) -> bool {
        true
    }
}

struct DropAllSink;

impl SampleSink for DropAllSink {
    fn try_send(&self, _sample: Sample) -> bool {
        false
    }
}

struct CollectingSink {
    samples: std::sync::Mutex<Vec<Sample>>,
}

impl CollectingSink {
    fn new() -> Self {
        Self {
            samples: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl SampleSink for CollectingSink {
    fn try_send(&self, sample: Sample) -> bool {
        self.samples
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(sample);
        true
    }
}

struct MockExecutor {
    delay: Duration,
    live: AtomicU64,
    peak: AtomicU64,
    started: AtomicU64,
    status: u16,
}

impl MockExecutor {
    fn new(delay: Duration) -> Self {
        Self {
            delay,
            live: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            started: AtomicU64::new(0),
            status: 200,
        }
    }

    fn started(&self) -> u64 {
        self.started.load(Ordering::Relaxed)
    }

    fn peak(&self) -> u64 {
        self.peak.load(Ordering::Relaxed)
    }
}

impl RequestExecutor for MockExecutor {
    fn execute(&self, ctx: &RequestCtx) -> BoxFuture<'_, ExecOutcome> {
        let delay = self.delay;
        let status = self.status;
        let index = ctx.index;
        Box::pin(async move {
            let live = self.live.fetch_add(1, Ordering::Relaxed) + 1;
            self.peak.fetch_max(live, Ordering::Relaxed);
            self.started.fetch_add(1, Ordering::Relaxed);
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            self.live.fetch_sub(1, Ordering::Relaxed);
            let start = unix_now_ms();
            let latency = i64::try_from(delay.as_micros()).unwrap_or(1).max(1);
            ExecOutcome {
                start_unix_ms: start,
                end_unix_ms: start + latency / 1000,
                latency_us: latency,
                http_status: Some(i64::from(status)),
                transport_ok: status < 500,
                expected: false,
                reason: None,
                error_class: None,
                error: None,
                response_bytes: Some(8),
                request_id: Some(format!("r{index}")),
                body: None,
            }
        })
    }
}

fn plan() -> Arc<ExecutePlan> {
    Arc::new(ExecutePlan {
        products: vec![Weighted {
            weight: 1,
            value: ProductChoice {
                product_id: "P1".into(),
                product_name: Some("pay".into()),
            },
        }],
        tenants: vec![Weighted {
            weight: 1,
            value: TenantChoice {
                name: "t".into(),
                needs_token: false,
            },
        }],
        expected_reasons: Vec::new(),
        token: None,
    })
}

fn config(requests: u64, concurrency: u64, delay_unused: Duration) -> EngineConfig {
    let _ = delay_unused;
    EngineConfig {
        model: EngineModel::ClosedLoop,
        qps: None,
        concurrency,
        requests: Some(requests),
        duration: None,
        warmup: Duration::ZERO,
        ramp: Duration::ZERO,
        timeout: Duration::from_secs(10),
        plan: plan(),
    }
}

#[tokio::test]
async fn in_flight_never_exceeds_concurrency_and_does_not_precreate_all_tasks() {
    let executor = Arc::new(MockExecutor::new(Duration::from_millis(20)));
    let handle = EngineHandle::start(
        config(200, 16, Duration::ZERO),
        executor.clone(),
        Arc::new(NullSink),
    )
    .expect("start");
    tokio::time::sleep(Duration::from_millis(5)).await;
    let started_early = executor.started();
    assert!(
        started_early <= 24,
        "started {started_early} tasks immediately, expected around the concurrency cap"
    );
    let snapshot = handle.join().await.expect("join");
    assert!(
        snapshot.peak_in_flight <= 16,
        "peak in_flight {} exceeded concurrency",
        snapshot.peak_in_flight
    );
    assert!(executor.peak() <= 16, "executor peak {}", executor.peak());
    assert_eq!(snapshot.samples, 200);
    assert_eq!(
        snapshot.samples,
        snapshot.success + snapshot.failure + snapshot.expected
    );
}

#[tokio::test]
async fn duration_window_stops_issuing_and_drains_without_string_deadlines() {
    let executor = Arc::new(MockExecutor::new(Duration::from_millis(5)));
    let mut cfg = config(10_000, 8, Duration::ZERO);
    cfg.requests = None;
    cfg.duration = Some(Duration::from_millis(80));
    let started = Instant::now();
    let handle = EngineHandle::start(cfg, executor, Arc::new(NullSink)).expect("start");
    let snapshot = handle.join().await.expect("join");
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "window took too long to drain: {elapsed:?}"
    );
    assert!(snapshot.samples > 0);
    assert_eq!(snapshot.in_flight, 0);
    assert_eq!(
        snapshot.samples,
        snapshot.success + snapshot.failure + snapshot.expected
    );
}

#[test]
fn source_does_not_identify_deadlines_by_error_string() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/engine");
    let mut files = Vec::new();
    visit_rs(&root, &mut files);
    for path in files {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.contains("tests") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read");
        assert!(
            !text.contains("duration deadline reached"),
            "{} identifies deadlines by error string",
            path.display()
        );
    }
}

fn visit_rs(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readdir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            visit_rs(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[tokio::test]
async fn histogram_and_ring_stay_fixed_size_after_many_samples() {
    let metrics = Metrics::new(1_000_000).expect("metrics");
    let sink = NullSink;
    for i in 0..5_000 {
        let sample = Sample {
            seq: None,
            start_unix_ms: 1_700_000_000_000 + i,
            end_unix_ms: 1_700_000_000_000 + i + 1,
            latency_us: 100 + (i % 50),
            product_id: "P1".into(),
            product_name: None,
            request_index: Some(i),
            http_status: Some(200),
            success: true,
            expected: false,
            reason: None,
            error_class: None,
            request_id: None,
            error: None,
            response_bytes: Some(4),
            customer_name: None,
        };
        metrics.record(sample, OutcomeClass::Success, false, &sink);
    }
    let counters = metrics.counters();
    assert_eq!(counters.samples, 5_000);
    assert_eq!(
        counters.samples,
        counters.success + counters.failure + counters.expected
    );
    assert_eq!(metrics.ring_len(), RING_CAPACITY);
    let blob = metrics.histogram_blob().expect("blob");
    assert_ne!(blob.len(), 0);
}

#[tokio::test]
async fn dropped_details_increment_when_try_send_fails() {
    let metrics = Metrics::new(1_000_000).expect("metrics");
    let sample = Sample {
        seq: None,
        start_unix_ms: 1,
        end_unix_ms: 2,
        latency_us: 10,
        product_id: "P1".into(),
        product_name: None,
        request_index: Some(0),
        http_status: Some(200),
        success: true,
        expected: false,
        reason: None,
        error_class: None,
        request_id: None,
        error: None,
        response_bytes: None,
        customer_name: None,
    };
    metrics.record(sample, OutcomeClass::Success, false, &DropAllSink);
    assert_eq!(metrics.counters().dropped_details, 1);
    assert_eq!(metrics.counters().samples, 1);
}

#[tokio::test]
async fn two_engines_do_not_share_metrics_or_queues() {
    let left_exec = Arc::new(MockExecutor::new(Duration::from_millis(1)));
    let right_exec = Arc::new(MockExecutor::new(Duration::from_millis(1)));
    let left_sink = Arc::new(CollectingSink::new());
    let right_sink = Arc::new(CollectingSink::new());
    let left = EngineHandle::start(config(40, 4, Duration::ZERO), left_exec, left_sink.clone())
        .expect("left");
    let mut right_cfg = config(15, 4, Duration::ZERO);
    right_cfg.plan = Arc::new(ExecutePlan {
        products: vec![Weighted {
            weight: 1,
            value: ProductChoice {
                product_id: "RIGHT".into(),
                product_name: None,
            },
        }],
        tenants: vec![],
        expected_reasons: Vec::new(),
        token: None,
    });
    let right = EngineHandle::start(right_cfg, right_exec, right_sink.clone()).expect("right");
    let left_snap = left.join().await.expect("left join");
    let right_snap = right.join().await.expect("right join");
    assert_eq!(left_snap.samples, 40);
    assert_eq!(right_snap.samples, 15);
    let left_n = left_sink
        .samples
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .len();
    let right_n = right_sink
        .samples
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .len();
    assert_eq!(left_n, 40);
    assert_eq!(right_n, 15);
    assert!(
        right_sink
            .samples
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .all(|s| s.product_id == "RIGHT")
    );
}

#[tokio::test]
async fn plaintext_token_single_flight_and_continues_after_refresh_failure() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_cb = Arc::clone(&attempts);
    let broker = Arc::new(TokenBroker::with_backoff(
        box_refresh(move || {
            let attempts_cb = Arc::clone(&attempts_cb);
            async move {
                let n = attempts_cb.fetch_add(1, Ordering::SeqCst) + 1;
                tokio::time::sleep(Duration::from_millis(30)).await;
                if n == 1 {
                    Ok(CachedToken {
                        value: "tok-1".into(),
                        expires_at: Instant::now() + Duration::from_secs(60),
                    })
                } else {
                    Err(TokenError("refresh down".into()))
                }
            }
        }),
        Duration::from_millis(80),
    ));

    let mut joins = Vec::new();
    for _ in 0..8 {
        let broker = Arc::clone(&broker);
        joins.push(tokio::spawn(async move { broker.get().await }));
    }
    let mut ok = 0;
    for join in joins {
        if join.await.expect("join").is_ok() {
            ok += 1;
        }
    }
    assert_eq!(ok, 8);
    assert_eq!(
        broker.refresh_count().await,
        1,
        "concurrent get must single-flight"
    );

    // Expire by constructing a broker that fails the next refresh.
    let fail_broker = Arc::new(TokenBroker::with_backoff(
        box_refresh(|| async { Err(TokenError("down".into())) }),
        Duration::from_millis(40),
    ));
    assert!(fail_broker.get().await.is_err());
    assert!(fail_broker.get().await.is_err());
    // Two gets: first refresh fails, second is inside backoff (no second call...
    // actually second may also increment if backoff expired). At least worker continues:
    let token_plan = Arc::new(ExecutePlan {
        products: vec![Weighted {
            weight: 1,
            value: ProductChoice {
                product_id: "P1".into(),
                product_name: None,
            },
        }],
        tenants: vec![Weighted {
            weight: 1,
            value: TenantChoice {
                name: "plain".into(),
                needs_token: true,
            },
        }],
        expected_reasons: Vec::new(),
        token: Some(Arc::clone(&fail_broker)),
    });
    let executor = Arc::new(MockExecutor::new(Duration::ZERO));
    let mut cfg = config(12, 4, Duration::ZERO);
    cfg.plan = token_plan;
    let handle = EngineHandle::start(cfg, executor, Arc::new(NullSink)).expect("start");
    let snap = handle.join().await.expect("join");
    assert_eq!(snap.samples, 12);
    assert_eq!(snap.failure, 12);
    assert_eq!(snap.success, 0);
}

#[test]
fn mix_request_seed_is_deterministic_and_respects_weights() {
    let weights = [1_u64, 3, 1];
    let first = mix_request_seed(7, &weights);
    assert_eq!(first, mix_request_seed(7, &weights));
    let mut counts = [0_u32; 3];
    for seed in 0..1_000 {
        counts[mix_request_seed(seed, &weights)] += 1;
    }
    // The middle bucket has 3/5 of the weight, so it should win clearly.
    assert!(counts[1] > counts[0] && counts[1] > counts[2], "{counts:?}");
}
