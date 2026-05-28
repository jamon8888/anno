//! Export layer — serialise a completed review into CSV, XLSX, or
//! Markdown.
//!
//! All three exporters share the same fetch pattern:
//! 1. Load the columns (sorted by `order`) and rows for the review.
//! 2. Pull every cell at its latest version (`all_for_review_latest`).
//! 3. Build a `HashMap<(RowId, ColumnId), &Cell>` for O(1) lookup.
//! 4. Walk rows × columns and render each cell into the target format.
//!
//! The doc-label for a row is derived from the first non-None value in:
//! `folder_path` last path segment → `doc_id` UUID string (fallback).
//!
//! (`Row` has no `doc_name` field in this codebase; `folder_path` carries
//! the document path and we take its last `/`-separated component as the
//! human-readable label.)

pub mod csv;
pub mod markdown;
pub mod xlsx;

pub use csv::export_csv;
pub use markdown::export_markdown;
pub use xlsx::export_xlsx;
