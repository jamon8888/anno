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

/// How a column's cells are extracted — local NER, LLM, or manual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMode {
    /// Choose automatically based on cell type and available models.
    Auto,
    /// Extract a contiguous text span with a local GLiNER model.
    LocalSpan,
    /// Extract a full clause (multi-span) with a local model.
    LocalClause,
    /// Classify into a fixed label set with a local model.
    LocalClassifier,
    /// Always delegate to the remote LLM; never run locally.
    LlmRequired,
    /// Human-input only — no extractor runs.
    Manual,
}

/// A GLiNER label (name + natural-language description) for local extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionLabel {
    /// Short identifier used to match extracted spans (e.g. `"bailleur"`).
    pub name: String,
    /// Natural-language description forwarded to GLiNER as the label text.
    pub description: String,
}

/// Post-processing normalizer applied to raw extracted spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionNormalizer {
    /// Normalize to a canonical legal entity name.
    LegalName,
    /// Parse and reformat as ISO 8601 date.
    DateIso,
    /// Parse as a EUR monetary amount.
    EurCurrency,
    /// Parse as a floating-point number.
    Number,
    /// Map to the nearest allowed enum option.
    Enum,
    /// Return the raw clause text verbatim.
    VerbatimClause,
}

/// Configuration controlling how a column's cells are extracted locally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionSpec {
    /// Extraction strategy for this column.
    #[serde(default = "ExtractionSpec::default_mode")]
    pub mode: ExtractionMode,
    /// GLiNER labels used in `local_span` / `local_clause` modes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<ExtractionLabel>,
    /// Keyword hints used to narrow the search window before extraction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Minimum GLiNER confidence threshold (default `0.45`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
    /// Optional post-processing normalizer applied to the raw span.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalizer: Option<ExtractionNormalizer>,
    /// Characters to include before the keyword match when building the extraction window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_before_chars: Option<usize>,
    /// Characters to include after the keyword match when building the extraction window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_after_chars: Option<usize>,
}

impl ExtractionSpec {
    fn default_mode() -> ExtractionMode {
        ExtractionMode::Auto
    }
}

impl Default for ExtractionSpec {
    fn default() -> Self {
        Self {
            mode: ExtractionMode::Auto,
            labels: Vec::new(),
            keywords: Vec::new(),
            threshold: None,
            normalizer: None,
            window_before_chars: None,
            window_after_chars: None,
        }
    }
}

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
    /// Local extraction configuration for this column.
    #[serde(default)]
    pub extraction: ExtractionSpec,
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
    extraction: ExtractionSpec,
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
            extraction: ExtractionSpec::default(),
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

    /// Set the local extraction configuration for this column.
    #[must_use]
    pub fn extraction(mut self, extraction: ExtractionSpec) -> Self {
        self.extraction = extraction;
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
            extraction: self.extraction,
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
    fn column_defaults_to_auto_extraction() {
        let review = ReviewId(uuid::Uuid::now_v7());
        let col = ColumnBuilder::new(review, "landlord", "Landlord?", CellType::Text).build();

        assert_eq!(col.extraction.mode, ExtractionMode::Auto);
        assert!(col.extraction.labels.is_empty());
        assert!(col.extraction.keywords.is_empty());
        assert_eq!(col.extraction.threshold, None);
        assert_eq!(col.extraction.normalizer, None);
    }

    #[test]
    fn round_trips_through_json() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(
            r,
            "amount",
            "Total amount",
            CellType::Currency { code: "EUR".into() },
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
