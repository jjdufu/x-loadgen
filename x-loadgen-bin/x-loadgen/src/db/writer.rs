//! Bounded in-memory queue plus a dedicated SQLite write thread.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::dump::{insert_samples, write_pending};
use super::paths::DataPaths;
use super::{open_worker_connection, unix_now_ms};

/// Default bound on the in-memory sample queue.
pub const DEFAULT_CAPACITY: usize = 8192;

/// Flush when at least this many samples are waiting, even before the timer.
pub const DEFAULT_BATCH_SIZE: usize = 500;

/// How often the write thread commits a non-empty batch.
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(100);

/// How often a checkpoint row is written.
pub const DEFAULT_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);

/// One recorded request written to the worker `samples` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sample {
    /// Optional primary key; `None` lets SQLite assign `seq`.
    pub seq: Option<i64>,
    pub start_unix_ms: i64,
    pub end_unix_ms: i64,
    pub latency_us: i64,
    pub product_id: String,
    pub product_name: Option<String>,
    pub request_index: Option<i64>,
    pub http_status: Option<i64>,
    pub success: bool,
    pub expected: bool,
    pub reason: Option<String>,
    pub error_class: Option<String>,
    pub request_id: Option<String>,
    pub error: Option<String>,
    pub response_bytes: Option<i64>,
    pub customer_name: Option<String>,
}

/// Tuning knobs for a [`DetailWriter`].
#[derive(Debug, Clone)]
pub struct WriterConfig {
    /// Bounded channel capacity (engine `try_send` drops when full).
    pub capacity: usize,
    /// Commit as soon as this many samples have accumulated.
    pub batch_size: usize,
    /// Commit a non-empty batch at least this often.
    pub flush_interval: Duration,
    /// Write `worker_checkpoints` at least this often, and once more on close.
    pub checkpoint_interval: Duration,
    /// When true, every `COMMIT` is failed so the drain path can dump `pending-*.bin`.
    /// Used by tests; production always leaves this false.
    pub fail_commits: bool,
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
            batch_size: DEFAULT_BATCH_SIZE,
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            checkpoint_interval: DEFAULT_CHECKPOINT_INTERVAL,
            fail_commits: false,
        }
    }
}

/// Latest counters the engine wants persisted at the next checkpoint.
#[derive(Debug, Clone)]
pub struct CheckpointUpdate {
    pub window_index: Option<i64>,
    pub counters_json: String,
    pub histogram_blob: Option<Vec<u8>>,
}

/// Cloneable enqueue handle. Dropping a handle does not stop the write thread;
/// [`DetailWriter::close`] does, even while handles are still held.
#[derive(Clone)]
pub struct WriterHandle {
    tx: Option<SyncSender<Sample>>,
    dropped: Arc<AtomicU64>,
}

impl WriterHandle {
    /// Enqueue a sample without waiting. Matches [`DetailWriter::try_send`].
    #[must_use]
    pub fn try_send(&self, sample: Sample) -> bool {
        let Some(tx) = self.tx.as_ref() else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        };
        match tx.try_send(sample) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}

/// Per-worker sample writer. The hot path is only [`DetailWriter::try_send`].
pub struct DetailWriter {
    tx: Option<SyncSender<Sample>>,
    dropped: Arc<AtomicU64>,
    written: Arc<AtomicU64>,
    checkpoint: Arc<Mutex<Option<CheckpointUpdate>>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<Result<()>>>,
}

impl DetailWriter {
    /// Open the worker database on a dedicated thread and start draining the queue.
    ///
    /// # Errors
    ///
    /// Returns an error if the run directory cannot be created, the worker
    /// database cannot be opened or migrated, or the thread cannot be started.
    pub fn start(
        paths: &DataPaths,
        run_id: &str,
        worker_index: u32,
        config: WriterConfig,
    ) -> Result<Self> {
        if config.capacity == 0 {
            bail!("writer capacity must be at least 1");
        }
        if config.batch_size == 0 {
            bail!("writer batch size must be at least 1");
        }
        paths.ensure_run(run_id).context("create run directory")?;
        let (tx, rx) = mpsc::sync_channel(config.capacity);
        let dropped = Arc::new(AtomicU64::new(0));
        let written = Arc::new(AtomicU64::new(0));
        let checkpoint = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread_paths = paths.clone();
        let thread_run = run_id.to_string();
        let thread_dropped = Arc::clone(&dropped);
        let thread_written = Arc::clone(&written);
        let thread_checkpoint = Arc::clone(&checkpoint);
        let thread_stop = Arc::clone(&stop);
        let name = format!("x-loadgen-w{worker_index:02}");
        let join = thread::Builder::new()
            .name(name)
            .spawn(move || {
                let conn = match open_worker_connection(&thread_paths, &thread_run, worker_index) {
                    Ok(conn) => {
                        let _ = ready_tx.send(Ok(()));
                        conn
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(anyhow!("{err:#}")));
                        return Err(err);
                    }
                };
                let state = LoopState {
                    conn,
                    paths: thread_paths,
                    run_id: thread_run,
                    worker_index,
                    config,
                    dropped: thread_dropped,
                    written: thread_written,
                    checkpoint: thread_checkpoint,
                    stop: thread_stop,
                };
                state.run(&rx)
            })
            .context("spawn worker write thread")?;
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                let _ = join.join();
                return Err(err.context("open worker database on write thread"));
            }
            Err(err) => {
                let _ = join.join();
                return Err(err).context("write thread ended before it opened the database");
            }
        }
        Ok(Self {
            tx: Some(tx),
            dropped,
            written,
            checkpoint,
            stop,
            join: Some(join),
        })
    }

    /// Enqueue a sample without waiting. Returns `false` when the queue is full
    /// or the writer is already closed; the sample is dropped and counted.
    #[must_use]
    pub fn try_send(&self, sample: Sample) -> bool {
        let Some(tx) = self.tx.as_ref() else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        };
        match tx.try_send(sample) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// Number of samples dropped because the queue was full or closed.
    #[must_use]
    pub fn dropped_details(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Number of samples this writer has committed (not counting dumps).
    #[must_use]
    pub fn written_details(&self) -> u64 {
        self.written.load(Ordering::Relaxed)
    }

    /// A cloneable, non-blocking sender used as a [`crate::engine::SampleSink`].
    #[must_use]
    pub fn handle(&self) -> WriterHandle {
        WriterHandle {
            tx: self.tx.clone(),
            dropped: Arc::clone(&self.dropped),
        }
    }

    /// Replace the payload used by the next checkpoint write.
    pub fn publish_checkpoint(&self, update: CheckpointUpdate) {
        let mut guard = self
            .checkpoint
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(update);
    }

    /// Stop accepting samples, drain the queue, write a final checkpoint, and
    /// join the write thread. Outstanding [`WriterHandle`] clones do not keep
    /// the thread alive. On a write or drain failure the remaining samples are
    /// dumped to `pending-*.bin`.
    ///
    /// # Errors
    ///
    /// Returns an error if the write thread reported a failure (after dumping
    /// when possible) or if the thread panicked.
    pub fn close(mut self) -> Result<()> {
        self.shutdown_and_join()
    }

    fn shutdown_and_join(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.as_ref() {
            join.thread().unpark();
        }
        self.tx.take();
        let Some(join) = self.join.take() else {
            return Ok(());
        };
        match join.join() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err)) => Err(err),
            Err(_) => bail!("worker write thread panicked"),
        }
    }
}

impl Drop for DetailWriter {
    fn drop(&mut self) {
        let _ = self.shutdown_and_join();
    }
}

struct LoopState {
    conn: Connection,
    paths: DataPaths,
    run_id: String,
    worker_index: u32,
    config: WriterConfig,
    dropped: Arc<AtomicU64>,
    written: Arc<AtomicU64>,
    checkpoint: Arc<Mutex<Option<CheckpointUpdate>>>,
    stop: Arc<AtomicBool>,
}

impl LoopState {
    fn run(mut self, rx: &mpsc::Receiver<Sample>) -> Result<()> {
        let mut batch: Vec<Sample> = Vec::new();
        let mut last_checkpoint = Instant::now();
        let mut disconnected = false;
        let mut result: Result<()> = Ok(());

        while result.is_ok() {
            if disconnected && batch.is_empty() {
                break;
            }
            if !disconnected {
                match recv_until(
                    rx,
                    self.config.flush_interval,
                    &mut batch,
                    self.config.batch_size,
                    &self.stop,
                ) {
                    RecvOutcome::BatchReady | RecvOutcome::TimedOut => {}
                    RecvOutcome::Disconnected => {
                        drain_receiver(rx, &mut batch);
                        disconnected = true;
                    }
                }
            }
            if !batch.is_empty()
                && let Err(err) = self.commit_batch(&mut batch)
            {
                result = Err(err);
                break;
            }
            if last_checkpoint.elapsed() >= self.config.checkpoint_interval || disconnected {
                if let Err(err) = self.write_checkpoint() {
                    result = Err(err);
                    break;
                }
                last_checkpoint = Instant::now();
            }
        }

        if result.is_ok() && !batch.is_empty() {
            result = self.commit_batch(&mut batch);
        }
        if result.is_ok() {
            result = self.write_checkpoint();
        }

        if result.is_err() {
            drain_receiver(rx, &mut batch);
            if !batch.is_empty() {
                if let Err(dump_err) =
                    write_pending(&self.paths, &self.run_id, self.worker_index, &batch)
                {
                    result = result.and(Err(dump_err));
                }
                batch.clear();
            }
        }
        result
    }

    fn commit_batch(&mut self, batch: &mut Vec<Sample>) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("begin sample transaction")?;
        insert_samples(&tx, batch).context("insert sample batch")?;
        if self.config.fail_commits {
            // Roll back so the drain path can dump the same rows.
            drop(tx);
            bail!("forced commit failure for test");
        }
        tx.commit().context("commit sample batch")?;
        let added = u64::try_from(batch.len()).unwrap_or(u64::MAX);
        self.written.fetch_add(added, Ordering::Relaxed);
        batch.clear();
        Ok(())
    }

    fn write_checkpoint(&self) -> Result<()> {
        let update = {
            let guard = self
                .checkpoint
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.clone()
        };
        let (window_index, counters_json, histogram_blob) = if let Some(update) = update {
            (
                update.window_index,
                update.counters_json,
                update.histogram_blob,
            )
        } else {
            let written = self.written.load(Ordering::Relaxed);
            let dropped = self.dropped.load(Ordering::Relaxed);
            let json = serde_json::json!({
                "written": written,
                "dropped_details": dropped,
            })
            .to_string();
            (None, json, None)
        };
        let ts = i64::try_from(unix_now_ms()?).unwrap_or(i64::MAX);
        let dropped = i64::try_from(self.dropped.load(Ordering::Relaxed)).unwrap_or(i64::MAX);
        self.conn
            .execute(
                "INSERT INTO worker_checkpoints (ts, window_index, counters_json, histogram_blob, dropped_details)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(ts) DO UPDATE SET
                    window_index = excluded.window_index,
                    counters_json = excluded.counters_json,
                    histogram_blob = excluded.histogram_blob,
                    dropped_details = excluded.dropped_details",
                rusqlite::params![ts, window_index, counters_json, histogram_blob, dropped],
            )
            .context("insert worker checkpoint")?;
        Ok(())
    }
}

enum RecvOutcome {
    BatchReady,
    TimedOut,
    Disconnected,
}

fn recv_until(
    rx: &mpsc::Receiver<Sample>,
    flush_interval: Duration,
    batch: &mut Vec<Sample>,
    batch_size: usize,
    stop: &AtomicBool,
) -> RecvOutcome {
    let deadline = Instant::now() + flush_interval;
    loop {
        if stop.load(Ordering::Acquire) {
            return RecvOutcome::Disconnected;
        }
        if batch.len() >= batch_size {
            return RecvOutcome::BatchReady;
        }
        match rx.try_recv() {
            Ok(sample) => {
                batch.push(sample);
                continue;
            }
            Err(TryRecvError::Disconnected) => return RecvOutcome::Disconnected,
            Err(TryRecvError::Empty) => {}
        }
        let remain = deadline.saturating_duration_since(Instant::now());
        if remain.is_zero() {
            return RecvOutcome::TimedOut;
        }
        thread::park_timeout(remain);
    }
}

fn drain_receiver(rx: &mpsc::Receiver<Sample>, batch: &mut Vec<Sample>) {
    while let Ok(sample) = rx.try_recv() {
        batch.push(sample);
    }
}
