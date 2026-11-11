//! Paths under the process data directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Well-known locations under a data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    root: PathBuf,
}

impl DataPaths {
    /// Interpret `root` as the data directory (not created yet).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Data directory root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<data-dir>/admin.passwd`.
    #[must_use]
    pub fn admin_passwd(&self) -> PathBuf {
        self.root.join("admin.passwd")
    }

    /// `<data-dir>/loadgen.sqlite`.
    #[must_use]
    pub fn central_db(&self) -> PathBuf {
        self.root.join("loadgen.sqlite")
    }

    /// `<data-dir>/logs`.
    #[must_use]
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// `<data-dir>/runs`.
    #[must_use]
    pub fn runs_dir(&self) -> PathBuf {
        self.root.join("runs")
    }

    /// `<data-dir>/runs/<run_id>`.
    #[must_use]
    pub fn run_dir(&self, run_id: &str) -> PathBuf {
        self.runs_dir().join(run_id)
    }

    /// Worker database `<data-dir>/runs/<run_id>/wNN.sqlite`.
    #[must_use]
    pub fn worker_db(&self, run_id: &str, worker_index: u32) -> PathBuf {
        self.run_dir(run_id)
            .join(format!("w{worker_index:02}.sqlite"))
    }

    /// Live snapshot `<data-dir>/runs/<run_id>/wNN-live.json`.
    #[must_use]
    pub fn worker_live(&self, run_id: &str, worker_index: u32) -> PathBuf {
        self.run_dir(run_id)
            .join(format!("w{worker_index:02}-live.json"))
    }

    /// Pending dump path `pending-wNN-<unix_ms>.bin` under the run directory.
    #[must_use]
    pub fn pending_dump(&self, run_id: &str, worker_index: u32, unix_ms: u64) -> PathBuf {
        self.run_dir(run_id)
            .join(format!("pending-w{worker_index:02}-{unix_ms}.bin"))
    }

    /// Worker indexes that already have a `wNN.sqlite` file for this run.
    #[must_use]
    pub fn worker_indexes(&self, run_id: &str) -> Vec<u32> {
        let mut indexes = Vec::new();
        let Ok(entries) = fs::read_dir(self.run_dir(run_id)) else {
            return indexes;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(rest) = name
                .strip_prefix('w')
                .and_then(|s| s.strip_suffix(".sqlite"))
            else {
                continue;
            };
            if let Ok(index) = rest.parse::<u32>() {
                indexes.push(index);
            }
        }
        indexes.sort_unstable();
        indexes
    }

    /// Create the data directory (mode 700 when the OS allows) and standard subdirectories.
    ///
    /// # Errors
    ///
    /// Returns an error if a directory cannot be created or permissions cannot be applied.
    pub fn ensure(&self) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?;
        }
        fs::create_dir_all(self.logs_dir())?;
        fs::create_dir_all(self.runs_dir())?;
        Ok(())
    }

    /// Ensure the per-run directory exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn ensure_run(&self, run_id: &str) -> io::Result<PathBuf> {
        self.ensure()?;
        let dir = self.run_dir(run_id);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}
