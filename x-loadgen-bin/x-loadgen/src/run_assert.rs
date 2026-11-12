//! Run-level assertion thresholds and verdict evaluation.
//!
//! Thresholds are supplied when a run is created, stored in the run snapshot,
//! and evaluated against live or final counters. A missing `assert` object
//! yields a `none` verdict (HTTP 200 on the verdict endpoint).

use serde::{Deserialize, Serialize};

use crate::db::StoreError;

/// Upper and lower bounds checked against a run's metrics.
///
/// Latency bounds are milliseconds. `max_error_rate` and `min_qps_ratio` are
/// ratios in `[0, 1]` and `(0, +∞)` respectively. Omitted fields are not
/// checked. All comparisons are inclusive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AssertSpec {
    /// Upper bound on p50 latency, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p50_ms: Option<f64>,
    /// Upper bound on p95 latency, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p95_ms: Option<f64>,
    /// Upper bound on p99 latency, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p99_ms: Option<f64>,
    /// Upper bound on the maximum sample latency, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ms: Option<f64>,
    /// Upper bound on `failure / samples` (0–1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_error_rate: Option<f64>,
    /// Lower bound on `achieved_qps / configured_qps`.
    ///
    /// Achieved QPS is `samples / elapsed_seconds` over the wall-clock interval
    /// from `started_at` to `ended_at` (or now, while the run is still going).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_qps_ratio: Option<f64>,
}

impl AssertSpec {
    /// `true` when no threshold is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.p50_ms.is_none()
            && self.p95_ms.is_none()
            && self.p99_ms.is_none()
            && self.max_ms.is_none()
            && self.max_error_rate.is_none()
            && self.min_qps_ratio.is_none()
    }

    /// Reject non-positive latencies, error rates outside `[0, 1]`, or a
    /// non-positive QPS ratio.
    ///
    /// # Errors
    ///
    /// Returns a validation error describing the first invalid field.
    pub fn validate(&self) -> Result<(), StoreError> {
        for (name, value) in [
            ("p50_ms", self.p50_ms),
            ("p95_ms", self.p95_ms),
            ("p99_ms", self.p99_ms),
            ("max_ms", self.max_ms),
        ] {
            if let Some(v) = value
                && v <= 0.0
            {
                return Err(StoreError::invalid(format!(
                    "{name} must be greater than 0"
                )));
            }
        }
        if let Some(rate) = self.max_error_rate
            && !(0.0..=1.0).contains(&rate)
        {
            return Err(StoreError::invalid(
                "max_error_rate must be between 0 and 1 inclusive",
            ));
        }
        if let Some(ratio) = self.min_qps_ratio
            && ratio <= 0.0
        {
            return Err(StoreError::invalid("min_qps_ratio must be greater than 0"));
        }
        Ok(())
    }
}

/// Observed metrics used for assertion evaluation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActualMetrics {
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p50_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p99_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub achieved_qps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_qps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qps_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<i64>,
}

/// Raw counters and timings used to build [`ActualMetrics`].
#[derive(Debug, Clone, Default)]
pub struct ActualParts {
    pub samples: u64,
    pub success: u64,
    pub failure: u64,
    pub p50_us: Option<u64>,
    pub p95_us: Option<u64>,
    pub p99_us: Option<u64>,
    pub max_us: Option<u64>,
    pub configured_qps: Option<u64>,
    pub elapsed_ms: Option<i64>,
}

impl ActualMetrics {
    /// Build actuals from counters, microsecond percentiles, and wall-clock
    /// elapsed time.
    #[must_use]
    pub fn from_parts(parts: &ActualParts) -> Self {
        let error_rate = if parts.samples == 0 {
            None
        } else {
            Some(u64_to_f64(parts.failure) / u64_to_f64(parts.samples))
        };
        let achieved_qps = parts
            .elapsed_ms
            .and_then(|ms| achieved_qps(parts.samples, ms));
        let configured = parts.configured_qps.filter(|q| *q > 0).map(u64_to_f64);
        let qps_ratio = match (achieved_qps, configured) {
            (Some(got), Some(want)) if want > 0.0 => Some(got / want),
            _ => None,
        };
        Self {
            samples: parts.samples,
            success: parts.success,
            failure: parts.failure,
            p50_ms: parts.p50_us.map(us_to_ms),
            p95_ms: parts.p95_us.map(us_to_ms),
            p99_ms: parts.p99_us.map(us_to_ms),
            max_ms: parts.max_us.map(us_to_ms),
            error_rate,
            achieved_qps,
            configured_qps: configured,
            qps_ratio,
            elapsed_ms: parts.elapsed_ms,
        }
    }
}

/// Outcome of evaluating an [`AssertSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VerdictResult {
    Pass,
    Fail,
    Pending,
    None,
}

/// A single threshold that was not met (or could not be computed).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FailedCheck {
    pub check: String,
    pub wanted: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<f64>,
}

/// Full verdict returned in run detail, `report.json`, and `GET .../verdict`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verdict {
    pub result: VerdictResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assert: Option<AssertSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<ActualMetrics>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<FailedCheck>,
}

/// Evaluate `spec` against `actual`.
///
/// When `spec` is missing or empty the result is [`VerdictResult::None`].
/// When the run is still in progress the result is [`VerdictResult::Pending`]
/// (failed checks so far are still reported). A finished run is `pass` or
/// `fail`.
#[must_use]
pub fn evaluate(spec: Option<&AssertSpec>, actual: &ActualMetrics, running: bool) -> Verdict {
    let Some(spec) = spec.filter(|s| !s.is_empty()) else {
        return Verdict {
            result: VerdictResult::None,
            assert: None,
            actual: Some(actual.clone()),
            failed: Vec::new(),
        };
    };
    let mut failed = Vec::new();
    push_upper(&mut failed, "p50_ms", spec.p50_ms, actual.p50_ms);
    push_upper(&mut failed, "p95_ms", spec.p95_ms, actual.p95_ms);
    push_upper(&mut failed, "p99_ms", spec.p99_ms, actual.p99_ms);
    push_upper(&mut failed, "max_ms", spec.max_ms, actual.max_ms);
    push_upper(
        &mut failed,
        "max_error_rate",
        spec.max_error_rate,
        actual.error_rate,
    );
    push_lower(
        &mut failed,
        "min_qps_ratio",
        spec.min_qps_ratio,
        actual.qps_ratio,
    );
    let result = if running {
        VerdictResult::Pending
    } else if failed.is_empty() {
        VerdictResult::Pass
    } else {
        VerdictResult::Fail
    };
    Verdict {
        result,
        assert: Some(spec.clone()),
        actual: Some(actual.clone()),
        failed,
    }
}

/// Whether a run status is still in progress (verdict endpoint returns 409).
#[must_use]
pub fn status_is_running(status: &str) -> bool {
    matches!(status, "running" | "stopping")
}

fn push_upper(
    failed: &mut Vec<FailedCheck>,
    check: &str,
    wanted: Option<f64>,
    actual: Option<f64>,
) {
    let Some(wanted) = wanted else {
        return;
    };
    let unmet = match actual {
        Some(got) => got > wanted,
        None => true,
    };
    if unmet {
        failed.push(FailedCheck {
            check: check.to_string(),
            wanted,
            actual,
        });
    }
}

fn push_lower(
    failed: &mut Vec<FailedCheck>,
    check: &str,
    wanted: Option<f64>,
    actual: Option<f64>,
) {
    let Some(wanted) = wanted else {
        return;
    };
    let unmet = match actual {
        Some(got) => got < wanted,
        None => true,
    };
    if unmet {
        failed.push(FailedCheck {
            check: check.to_string(),
            wanted,
            actual,
        });
    }
}

fn us_to_ms(us: u64) -> f64 {
    u64_to_f64(us) / 1000.0
}

fn achieved_qps(samples: u64, elapsed_ms: i64) -> Option<f64> {
    if elapsed_ms <= 0 {
        return None;
    }
    let secs = i64_to_f64(elapsed_ms) / 1000.0;
    if secs <= 0.0 {
        None
    } else {
        Some(u64_to_f64(samples) / secs)
    }
}

fn u64_to_f64(value: u64) -> f64 {
    f64::from(u32::try_from(value.min(u64::from(u32::MAX))).unwrap_or(u32::MAX))
}

fn i64_to_f64(value: i64) -> f64 {
    let clamped = value.clamp(0, i64::from(i32::MAX));
    f64::from(i32::try_from(clamped).unwrap_or(i32::MAX))
}

#[cfg(test)]
mod tests {
    use super::{ActualMetrics, AssertSpec, VerdictResult, evaluate};

    fn actual(
        samples: u64,
        failure: u64,
        p99_us: Option<u64>,
        elapsed_ms: i64,
        qps: u64,
    ) -> ActualMetrics {
        ActualMetrics::from_parts(&super::ActualParts {
            samples,
            success: samples.saturating_sub(failure),
            failure,
            p50_us: Some(1_000),
            p95_us: Some(2_000),
            p99_us,
            max_us: Some(5_000),
            configured_qps: Some(qps),
            elapsed_ms: Some(elapsed_ms),
        })
    }

    #[test]
    fn empty_spec_is_none() {
        let v = evaluate(None, &actual(10, 0, Some(3_000), 1_000, 10), false);
        assert_eq!(v.result, VerdictResult::None);
        assert_eq!(v.failed.len(), 0);
        let empty = AssertSpec::default();
        let v = evaluate(Some(&empty), &actual(10, 0, Some(3_000), 1_000, 10), false);
        assert_eq!(v.result, VerdictResult::None);
    }

    #[test]
    fn pass_when_all_thresholds_hold() {
        let spec = AssertSpec {
            p99_ms: Some(10.0),
            max_error_rate: Some(0.1),
            min_qps_ratio: Some(0.5),
            ..AssertSpec::default()
        };
        // 100 samples in 1000 ms = 100 qps, configured 100, ratio 1.0
        // p99 = 3000 us = 3 ms
        let v = evaluate(Some(&spec), &actual(100, 1, Some(3_000), 1_000, 100), false);
        assert_eq!(v.result, VerdictResult::Pass);
        assert_eq!(v.failed.len(), 0);
    }

    #[test]
    fn fail_lists_each_broken_check() {
        let spec = AssertSpec {
            p99_ms: Some(1.0),
            max_error_rate: Some(0.01),
            min_qps_ratio: Some(0.95),
            ..AssertSpec::default()
        };
        // p99 = 5 ms > 1; 10/100 = 0.1 > 0.01; 10 samples / 2s = 5 qps vs 100, ratio 0.05
        let v = evaluate(
            Some(&spec),
            &actual(100, 10, Some(5_000), 2_000, 100),
            false,
        );
        assert_eq!(v.result, VerdictResult::Fail);
        let names: Vec<_> = v.failed.iter().map(|c| c.check.as_str()).collect();
        assert_eq!(names, ["p99_ms", "max_error_rate", "min_qps_ratio"]);
    }

    #[test]
    fn running_is_pending_even_when_checks_fail() {
        let spec = AssertSpec {
            max_error_rate: Some(0.0),
            ..AssertSpec::default()
        };
        let v = evaluate(Some(&spec), &actual(10, 1, Some(1_000), 500, 10), true);
        assert_eq!(v.result, VerdictResult::Pending);
        assert_eq!(v.failed.len(), 1);
    }

    #[test]
    fn missing_actual_fails_the_check() {
        let spec = AssertSpec {
            p99_ms: Some(10.0),
            min_qps_ratio: Some(0.9),
            ..AssertSpec::default()
        };
        let actual = ActualMetrics::from_parts(&super::ActualParts::default());
        let v = evaluate(Some(&spec), &actual, false);
        assert_eq!(v.result, VerdictResult::Fail);
        assert_eq!(v.failed.len(), 2);
        assert!(v.failed.iter().all(|c| c.actual.is_none()));
    }

    #[test]
    fn validate_rejects_out_of_range() {
        let mut spec = AssertSpec {
            p95_ms: Some(0.0),
            ..AssertSpec::default()
        };
        assert!(spec.validate().is_err());
        spec.p95_ms = Some(10.0);
        spec.max_error_rate = Some(1.5);
        assert!(spec.validate().is_err());
        spec.max_error_rate = Some(0.0);
        spec.min_qps_ratio = Some(-1.0);
        assert!(spec.validate().is_err());
        spec.min_qps_ratio = Some(0.9);
        spec.validate().expect("ok");
    }

    #[test]
    fn inclusive_boundaries_pass() {
        let spec = AssertSpec {
            p99_ms: Some(3.0),
            max_error_rate: Some(0.1),
            min_qps_ratio: Some(1.0),
            ..AssertSpec::default()
        };
        let v = evaluate(
            Some(&spec),
            &actual(100, 10, Some(3_000), 1_000, 100),
            false,
        );
        assert_eq!(v.result, VerdictResult::Pass, "{v:?}");
    }
}
