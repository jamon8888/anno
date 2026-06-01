//! Error types for the knowledge store.

/// Store result type.
pub type Result<T> = std::result::Result<T, KnowledgeStoreError>;

/// Errors from SQLite-backed knowledge storage.
#[derive(Debug, thiserror::Error)]
pub enum KnowledgeStoreError {
    /// SQLite failed.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    /// Filesystem IO failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// JSON serialization failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
