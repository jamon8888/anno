//! Error types for the corpus store.

/// Store result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors from SQLite-backed corpus storage.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// SQLite failed.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    /// JSON serialization failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Corpus root normalization failed.
    #[error(transparent)]
    Root(#[from] anno_corpus_core::RootError),
    /// New corpus root overlaps an existing corpus.
    #[error("corpus root overlaps existing corpus {corpus_id:?}")]
    Overlap {
        /// Existing overlapping corpus id.
        corpus_id: anno_corpus_core::CorpusId,
    },
    /// Operation referenced an unknown corpus.
    #[error("unknown corpus: {0}")]
    UnknownCorpus(String),
}
