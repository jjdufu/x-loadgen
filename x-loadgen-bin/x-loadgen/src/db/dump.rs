//! Binary dump of samples that could not be committed (`pending-*.bin`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};

use super::paths::DataPaths;
use super::writer::Sample;

/// File magic at the start of a pending dump.
pub const MAGIC: &[u8; 4] = b"LGPD";

/// Dump format version written after the magic.
pub const VERSION: u16 = 1;

/// Write `samples` to `pending-wNN-<unix_ms>.bin` under the run directory.
///
/// Layout: magic `LGPD`, little-endian `u16` version, little-endian `u32`
/// worker index, then a JSON array of [`Sample`].
///
/// # Errors
///
/// Returns an error if the run directory cannot be created or the file cannot
/// be written.
pub fn write_pending(
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
    samples: &[Sample],
) -> Result<PathBuf> {
    paths.ensure_run(run_id)?;
    let path = unique_pending_path(paths, run_id, worker_index)?;
    let body = serde_json::to_vec(samples).context("serialize pending samples")?;
    let mut bytes = Vec::with_capacity(8 + body.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&worker_index.to_le_bytes());
    bytes.extend_from_slice(&body);
    let mut file = fs::File::create(&path)
        .with_context(|| format!("create pending dump {}", path.display()))?;
    file.write_all(&bytes)
        .with_context(|| format!("write pending dump {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("sync pending dump {}", path.display()))?;
    Ok(path)
}

/// Load a dump file into `conn` and delete the file on success.
///
/// # Errors
///
/// Returns an error if the file is malformed, has the wrong worker index, or
/// a row cannot be inserted. The file is left in place on failure.
pub fn import_pending_file(conn: &Connection, path: &Path, worker_index: u32) -> Result<usize> {
    let bytes = fs::read(path).with_context(|| format!("read pending dump {}", path.display()))?;
    let samples = parse_pending(&bytes, worker_index)
        .with_context(|| format!("parse pending dump {}", path.display()))?;
    insert_samples(conn, &samples)?;
    fs::remove_file(path).with_context(|| format!("delete pending dump {}", path.display()))?;
    Ok(samples.len())
}

/// Import every `pending-wNN-*.bin` for `worker_index` under the run directory,
/// in file-name order. Successfully imported files are deleted.
///
/// # Errors
///
/// Returns on the first file that cannot be imported. Earlier files in the
/// same call that already imported remain deleted.
pub fn import_pending_dir(
    conn: &Connection,
    paths: &DataPaths,
    run_id: &str,
    worker_index: u32,
) -> Result<usize> {
    let mut files = list_pending(paths, run_id, worker_index)?;
    files.sort();
    let mut total = 0;
    for path in files {
        total += import_pending_file(conn, &path, worker_index)?;
    }
    Ok(total)
}

/// List pending dump files for one worker, unsorted.
///
/// # Errors
///
/// Returns an error if the run directory cannot be read. A missing directory
/// yields an empty list.
pub fn list_pending(paths: &DataPaths, run_id: &str, worker_index: u32) -> Result<Vec<PathBuf>> {
    let dir = paths.run_dir(run_id);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| format!("read run directory {}", dir.display()));
        }
    };
    let prefix = format!("pending-w{worker_index:02}-");
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("read entry in {}", dir.display()))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(&prefix)
            && Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("bin"))
        {
            files.push(entry.path());
        }
    }
    Ok(files)
}

/// Parse a pending dump and check the worker index.
///
/// # Errors
///
/// Returns an error if the header is short, the magic or version is wrong, the
/// worker index does not match, or the JSON body is invalid.
pub fn parse_pending(bytes: &[u8], worker_index: u32) -> Result<Vec<Sample>> {
    const HEADER: usize = 4 + 2 + 4;
    ensure!(
        bytes.len() >= HEADER,
        "pending dump is shorter than the header"
    );
    ensure!(&bytes[..4] == MAGIC, "pending dump magic is not LGPD");
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    ensure!(
        version == VERSION,
        "unsupported pending dump version {version}"
    );
    let file_worker = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
    ensure!(
        file_worker == worker_index,
        "pending dump worker {file_worker} does not match {worker_index}"
    );
    serde_json::from_slice(&bytes[HEADER..]).context("decode pending sample array")
}

pub(crate) fn insert_samples(conn: &Connection, samples: &[Sample]) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "INSERT INTO samples (
                seq, start_unix_ms, end_unix_ms, latency_us, product_id, product_name,
                request_index, http_status, success, expected, reason, error_class,
                request_id, error, response_bytes, customer_name
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        )
        .context("prepare sample insert")?;
    for sample in samples {
        stmt.execute(params![
            sample.seq,
            sample.start_unix_ms,
            sample.end_unix_ms,
            sample.latency_us,
            sample.product_id,
            sample.product_name,
            sample.request_index,
            sample.http_status,
            i64::from(sample.success),
            i64::from(sample.expected),
            sample.reason,
            sample.error_class,
            sample.request_id,
            sample.error,
            sample.response_bytes,
            sample.customer_name,
        ])
        .context("insert sample")?;
    }
    Ok(())
}

fn unique_pending_path(paths: &DataPaths, run_id: &str, worker_index: u32) -> Result<PathBuf> {
    let mut unix_ms = super::unix_now_ms()?;
    loop {
        let path = paths.pending_dump(run_id, worker_index, unix_ms);
        if !path.exists() {
            return Ok(path);
        }
        unix_ms = unix_ms
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("pending dump timestamp overflow"))?;
    }
}
