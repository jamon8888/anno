//! `Column` — one column in a review's schema. A `Column` carries the
//! per-cell prompt the LLM extractor follows, the declared cell type
//! (used for constrained-decoding JSON-Schema generation in Task 7),
//! optional conditional gate, and grid metadata (manual flag, display
//! order).
//!
//! Construct via [`ColumnBuilder`] so the deterministic id derivation
//! from `(review_id, name)` (see [`ColumnId::for_name`]) stays
//! encapsulated.

use crate::ids::{ColumnId, ReviewId};
use crate::schema::{CellType, ConditionalSpec};
use serde::{Deserialize, Serialize};

/// One column in a review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    /// Stable id — deterministic in `(review_id, name)`.
    pub id: ColumnId,
    /// Column key (machine-readable; appears in JSON-Schema and exports).
    pub name: String,
    /// The instruction the extractor passes to the LLM per cell.
    pub prompt: String,
    /// Declared value shape.
    pub cell_type: CellType,
    /// Optional gate — see [`ConditionalSpec`].
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub conditional: Option<ConditionalSpec>,
    /// Human-input only — extractor skips this column.
    #[serde(default)]
    pub manual: bool,
    /// Display order in the grid.
    #[serde(default)]
    pub order: u32,
}

/// Builder for a [`Column`]. Hides the deterministic id derivation +
/// keeps optional fields ergonomic.
pub struct ColumnBuilder {
    review_id: ReviewId,
    name: String,
    prompt: String,
    cell_type: CellType,
    conditional: Option<ConditionalSpec>,
    manual: bool,
    order: u32,
}

impl ColumnBuilder {
    /// Start a builder for a column with the given identity + cell type.
    #[must_use]
    pub fn new(review_id: ReviewId, name: &str, prompt: &str, cell_type: CellType) -> Self {
        Self {
            review_id,
            name: name.into(),
            prompt: prompt.into(),
            cell_type,
            conditional: None,
            manual: false,
            order: 0,
        }
    }

    /// Attach a [`ConditionalSpec`]; the extractor skips this column when
    /// the parent column's cell doesn't satisfy the predicate.
    #[must_use]
    pub fn conditional(mut self, c: ConditionalSpec) -> Self {
        self.conditional = Some(c);
        self
    }

    /// Mark the column as human-input-only.
    #[must_use]
    pub fn manual(mut self) -> Self {
        self.manual = true;
        self
    }

    /// Set the display order.
    #[must_use]
    pub fn order(mut self, n: u32) -> Self {
        self.order = n;
        self
    }

    /// Finalise.
    #[must_use]
    pub fn build(self) -> Column {
        Column {
            id: ColumnId::for_name(self.review_id, &self.name),
            name: self.name,
            prompt: self.prompt,
            cell_type: self.cell_type,
            conditional: self.conditional,
            manual: self.manual,
            order: self.order,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_produces_deterministic_id() {
        let r = ReviewId::new();
        let a = ColumnBuilder::new(r, "term", "What is the term?", CellType::Text).build();
        let b = ColumnBuilder::new(r, "term", "different prompt", CellType::Text).build();
        // ID is derived from (review_id, name) — prompt change does not
        // invalidate the id, so re-running extraction with an edited
        // prompt upserts into the same row.
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn manual_columns_are_marked() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "reviewer_notes", "Reviewer comments", CellType::Text)
            .manual()
            .build();
        assert!(c.manual);
    }

    #[test]
    fn round_trips_through_json() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(
            r,
            "amount",
            "Total amount",
            CellType::Currency {
                code: "EUR".into(),
            },
        )
        .order(3)
        .build();
        let s = serde_json::to_string(&c).unwrap();
        let back: Column = serde_json::from_str(&s).unwrap();
        assert_eq!(c.name, back.name);
        assert_eq!(c.order, back.order);
        assert_eq!(c.id, back.id);
    }

    #[test]
    fn conditional_is_skipped_when_absent() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "term", "Term?", CellType::Text).build();
        let s = serde_json::to_string(&c).unwrap();
        assert!(
            !s.contains("conditional"),
            "absent conditional must be skipped on serialise, got: {s}"
        );
    }
}
