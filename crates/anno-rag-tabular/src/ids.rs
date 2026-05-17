//! Strongly-typed UUID newtypes for the tabular-review feature.
//!
//! `ReviewId` is freshly generated (UUID v7 — time-sortable). `ColumnId`
//! and `RowId` are **deterministic** UUID v5 derivations from
//! `(review_id, key)` so that re-running an extraction over the same
//! review + column-name (or review + doc-id) lands in the same row and
//! upserts cleanly.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique id of a tabular review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReviewId(pub Uuid);

/// Unique id of a column within a review. Deterministic in `(review, name)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColumnId(pub Uuid);

/// Unique id of a row within a review (one row per source document).
/// Deterministic in `(review, doc_id)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RowId(pub Uuid);

impl ReviewId {
    /// Mint a fresh ReviewId.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for ReviewId {
    fn default() -> Self {
        Self::new()
    }
}

impl ColumnId {
    /// Deterministic ColumnId — same review_id + name → same id. Lets the
    /// extraction engine upsert cells without tracking ephemeral mapping.
    #[must_use]
    pub fn for_name(review_id: ReviewId, name: &str) -> Self {
        let ns = Uuid::NAMESPACE_OID;
        // "::" is a safe separator: UUIDs contain only [0-9a-f-], so no
        // (review_id, name) pair can collide with a differently-split one.
        let key = format!("{}::{}", review_id.0, name);
        Self(Uuid::new_v5(&ns, key.as_bytes()))
    }
}

impl RowId {
    /// Deterministic RowId — same review_id + doc_id → same id.
    ///
    /// `doc_id` is the document's UUID as used throughout `anno-rag::store`
    /// (the workspace uses raw `uuid::Uuid` for document identity — no
    /// dedicated `DocId` newtype exists today; v1.1 follows that convention).
    /// Production doc-ids are minted as `Uuid::now_v7()` (see
    /// `anno-rag::pipeline::ingest_one`); v5 hashing works on any 16-byte
    /// UUID so the version mix is harmless.
    #[must_use]
    pub fn for_doc(review_id: ReviewId, doc_id: Uuid) -> Self {
        let ns = Uuid::NAMESPACE_OID;
        // Same separator-safety argument as `ColumnId::for_name`.
        let key = format!("{}::{}", review_id.0, doc_id);
        Self(Uuid::new_v5(&ns, key.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_id_is_deterministic() {
        let r = ReviewId::new();
        let a = ColumnId::for_name(r, "governing_law");
        let b = ColumnId::for_name(r, "governing_law");
        assert_eq!(a, b);
    }

    #[test]
    fn column_id_differs_per_review() {
        let r1 = ReviewId::new();
        let r2 = ReviewId::new();
        let a = ColumnId::for_name(r1, "governing_law");
        let b = ColumnId::for_name(r2, "governing_law");
        assert_ne!(a, b);
    }

    #[test]
    fn row_id_is_deterministic() {
        let r = ReviewId::new();
        // v7 matches the doc-id flavour anno-rag::pipeline + ::store emit
        // (Uuid::now_v7); v5 hashing is version-agnostic so the choice is
        // about realistic test inputs.
        let d = Uuid::now_v7();
        assert_eq!(RowId::for_doc(r, d), RowId::for_doc(r, d));
    }

    #[test]
    fn review_id_is_time_sortable() {
        let a = ReviewId::new();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = ReviewId::new();
        assert!(b.0 > a.0, "v7 UUIDs should be monotonic");
    }
}
