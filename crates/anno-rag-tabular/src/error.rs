//! Error + Result types for the tabular-review feature.
//!
//! Split into:
//! - **Recoverable per-cell** (`Extract`, `VerifierRejected`,
//!   `ConditionalSkip`) — a single cell can fail or be skipped without
//!   sinking the whole review. The extraction engine records these on
//!   the cell row and continues.
//! - **Fatal** (`TemplateNotFound`, `SchemaMismatch`, `LockedCell`,
//!   `ConditionalCycle`) — bad inputs or impossible states; the
//!   operation aborts before any partial state is written.
//! - **Pass-through** (`Lance`, `Arrow`, `Io`, `Json`, `Toml`, `Core`) —
//!   wraps lower-layer errors via `#[from]` so callers can `?`-propagate.
//!
//! Subsequent phases will fill in the variants' call sites; today they
//! exist as the contract.

use thiserror::Error;

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// All errors the tabular-review feature can return.
#[derive(Error, Debug)]
pub enum Error {
    // ---- Recoverable per-cell ----
    /// Extraction for a single `(doc, column)` pair failed. The engine
    /// records the failure on the cell and moves on.
    #[error("extraction failed for doc {doc} col {col}: {source}")]
    Extract {
        /// Document id (stringified UUID).
        doc: String,
        /// Column name.
        col: String,
        /// Underlying failure. Boxed because the concrete cause varies by
        /// extractor (LLM HTTP, JSON parse, vault lookup, schema mismatch
        /// downstream, …); enumerating them here would couple this crate
        /// to every layer below.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The verifier rejected an extracted cell — citation didn't survive
    /// the cross-encoder support check or the offset/quote round-trip.
    #[error("verifier rejected cell (score {score}): {reason}")]
    VerifierRejected {
        /// Cross-encoder support score (0.0–1.0).
        score: f32,
        /// Human-readable reason — pinned for the audit log.
        reason: String,
    },

    /// Conditional column gate fired — the parent cell's predicate was
    /// false so the child column is skipped. Not really an error; carried
    /// as a `Result::Err` so the engine can branch cleanly without an
    /// extra Option.
    #[error("conditional gate skipped column {col}")]
    ConditionalSkip {
        /// Column name that got skipped.
        col: String,
    },

    // ---- Fatal ----
    /// Template lookup by name returned nothing.
    #[error("template '{name}' not found")]
    TemplateNotFound {
        /// Template name the caller asked for.
        name: String,
    },

    /// The LLM output did not type-check against the column's declared
    /// `CellType`. The cell is rejected; no row is written.
    #[error("schema mismatch: cell type {expected} vs LLM output {got}")]
    SchemaMismatch {
        /// Declared type the column expected (e.g. `"date"`).
        expected: String,
        /// What the LLM actually returned (e.g. `"string"`).
        got: String,
    },

    /// Auto-overwrite attempted on a human-locked cell. The override path
    /// is the only way past this.
    #[error("locked cell cannot be auto-overwritten: review={review} row={row} col={col}")]
    LockedCell {
        /// Stringified ReviewId.
        review: String,
        /// Stringified RowId.
        row: String,
        /// Stringified ColumnId.
        col: String,
    },

    /// Conditional column dependency forms a cycle; cannot schedule.
    #[error("conditional column dependency cycle: {path}")]
    ConditionalCycle {
        /// Cycle path as `col_a -> col_b -> col_c -> col_a`.
        path: String,
    },

    // ---- Pass-through ----
    /// LanceDB-side failure.
    #[error(transparent)]
    Lance(#[from] lancedb::Error),

    /// Arrow batch encode/decode failure.
    #[error(transparent)]
    Arrow(#[from] arrow::error::ArrowError),

    /// std::io failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON encode/decode failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// TOML decode failure (template loading).
    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    /// anno-rag layer failure. (Plan referenced `anno_rag_core::Error`
    /// from a hypothetical v1.0 crate split; this workspace has a single
    /// `anno-rag` crate exposing `anno_rag::Error` at the root.)
    #[error(transparent)]
    Core(#[from] anno_rag::Error),
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
    fn error_display_includes_context() {
        let e = Error::TemplateNotFound {
            name: "ndax".into(),
        };
        assert_eq!(format!("{e}"), "template 'ndax' not found");
    }

    #[test]
    fn locked_cell_error_serializes_useful_fields() {
        let e = Error::LockedCell {
            review: "r1".into(),
            row: "row1".into(),
            col: "col1".into(),
        };
        let msg = format!("{e}");
        assert!(msg.contains("r1"));
        assert!(msg.contains("row1"));
        assert!(msg.contains("col1"));
    }

    #[test]
    fn verifier_rejected_carries_score_and_reason() {
        let e = Error::VerifierRejected {
            score: 0.32,
            reason: "cosine below threshold".into(),
        };
        let msg = format!("{e}");
        assert!(msg.contains("0.32"));
        assert!(msg.contains("cosine below threshold"));
    }
}
