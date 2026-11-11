//! `x-loadgen` binary: parse a basic command and dispatch it.

use anyhow::Result;
use std::process::ExitCode;
use x_loadgen::cli;

fn main() -> Result<ExitCode> {
    let cli = cli::parse();
    cli::run(cli)
}
