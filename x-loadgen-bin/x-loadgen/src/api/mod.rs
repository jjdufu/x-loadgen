//! HTTP API surface.

pub mod auth;
pub mod collections;
pub mod environments;
pub mod error;
pub mod openapi;
pub mod probe;
pub mod profiles;
pub mod report;
pub mod runs;
pub mod server;
pub mod state;
pub mod static_files;

#[cfg(test)]
mod automation_tests;
#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod report_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod tests;
