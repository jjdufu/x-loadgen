//! Command-line interface: only basic commands are public.

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

/// Public command-line interface for `x-loadgen`.
#[derive(Debug, Parser)]
#[command(
    name = "x-loadgen",
    about = "Gateway load generator (Web UI and HTTP API; CLI provides basic commands only)",
    version = env!("CARGO_PKG_VERSION")
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Public and hidden subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the HTTP server (Web UI and API).
    Serve {
        /// Listen address. Bridge port mapping requires `0.0.0.0`, not loopback.
        #[arg(long, default_value = "0.0.0.0:8757", env = "X_LOADGEN_LISTEN")]
        listen: String,
        /// Data directory (SQLite, logs, dumps, `admin.passwd`).
        #[arg(long, default_value = "./data", env = "X_LOADGEN_DATA_DIR")]
        data_dir: PathBuf,
        /// Disable login. Prefer the flag; `X_LOADGEN_NO_AUTH=1|true` also enables this.
        #[arg(long)]
        no_auth: bool,
        /// Global QPS cap across all running tasks. Unset means no cap.
        #[arg(long, env = "X_LOADGEN_MAX_QPS")]
        max_qps: Option<u64>,
        /// Global concurrency cap across all running tasks. Unset means no cap.
        #[arg(long, env = "X_LOADGEN_MAX_CONCURRENCY")]
        max_concurrency: Option<u64>,
    },
    /// Regenerate the initial admin password file.
    ResetPasswd {
        /// Data directory that holds `admin.passwd` and the central database.
        #[arg(long, default_value = "./data", env = "X_LOADGEN_DATA_DIR")]
        data_dir: PathBuf,
    },
    /// Print the crate version and exit.
    Version,
    /// Probe a running `serve` process via `GET /health`.
    Health {
        /// Health URL of the local server.
        #[arg(long, default_value = "http://127.0.0.1:8757/health")]
        url: String,
    },
    /// Internal worker process started by `serve` (hidden from `--help`).
    #[command(hide = true)]
    Worker {
        #[arg(long)]
        data_dir: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        worker_index: u32,
        #[arg(long)]
        worker_count: u32,
    },
}

/// Parsed `serve` arguments.
#[derive(Debug, Clone)]
pub struct ServeArgs {
    /// Bind address, for example `0.0.0.0:8757`.
    pub listen: String,
    /// Data directory path.
    pub data_dir: PathBuf,
    /// When true, login is disabled.
    pub no_auth: bool,
    /// Optional global QPS cap.
    pub max_qps: Option<u64>,
    /// Optional global concurrency cap.
    pub max_concurrency: Option<u64>,
}

/// Parse process arguments.
#[must_use]
pub fn parse() -> Cli {
    Cli::parse()
}

/// Execute the selected command.
///
/// # Errors
///
/// Returns an error when a command cannot be completed (for example `health`
/// cannot reach the server, or `serve` / `reset-passwd` cannot use the data
/// directory).
pub fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Serve {
            listen,
            data_dir,
            no_auth,
            max_qps,
            max_concurrency,
        } => {
            let args = ServeArgs {
                listen,
                data_dir,
                no_auth: no_auth || env_flag("X_LOADGEN_NO_AUTH"),
                max_qps,
                max_concurrency,
            };
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("start tokio runtime")?;
            runtime.block_on(crate::api::server::run(args))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ResetPasswd { data_dir } => {
            crate::auth::reset_passwd(&crate::db::DataPaths::new(data_dir))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Version => {
            print_version();
            Ok(ExitCode::SUCCESS)
        }
        Command::Health { url } => health(&url),
        Command::Worker { .. } => {
            bail!("internal worker is started by serve and is not implemented yet")
        }
    }
}

/// Print the package version (no `v` prefix).
pub fn print_version() {
    println!("{}", env!("CARGO_PKG_VERSION"));
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| flag_value(&value))
}

fn flag_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn health(url: &str) -> Result<ExitCode> {
    let response = reqwest::blocking::Client::new()
        .get(url)
        .send()
        .with_context(|| format!("request {url}"))?;
    if response.status().is_success() {
        Ok(ExitCode::SUCCESS)
    } else {
        bail!("health check failed: HTTP {}", response.status());
    }
}

/// Help text used by unit tests (public commands only).
#[must_use]
pub fn help_text() -> String {
    format!("{}", Cli::command().render_help())
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command, help_text};
    use clap::{CommandFactory, Parser};

    #[test]
    fn help_lists_only_public_commands() {
        let help = help_text();
        assert!(help.contains("serve"), "{help}");
        assert!(help.contains("reset-passwd"), "{help}");
        assert!(help.contains("version"), "{help}");
        assert!(help.contains("health"), "{help}");
        assert!(!help.contains("worker"), "{help}");
        assert!(!help.contains("--config"), "{help}");
        assert!(!help.contains("--report-dir"), "{help}");
    }

    #[test]
    fn rejects_legacy_config_flag() {
        let err = Cli::try_parse_from(["x-loadgen", "--config", "x.toml"]).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("unexpected argument") || text.contains("unrecognized"),
            "{text}"
        );
    }

    #[test]
    fn parses_serve_defaults() {
        let cli = Cli::try_parse_from(["x-loadgen", "serve"]).expect("parse serve");
        match cli.command {
            Command::Serve {
                listen,
                data_dir,
                no_auth,
                max_qps,
                max_concurrency,
            } => {
                assert_eq!(listen, "0.0.0.0:8757");
                assert_eq!(data_dir, std::path::PathBuf::from("./data"));
                assert!(!no_auth);
                assert!(max_qps.is_none());
                assert!(max_concurrency.is_none());
            }
            other => panic!("expected serve, got {other:?}"),
        }
    }

    #[test]
    fn public_command_names() {
        let command = Cli::command();
        let names: Vec<_> = command
            .get_subcommands()
            .filter(|c| !c.is_hide_set())
            .map(clap::Command::get_name)
            .collect();
        assert_eq!(names, ["serve", "reset-passwd", "version", "health"]);
    }

    #[test]
    fn flag_values_parse_as_expected() {
        assert!(super::flag_value("1"));
        assert!(super::flag_value("TRUE"));
        assert!(super::flag_value(" yes "));
        assert!(!super::flag_value("0"));
        assert!(!super::flag_value("false"));
        assert!(!super::flag_value(""));
    }
}
