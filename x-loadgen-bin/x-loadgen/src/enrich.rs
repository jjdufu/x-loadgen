//! Optional platform-side metrics enrichment (generic `ClickHouse` post-step).

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::info;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ClickHouseEnrichment {
    #[serde(default)]
    pub enabled: bool,
    /// `clickhouse-client` binary or `docker` when `docker_container` is set.
    #[serde(default = "default_cli")]
    pub cli: String,
    /// When set, run `docker exec <container> clickhouse-client ...`.
    #[serde(default)]
    pub docker_container: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password_env: Option<String>,
    /// SQL file path; placeholders `{{from_ms}}` `{{to_ms}}` `{{request_ids_csv}}` expanded.
    #[serde(default)]
    pub sql_file: Option<String>,
    /// Inline SQL (used when `sql_file` empty).
    #[serde(default)]
    pub sql: Option<String>,
    /// Output JSON path relative to report dir when unset → `data/platform-enrichment.json`.
    #[serde(default)]
    pub output: Option<String>,
}

fn default_cli() -> String {
    "clickhouse-client".into()
}

#[derive(Debug, Clone)]
pub struct EnrichmentWindow {
    pub from_ms: u64,
    pub to_ms: u64,
    pub request_ids: Vec<String>,
}

pub fn run_clickhouse_enrichment(
    cfg: &ClickHouseEnrichment,
    window: &EnrichmentWindow,
    report_dir: &Path,
) -> Result<Option<std::path::PathBuf>> {
    if !cfg.enabled {
        return Ok(None);
    }
    let sql_template = if let Some(path) = cfg.sql_file.as_deref().filter(|s| !s.trim().is_empty())
    {
        fs::read_to_string(path).with_context(|| format!("read enrichment sql_file {path}"))?
    } else if let Some(sql) = cfg.sql.as_deref().filter(|s| !s.trim().is_empty()) {
        sql.to_owned()
    } else {
        bail!("enrichment.clickhouse enabled but sql_file/sql not set");
    };
    let ids = window
        .request_ids
        .iter()
        .map(|id| format!("'{}'", id.replace('\'', "\\'")))
        .collect::<Vec<_>>()
        .join(",");
    let sql = sql_template
        .replace("{{from_ms}}", &window.from_ms.to_string())
        .replace("{{to_ms}}", &window.to_ms.to_string())
        .replace(
            "{{request_ids_csv}}",
            if ids.is_empty() { "''" } else { &ids },
        );

    let password = cfg
        .password_env
        .as_deref()
        .and_then(|k| std::env::var(k).ok());

    let output = Command::new(if cfg.docker_container.is_some() {
        "docker"
    } else {
        cfg.cli.as_str()
    })
    .args(docker_or_direct_args(cfg, &sql, password.as_deref()))
    .output()
    .context("run clickhouse enrichment")?;
    if !output.status.success() {
        bail!(
            "clickhouse enrichment failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let out_rel = cfg
        .output
        .clone()
        .unwrap_or_else(|| "data/platform-enrichment.json".into());
    let out_path = report_dir.join(out_rel);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, &output.stdout)?;
    info!(path = %out_path.display(), "wrote platform enrichment");
    Ok(Some(out_path))
}

fn docker_or_direct_args(
    cfg: &ClickHouseEnrichment,
    sql: &str,
    password: Option<&str>,
) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(container) = cfg.docker_container.as_deref() {
        args.push("exec".into());
        if let Some(user) = cfg.user.as_deref() {
            args.push("-e".into());
            args.push(format!("CLICKHOUSE_USER={user}"));
        }
        if let Some(pw) = password {
            args.push("-e".into());
            args.push(format!("CLICKHOUSE_PASSWORD={pw}"));
        }
        args.push(container.into());
        args.push(cfg.cli.clone());
    }
    if let Some(user) = cfg.user.as_deref() {
        args.push("--user".into());
        args.push(user.into());
    }
    if let Some(pw) = password {
        args.push("--password".into());
        args.push(pw.into());
    }
    args.push("--query".into());
    args.push(sql.to_owned());
    args.push("--format".into());
    args.push("JSON".into());
    args
}
