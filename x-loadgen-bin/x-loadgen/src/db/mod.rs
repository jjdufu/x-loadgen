//! Data directory, SQLite databases, Hoppscotch workspace stores, and the bounded detail writer.

pub mod collections;
mod dump;
pub mod environments;
pub mod error;
pub mod janitor;
mod paths;
pub mod profiles;
pub mod runs;
mod schema;
pub mod workspace;
mod writer;

#[cfg(test)]
pub(crate) use dump::insert_samples;
pub use dump::{
    MAGIC, VERSION, import_pending_dir, import_pending_file, list_pending, write_pending,
};
pub use error::StoreError;
pub use paths::DataPaths;
pub use schema::{USER_VERSION, WORKER_USER_VERSION};
pub use writer::{
    CheckpointUpdate, DEFAULT_BATCH_SIZE, DEFAULT_CAPACITY, DEFAULT_CHECKPOINT_INTERVAL,
    DEFAULT_FLUSH_INTERVAL, DetailWriter, Sample, WriterConfig, WriterHandle,
};

use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};

use crate::db::schema::{CENTRAL_SCHEMA_V2, DROP_LEGACY_V1_CATALOG, WORKER_SCHEMA_V1};

/// Open (or create) the central database, apply PRAGMAs, and migrate to v2.
///
/// # Errors
///
/// Returns an error if the data directory cannot be created, the file cannot
/// be opened, a PRAGMA cannot be applied, or the schema is newer than this
/// binary supports.
pub fn open_central(paths: &DataPaths) -> Result<Connection> {
    paths.ensure().context("create data directory")?;
    let path = paths.central_db();
    let conn = Connection::open(&path)
        .with_context(|| format!("open central database {}", path.display()))?;
    apply_pragmas(&conn)?;
    migrate_central(&conn)?;
    drop_legacy_sessions(&conn)?;
    let _ = environments::ensure_global_environment(&conn)
        .map_err(|err| anyhow::anyhow!("ensure global environment: {err}"))?;
    Ok(conn)
}

/// Open (or create) a worker database, apply PRAGMAs, migrate, and record
/// `worker_meta` when the table is empty.
///
/// # Errors
///
/// Returns an error if the run directory cannot be created, the file cannot
/// be opened, or migration fails.
pub fn open_worker_connection(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
) -> Result<Connection> {
    paths.ensure_run(run_id).context("create run directory")?;
    let path = paths.worker_db(run_id, worker_index);
    let conn = Connection::open(&path)
        .with_context(|| format!("open worker database {}", path.display()))?;
    apply_pragmas(&conn)?;
    migrate_worker(&conn)?;
    ensure_worker_meta(&conn, run_id, worker_index)?;
    Ok(conn)
}

/// Open an existing worker database without creating a new file or writing
/// `worker_meta`. Used by report queries (read-only) and retention cleanup.
///
/// # Errors
///
/// Returns an error if an existing file cannot be opened.
pub fn open_existing_worker(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
    read_only: bool,
) -> Result<Option<Connection>> {
    let path = paths.worker_db(run_id, worker_index);
    if !path.exists() {
        return Ok(None);
    }
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let conn = Connection::open_with_flags(&path, flags)
        .with_context(|| format!("open worker database {}", path.display()))?;
    if read_only {
        conn.busy_timeout(std::time::Duration::from_millis(5000))
            .context("busy_timeout")?;
    } else {
        apply_pragmas(&conn)?;
    }
    Ok(Some(conn))
}

/// WAL, `synchronous=NORMAL`, foreign keys, and a 5 second busy timeout.
///
/// # Errors
///
/// Returns an error if SQLite rejects a PRAGMA.
pub fn apply_pragmas(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")
        .context("PRAGMA journal_mode=WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .context("PRAGMA synchronous=NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .context("PRAGMA foreign_keys=ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)
        .context("PRAGMA busy_timeout=5000")?;
    Ok(())
}

/// Current journal mode of an open connection (`wal`, `delete`, …).
///
/// # Errors
///
/// Returns an error if the PRAGMA cannot be read.
pub fn journal_mode(conn: &Connection) -> Result<String> {
    conn.pragma_query_value(None, "journal_mode", |row| row.get(0))
        .context("read PRAGMA journal_mode")
}

/// Current `synchronous` setting as an integer (1 is `NORMAL`).
///
/// # Errors
///
/// Returns an error if the PRAGMA cannot be read.
pub fn synchronous(conn: &Connection) -> Result<i64> {
    conn.pragma_query_value(None, "synchronous", |row| row.get(0))
        .context("read PRAGMA synchronous")
}

/// Unix time in milliseconds.
///
/// # Errors
///
/// Returns an error if the system clock is before the Unix epoch.
pub fn unix_now_ms() -> Result<u64, SystemTimeError> {
    Ok(
        u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())
            .unwrap_or(u64::MAX),
    )
}

fn drop_legacy_sessions(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS sessions")
        .context("drop leftover sessions table")?;
    Ok(())
}

fn migrate_central(conn: &Connection) -> Result<()> {
    let version = user_version(conn)?;
    if version == 0 {
        conn.execute_batch(CENTRAL_SCHEMA_V2)
            .context("apply central schema v2")?;
        conn.pragma_update(None, "user_version", USER_VERSION)
            .context("set central user_version")?;
        return Ok(());
    }
    if version == 1 {
        conn.execute_batch(DROP_LEGACY_V1_CATALOG)
            .context("drop legacy v1 catalog tables")?;
        conn.execute_batch(CENTRAL_SCHEMA_V2)
            .context("apply central schema v2 upgrade")?;
        conn.pragma_update(None, "user_version", USER_VERSION)
            .context("set central user_version to 2")?;
        return Ok(());
    }
    if version == USER_VERSION {
        return Ok(());
    }
    if version > USER_VERSION {
        bail!("central database user_version {version} is newer than {USER_VERSION}");
    }
    bail!("central database user_version {version} has no migration path")
}

fn migrate_worker(conn: &Connection) -> Result<()> {
    let version = user_version(conn)?;
    if version == 0 {
        conn.execute_batch(WORKER_SCHEMA_V1)
            .context("apply worker schema v1")?;
        conn.pragma_update(None, "user_version", WORKER_USER_VERSION)
            .context("set worker user_version")?;
        return Ok(());
    }
    if version == WORKER_USER_VERSION {
        return Ok(());
    }
    if version > WORKER_USER_VERSION {
        bail!("worker database user_version {version} is newer than {WORKER_USER_VERSION}");
    }
    bail!("worker database user_version {version} has no migration path")
}

fn user_version(conn: &Connection) -> Result<i32> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .context("read PRAGMA user_version")
}

fn ensure_worker_meta(conn: &Connection, run_id: &str, worker_index: u32) -> Result<()> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM worker_meta", [], |row| row.get(0))
        .context("count worker_meta")?;
    if count > 0 {
        return Ok(());
    }
    let started = i64::try_from(unix_now_ms()?).unwrap_or(i64::MAX);
    conn.execute(
        "INSERT INTO worker_meta (run_id, worker_index, started_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![run_id, worker_index, started],
    )
    .context("insert worker_meta")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use rusqlite::Connection;

    use super::dump::list_pending;
    use super::{
        DataPaths, DetailWriter, Sample, USER_VERSION, WriterConfig, import_pending_dir,
        journal_mode, open_central, open_worker_connection, synchronous, write_pending,
    };

    fn sample_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM samples", [], |row| row.get(0))
            .expect("count samples")
    }

    fn checkpoint_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM worker_checkpoints", [], |row| {
            row.get(0)
        })
        .expect("count checkpoints")
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "x-loadgen-db-{}-{}-{n}-{tag}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis())
            ));
            fs::create_dir_all(&path).expect("temp data dir");
            Self { path }
        }

        fn paths(&self) -> DataPaths {
            DataPaths::new(&self.path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn sample(seq: i64, product: &str) -> Sample {
        Sample {
            seq: Some(seq),
            start_unix_ms: 1_700_000_000_000 + seq,
            end_unix_ms: 1_700_000_000_100 + seq,
            latency_us: 100 + seq,
            product_id: product.to_string(),
            product_name: Some(format!("p-{product}")),
            request_index: Some(seq),
            http_status: Some(200),
            success: true,
            expected: true,
            reason: None,
            error_class: None,
            request_id: Some(format!("rid-{seq}")),
            error: None,
            response_bytes: Some(16),
            customer_name: None,
        }
    }

    fn fast_config() -> WriterConfig {
        WriterConfig {
            capacity: 128,
            batch_size: 50,
            flush_interval: Duration::from_millis(20),
            checkpoint_interval: Duration::from_secs(1),
            fail_commits: false,
        }
    }

    #[test]
    fn data_dir_is_created_with_mode_700_and_central_is_wal_normal() {
        let tmp = TempDir::new("central");
        let paths = tmp.paths();
        let conn = open_central(&paths).expect("open central");
        let mode = fs::metadata(paths.root())
            .expect("stat data dir")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "data dir mode {mode:#o}");
        assert_eq!(
            journal_mode(&conn).expect("journal_mode").to_lowercase(),
            "wal"
        );
        assert_eq!(synchronous(&conn).expect("synchronous"), 1);
        assert!(paths.central_db().is_file());
        assert!(paths.logs_dir().is_dir());
        assert!(paths.runs_dir().is_dir());
    }

    #[test]
    fn writer_commits_all_samples_and_a_checkpoint() {
        const N: i64 = 40;
        let tmp = TempDir::new("commit");
        let paths = tmp.paths();
        let writer = DetailWriter::start(&paths, "run-a", 0, fast_config()).expect("start");
        for i in 1..=N {
            assert!(writer.try_send(sample(i, "alpha")), "try_send {i}");
        }
        writer.close().expect("close");
        let conn = open_worker_connection(&paths, "run-a", 0).expect("reopen");
        assert_eq!(sample_count(&conn), N);
        assert!(checkpoint_count(&conn) >= 1);
    }

    #[test]
    fn close_returns_while_a_handle_is_still_held() {
        let tmp = TempDir::new("held-handle");
        let paths = tmp.paths();
        let mut config = fast_config();
        config.flush_interval = Duration::from_secs(30);
        config.checkpoint_interval = Duration::from_secs(30);
        let writer = DetailWriter::start(&paths, "run-held", 0, config).expect("start");
        let handle = writer.handle();
        assert!(handle.try_send(sample(1, "held")), "send before close");
        let start = Instant::now();
        writer.close().expect("close with live handle");
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "close wedged while a handle was held ({elapsed:?})"
        );
        assert!(!handle.try_send(sample(2, "held")), "send after close");
        let conn = open_worker_connection(&paths, "run-held", 0).expect("reopen");
        assert_eq!(sample_count(&conn), 1);
    }

    #[test]
    fn try_send_does_not_block_when_the_queue_is_full() {
        const TOTAL: u64 = 200;
        let tmp = TempDir::new("drop");
        let paths = tmp.paths();
        let mut config = fast_config();
        config.capacity = 8;
        config.flush_interval = Duration::from_secs(30);
        config.batch_size = 10_000;
        let writer = DetailWriter::start(&paths, "run-b", 0, config).expect("start");
        let start = Instant::now();
        let mut accepted = 0_u64;
        for i in 0..TOTAL {
            if writer.try_send(sample(i64::try_from(i).expect("seq"), "beta")) {
                accepted += 1;
            }
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(200),
            "try_send blocked ({elapsed:?})"
        );
        let dropped = writer.dropped_details();
        assert_eq!(accepted + dropped, TOTAL);
        assert!(dropped > 0, "expected overflow drops, accepted={accepted}");
        writer.close().expect("close");
    }

    #[test]
    fn commit_failure_dumps_pending_and_import_restores_then_deletes() {
        const N: i64 = 12;
        let tmp = TempDir::new("dump");
        let paths = tmp.paths();
        let mut config = fast_config();
        config.fail_commits = true;
        let writer = DetailWriter::start(&paths, "run-c", 1, config).expect("start");
        for i in 1..=N {
            assert!(writer.try_send(sample(i, "gamma")));
        }
        let close_err = writer.close();
        assert!(close_err.is_err(), "close should report the forced failure");
        let pending = list_pending(&paths, "run-c", 1).expect("list");
        assert_eq!(
            pending.len(),
            1,
            "expected one pending dump, got {pending:?}"
        );
        let conn = open_worker_connection(&paths, "run-c", 1).expect("reopen");
        assert_eq!(sample_count(&conn), 0);
        let imported = import_pending_dir(&conn, &paths, "run-c", 1).expect("import");
        assert_eq!(imported, usize::try_from(N).expect("n"));
        assert_eq!(sample_count(&conn), N);
        let leftover = list_pending(&paths, "run-c", 1).expect("list after");
        assert!(
            leftover.is_empty(),
            "dump file should be deleted: {leftover:?}"
        );
    }

    #[test]
    fn two_writers_on_two_worker_dbs_do_not_interfere() {
        let tmp = TempDir::new("parallel");
        let paths = tmp.paths();
        let left_paths = paths.clone();
        let right_paths = paths.clone();
        let left = thread::spawn(move || {
            let writer =
                DetailWriter::start(&left_paths, "run-d", 0, fast_config()).expect("start 0");
            for i in 1..=25 {
                assert!(writer.try_send(sample(i, "left")));
            }
            writer.close().expect("close 0");
        });
        let right = thread::spawn(move || {
            let writer =
                DetailWriter::start(&right_paths, "run-d", 1, fast_config()).expect("start 1");
            for i in 100..130 {
                assert!(writer.try_send(sample(i, "right")));
            }
            writer.close().expect("close 1");
        });
        left.join().expect("left thread");
        right.join().expect("right thread");

        let left_conn = open_worker_connection(&paths, "run-d", 0).expect("open 0");
        let right_conn = open_worker_connection(&paths, "run-d", 1).expect("open 1");
        assert_eq!(sample_count(&left_conn), 25);
        assert_eq!(sample_count(&right_conn), 30);
        let left_products: i64 = left_conn
            .query_row(
                "SELECT COUNT(*) FROM samples WHERE product_id = 'left'",
                [],
                |row| row.get(0),
            )
            .expect("left products");
        let right_products: i64 = right_conn
            .query_row(
                "SELECT COUNT(*) FROM samples WHERE product_id = 'right'",
                [],
                |row| row.get(0),
            )
            .expect("right products");
        assert_eq!(left_products, 25);
        assert_eq!(right_products, 30);
        assert!(checkpoint_count(&left_conn) >= 1);
        assert!(checkpoint_count(&right_conn) >= 1);
    }

    #[test]
    fn write_and_parse_pending_roundtrip() {
        let tmp = TempDir::new("roundtrip");
        let paths = tmp.paths();
        paths.ensure_run("run-e").expect("run dir");
        let samples = vec![sample(7, "delta"), sample(8, "delta")];
        let path = write_pending(&paths, "run-e", 3, &samples).expect("write");
        assert!(
            path.file_name()
                .expect("name")
                .to_string_lossy()
                .starts_with("pending-w03-")
        );
        let conn = open_worker_connection(&paths, "run-e", 3).expect("open");
        let n = import_pending_dir(&conn, &paths, "run-e", 3).expect("import");
        assert_eq!(n, 2);
        assert_eq!(sample_count(&conn), 2);
        assert!(!path.exists(), "imported dump should be removed");
    }

    #[test]
    fn v1_database_upgrades_to_v2_keeping_auth_and_dropping_legacy_tables() {
        let tmp = TempDir::new("legacy-v1-upgrade");
        let paths = tmp.paths();
        paths.ensure().expect("data dir");
        {
            let conn = Connection::open(paths.central_db()).expect("open raw");
            conn.execute_batch(
                "CREATE TABLE sessions (
                    id TEXT PRIMARY KEY,
                    token_hash TEXT NOT NULL UNIQUE,
                    created_at INTEGER NOT NULL,
                    last_seen_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL
                );
                CREATE TABLE targets (id TEXT PRIMARY KEY, name TEXT NOT NULL);
                CREATE TABLE auth_state (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    password_hash TEXT,
                    must_change INTEGER NOT NULL DEFAULT 1,
                    updated_at INTEGER NOT NULL
                );
                INSERT INTO auth_state (id, password_hash, must_change, updated_at)
                    VALUES (1, 'kept', 0, 1);
                INSERT INTO sessions (id, token_hash, created_at, last_seen_at, expires_at)
                    VALUES ('s', 'h', 1, 1, 1);",
            )
            .expect("seed v1 schema");
            conn.pragma_update(None, "user_version", 1)
                .expect("set user_version");
        }
        let conn = open_central(&paths).expect("open_central");
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, USER_VERSION);
        let legacy: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('sessions', 'targets')",
                [],
                |row| row.get(0),
            )
            .expect("count legacy tables");
        assert_eq!(legacy, 0, "legacy v1 tables must be dropped");
        let hash: String = conn
            .query_row(
                "SELECT password_hash FROM auth_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("auth_state kept");
        assert_eq!(hash, "kept");
    }
}
