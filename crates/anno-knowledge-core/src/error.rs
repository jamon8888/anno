//! Error types for source-neutral knowledge domain code.

/// Result type for knowledge core operations.
pub type Result<T> = std::result::Result<T, KnowledgeCoreError>;

/// Source-neutral domain errors.
#[derive(Debug, thiserror::Error)]
pub enum KnowledgeCoreError {
    /// A required stable provider key or namespace component was empty.
    #[error("{field} must not be empty")]
    EmptyStablePart {
        /// Name of the missing field.
        field: &'static str,
    },
}
