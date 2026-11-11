//! Recoverable storage errors for the HTTP layer to map into status codes.

use thiserror::Error;

/// Failures when reading or writing catalog objects.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The request body or field combination is invalid.
    #[error("{0}")]
    Invalid(String),
    /// The referenced object does not exist.
    #[error("{0}")]
    NotFound(String),
    /// The change conflicts with an existing object or an in-use reference.
    #[error("{0}")]
    Conflict(String),
    /// Unexpected SQLite or I/O failure.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl StoreError {
    /// Validation failure.
    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    /// Missing object.
    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// Unique name or in-use reference.
    #[must_use]
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Other(err.into())
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(err: serde_json::Error) -> Self {
        Self::Other(err.into())
    }
}
