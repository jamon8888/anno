//! Schema definition: cell types, columns, conditional gates, templates.
//!
//! Sub-modules land progressively through Phase 2:
//! - `ttype` — `CellType` enum (Task 4, this commit).
//! - `column` — `Column` struct + builder (Task 5).
//! - `conditional` — `ConditionalSpec` + `Predicate` (Task 6).
//! - `json_schema` — `CellType` / `Column` → JSON Schema generation (Task 7).
//! - `template` — TOML template loader (Task 8).

pub mod ttype;

pub use ttype::CellType;
