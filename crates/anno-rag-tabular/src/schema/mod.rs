//! Schema definition: cell types, columns, conditional gates, templates.
//!
//! Sub-modules land progressively through Phase 2:
//! - `ttype` — `CellType` enum (Task 4).
//! - `column` — `Column` struct + builder (Task 5).
//! - `conditional` — `ConditionalSpec` + `Predicate` shapes (Task 5 stub,
//!   evaluation logic in Task 6).
//! - `json_schema` — `CellType` / `Column` → JSON Schema generation (Task 7).
//! - `template` — TOML template loader (Task 8).

pub mod column;
pub mod conditional;
pub mod json_schema;
pub mod template;
pub mod ttype;

pub use column::{Column, ColumnBuilder};
pub use conditional::{ConditionalSpec, Predicate};
pub use ttype::CellType;
