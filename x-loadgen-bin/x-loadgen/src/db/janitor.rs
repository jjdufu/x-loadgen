//! Detail retention: drop expired or surplus samples in worker databases.

use anyhow::{Context, Result};
use rusqlite::params;
use serde_json::Value;
use tracing::warn;

use super::open_existing_worker;
use super::paths::DataPaths;
use super::runs::RunRecord;

/// Apply the snapshot's retention settings to every worker database of `run`.
///
/// # Errors
///
/// Returns an error if a worker database cannot be opened or a DELETE fails.
pub fn apply_for_run(paths: &DataPaths, run: &RunRecord) -> Result<()> {
    let (days, rows) = retain_limits(&run.snapshot_json);
    if days.is_none() && rows.is_none() {
        return Ok(());
    }
    apply(paths, &run.id, days, rows)
}

/// Sweep every run under `paths` that has a retention policy.
///
/// # Errors
///
/// Returns an error if the central database cannot be opened. Per-run failures
/// are logged and skipped so one bad worker file does not stop the sweep.
pub fn sweep(paths: &DataPaths) -> Result<()> {
    let conn = super::open_central(paths)?;
    let runs = super::runs::list_runs(&conn, None)?;
    drop(conn);
    for run in runs {
        if let Err(err) = apply_for_run(paths, &run) {
            warn!(run_id = %run.id, error = %err, "detail retention failed");
        }
    }
    Ok(())
}

/// Apply `detail_retain_days` and/or `detail_retain_rows` to one run.
///
/// # Errors
///
/// Returns an error if a worker database cannot be updated.
pub fn apply(
    paths: &DataPaths,
    run_id: &str,
    retain_days: Option<i64>,
    retain_rows: Option<i64>,
) -> Result<()> {
    let indexes = paths.worker_indexes(run_id);
    if let Some(days) = retain_days.filter(|d| *d > 0) {
        let now = i64::try_from(super::unix_now_ms()?).unwrap_or(i64::MAX);
        let cutoff = now.saturating_sub(days.saturating_mul(86_400_000));
        for index in &indexes {
            delete_older_than(paths, run_id, *index, cutoff)?;
        }
    }
    if let Some(limit) = retain_rows.filter(|n| *n > 0) {
        trim_to_latest(paths, run_id, &indexes, limit)?;
    }
    Ok(())
}

fn retain_limits(snapshot_json: &str) -> (Option<i64>, Option<i64>) {
    let Ok(value) = serde_json::from_str::<Value>(snapshot_json) else {
        return (None, None);
    };
    let template = value.get("template").cloned().unwrap_or(Value::Null);
    (
        template.get("detail_retain_days").and_then(Value::as_i64),
        template.get("detail_retain_rows").and_then(Value::as_i64),
    )
}

fn delete_older_than(paths: &DataPaths, run_id: &str, index: u32, cutoff_ms: i64) -> Result<()> {
    let Some(conn) = open_existing_worker(paths, run_id, index, false)? else {
        return Ok(());
    };
    conn.execute(
        "DELETE FROM samples WHERE end_unix_ms < ?1",
        params![cutoff_ms],
    )
    .with_context(|| format!("delete expired samples in w{index:02} of {run_id}"))?;
    Ok(())
}

struct SampleKey {
    worker: u32,
    seq: i64,
    end_unix_ms: i64,
}

fn trim_to_latest(paths: &DataPaths, run_id: &str, indexes: &[u32], limit: i64) -> Result<()> {
    let keep = usize::try_from(limit).unwrap_or(usize::MAX);
    let mut keys = Vec::new();
    for index in indexes {
        let Some(conn) = open_existing_worker(paths, run_id, *index, true)? else {
            continue;
        };
        let mut stmt = conn.prepare("SELECT seq, end_unix_ms FROM samples")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows {
            let (seq, end_unix_ms) = row?;
            keys.push(SampleKey {
                worker: *index,
                seq,
                end_unix_ms,
            });
        }
    }
    if keys.len() <= keep {
        return Ok(());
    }
    keys.sort_by(|a, b| {
        b.end_unix_ms
            .cmp(&a.end_unix_ms)
            .then(b.worker.cmp(&a.worker))
            .then(b.seq.cmp(&a.seq))
    });
    let drop = &keys[keep..];
    let mut by_worker: std::collections::BTreeMap<u32, Vec<i64>> =
        std::collections::BTreeMap::new();
    for key in drop {
        by_worker.entry(key.worker).or_default().push(key.seq);
    }
    for (index, seqs) in by_worker {
        delete_seqs(paths, run_id, index, &seqs)?;
    }
    Ok(())
}

fn delete_seqs(paths: &DataPaths, run_id: &str, index: u32, seqs: &[i64]) -> Result<()> {
    if seqs.is_empty() {
        return Ok(());
    }
    let Some(conn) = open_existing_worker(paths, run_id, index, false)? else {
        return Ok(());
    };
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare("DELETE FROM samples WHERE seq = ?1")?;
        for seq in seqs {
            stmt.execute(params![seq])?;
        }
    }
    tx.commit()
        .with_context(|| format!("trim samples in w{index:02} of {run_id}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::insert_samples;
    use crate::db::{DataPaths, Sample, open_central, open_worker_connection};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "x-loadgen-janitor-{}-{n}-{tag}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("temp");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn sample(seq: i64, end: i64, product: &str) -> Sample {
        Sample {
            seq: Some(seq),
            start_unix_ms: end - 1,
            end_unix_ms: end,
            latency_us: 100,
            product_id: product.into(),
            product_name: None,
            request_index: Some(seq),
            http_status: Some(200),
            success: true,
            expected: false,
            reason: None,
            error_class: None,
            request_id: None,
            error: None,
            response_bytes: Some(1),
            customer_name: None,
        }
    }

    fn seed(
        paths: &DataPaths,
        run_id: &str,
        worker: u32,
        seqs: impl IntoIterator<Item = (i64, i64)>,
    ) {
        let conn = open_worker_connection(paths, run_id, worker).expect("worker");
        let samples: Vec<Sample> = seqs
            .into_iter()
            .map(|(seq, end)| sample(seq, end, "P"))
            .collect();
        insert_samples(&conn, &samples).expect("insert");
    }

    fn count(paths: &DataPaths, run_id: &str) -> i64 {
        let mut total = 0;
        for index in paths.worker_indexes(run_id) {
            let conn = open_existing_worker(paths, run_id, index, true)
                .expect("open")
                .expect("exists");
            total += conn
                .query_row("SELECT COUNT(*) FROM samples", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("count");
        }
        total
    }

    #[test]
    fn retain_rows_keeps_latest_across_workers() {
        let tmp = TempDir::new("rows");
        let paths = DataPaths::new(&tmp.path);
        open_central(&paths).expect("central");
        let run_id = "run-rows";
        // 80 on w00 (ends 1..80), 70 on w01 (ends 81..150)
        seed(&paths, run_id, 0, (1..=80).map(|i| (i, i * 10)));
        seed(&paths, run_id, 1, (1..=70).map(|i| (i, 800 + i * 10)));
        assert_eq!(count(&paths, run_id), 150);
        apply(&paths, run_id, None, Some(100)).expect("apply");
        assert!(count(&paths, run_id) <= 100);
        assert_eq!(count(&paths, run_id), 100);
        // All remaining ends must be among the latest 100 (800+10 .. 800+700, plus last of w00)
        let mut ends = Vec::new();
        for index in paths.worker_indexes(run_id) {
            let conn = open_existing_worker(&paths, run_id, index, true)
                .unwrap()
                .unwrap();
            let mut stmt = conn.prepare("SELECT end_unix_ms FROM samples").unwrap();
            let rows = stmt.query_map([], |row| row.get::<_, i64>(0)).unwrap();
            ends.extend(rows.map(|r| r.unwrap()));
        }
        ends.sort_unstable();
        assert_eq!(ends.len(), 100);
        assert_eq!(ends[0], 510); // 80*10 - 29*10? latest 100 of 150: drop 50 oldest of w00 (ends 10..500), keep 510..800 and 810..1500
        // w00 ends: 10,20,...,800 (80 values). w01: 810,820,...,1500 (70). Latest 100 = last 30 of w00 (510..800) + all 70 of w01.
        assert_eq!(ends[0], 510);
        assert_eq!(*ends.last().unwrap(), 1500);
    }

    #[test]
    fn retain_days_deletes_older_than_cutoff() {
        let tmp = TempDir::new("days");
        let paths = DataPaths::new(&tmp.path);
        open_central(&paths).expect("central");
        let run_id = "run-days";
        let now = i64::try_from(crate::db::unix_now_ms().expect("now")).expect("now fits");
        seed(
            &paths,
            run_id,
            0,
            [
                (1, now - 10 * 86_400_000),
                (2, now - 2 * 86_400_000),
                (3, now - 1_000),
            ],
        );
        apply(&paths, run_id, Some(7), None).expect("apply");
        assert_eq!(count(&paths, run_id), 2);
    }
}
