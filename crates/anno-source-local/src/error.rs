//! Errors for the local folder source connector.

/// Result type for the local source connector.
pub type Result<T> = std::result::Result<T, LocalSourceError>;

/// Errors raised while discovering local folder objects.
#[derive(Debug, thiserror::Error)]
pub enum LocalSourceError {
    /// Filesystem IO failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The configured folder path does not exist or is not a directory.
    #[error("not a directory: {0}")]
    NotADirectory(String),
}
