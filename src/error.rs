//! Central error type for the application.
//!
//! Every fallible path returns [`AppError`]. There are no `unwrap()` calls on
//! production code paths; the GUI surfaces any error through `AppState.last_error`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// A business-rule / input-range violation. Carries a human-readable,
    /// Thai-or-English message suitable for direct display in the UI.
    #[error("validation error: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),
}

impl AppError {
    /// Convenience constructor for validation failures.
    pub fn validation(msg: impl Into<String>) -> Self {
        AppError::Validation(msg.into())
    }
}

/// Project-wide result alias.
pub type Result<T> = std::result::Result<T, AppError>;
