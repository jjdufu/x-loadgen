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
    /// When true, this second fell in the warmup period.
    #[serde(default)]
    pub warmup: bool,
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
                slot.samples = slot.samples.saturating_add(incoming.samples);
                slot.success = slot.success.saturating_add(incoming.success);
                slot.failure = slot.failure.saturating_add(incoming.failure);
                slot.expected = slot.expected.saturating_add(incoming.expected);
                slot.bytes = slot.bytes.saturating_add(incoming.bytes);
                slot.warmup |= incoming.warmup;
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
        percentiles_of(&self.total)
    }

    /// Percentiles of the current window only, in microseconds.
    #[must_use]
    pub fn window_percentiles_us(&self) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
        percentiles_of(&self.histogram)
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
            };
        }
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

/// Percentiles of a locked histogram, or all-`None` when it is empty.
fn percentiles_of(
    hist: &Mutex<Histogram<u64>>,
) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
    let hist = hist
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if hist.is_empty() {
        return (None, None, None, None);
    }
    (
        Some(hist.value_at_quantile(0.50)),
        Some(hist.value_at_quantile(0.95)),
        Some(hist.value_at_quantile(0.99)),
        Some(hist.max()),
    )
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
        entry.samples = entry.samples.saturating_add(slot.samples);
        entry.success = entry.success.saturating_add(slot.success);
        entry.failure = entry.failure.saturating_add(slot.failure);
        entry.expected = entry.expected.saturating_add(slot.expected);
        entry.bytes = entry.bytes.saturating_add(slot.bytes);
        entry.warmup |= slot.warmup;
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
            },
            RingSlot {
                unix_sec: 100,
                warmup: true,
                samples: 3,
                success: 3,
                failure: 0,
                expected: 0,
                bytes: 30,
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
