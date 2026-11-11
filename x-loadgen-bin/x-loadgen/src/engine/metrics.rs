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
#[derive(Debug, Clone, Default)]
pub struct RingSlot {
    /// Unix timestamp in seconds covered by this slot; `0` means empty.
    pub unix_sec: u64,
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub bytes: u64,
    /// When true, this second fell in the warmup period.
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
    in_flight: AtomicU64,
    peak_in_flight: AtomicU64,
    histogram: Mutex<Histogram<u64>>,
    ring: Mutex<Vec<RingSlot>>,
}

/// Values that can be read without locking the histogram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterSnapshot {
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub expected: u64,
    pub bytes: u64,
    pub dropped_details: u64,
    pub in_flight: u64,
    pub peak_in_flight: u64,
}

impl Metrics {
    /// Create counters and a histogram with the given microsecond ceiling.
    ///
    /// # Errors
    ///
    /// Returns an error if the histogram cannot be constructed.
    pub fn new(highest_trackable_us: u64) -> Result<Self> {
        let highest = highest_trackable_us.max(2);
        let histogram =
            Histogram::new_with_bounds(1, highest, 3).context("create latency histogram")?;
        Ok(Self {
            samples: AtomicU64::new(0),
            success: AtomicU64::new(0),
            failure: AtomicU64::new(0),
            expected: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            dropped_details: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            peak_in_flight: AtomicU64::new(0),
            histogram: Mutex::new(histogram),
            ring: Mutex::new(vec![RingSlot::default(); RING_CAPACITY]),
        })
    }

    /// Update memory counters and `try_send` the sample. Performs no I/O.
    pub fn record(&self, sample: Sample, class: OutcomeClass, warmup: bool, sink: &dyn SampleSink) {
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
            if let Ok(us) = u64::try_from(sample.latency_us.max(0)) {
                let mut hist = self
                    .histogram
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let _ = hist.record(us);
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

    /// Length of the ring allocation (always [`RING_CAPACITY`]).
    #[must_use]
    pub fn ring_len(&self) -> usize {
        self.ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Serialize the current-window histogram (V2 encoding).
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn histogram_blob(&self) -> Result<Vec<u8>> {
        let hist = self
            .histogram
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out = Vec::new();
        V2Serializer::new()
            .serialize(&hist, &mut out)
            .context("serialize histogram")?;
        Ok(out)
    }

    /// Replace the in-memory histogram with a deserialized checkpoint blob.
    ///
    /// # Errors
    ///
    /// Returns an error if the blob cannot be decoded.
    pub fn restore_histogram(&self, blob: &[u8]) -> Result<()> {
        let restored = hdrhistogram::serialization::Deserializer::new()
            .deserialize(&mut Cursor::new(blob))
            .context("restore histogram")?;
        *self
            .histogram
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = restored;
        Ok(())
    }

    /// Reset the current-window histogram (used at a loop window boundary).
    /// Counters are left untouched.
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

/// Current Unix time in milliseconds, or `0` if the clock is before the epoch.
#[must_use]
pub fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
