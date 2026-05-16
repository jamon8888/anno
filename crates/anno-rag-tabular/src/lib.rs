//! anno-rag-tabular — Harvey/Legora-style tabular review for legal docs.
//!
//! Provides schema-driven extraction with per-cell citations, extractive
//! verifier, conditional columns, and CSV/XLSX/Markdown export. Storage
//! lives in LanceDB alongside the existing chunks index.
//!
//! Modules are added in subsequent phase tasks:
//! - `ids` + `error` (Phase 1 Tasks 2 + 3)
//! - `schema` (Phase 2)
//! - `storage` (Phase 3)
//! - `llm` (Phase 4)
//! - `extract` + `verify` + `export` (Phases 5+)

pub mod ids;
pub use ids::{ColumnId, ReviewId, RowId};
