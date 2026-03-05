use std::path::PathBuf;

/// All errors that can occur in bhdump.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("Database not found: {}", .0.display())]
    DatabaseNotFound(PathBuf),

    #[error("Permission denied reading {}: {detail}", .path.display())]
    PermissionDenied { path: PathBuf, detail: String },

    #[error("No browsers detected")]
    NoBrowsersDetected,

    #[error("Invalid date/time: {0}")]
    InvalidDateTime(String),

    #[error("Filter expression error: {0}")]
    Expression(String),

    #[error("Unsupported platform")]
    UnsupportedPlatform,
}

pub type Result<T> = std::result::Result<T, Error>;
