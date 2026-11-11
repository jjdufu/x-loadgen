//! `x-loadgen-mockgw`: run the mock mazbri gateway as a standalone process.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use x_loadgen_mockgw::{MockConfig, MockGateway, example_config, parse_config};

/// Mock mazbri gateway for local and end-to-end load-test checks.
#[derive(Debug, Parser)]
#[command(name = "x-loadgen-mockgw", version)]
struct Args {
    /// Listen address.
    #[arg(long, default_value = "0.0.0.0:18080")]
    listen: String,
    /// Optional JSON config (tenants can also be set later with `PUT /__mock/tenants`).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Print an example config and exit.
    #[arg(long)]
    example: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.example {
        println!("{}", serde_json::to_string_pretty(&example_config())?);
        return Ok(());
    }
    let config = match args.config.as_ref() {
        Some(path) => parse_config(
            &std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?,
        )?,
        None => MockConfig::default(),
    };
    let gateway = MockGateway::start(&args.listen, &config).await?;
    println!(
        "x-loadgen-mockgw listening on {} ({} tenants)",
        gateway.base_url(),
        config.tenants.len()
    );
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("install SIGTERM handler")?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result.context("wait for ctrl-c")?,
        _ = terminate.recv() => {}
    }
    gateway.stop().await;
    Ok(())
}
