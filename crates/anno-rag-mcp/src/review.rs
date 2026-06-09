//! Tabular review tool parameter and result types for the anno-rag MCP server.

use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters for `review_create`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewCreateParams {
    /// Human-readable review name (e.g. "NDA batch 2026-05").
    pub name: String,
    /// Built-in template id to load columns from. One of:
    /// nda-v1, customer-contract-v1, real-estate-v1, employment-v1, ip-v1.
    /// When absent an empty review is created (add columns separately).
    #[serde(default)]
    pub template_id: Option<String>,
    /// Optional folder path scoped to this review (informational only).
    #[serde(default)]
    pub scope_folder: Option<String>,
    /// Optional corpus id that owns this review.
    #[serde(default)]
    pub corpus_id: Option<String>,
}

/// Parameters for `review_add_rows`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewAddRowsParams {
    /// Review UUID (returned by review_create).
    pub review_id: String,
    /// List of document UUIDs to add as rows. Must already be ingested.
    pub doc_ids: Vec<String>,
    /// When true, force re-extraction of all columns even if cells exist.
    #[serde(default)]
    pub force_reextract: bool,
}

/// Parameters for `review_extract`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewExtractParams {
    /// Review UUID.
    pub review_id: String,
    /// When true, force re-extraction of all columns even if cells exist.
    #[serde(default)]
    pub force_reextract: bool,
}

/// Parameters for `review_refine_cell`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewRefineCellParams {
    /// Review UUID.
    pub review_id: String,
    /// Row UUID.
    pub row_id: String,
    /// Column UUID.
    pub col_id: String,
    /// Extra instruction prepended to the column prompt for this re-extraction.
    pub instruction: String,
}

/// Parameters for `review_set_cell`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewSetCellParams {
    /// Review UUID.
    pub review_id: String,
    /// Row UUID.
    pub row_id: String,
    /// Column UUID.
    pub col_id: String,
    /// New cell value (any JSON: string, number, bool, array, object).
    pub value: serde_json::Value,
    /// Lock the cell after writing so it cannot be auto-overwritten.
    #[serde(default)]
    pub lock: bool,
    /// Reviewer identifier (email or name).
    #[serde(default)]
    pub actor: Option<String>,
}

/// Parameters for `review_lock_cell` and `review_unlock_cell`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewCellLockParams {
    /// Review UUID.
    pub review_id: String,
    /// Row UUID.
    pub row_id: String,
    /// Column UUID.
    pub col_id: String,
    /// Reviewer identifier (email or name).
    #[serde(default)]
    pub actor: Option<String>,
}

/// Parameters for `review_export`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewExportParams {
    /// Review UUID.
    pub review_id: String,
    /// Export format: "csv", "markdown", or "xlsx".
    #[serde(default = "default_export_format")]
    pub format: String,
    /// Absolute path where the XLSX file will be written. Required when
    /// format is "xlsx". Ignored for csv/markdown (returned as string).
    #[serde(default)]
    pub output_path: Option<String>,
}

pub(crate) fn default_export_format() -> String {
    "csv".into()
}

/// Parameters for `review_get`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewGetParams {
    /// Review UUID.
    pub review_id: String,
}

#[derive(Serialize)]
pub(crate) struct ReviewCreateResult {
    pub(crate) review_id: String,
    pub(crate) name: String,
    pub(crate) columns_loaded: usize,
}

#[derive(Serialize)]
pub(crate) struct ReviewAddRowsResult {
    pub(crate) rows_added: usize,
    pub(crate) extraction_started: bool,
    pub(crate) failed_doc_ids: Vec<String>,
    pub(crate) extraction_error: Option<String>,
}

pub(crate) struct ParsedReviewDocIds {
    pub(crate) valid: Vec<uuid::Uuid>,
    pub(crate) failed: Vec<String>,
}

pub(crate) fn parse_review_doc_ids(doc_ids: &[String]) -> ParsedReviewDocIds {
    let mut valid = Vec::new();
    let mut failed = Vec::new();

    for doc_id in doc_ids {
        match uuid::Uuid::parse_str(doc_id) {
            Ok(id) => valid.push(id),
            Err(_) => failed.push(doc_id.clone()),
        }
    }

    ParsedReviewDocIds { valid, failed }
}

pub(crate) fn combine_review_add_rows_extraction_error(
    mut row_errors: Vec<String>,
    extraction_error: Option<String>,
) -> Option<String> {
    if let Some(error) = extraction_error {
        row_errors.push(error);
    }
    if row_errors.is_empty() {
        None
    } else {
        Some(row_errors.join("; "))
    }
}

#[derive(Serialize)]
pub(crate) struct ReviewExtractResult {
    pub(crate) review_id: String,
    pub(crate) rows: usize,
    pub(crate) columns: usize,
    pub(crate) extraction_started: bool,
    pub(crate) extraction_error: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct ReviewRowErrorWire {
    pub(crate) row_id: String,
    pub(crate) doc_id: String,
    pub(crate) error: String,
}

#[derive(Clone)]
pub(crate) struct ReviewExtractionStatus {
    pub(crate) review_id: anno_rag_tabular::ReviewId,
    pub(crate) state: String,
    pub(crate) rows: usize,
    pub(crate) columns: usize,
    pub(crate) ok_rows: usize,
    pub(crate) failed_rows: usize,
    pub(crate) row_errors: Vec<ReviewRowErrorWire>,
    pub(crate) last_error: Option<String>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Serialize)]
pub(crate) struct ReviewExtractionStatusWire {
    pub(crate) review_id: String,
    pub(crate) state: String,
    pub(crate) rows: usize,
    pub(crate) columns: usize,
    pub(crate) ok_rows: usize,
    pub(crate) failed_rows: usize,
    pub(crate) row_errors: Vec<ReviewRowErrorWire>,
    pub(crate) last_error: Option<String>,
    pub(crate) updated_at: String,
}

impl ReviewExtractionStatus {
    pub(crate) fn running(
        review_id: anno_rag_tabular::ReviewId,
        rows: usize,
        columns: usize,
    ) -> Self {
        Self {
            review_id,
            state: "running".into(),
            rows,
            columns,
            ok_rows: 0,
            failed_rows: 0,
            row_errors: Vec::new(),
            last_error: None,
            updated_at: chrono::Utc::now(),
        }
    }

    pub(crate) fn completed(
        review_id: anno_rag_tabular::ReviewId,
        rows: usize,
        columns: usize,
        ok_rows: usize,
        row_errors: Vec<ReviewRowErrorWire>,
    ) -> Self {
        let failed_rows = row_errors.len();
        let state = if failed_rows == 0 {
            "completed"
        } else {
            "completed_with_errors"
        };
        Self {
            review_id,
            state: state.into(),
            rows,
            columns,
            ok_rows,
            failed_rows,
            row_errors,
            last_error: None,
            updated_at: chrono::Utc::now(),
        }
    }

    pub(crate) fn blocked(
        review_id: anno_rag_tabular::ReviewId,
        rows: usize,
        columns: usize,
        error: String,
    ) -> Self {
        Self {
            review_id,
            state: "blocked".into(),
            rows,
            columns,
            ok_rows: 0,
            failed_rows: rows,
            row_errors: Vec::new(),
            last_error: Some(error),
            updated_at: chrono::Utc::now(),
        }
    }

    pub(crate) fn to_wire(&self) -> ReviewExtractionStatusWire {
        ReviewExtractionStatusWire {
            review_id: self.review_id.0.to_string(),
            state: self.state.clone(),
            rows: self.rows,
            columns: self.columns,
            ok_rows: self.ok_rows,
            failed_rows: self.failed_rows,
            row_errors: self.row_errors.clone(),
            last_error: self.last_error.clone(),
            updated_at: self.updated_at.to_rfc3339(),
        }
    }
}

pub(crate) fn try_mark_review_extraction_running(
    statuses: &mut HashMap<anno_rag_tabular::ReviewId, ReviewExtractionStatus>,
    review_id: anno_rag_tabular::ReviewId,
    rows: usize,
    columns: usize,
) -> Result<(), ReviewExtractResult> {
    if let Some(existing) = statuses.get(&review_id) {
        if existing.state == "running" {
            return Err(ReviewExtractResult {
                review_id: review_id.0.to_string(),
                rows: existing.rows,
                columns: existing.columns,
                extraction_started: false,
                extraction_error: Some(format!(
                    "extraction already running for review {}",
                    review_id.0
                )),
            });
        }
    }

    statuses.insert(
        review_id,
        ReviewExtractionStatus::running(review_id, rows, columns),
    );
    Ok(())
}

#[derive(Serialize)]
pub(crate) struct ReviewRefineCellResult {
    pub(crate) ok: bool,
    pub(crate) note: String,
}

#[derive(Serialize)]
pub(crate) struct ReviewSetCellResult {
    pub(crate) ok: bool,
    pub(crate) locked: bool,
}

#[derive(Serialize)]
pub(crate) struct ReviewCellLockResult {
    pub(crate) ok: bool,
    pub(crate) locked: bool,
}

#[derive(Serialize)]
pub(crate) struct ReviewGetResult {
    pub(crate) review_id: String,
    pub(crate) name: String,
    pub(crate) columns: Vec<ReviewColumnWire>,
    pub(crate) rows: Vec<ReviewRowWire>,
    pub(crate) cells: Vec<ReviewCellWire>,
    pub(crate) extraction_status: Option<ReviewExtractionStatusWire>,
}

#[derive(Serialize)]
pub(crate) struct ReviewColumnWire {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) prompt: String,
    pub(crate) order: u32,
}

#[derive(Serialize)]
pub(crate) struct ReviewRowWire {
    pub(crate) id: String,
    pub(crate) doc_id: String,
    pub(crate) doc_label: String,
}

#[derive(Serialize)]
pub(crate) struct ReviewCellWire {
    pub(crate) row_id: String,
    pub(crate) col_id: String,
    pub(crate) value: serde_json::Value,
    pub(crate) confidence: String,
    pub(crate) support_score: f32,
    pub(crate) locked: bool,
    pub(crate) version: u32,
}
