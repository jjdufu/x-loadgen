//! Coordinates concurrent runs, caps, windows, stop/resume, and checkpoints.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::db::runs::{
    NewRun, RunRecord, STATUS_FAILED, STATUS_FINISHED, STATUS_INTERRUPTED, STATUS_RUNNING,
    STATUS_STOPPED, STATUS_STOPPING, append_interrupt_range, append_timeline, get_run, insert_run,
    last_window_index, latest_worker_checkpoint, list_runs, reopen_for_resume, set_current_traffic,
    set_status, set_stop_flag,
};
use crate::db::targets::{self, Product, TargetDetail};
use crate::db::templates::{self, Template};
use crate::db::{
    CheckpointUpdate, DataPaths, DetailWriter, StoreError, WriterConfig, import_pending_dir,
    open_worker_connection,
};
use crate::engine::limits::{self, TrafficShare};
use crate::engine::metrics::{CounterSnapshot, Metrics, unix_now_ms};
use crate::engine::windows::{WindowRecord, write_window};
use crate::engine::{
    EngineConfig, EngineHandle, EngineModel, ExecutePlan, HttpExecutor, LiveTraffic, ProductChoice,
    TenantChoice, Weighted,
};

enum WindowOutcome {
    Completed,
    Interrupted,
}

struct WindowLoop<'a> {
    run_id: &'a str,
    snapshot: &'a RunSnapshot,
    plan: &'a Arc<ExecutePlan>,
    task: &'a RunningTask,
    executor: Arc<dyn crate::engine::RequestExecutor>,
    sink: Arc<dyn crate::engine::SampleSink>,
}

/// Kind of stop requested against a running task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind {
    /// No stop requested; the window may finish on its own.
    None,
    /// `POST /stop` from a user or API client.
    User,
    /// SIGINT / SIGTERM / process shutdown.
    Signal,
}

/// Snapshot of the target and template frozen at start (includes credentials).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSnapshot {
    pub target: TargetDetail,
    pub template: Template,
}

/// In-memory handle for a run the supervisor is currently driving.
pub struct RunningTask {
    /// Cancellation token covering every window of this run.
    pub cancel: CancellationToken,
    /// Live QPS / concurrency knobs shared with the engine.
    pub traffic: Arc<LiveTraffic>,
    /// Isolated metrics for this run.
    pub metrics: Arc<Metrics>,
    current_qps: AtomicU64,
    current_concurrency: AtomicU64,
    window_index: AtomicI64,
    stop_kind: Mutex<StopKind>,
    cluster_status: Mutex<Option<String>>,
    processes: u32,
}

impl RunningTask {
    /// Configured QPS currently applied to this run.
    #[must_use]
    pub fn current_qps(&self) -> u64 {
        self.current_qps.load(Ordering::Relaxed)
    }

    /// Configured concurrency currently applied to this run.
    #[must_use]
    pub fn current_concurrency(&self) -> u64 {
        self.current_concurrency.load(Ordering::Relaxed)
    }

    /// Zero-based index of the window now in progress (or just finished).
    #[must_use]
    pub fn window_index(&self) -> i64 {
        self.window_index.load(Ordering::Relaxed)
    }

    /// Number of worker processes configured for this run.
    #[must_use]
    pub fn processes(&self) -> u32 {
        self.processes
    }

    /// Why this run was asked to stop, if at all.
    #[must_use]
    pub fn stop_kind(&self) -> StopKind {
        *self
            .stop_kind
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Persist the terminal status chosen by a multi-process driver.
    pub fn set_cluster_status(&self, status: &str) {
        *self
            .cluster_status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(status.to_string());
    }

    /// Terminal status recorded by a multi-process driver, if any.
    #[must_use]
    pub fn cluster_status(&self) -> Option<String> {
        self.cluster_status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Owns all in-process runs and the central database lock they share for metadata.
#[derive(Clone)]
pub struct Supervisor {
    paths: DataPaths,
    db: Arc<Mutex<Connection>>,
    max_qps: Option<u64>,
    max_concurrency: Option<u64>,
    tasks: Arc<Mutex<HashMap<String, Arc<RunningTask>>>>,
}

impl Supervisor {
    /// Bind the supervisor to a data directory and the already-open central database.
    #[must_use]
    pub fn new(
        paths: DataPaths,
        db: Arc<Mutex<Connection>>,
        max_qps: Option<u64>,
        max_concurrency: Option<u64>,
    ) -> Self {
        Self {
            paths,
            db,
            max_qps,
            max_concurrency,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Data directory used by this supervisor.
    #[must_use]
    pub fn paths(&self) -> &DataPaths {
        &self.paths
    }

    /// Append a cluster-related timeline event.
    pub fn append_cluster_event(&self, run_id: &str, event: &str, value: u32) {
        let conn = self.lock_db();
        if let Err(err) = append_timeline(
            &conn,
            run_id,
            event,
            Some(serde_json::json!({ "value": value })),
        ) {
            tracing::warn!(run_id, error = %err, "failed to append cluster timeline event");
        }
    }

    fn lock_db(&self) -> MutexGuard<'_, Connection> {
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock_tasks(&self) -> MutexGuard<'_, HashMap<String, Arc<RunningTask>>> {
        self.tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Live task, if this process is currently driving `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Arc<RunningTask>> {
        self.lock_tasks().get(id).cloned()
    }

    /// Configured QPS and concurrency sums of every task this process is driving.
    #[must_use]
    pub fn running_totals(&self) -> TrafficShare {
        limits::sum_traffic(&self.running_shares(None))
    }

    fn running_shares(&self, exclude: Option<&str>) -> Vec<TrafficShare> {
        self.lock_tasks()
            .iter()
            .filter(|(id, _)| exclude.is_none_or(|skip| *id != skip))
            .map(|(_, task)| TrafficShare {
                qps: task.current_qps(),
                concurrency: task.current_concurrency(),
            })
            .collect()
    }

    /// Start a new run from a template and immediately spawn its driver.
    ///
    /// # Errors
    ///
    /// Returns a validation, conflict, or storage error. Already-running tasks
    /// are not a conflict.
    pub fn start_new(
        &self,
        template_id: &str,
        mode: Option<&str>,
    ) -> Result<RunRecord, StoreError> {
        let (record, snapshot) = {
            let conn = self.lock_db();
            let template = templates::get_template(&conn, template_id)?;
            let target = targets::get_target(&conn, &template.target_id)?;
            let mode = resolve_mode(mode, template.loop_enabled)?;
            let qps = opt_u64(template.qps);
            let concurrency = as_u64(template.concurrency).max(1);
            limits::check_caps(
                &self.running_shares(None),
                TrafficShare { qps, concurrency },
                self.max_qps,
                self.max_concurrency,
            )?;
            let snapshot = RunSnapshot { target, template };
            let snapshot_json = serde_json::to_string(&snapshot)?;
            let qps_i = i64::try_from(qps).ok();
            let concurrency_i = i64::try_from(concurrency).unwrap_or(i64::MAX);
            let processes = snapshot.template.processes.max(1);
            let record = insert_run(
                &conn,
                &NewRun {
                    template_id,
                    target_id: &snapshot.target.id,
                    mode,
                    processes,
                    current_qps: qps_i,
                    current_concurrency: concurrency_i,
                    snapshot_json: &snapshot_json,
                },
            )?;
            (record, snapshot)
        };
        self.spawn_for(&record, snapshot, false);
        Ok(record)
    }

    /// Resume a `stopped` or `interrupted` run.
    ///
    /// # Errors
    ///
    /// Returns conflict when the run is `finished` or `failed`,  or a
    /// validation error when the caps would be exceeded.
    pub fn resume(&self, id: &str) -> Result<RunRecord, StoreError> {
        let (record, snapshot, ended_at) = {
            let conn = self.lock_db();
            let record = get_run(&conn, id)?;
            match record.status.as_str() {
                STATUS_FINISHED | STATUS_FAILED => {
                    return Err(StoreError::conflict(format!(
                        "run {id} is {} and cannot be resumed",
                        record.status
                    )));
                }
                STATUS_RUNNING | STATUS_STOPPING => {
                    return Err(StoreError::conflict(format!(
                        "run {id} is already {}",
                        record.status
                    )));
                }
                STATUS_STOPPED | STATUS_INTERRUPTED => {}
                other => {
                    return Err(StoreError::conflict(format!(
                        "run {id} has status {other} and cannot be resumed"
                    )));
                }
            }
            let qps = opt_u64(record.current_qps);
            let concurrency = as_u64(record.current_concurrency.unwrap_or(1)).max(1);
            limits::check_caps(
                &self.running_shares(Some(id)),
                TrafficShare { qps, concurrency },
                self.max_qps,
                self.max_concurrency,
            )?;
            let snapshot: RunSnapshot = serde_json::from_str(&record.snapshot_json)
                .map_err(|err| StoreError::invalid(format!("run snapshot is unreadable: {err}")))?;
            let ended_at = record.ended_at;
            reopen_for_resume(&conn, id)?;
            let record = get_run(&conn, id)?;
            (record, snapshot, ended_at)
        };
        if let Some(start_ms) = ended_at {
            let end_ms = i64::try_from(crate::db::unix_now_ms().unwrap_or(0)).unwrap_or(0);
            let conn = self.lock_db();
            append_interrupt_range(&conn, id, start_ms, end_ms.max(start_ms))?;
        }
        self.spawn_for(&record, snapshot, true);
        let conn = self.lock_db();
        get_run(&conn, id)
    }

    /// Request a graceful stop. The driver records `stopped` or `interrupted`.
    ///
    /// # Errors
    ///
    /// Returns not-found when the id is unknown.
    pub fn request_stop(&self, id: &str, kind: StopKind) -> Result<RunRecord, StoreError> {
        if let Some(task) = self.get(id) {
            *task
                .stop_kind
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = kind;
            task.cancel.cancel();
        }
        let conn = self.lock_db();
        let record = get_run(&conn, id)?;
        if record.status == STATUS_RUNNING {
            set_status(&conn, id, STATUS_STOPPING, None, false)?;
            set_stop_flag(&conn, id, true)?;
            append_timeline(
                &conn,
                id,
                "stop",
                Some(serde_json::json!({ "kind": stop_kind_name(kind) })),
            )?;
        }
        get_run(&conn, id)
    }

    /// Apply a live traffic change if the new values still fit the caps.
    ///
    /// # Errors
    ///
    /// Returns conflict when the run is not `running`, or a validation error
    /// when the caps would be exceeded.
    pub fn patch_traffic(
        &self,
        id: &str,
        qps: Option<u64>,
        concurrency: Option<u64>,
    ) -> Result<RunRecord, StoreError> {
        if qps.is_none() && concurrency.is_none() {
            return Err(StoreError::invalid(
                "traffic patch must include qps and/or concurrency",
            ));
        }
        let task = self
            .get(id)
            .ok_or_else(|| StoreError::conflict(format!("run {id} is not running")))?;
        let conn = self.lock_db();
        let record = get_run(&conn, id)?;
        if record.status != STATUS_RUNNING {
            return Err(StoreError::conflict(
                "traffic can only be changed while the run is running",
            ));
        }
        let old_qps = task.current_qps();
        let old_conc = task.current_concurrency();
        let new_qps = qps.unwrap_or(old_qps);
        let new_conc = concurrency.unwrap_or(old_conc).max(1);
        limits::check_caps(
            &self.running_shares(Some(id)),
            TrafficShare {
                qps: new_qps,
                concurrency: new_conc,
            },
            self.max_qps,
            self.max_concurrency,
        )?;
        task.traffic.qps.store(new_qps, Ordering::Relaxed);
        task.traffic.concurrency.store(new_conc, Ordering::Relaxed);
        task.current_qps.store(new_qps, Ordering::Relaxed);
        task.current_concurrency.store(new_conc, Ordering::Relaxed);
        let qps_i = i64::try_from(new_qps).ok();
        let conc_i = i64::try_from(new_conc).unwrap_or(i64::MAX);
        set_current_traffic(&conn, id, qps_i, conc_i)?;
        append_timeline(
            &conn,
            id,
            "traffic",
            Some(serde_json::json!({
                "old_qps": old_qps,
                "old_concurrency": old_conc,
                "qps": new_qps,
                "concurrency": new_conc,
            })),
        )?;
        get_run(&conn, id)
    }

    /// Cancel every running task and wait for their drivers to finish.
    pub async fn stop_all(&self, kind: StopKind) {
        let ids: Vec<String> = self.lock_tasks().keys().cloned().collect();
        for id in &ids {
            if let Err(err) = self.request_stop(id, kind) {
                warn!(run_id = %id, error = %err, "failed to request stop");
            }
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            if self.lock_tasks().is_empty() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                warn!("timed out waiting for running tasks to stop");
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// List runs from the central database.
    ///
    /// # Errors
    ///
    /// Returns a storage error if the query fails.
    pub fn list(&self, status: Option<&str>) -> Result<Vec<RunRecord>, StoreError> {
        let conn = self.lock_db();
        list_runs(&conn, status)
    }

    /// Load one run.
    ///
    /// # Errors
    ///
    /// Returns not-found when the id is unknown.
    pub fn get_run(&self, id: &str) -> Result<RunRecord, StoreError> {
        let conn = self.lock_db();
        get_run(&conn, id)
    }

    fn spawn_for(&self, record: &RunRecord, snapshot: RunSnapshot, resume: bool) {
        if record.processes > 1 {
            self.spawn_cluster(record.id.clone(), snapshot, record.processes);
        } else {
            self.spawn_driver(record.id.clone(), snapshot, resume);
        }
    }

    fn spawn_cluster(&self, run_id: String, snapshot: RunSnapshot, processes: i64) {
        let processes = u32::try_from(processes.max(1)).unwrap_or(1);
        let timeout_us = u64::try_from(snapshot.template.timeout_secs.max(0))
            .unwrap_or(0)
            .saturating_mul(1_000_000)
            .max(2);
        let metrics = Arc::new(
            Metrics::new(timeout_us)
                .unwrap_or_else(|_| Metrics::new(2).expect("fallback histogram")),
        );
        let cancel = CancellationToken::new();
        let qps = opt_u64(snapshot.template.qps);
        let concurrency = as_u64(snapshot.template.concurrency).max(1);
        let traffic = Arc::new(LiveTraffic {
            qps: AtomicU64::new(qps),
            concurrency: AtomicU64::new(concurrency),
        });
        let task = Arc::new(RunningTask {
            cancel: cancel.clone(),
            traffic,
            metrics,
            current_qps: AtomicU64::new(qps),
            current_concurrency: AtomicU64::new(concurrency),
            window_index: AtomicI64::new(0),
            stop_kind: Mutex::new(StopKind::None),
            cluster_status: Mutex::new(None),
            processes,
        });
        self.lock_tasks().insert(run_id.clone(), Arc::clone(&task));
        let supervisor = self.clone();
        tokio::spawn(async move {
            let result = crate::engine::cluster::drive_cluster(
                supervisor.clone(),
                run_id.clone(),
                snapshot,
                Arc::clone(&task),
            )
            .await;
            let status = task
                .cluster_status()
                .unwrap_or_else(|| STATUS_FINISHED.to_string());
            let error = result.err().map(|err| err.to_string());
            supervisor.finish_run(&run_id, &status, error.as_deref());
        });
    }

    fn spawn_driver(&self, run_id: String, snapshot: RunSnapshot, resume: bool) {
        let supervisor = self.clone();
        tokio::spawn(async move {
            if let Err(err) = supervisor.drive(run_id.clone(), snapshot, resume).await {
                error!(run_id = %run_id, error = %err, "run driver failed");
            }
        });
    }

    async fn drive(
        &self,
        run_id: String,
        snapshot: RunSnapshot,
        resume: bool,
    ) -> Result<(), StoreError> {
        match self.drive_inner(&run_id, &snapshot, resume).await {
            Ok(status) => {
                self.finish_run(&run_id, &status, None);
                Ok(())
            }
            Err(err) => {
                let in_map = self.get(&run_id).is_some();
                let status = if in_map {
                    STATUS_INTERRUPTED
                } else {
                    STATUS_FAILED
                };
                self.finish_run(&run_id, status, Some(&err.to_string()));
                Err(err)
            }
        }
    }

    async fn drive_inner(
        &self,
        run_id: &str,
        snapshot: &RunSnapshot,
        resume: bool,
    ) -> Result<String, StoreError> {
        let executor = HttpExecutor::from_snapshot(&snapshot.target, &snapshot.template)
            .map_err(|err| StoreError::invalid(format!("cannot build HTTP executor: {err}")))?;
        let plan = Arc::new(build_plan(snapshot));
        let timeout_us = secs_to_micros(snapshot.template.timeout_secs);
        let metrics = Arc::new(
            Metrics::new(timeout_us)
                .map_err(|err| StoreError::Other(err.context("create metrics")))?,
        );
        let start_window = if resume {
            restore_resume(&self.paths, run_id, &metrics)?;
            let conn = self.lock_db();
            next_window_index(&conn, run_id)?
        } else {
            0
        };
        let writer = DetailWriter::start(&self.paths, run_id, 0, WriterConfig::default())
            .map_err(|err| StoreError::Other(err.context("start detail writer")))?;
        let sink: Arc<dyn crate::engine::SampleSink> = Arc::new(writer.handle());

        let (qps, concurrency) = {
            let conn = self.lock_db();
            let record = get_run(&conn, run_id)?;
            let qps = record
                .current_qps
                .map_or_else(|| opt_u64(snapshot.template.qps), as_u64);
            let concurrency = record
                .current_concurrency
                .map_or_else(|| as_u64(snapshot.template.concurrency), as_u64)
                .max(1);
            (qps, concurrency)
        };
        let cancel = CancellationToken::new();
        let traffic = Arc::new(LiveTraffic {
            qps: AtomicU64::new(qps),
            concurrency: AtomicU64::new(concurrency),
        });
        let task = Arc::new(RunningTask {
            cancel: cancel.clone(),
            traffic: Arc::clone(&traffic),
            metrics: Arc::clone(&metrics),
            current_qps: AtomicU64::new(qps),
            current_concurrency: AtomicU64::new(concurrency),
            window_index: AtomicI64::new(start_window),
            stop_kind: Mutex::new(StopKind::None),
            cluster_status: Mutex::new(None),
            processes: 1,
        });
        self.lock_tasks()
            .insert(run_id.to_string(), Arc::clone(&task));

        let checkpoint_cancel = cancel.clone();
        let checkpoint_metrics = Arc::clone(&metrics);
        let checkpoint_task = Arc::clone(&task);
        // The writer is moved into the checkpoint loop's close path via a slot.
        let writer = Arc::new(Mutex::new(Some(writer)));
        let writer_for_cp = Arc::clone(&writer);
        let run_id_cp = run_id.to_string();
        let db_cp = Arc::clone(&self.db);
        let cp_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = checkpoint_cancel.cancelled() => break,
                    () = tokio::time::sleep(Duration::from_secs(1)) => {}
                }
                publish_checkpoint(
                    &writer_for_cp,
                    &checkpoint_metrics,
                    checkpoint_task.window_index(),
                    &db_cp,
                    &run_id_cp,
                );
                if checkpoint_cancel.is_cancelled() {
                    break;
                }
            }
        });

        let executor: Arc<dyn crate::engine::RequestExecutor> = Arc::new(executor);
        let terminal = self
            .run_window_loop(
                WindowLoop {
                    run_id,
                    snapshot,
                    plan: &plan,
                    task: &task,
                    executor,
                    sink,
                },
                start_window,
            )
            .await?;

        cancel.cancel();
        let _ = cp_handle.await;
        publish_checkpoint(&writer, &metrics, task.window_index(), &self.db, run_id);
        close_writer(&writer);
        self.lock_tasks().remove(run_id);
        Ok(terminal)
    }

    async fn run_window_loop(
        &self,
        ctx: WindowLoop<'_>,
        start_window: i64,
    ) -> Result<String, StoreError> {
        let mode = {
            let conn = self.lock_db();
            get_run(&conn, ctx.run_id)?.mode == "loop"
        };
        let rounds = if mode {
            ctx.snapshot.template.rounds.map(as_u64)
        } else {
            Some(1)
        };
        let interval = Duration::from_secs(as_u64(ctx.snapshot.template.loop_interval_secs));
        let mut window_index = start_window;
        let mut terminal = STATUS_FINISHED.to_string();
        loop {
            if ctx.task.cancel.is_cancelled() {
                terminal = terminal_status(ctx.task);
                break;
            }
            if let Some(limit) = rounds
                && as_u64(window_index) >= limit
            {
                break;
            }
            ctx.task.window_index.store(window_index, Ordering::Relaxed);
            match self.run_one_window(&ctx, window_index).await? {
                WindowOutcome::Interrupted => {
                    terminal = STATUS_INTERRUPTED.to_string();
                    break;
                }
                WindowOutcome::Completed => {}
            }
            window_index = window_index.saturating_add(1);
            if ctx.task.cancel.is_cancelled() {
                terminal = terminal_status(ctx.task);
                break;
            }
            if let Some(limit) = rounds
                && as_u64(window_index) >= limit
            {
                break;
            }
            if !interval.is_zero() {
                tokio::select! {
                    () = ctx.task.cancel.cancelled() => {
                        terminal = terminal_status(ctx.task);
                        break;
                    }
                    () = tokio::time::sleep(interval) => {}
                }
            }
        }
        Ok(terminal)
    }

    async fn run_one_window(
        &self,
        ctx: &WindowLoop<'_>,
        window_index: i64,
    ) -> Result<WindowOutcome, StoreError> {
        let started_at = unix_now_ms();
        let before = ctx.task.metrics.counters();
        let config = window_config(ctx.snapshot, ctx.plan, ctx.task);
        let handle = EngineHandle::start_with(
            config,
            Arc::clone(&ctx.executor),
            Arc::clone(&ctx.sink),
            Arc::clone(&ctx.task.traffic),
            Arc::clone(&ctx.task.metrics),
            ctx.task.cancel.child_token(),
        );
        if let Err(err) = handle.join().await {
            warn!(run_id = ctx.run_id, error = %err, "engine window join failed");
            return Ok(WindowOutcome::Interrupted);
        }
        let after = ctx.task.metrics.counters();
        let ended_at = unix_now_ms();
        {
            let conn = self.lock_db();
            write_window(
                &conn,
                &WindowRecord {
                    run_id: ctx.run_id,
                    window_index,
                    started_at,
                    ended_at,
                    samples: delta(after.samples, before.samples),
                    success: delta(after.success, before.success),
                    failure: delta(after.failure, before.failure),
                    expected: delta(after.expected, before.expected),
                    dropped: delta(after.dropped_details, before.dropped_details),
                    bytes: delta(after.bytes, before.bytes),
                },
                &ctx.task.metrics,
            )?;
            append_timeline(
                &conn,
                ctx.run_id,
                "window",
                Some(serde_json::json!({ "window_index": window_index })),
            )?;
        }
        ctx.task.metrics.reset_window_histogram();
        Ok(WindowOutcome::Completed)
    }

    fn finish_run(&self, run_id: &str, status: &str, error: Option<&str>) {
        self.lock_tasks().remove(run_id);
        let conn = self.lock_db();
        if let Err(err) = set_status(&conn, run_id, status, error, true) {
            error!(run_id, error = %err, "failed to persist final run status");
        }
        if let Err(err) = append_timeline(
            &conn,
            run_id,
            "end",
            Some(serde_json::json!({ "status": status, "error": error })),
        ) {
            error!(run_id, error = %err, "failed to append end timeline event");
        }
        info!(run_id, status, "run finished");
        if let Ok(record) = get_run(&conn, run_id)
            && let Err(err) = crate::db::janitor::apply_for_run(&self.paths, &record)
        {
            warn!(run_id, error = %err, "detail retention at end of run failed");
        }
    }
}

fn terminal_status(task: &RunningTask) -> String {
    match *task
        .stop_kind
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        StopKind::User => STATUS_STOPPED.to_string(),
        StopKind::Signal | StopKind::None => STATUS_INTERRUPTED.to_string(),
    }
}

fn resolve_mode(mode: Option<&str>, loop_enabled: bool) -> Result<&'static str, StoreError> {
    match mode.map(str::trim) {
        None => Ok(if loop_enabled { "loop" } else { "once" }),
        Some("once") => Ok("once"),
        Some("loop") => Ok("loop"),
        Some(other) => Err(StoreError::invalid(format!(
            "mode must be once or loop, got {other}"
        ))),
    }
}

fn stop_kind_name(kind: StopKind) -> &'static str {
    match kind {
        StopKind::None => "none",
        StopKind::User => "user",
        StopKind::Signal => "signal",
    }
}

fn opt_u64(value: Option<i64>) -> u64 {
    value
        .and_then(|v| u64::try_from(v.max(0)).ok())
        .unwrap_or(0)
}

fn as_u64(value: i64) -> u64 {
    u64::try_from(value.max(0)).unwrap_or(0)
}

fn secs_to_micros(secs: i64) -> u64 {
    as_u64(secs).saturating_mul(1_000_000).max(2)
}

fn delta(after: u64, before: u64) -> i64 {
    i64::try_from(after.saturating_sub(before)).unwrap_or(i64::MAX)
}

fn build_plan(snapshot: &RunSnapshot) -> ExecutePlan {
    let products = select_products(&snapshot.target, &snapshot.template)
        .into_iter()
        .map(|product| {
            let weight = override_weight(&snapshot.template, &product.id, product.weight);
            Weighted {
                weight,
                value: ProductChoice {
                    product_id: product.product_id.clone(),
                    product_name: Some(product.name.clone()),
                },
            }
        })
        .collect();
    let tenants = snapshot
        .target
        .tenants
        .iter()
        .map(|tenant| Weighted {
            weight: as_u64(tenant.weight).max(1),
            value: TenantChoice {
                name: tenant.name.clone(),
            },
        })
        .collect();
    ExecutePlan {
        products,
        tenants,
        expected_reasons: snapshot
            .template
            .expected_reasons
            .clone()
            .unwrap_or_default(),
    }
}

fn select_products<'a>(target: &'a TargetDetail, template: &Template) -> Vec<&'a Product> {
    match template.product_ids.as_ref() {
        Some(ids) if !ids.is_empty() => target
            .products
            .iter()
            .filter(|product| ids.iter().any(|id| id == &product.id))
            .collect(),
        _ => target.products.iter().collect(),
    }
}

fn override_weight(template: &Template, product_row_id: &str, default: i64) -> u64 {
    let overridden = template
        .weight_overrides
        .as_ref()
        .and_then(|map| map.get(product_row_id).copied())
        .unwrap_or(default);
    as_u64(overridden).max(1)
}

fn window_config(
    snapshot: &RunSnapshot,
    plan: &Arc<ExecutePlan>,
    task: &RunningTask,
) -> EngineConfig {
    let template = &snapshot.template;
    let model = if template.model == "closed_loop" {
        EngineModel::ClosedLoop
    } else {
        EngineModel::OpenLoop
    };
    EngineConfig {
        model,
        qps: Some(task.current_qps()),
        concurrency: task.current_concurrency(),
        requests: template.requests.map(as_u64),
        duration: template
            .duration_secs
            .filter(|secs| *secs > 0)
            .map(|secs| Duration::from_secs(as_u64(secs))),
        warmup: Duration::from_secs(as_u64(template.warmup_secs)),
        ramp: Duration::from_secs(as_u64(template.ramp_secs)),
        timeout: Duration::from_secs(as_u64(template.timeout_secs).max(1)),
        plan: Arc::clone(plan),
    }
}

fn restore_resume(paths: &DataPaths, run_id: &str, metrics: &Metrics) -> Result<i64, StoreError> {
    let conn = open_worker_connection(paths, run_id, 0).map_err(StoreError::Other)?;
    let imported = import_pending_dir(&conn, paths, run_id, 0).map_err(StoreError::Other)?;
    if imported > 0 {
        info!(run_id, imported, "imported pending dump samples");
    }
    if let Some(checkpoint) = latest_worker_checkpoint(&conn)? {
        if let Ok(mut snapshot) = serde_json::from_str::<CounterSnapshot>(&checkpoint.counters_json)
        {
            snapshot.in_flight = 0;
            metrics.restore_counters(snapshot);
        }
        if let Some(blob) = checkpoint.histogram_blob.as_deref()
            && !blob.is_empty()
        {
            metrics
                .restore_histogram(blob)
                .map_err(|err| StoreError::Other(err.context("restore histogram")))?;
        }
    }
    drop(conn);
    // Next window is the first one that has not yet been fully written.
    // The central database is not opened here; the driver starts at 0 and the
    // caller overwrites this with last_window_index + 1 when available.
    Ok(0)
}

fn publish_checkpoint(
    writer: &Mutex<Option<DetailWriter>>,
    metrics: &Metrics,
    window_index: i64,
    db: &Mutex<Connection>,
    run_id: &str,
) {
    let blob = metrics.histogram_blob().ok();
    let counters = metrics.counters();
    let counters_json = serde_json::to_string(&counters).unwrap_or_else(|_| "{}".into());
    if let Some(writer) = writer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        writer.publish_checkpoint(CheckpointUpdate {
            window_index: Some(window_index),
            counters_json: counters_json.clone(),
            histogram_blob: blob.clone(),
        });
    }
    let conn = db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let ts = unix_now_ms();
    let _ = conn.execute(
        "INSERT INTO run_checkpoints (run_id, worker_index, ts, window_index, counters_json, histogram_blob)
         VALUES (?1, 0, ?2, ?3, ?4, ?5)
         ON CONFLICT(run_id, worker_index, ts) DO UPDATE SET
            window_index = excluded.window_index,
            counters_json = excluded.counters_json,
            histogram_blob = excluded.histogram_blob",
        rusqlite::params![run_id, ts, window_index, counters_json, blob],
    );
}

fn close_writer(writer: &Mutex<Option<DetailWriter>>) {
    if let Some(writer) = writer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        && let Err(err) = writer.close()
    {
        warn!(error = %err, "detail writer close reported an error");
    }
}

/// Look up the next window index from the central database (0 if none).
///
/// # Errors
///
/// Returns a storage error if the query fails.
pub fn next_window_index(conn: &Connection, run_id: &str) -> Result<i64, StoreError> {
    Ok(last_window_index(conn, run_id)?.map_or(0, |index| index.saturating_add(1)))
}
