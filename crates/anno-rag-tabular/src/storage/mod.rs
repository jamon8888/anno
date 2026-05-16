//! Storage layer — LanceDB-backed CRUD for the four tabular tables
//! (`tabular_reviews`, `tabular_columns`, `tabular_rows`,
//! `tabular_cells`).
//!
//! Sub-modules land progressively through Phase 3:
//! - `arrow_schema` — `RecordBatch` field layout (Task 13).
//! - `reviews` / `columns` / `rows` / `cells` — per-table CRUD
//!   (Tasks 14-17).
//! - `lock` — locked-cell enforcement helper (Task 18).

pub mod arrow_schema;
