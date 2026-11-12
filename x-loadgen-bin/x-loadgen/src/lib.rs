//! Shared library for the `x-loadgen` binary.
//!
//! The binary only exposes basic commands (`serve`, `reset-passwd`, `version`,
//! `health`). Load tests are started through the HTTP API or the Web UI, which
//! are added in later steps.

pub mod api;
pub mod auth;
pub mod cli;
pub mod db;
pub mod engine;
pub mod expect;
pub mod http;
pub mod log;
pub mod num;
pub mod report;
pub mod run_assert;
pub mod signal;
