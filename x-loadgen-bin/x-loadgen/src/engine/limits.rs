//! Global hard caps over the *configured* QPS and concurrency of running tasks.

use crate::db::StoreError;

/// One running (or about-to-start) task's configured traffic.
#[derive(Debug, Clone, Copy)]
pub struct TrafficShare {
    pub qps: u64,
    pub concurrency: u64,
}

/// Sum configured QPS and concurrency. Unset caps are ignored.
#[must_use]
pub fn sum_traffic(shares: &[TrafficShare]) -> TrafficShare {
    TrafficShare {
        qps: shares
            .iter()
            .fold(0_u64, |acc, s| acc.saturating_add(s.qps)),
        concurrency: shares
            .iter()
            .fold(0_u64, |acc, s| acc.saturating_add(s.concurrency)),
    }
}

/// Check whether adding `extra` to `running` would exceed the process caps.
///
/// # Errors
///
/// Returns [`StoreError::Invalid`] when a cap would be exceeded.
pub fn check_caps(
    running: &[TrafficShare],
    extra: TrafficShare,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
) -> Result<(), StoreError> {
    let mut shares = running.to_vec();
    shares.push(extra);
    let total = sum_traffic(&shares);
    if let Some(max) = max_qps
        && total.qps > max
    {
        return Err(StoreError::invalid(format!(
            "configured QPS sum {} exceeds the global cap of {max}",
            total.qps
        )));
    }
    if let Some(max) = max_concurrency
        && total.concurrency > max
    {
        return Err(StoreError::invalid(format!(
            "configured concurrency sum {} exceeds the global cap of {max}",
            total.concurrency
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TrafficShare, check_caps, sum_traffic};

    #[test]
    fn sums_configured_values() {
        let total = sum_traffic(&[
            TrafficShare {
                qps: 4,
                concurrency: 2,
            },
            TrafficShare {
                qps: 6,
                concurrency: 3,
            },
        ]);
        assert_eq!(total.qps, 10);
        assert_eq!(total.concurrency, 5);
    }

    #[test]
    fn rejects_when_sum_exceeds_cap() {
        let running = [TrafficShare {
            qps: 4,
            concurrency: 2,
        }];
        let extra = TrafficShare {
            qps: 7,
            concurrency: 1,
        };
        assert!(check_caps(&running, extra, Some(10), None).is_err());
        assert!(check_caps(&running, extra, Some(11), None).is_ok());
        assert!(check_caps(&running, extra, Some(11), Some(2)).is_err());
        assert!(check_caps(&running, extra, Some(11), Some(3)).is_ok());
    }
}
