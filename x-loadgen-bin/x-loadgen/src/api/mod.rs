//! HTTP API surface.

pub mod auth;
pub mod error;
pub mod report;
pub mod runs;
pub mod server;
pub mod state;
pub mod static_files;
pub mod targets;
pub mod templates;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod report_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod tests;
