//! Per-task atomic counters, an in-memory histogram, and a fixed-size ring buffer.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use std::io::Cursor;

use anyhow::{Context, Result};
use hdrhistogram::Histogram;
use hdrhistogram::serialization::{Serializer, V2Serializer};

use crate::db::Sample;

/// Number of one-second slots kept in the in-memory ring.
pub const RING_CAPACITY: usize = 3600;

/// Upper bounds in microseconds for compact per-second latency buckets.
pub const RING_BUCKET_BOUNDS_US: [u64; 24] = [
    500, 1_000, 2_000, 5_000, 10_000, 20_000, 35_000, 50_000, 75_000, 100_000, 150_000, 200_000,
    300_000, 500_000, 750_000, 1_000_000, 1_500_000, 2_000_000, 3_000_000, 5_000_000, 10_000_000,
    20_000_000, 30_000_000, 60_000_000,
];
pub const RING_BUCKET_COUNT: usize = RING_BUCKET_BOUNDS_US.len() + 1;

/// Full latency distribution statistics extracted from an `HdrHistogram` (microseconds).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LatencyStats {
    pub min_us: Option<u64>,
    pub avg_us: Option<u64>,
    pub stdev_us: Option<u64>,
    pub p50_us: Option<u64>,
    pub p75_us: Option<u64>,
    pub p90_us: Option<u64>,
    pub p95_us: Option<u64>,
    pub p99_us: Option<u64>,
    pub p999_us: Option<u64>,
    pub max_us: Option<u64>,
}

impl LatencyStats {
    /// Extract full latency stats from a histogram.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn from_histogram(hist: &Histogram<u64>) -> Self {
        if hist.is_empty() {
            return Self::default();
        }
        let avg = hist.mean();
        let stdev = hist.stdev();
        Self {
            min_us: Some(hist.min()),
            avg_us: Some(avg.max(0.0).round() as u64),
            stdev_us: Some(stdev.max(0.0).round() as u64),
            p50_us: Some(hist.value_at_quantile(0.50)),
            p75_us: Some(hist.value_at_quantile(0.75)),
            p90_us: Some(hist.value_at_quantile(0.90)),
            p95_us: Some(hist.value_at_quantile(0.95)),
            p99_us: Some(hist.value_at_quantile(0.99)),
            p999_us: Some(hist.value_at_quantile(0.999)),
            max_us: Some(hist.max()),
        }
    }

    /// Backwards-style 4-tuple `(p50, p95, p99, max)`.
    #[must_use]
    pub const fn tuple4(self) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
        (self.p50_us, self.p95_us, self.p99_us, self.max_us)
    }
}

/// How a completed request is counted (the three buckets are disjoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeClass {
    /// Transport and expectation checks both succeeded.
    Success,
    /// An unexpected failure.
    Failure,
    /// The outcome was predicted (for example an expected reject).
    Expected,
}

/// A single second in the ring buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RingSlot {
    /// Unix timestamp in seconds covered by this slot; `0` means empty.
    pub unix_sec: u64,
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub bytes: u64,
    #[serde(default)]
    pub request_bytes: u64,
    #[serde(default)]
    pub latency_count: u64,
    #[serde(default)]
    pub latency_sum_us: u64,
    #[serde(default)]
    pub latency_min_us: u64,
    #[serde(default)]
    pub latency_max_us: u64,
    #[serde(default)]
    pub in_flight_max: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub latency_buckets: Vec<u64>,
    /// When true, this second fell in the warmup period.
    #[serde(default)]
    pub warmup: bool,
}

impl RingSlot {
    /// Average latency for this second in microseconds.
    #[must_use]
    pub fn avg_latency_us(&self) -> Option<u64> {
        (self.latency_count > 0).then(|| self.latency_sum_us / self.latency_count)
    }

    /// Approximate quantile (`0.0..=1.0`) from the compact bucket counts.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    pub fn approx_quantile_us(&self, q: f64) -> Option<u64> {
        if self.latency_count == 0 || self.latency_buckets.len() != RING_BUCKET_COUNT {
            return None;
        }
        let target = ((self.latency_count as f64) * q.clamp(0.0, 1.0))
            .ceil()
            .max(1.0) as u64;
        let mut cum = 0_u64;
        for (idx, &count) in self.latency_buckets.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let prev_cum = cum;
            cum = cum.saturating_add(count);
            if cum >= target {
                let lo = if idx == 0 {
                    self.latency_min_us
                } else {
                    RING_BUCKET_BOUNDS_US[idx - 1].max(self.latency_min_us)
                };
                let hi = if idx < RING_BUCKET_BOUNDS_US.len() {
                    RING_BUCKET_BOUNDS_US[idx].min(self.latency_max_us.max(lo))
                } else {
                    self.latency_max_us.max(lo)
                };
                let frac = (target.saturating_sub(prev_cum) as f64) / (count as f64);
                let est = (lo as f64) + frac * ((hi.saturating_sub(lo)) as f64);
                return Some((est.round() as u64).clamp(
                    self.latency_min_us,
                    self.latency_max_us.max(self.latency_min_us),
                ));
            }
        }
        Some(self.latency_max_us)
    }
}

fn bucket_index_for_us(us: u64) -> usize {
    RING_BUCKET_BOUNDS_US
        .iter()
        .position(|&bound| us <= bound)
        .unwrap_or(RING_BUCKET_BOUNDS_US.len())
}

/// Per-task live metrics. The histogram and ring have a fixed size.
pub struct Metrics {
    samples: AtomicU64,
    success: AtomicU64,
    failure: AtomicU64,
    expected: AtomicU64,
    bytes: AtomicU64,
    dropped_details: AtomicU64,
    missed_sends: AtomicU64,
    in_flight: AtomicU64,
    peak_in_flight: AtomicU64,
    /// Current window only; reset at each window boundary.
    histogram: Mutex<Histogram<u64>>,
    /// Entire run; never reset except when restoring a checkpoint on resume.
    total: Mutex<Histogram<u64>>,
    ring: Mutex<Vec<RingSlot>>,
}

/// Values that can be read without locking the histogram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CounterSnapshot {
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub bytes: u64,
    pub dropped_details: u64,
    pub in_flight: u64,
    pub peak_in_flight: u64,
    /// Open-loop sends dropped because the dispatcher fell more than
    /// [`crate::engine::MAX_SCHEDULE_LAG`] behind schedule.
    pub missed_sends: u64,
}

impl Metrics {
    /// Create counters and histograms sized for the given microsecond ceiling.
    ///
    /// The ceiling is only the initial size: histograms auto-resize, so
    /// samples slower than the configured timeout are still recorded instead
    /// of being silently dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if the histogram cannot be constructed.
    pub fn new(highest_trackable_us: u64) -> Result<Self> {
        let histogram = new_latency_histogram(highest_trackable_us)?;
        let total = new_latency_histogram(highest_trackable_us)?;
        Ok(Self {
            samples: AtomicU64::new(0),
            success: AtomicU64::new(0),
            failure: AtomicU64::new(0),
            expected: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            dropped_details: AtomicU64::new(0),
            missed_sends: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            peak_in_flight: AtomicU64::new(0),
            histogram: Mutex::new(histogram),
            total: Mutex::new(total),
            ring: Mutex::new(vec![RingSlot::default(); RING_CAPACITY]),
        })
    }

    /// Update memory counters and `try_send` the sample. Performs no I/O.
    pub fn record(&self, sample: Sample, class: OutcomeClass, warmup: bool, sink: &dyn SampleSink) {
        self.record_with_queue(sample, class, warmup, 0, sink);
    }

    /// Like [`Self::record`], but charges `queue_us` (time the request waited
    /// past its intended open-loop send time) to the latency histogram.
    ///
    /// The stored sample keeps the raw on-the-wire latency; only the
    /// percentiles are corrected for coordinated omission. Requests that never
    /// reached the wire (`latency_us <= 0`, e.g. config or token failures) are
    /// counted but not added to the histogram, so they cannot drag p50 down.
    pub fn record_with_queue(
        &self,
        mut sample: Sample,
        class: OutcomeClass,
        warmup: bool,
        queue_us: u64,
        sink: &dyn SampleSink,
    ) {
        sample.warmup = warmup;
        if warmup {
            self.touch_ring(&sample, class, true);
        } else {
            self.samples.fetch_add(1, Ordering::Relaxed);
            match class {
                OutcomeClass::Success => {
                    self.success.fetch_add(1, Ordering::Relaxed);
                }
                OutcomeClass::Failure => {
                    self.failure.fetch_add(1, Ordering::Relaxed);
                }
                OutcomeClass::Expected => {
                    self.expected.fetch_add(1, Ordering::Relaxed);
                }
            }
            if sample.latency_us > 0
                && let Ok(raw) = u64::try_from(sample.latency_us)
            {
                let us = raw.saturating_add(queue_us);
                {
                    let mut hist = self
                        .histogram
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    record_saturating(&mut hist, us);
                }
                {
                    let mut total = self
                        .total
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    record_saturating(&mut total, us);
                }
            }
            if let Some(bytes) = sample.response_bytes
                && let Ok(bytes) = u64::try_from(bytes.max(0))
            {
                self.bytes.fetch_add(bytes, Ordering::Relaxed);
            }
            self.touch_ring(&sample, class, false);
        }
        if !sink.try_send(sample) {
            self.dropped_details.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Current in-flight request count.
    #[must_use]
    pub fn in_flight(&self) -> u64 {
        self.in_flight.load(Ordering::Relaxed)
    }

    /// Increment in-flight and track the peak.
    pub fn begin_in_flight(&self) -> u64 {
        let now = self.in_flight.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak_in_flight.fetch_max(now, Ordering::Relaxed);
        now
    }

    /// Count open-loop sends dropped by the scheduler.
    pub fn add_missed_sends(&self, n: u64) {
        self.missed_sends.fetch_add(n, Ordering::Relaxed);
    }

    /// Decrement in-flight after a request finishes.
    pub fn end_in_flight(&self) {
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
    }

    /// Snapshot of the atomic counters.
    #[must_use]
    pub fn counters(&self) -> CounterSnapshot {
        CounterSnapshot {
            samples: self.samples.load(Ordering::Relaxed),
            success: self.success.load(Ordering::Relaxed),
            failure: self.failure.load(Ordering::Relaxed),
            expected: self.expected.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            dropped_details: self.dropped_details.load(Ordering::Relaxed),
            in_flight: self.in_flight.load(Ordering::Relaxed),
            peak_in_flight: self.peak_in_flight.load(Ordering::Relaxed),
            missed_sends: self.missed_sends.load(Ordering::Relaxed),
        }
    }

    /// Copy the ring (fixed 3600 slots) for `/timeseries`.
    #[must_use]
    pub fn ring_snapshot(&self) -> Vec<RingSlot> {
        self.ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Non-empty second buckets ordered chronologically by `unix_sec`.
    #[must_use]
    pub fn timeseries_slots(&self) -> Vec<RingSlot> {
        let mut slots: Vec<RingSlot> = self
            .ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|slot| slot.unix_sec != 0)
            .cloned()
            .collect();
        slots.sort_by_key(|slot| slot.unix_sec);
        slots
    }

    /// Restore previously checkpointed second buckets into the ring on resume.
    pub fn restore_timeseries(&self, slots: &[RingSlot]) {
        let mut ring = self
            .ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for incoming in slots {
            if incoming.unix_sec == 0 {
                continue;
            }
            let idx =
                usize::try_from(incoming.unix_sec % u64::try_from(RING_CAPACITY).unwrap_or(1))
                    .unwrap_or(0);
            let slot = &mut ring[idx];
            if slot.unix_sec < incoming.unix_sec {
                *slot = incoming.clone();
            } else if slot.unix_sec == incoming.unix_sec {
                merge_ring_slot_into(slot, incoming);
            }
        }
    }

    /// Length of the ring allocation (always [`RING_CAPACITY`]).
    #[must_use]
    pub fn ring_len(&self) -> usize {
        self.ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Serialize the run-wide histogram (V2 encoding), used in checkpoints
    /// and reports.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn histogram_blob(&self) -> Result<Vec<u8>> {
        let hist = self
            .total
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out = Vec::new();
        V2Serializer::new()
            .serialize(&hist, &mut out)
            .context("serialize histogram")?;
        Ok(out)
    }

    /// Serialize the current-window histogram (V2 encoding).
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn window_histogram_blob(&self) -> Result<Vec<u8>> {
        let hist = self
            .histogram
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out = Vec::new();
        V2Serializer::new()
            .serialize(&hist, &mut out)
            .context("serialize window histogram")?;
        Ok(out)
    }

    /// Replace the run-wide histogram with a deserialized checkpoint blob.
    /// The current-window histogram is left untouched so a resumed window
    /// starts clean.
    ///
    /// # Errors
    ///
    /// Returns an error if the blob cannot be decoded.
    pub fn restore_histogram(&self, blob: &[u8]) -> Result<()> {
        let mut restored: Histogram<u64> = hdrhistogram::serialization::Deserializer::new()
            .deserialize(&mut Cursor::new(blob))
            .context("restore histogram")?;
        restored.auto(true);
        *self
            .total
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = restored;
        Ok(())
    }

    /// Overlay counters from a checkpoint so a resumed run continues the totals.
    pub fn restore_counters(&self, snapshot: CounterSnapshot) {
        self.samples.store(snapshot.samples, Ordering::Relaxed);
        self.success.store(snapshot.success, Ordering::Relaxed);
        self.failure.store(snapshot.failure, Ordering::Relaxed);
        self.expected.store(snapshot.expected, Ordering::Relaxed);
        self.bytes.store(snapshot.bytes, Ordering::Relaxed);
        self.dropped_details
            .store(snapshot.dropped_details, Ordering::Relaxed);
        self.missed_sends
            .store(snapshot.missed_sends, Ordering::Relaxed);
    }

    /// Percentiles of the run-wide histogram, in microseconds.
    #[must_use]
    pub fn percentiles_us(&self) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
        self.latency_stats_us().tuple4()
    }

    /// Full latency stats of the run-wide histogram, in microseconds.
    #[must_use]
    pub fn latency_stats_us(&self) -> LatencyStats {
        let hist = self
            .total
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        LatencyStats::from_histogram(&hist)
    }

    /// Percentiles of the current window only, in microseconds.
    #[must_use]
    pub fn window_percentiles_us(&self) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
        let hist = self
            .histogram
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        LatencyStats::from_histogram(&hist).tuple4()
    }

    /// Reset the current-window histogram (used at a loop window boundary).
    /// The run-wide histogram and counters are left untouched.
    pub fn reset_window_histogram(&self) {
        let mut hist = self
            .histogram
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        hist.reset();
    }

    fn touch_ring(&self, sample: &Sample, class: OutcomeClass, warmup: bool) {
        let unix_sec = u64::try_from((sample.end_unix_ms / 1000).max(0)).unwrap_or(0);
        if unix_sec == 0 {
            return;
        }
        let in_flight = self.in_flight.load(Ordering::Relaxed);
        let idx =
            usize::try_from(unix_sec % u64::try_from(RING_CAPACITY).unwrap_or(1)).unwrap_or(0);
        let mut ring = self
            .ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let slot = &mut ring[idx];
        if slot.unix_sec != unix_sec {
            *slot = RingSlot {
                unix_sec,
                warmup,
                samples: 0,
                success: 0,
                failure: 0,
                expected: 0,
                bytes: 0,
                request_bytes: 0,
                latency_count: 0,
                latency_sum_us: 0,
                latency_min_us: 0,
                latency_max_us: 0,
                in_flight_max: 0,
                latency_buckets: Vec::new(),
            };
        }
        slot.in_flight_max = slot.in_flight_max.max(in_flight);
        if !warmup {
            slot.samples = slot.samples.saturating_add(1);
            match class {
                OutcomeClass::Success => slot.success = slot.success.saturating_add(1),
                OutcomeClass::Failure => slot.failure = slot.failure.saturating_add(1),
                OutcomeClass::Expected => slot.expected = slot.expected.saturating_add(1),
            }
            if let Some(bytes) = sample.response_bytes
                && let Ok(bytes) = u64::try_from(bytes.max(0))
            {
                slot.bytes = slot.bytes.saturating_add(bytes);
            }
            if let Some(req_bytes) = sample.request_bytes
                && let Ok(req_bytes) = u64::try_from(req_bytes.max(0))
            {
                slot.request_bytes = slot.request_bytes.saturating_add(req_bytes);
            }
            if sample.latency_us > 0
                && let Ok(us) = u64::try_from(sample.latency_us)
            {
                slot.latency_count = slot.latency_count.saturating_add(1);
                slot.latency_sum_us = slot.latency_sum_us.saturating_add(us);
                slot.latency_min_us = if slot.latency_min_us == 0 {
                    us
                } else {
                    slot.latency_min_us.min(us)
                };
                slot.latency_max_us = slot.latency_max_us.max(us);
                if slot.latency_buckets.len() != RING_BUCKET_COUNT {
                    slot.latency_buckets = vec![0; RING_BUCKET_COUNT];
                }
                let bidx = bucket_index_for_us(us);
                slot.latency_buckets[bidx] = slot.latency_buckets[bidx].saturating_add(1);
            }
        }
        slot.warmup |= warmup;
    }
}

/// Destination for a recorded sample on the hot path. Implementations must not block.
pub trait SampleSink: Send + Sync {
    /// Enqueue without waiting. Returns `false` if the sample was dropped.
    fn try_send(&self, sample: Sample) -> bool;
}

impl SampleSink for crate::db::DetailWriter {
    fn try_send(&self, sample: Sample) -> bool {
        crate::db::DetailWriter::try_send(self, sample)
    }
}

impl SampleSink for crate::db::WriterHandle {
    fn try_send(&self, sample: Sample) -> bool {
        crate::db::WriterHandle::try_send(self, sample)
    }
}

/// Create an auto-resizing latency histogram (1 µs .. `initial_highest_us`, 3 significant digits).
///
/// # Errors
///
/// Returns an error if the histogram cannot be constructed.
pub fn new_latency_histogram(initial_highest_us: u64) -> Result<Histogram<u64>> {
    let mut hist = Histogram::new_with_bounds(1, initial_highest_us.max(2), 3)
        .context("create latency histogram")?;
    hist.auto(true);
    Ok(hist)
}

/// Record a value, clamping to the largest representable value if the
/// histogram still cannot hold it (it should not happen with auto-resize).
fn record_saturating(hist: &mut Histogram<u64>, us: u64) {
    if hist.record(us).is_err() {
        hist.saturating_record(us);
    }
}

fn merge_ring_slot_into(target: &mut RingSlot, incoming: &RingSlot) {
    target.samples = target.samples.saturating_add(incoming.samples);
    target.success = target.success.saturating_add(incoming.success);
    target.failure = target.failure.saturating_add(incoming.failure);
    target.expected = target.expected.saturating_add(incoming.expected);
    target.bytes = target.bytes.saturating_add(incoming.bytes);
    target.request_bytes = target.request_bytes.saturating_add(incoming.request_bytes);
    target.latency_count = target.latency_count.saturating_add(incoming.latency_count);
    target.latency_sum_us = target
        .latency_sum_us
        .saturating_add(incoming.latency_sum_us);
    if incoming.latency_min_us > 0 {
        target.latency_min_us = if target.latency_min_us == 0 {
            incoming.latency_min_us
        } else {
            target.latency_min_us.min(incoming.latency_min_us)
        };
    }
    target.latency_max_us = target.latency_max_us.max(incoming.latency_max_us);
    target.in_flight_max = target.in_flight_max.saturating_add(incoming.in_flight_max);
    if incoming.latency_buckets.len() == RING_BUCKET_COUNT {
        if target.latency_buckets.len() != RING_BUCKET_COUNT {
            target.latency_buckets = vec![0; RING_BUCKET_COUNT];
        }
        for (dst, &src) in target
            .latency_buckets
            .iter_mut()
            .zip(incoming.latency_buckets.iter())
        {
            *dst = dst.saturating_add(src);
        }
    }
    target.warmup |= incoming.warmup;
}

/// Current Unix time in milliseconds, or `0` if the clock is before the epoch.
#[must_use]
pub fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Merge second-level [`RingSlot`] buckets across workers, ordered by `unix_sec`
/// and bounded to the most recent [`RING_CAPACITY`] seconds.
#[must_use]
pub fn merge_timeseries_slots<I: IntoIterator<Item = RingSlot>>(iter: I) -> Vec<RingSlot> {
    let mut by_sec: std::collections::BTreeMap<u64, RingSlot> = std::collections::BTreeMap::new();
    for slot in iter {
        if slot.unix_sec == 0 {
            continue;
        }
        let entry = by_sec.entry(slot.unix_sec).or_insert_with(|| RingSlot {
            unix_sec: slot.unix_sec,
            ..RingSlot::default()
        });
        merge_ring_slot_into(entry, &slot);
    }
    let len = by_sec.len();
    by_sec
        .into_values()
        .skip(len.saturating_sub(RING_CAPACITY))
        .collect()
}

#[cfg(test)]
mod timeseries_tests {
    use super::{Metrics, OutcomeClass, RingSlot, SampleSink, merge_timeseries_slots};
    use crate::db::Sample;

    struct NullSink;
    impl SampleSink for NullSink {
        fn try_send(&self, _sample: Sample) -> bool {
            true
        }
    }

    fn sample_at_sec(sec: i64) -> Sample {
        Sample {
            seq: None,
            start_unix_ms: sec * 1000,
            end_unix_ms: sec * 1000 + 10,
            latency_us: 10_000,
            product_id: "P1".to_string(),
            product_name: None,
            customer_name: None,
            request_index: Some(0),
            http_status: Some(200),
            success: true,
            expected: false,
            reason: None,
            error_class: None,
            request_id: None,
            error: None,
            response_bytes: Some(128),
            ..Sample::default()
        }
    }

    #[test]
    fn timeseries_slots_stay_chronological_across_hour_wrap_and_restore() {
        let metrics = Metrics::new(60_000_000).expect("metrics");
        // 3599 maps to slot 3599; 3600 maps to slot 0; 3601 maps to slot 1.
        for sec in [3599_i64, 3600, 3601] {
            metrics.record(sample_at_sec(sec), OutcomeClass::Success, false, &NullSink);
        }
        let slots = metrics.timeseries_slots();
        let secs: Vec<u64> = slots.iter().map(|s| s.unix_sec).collect();
        assert_eq!(secs, vec![3599, 3600, 3601]);

        let resumed = Metrics::new(60_000_000).expect("resumed");
        resumed.restore_timeseries(&slots);
        resumed.record(sample_at_sec(3601), OutcomeClass::Failure, false, &NullSink);
        let after = resumed.timeseries_slots();
        assert_eq!(after.len(), 3);
        assert_eq!(after[2].unix_sec, 3601);
        assert_eq!(after[2].samples, 2);
        assert_eq!(after[2].success, 1);
        assert_eq!(after[2].failure, 1);
    }

    #[test]
    fn merge_timeseries_sums_matching_seconds_across_workers() {
        let merged = merge_timeseries_slots([
            RingSlot {
                unix_sec: 100,
                warmup: false,
                samples: 5,
                success: 4,
                failure: 1,
                expected: 0,
                bytes: 50,
                ..RingSlot::default()
            },
            RingSlot {
                unix_sec: 100,
                warmup: true,
                samples: 3,
                success: 3,
                failure: 0,
                expected: 0,
                bytes: 30,
                ..RingSlot::default()
            },
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].samples, 8);
        assert_eq!(merged[0].success, 7);
        assert_eq!(merged[0].failure, 1);
        assert_eq!(merged[0].bytes, 80);
        assert!(merged[0].warmup);
    }
}
