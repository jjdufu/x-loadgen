mod binding;
mod cluster;
mod config;
mod enrich;
mod expect;
mod metrics;
mod num;
mod report;
mod resolve;
mod runner;
mod template;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::{LoadgenConfig, build_http_client};
use runner::run_load_test;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "x-loadgen",
    about = "Generic gateway load generator (config-driven tenants, products, traffic, multi-process)",
    subcommand_negates_reqs = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Path to loadgen TOML config. Required — no baked-in tenant.
    #[arg(short, long, value_name = "PATH", required = true)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub qps: Option<u32>,
    #[arg(long)]
    pub concurrency: Option<usize>,
    /// Override `traffic.requests` (clears duration).
    #[arg(long)]
    pub requests: Option<u32>,
    /// Override `traffic.duration_secs` (clears requests).
    #[arg(long)]
    pub duration_secs: Option<u64>,
    /// Override `cluster.processes` (launcher spawns workers when > 1).
    #[arg(long)]
    pub processes: Option<usize>,
    /// Worker index when spawned by the cluster launcher (0-based).
    #[arg(long, hide = true)]
    pub worker_index: Option<usize>,
    /// Worker count when spawned by the cluster launcher.
    #[arg(long, hide = true)]
    pub worker_count: Option<usize>,
    #[arg(long)]
    pub token_only: bool,
    /// Disable HTTP connection pooling (no keep-alive reuse).
    #[arg(long, default_value_t = false)]
    pub disable_connection_pool: bool,
    /// Override idle connection pool timeout in seconds.
    #[arg(long)]
    pub pool_idle_timeout_secs: Option<u64>,
    /// Only run matching product (`product_id`, name substring, `channel_code`, or path).
    #[arg(long)]
    pub only_product: Option<String>,
    /// Disable HTML/JSON report generation.
    #[arg(long, default_value_t = false)]
    pub no_report: bool,
    /// Report output directory (contains `<run_id>/report.html`).
    #[arg(long)]
    pub report_dir: Option<String>,
    /// Report title.
    #[arg(long)]
    pub report_title: Option<String>,
    /// Report default language: zh | en (page can still switch).
    #[arg(long)]
    pub report_lang: Option<String>,
    /// Report profile: quick | standard | latency | ratelimit | full
    #[arg(long)]
    pub report_profile: Option<String>,
    /// Comma-separated extra metrics (merged with profile).
    #[arg(long)]
    pub report_metrics: Option<String>,
    /// Comma-separated expected 429 reasons (count as effective success).
    #[arg(long)]
    pub expected_reasons: Option<String>,
    /// Custom run id for report directory name.
    #[arg(long)]
    pub run_id: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Generate a request-binding signature key pair (private PKCS#8 PEM mode 600 +
    /// public SPKI PEM to register via mgmt `POST /tenants/{code}/request-keys`).
    Keygen {
        /// rsa-pkcs1v15-sign-sha256|384|512, ecdsa-p256-sha256, ecdsa-p384-sha384, ed25519, sm2-sign
        #[arg(long)]
        alg: String,
        /// Private key output path (must not exist; created with mode 600).
        #[arg(long, value_name = "PATH")]
        private_out: PathBuf,
        /// Public key output path.
        #[arg(long, value_name = "PATH")]
        public_out: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Keygen {
        alg,
        private_out,
        public_out,
    }) = &cli.command
    {
        binding::keygen(alg, private_out, public_out)?;
        println!(
            "keygen ok alg={alg} private={} public={}",
            private_out.display(),
            public_out.display()
        );
        return Ok(());
    }
    let config_path = cli
        .config
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--config is required"))?;
    let mut config = LoadgenConfig::load(config_path)?;
    config.apply_cli_overrides(&cli);
    let run_id = resolve_run_id(cli.run_id.as_deref());

    let is_worker = cli.worker_index.is_some() || cli.worker_count.is_some();
    if !is_worker && config.cluster.processes > 1 {
        return run_as_cluster_launcher(&cli, config_path, &config, &run_id);
    }
    if let (Some(index), Some(count)) = (cli.worker_index, cli.worker_count) {
        config.apply_worker_split(index, count);
    }
    run_as_worker(cli, config, run_id).await
}

fn resolve_run_id(cli_run_id: Option<&str>) -> String {
    if let Some(id) = cli_run_id {
        return id.to_owned();
    }
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let short = &Uuid::new_v4().to_string()[..8];
    format!("{ms}-{short}")
}

fn run_as_cluster_launcher(
    cli: &Cli,
    config_path: &Path,
    config: &LoadgenConfig,
    run_id: &str,
) -> Result<()> {
    let _guard = config.init_logging()?;
    let aggregate_dir = PathBuf::from(&config.report.dir);
    let launch = cluster::ClusterLaunch {
        processes: config.cluster.processes,
        config_path: config_path.to_path_buf(),
        worker_extra_args: worker_passthrough_args(cli),
        aggregate_dir: aggregate_dir.clone(),
        run_id: run_id.to_owned(),
    };
    let results = cluster::run_cluster(&launch)?;
    let failed = results.iter().filter(|r| r.exit_code != 0).count();
    if config.report.enabled {
        match cluster::aggregate_summaries(&results, &aggregate_dir, run_id) {
            Ok(path) => println!("cluster aggregate: {}", path.display()),
            Err(err) => tracing::error!(error = ?err, "failed to aggregate worker summaries"),
        }
    }
    if failed > 0 {
        anyhow::bail!("{failed}/{} workers failed", results.len());
    }
    Ok(())
}

async fn run_as_worker(cli: Cli, config: LoadgenConfig, run_id: String) -> Result<()> {
    let _guard = config.init_logging()?;
    if config.products.is_empty() {
        anyhow::bail!("at least one [[products]] entry is required");
    }
    log_config_banner(&config);
    if cli.token_only {
        let http = build_http_client(&config)?;
        let token_url = config.token_url();
        for c in &config.resolved_customers {
            let token = c.tenant.fetch_token(&http, &token_url).await?;
            println!(
                "token ok customer={} type={} expires_in={}s",
                c.name, token.token_type, token.expires_in
            );
        }
        return Ok(());
    }
    run_load_test(config, Some(run_id)).await
}

fn log_config_banner(config: &LoadgenConfig) {
    let modes: Vec<String> = config
        .resolved_customers
        .iter()
        .map(|c| format!("{}:{}", c.name, c.tenant.encryption_mode))
        .collect();
    tracing::info!(
        customers = ?modes,
        gateway = %config.gateway_base_url(),
        traffic_model = %config.traffic.model,
        requests = ?config.traffic.requests,
        duration_secs = ?config.traffic.duration_secs,
        concurrency = config.traffic.concurrency,
        qps = ?config.traffic.qps,
        processes = config.cluster.processes,
        products = config.products.len(),
        report_enabled = config.report.enabled,
        report_profile = %config.report.profile,
        "loadgen configuration loaded"
    );
    println!(
        "loadgen customers={} gateway={} model={} requests={:?} duration_secs={:?} concurrency={} qps={:?} products={} report={} profile={}",
        config.resolved_customers.len(),
        config.gateway_base_url(),
        config.traffic.model,
        config.traffic.requests,
        config.traffic.duration_secs,
        config.traffic.concurrency,
        config.traffic.qps,
        config.products.len(),
        config.report.enabled,
        config.report.profile,
    );
}

fn worker_passthrough_args(cli: &Cli) -> Vec<String> {
    let mut args = Vec::new();
    push_opt(&mut args, "--qps", cli.qps.map(|v| v.to_string()));
    push_opt(
        &mut args,
        "--concurrency",
        cli.concurrency.map(|v| v.to_string()),
    );
    push_opt(&mut args, "--requests", cli.requests.map(|v| v.to_string()));
    push_opt(
        &mut args,
        "--duration-secs",
        cli.duration_secs.map(|v| v.to_string()),
    );
    if cli.disable_connection_pool {
        args.push("--disable-connection-pool".into());
    }
    push_opt(
        &mut args,
        "--pool-idle-timeout-secs",
        cli.pool_idle_timeout_secs.map(|v| v.to_string()),
    );
    push_opt(&mut args, "--only-product", cli.only_product.clone());
    if cli.no_report {
        args.push("--no-report".into());
    }
    push_opt(&mut args, "--report-title", cli.report_title.clone());
    push_opt(&mut args, "--report-lang", cli.report_lang.clone());
    push_opt(&mut args, "--report-profile", cli.report_profile.clone());
    push_opt(&mut args, "--report-metrics", cli.report_metrics.clone());
    push_opt(
        &mut args,
        "--expected-reasons",
        cli.expected_reasons.clone(),
    );
    args
}

fn push_opt(args: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(v) = value {
        args.push(flag.into());
        args.push(v);
    }
}
