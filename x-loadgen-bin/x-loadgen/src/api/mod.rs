//! HTTP API surface.

pub mod auth;
pub mod error;
pub mod server;
pub mod targets;
pub mod templates;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod tests;
