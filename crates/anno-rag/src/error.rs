//! Error type for `anno-rag`.
//!
//! Distinguishes recoverable errors (per-doc ingest failures the pipeline
//! skips with a warn) from fatal errors (vault corruption — the binary
//! exits non-zero after audit-logging in later versions).

use thiserror::Error;

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// All errors `anno-rag` can return.
#[derive(Error, Debug)]
pub enum Error {
    /// Ingest of a single document failed. Recoverable: pipeline skips this doc.
    #[error("ingest failed for {path}: {source}")]
    Ingest {
        /// Path of the document that failed.
        path: String,
        /// Underlying failure.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Detection (anno + cloakpipe pattern set) failed.
    #[error("detection failed: {0}")]
    Detect(String),

    /// Vault open / pseudonymize / lookup failed. Vault corruption is fatal upstream.
    #[error("vault error: {0}")]
    Vault(String),

    /// Embedding model load or inference failed.
    #[error("embedding failed: {0}")]
    Embed(String),

    /// LanceDB / index store operation failed.
    #[error("store error: {0}")]
    Store(String),

    /// Configuration validation failed.
    #[error("config error: {0}")]
    Config(String),

    /// I/O error from std.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// LanceDB error.
    #[error(transparent)]
    Lance(#[from] lancedb::Error),

    /// Arrow error.
    #[error(transparent)]
    Arrow(#[from] arrow::error::ArrowError),

    /// anno crate error.
    #[error(transparent)]
    Anno(#[from] anno::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Error>();
    }

    #[test]
    fn display_includes_context() {
        let e = Error::Config("missing data_dir".into());
        assert_eq!(format!("{e}"), "config error: missing data_dir");
    }
}
