//! Conditional-column gates. v0.1 stub: defines the type surface used by
//! [`super::column::Column::conditional`]. T6 fleshes out predicate
//! evaluation; T31/T32 (Phase 7) wire the DAG into the extraction engine.

use crate::ids::ColumnId;
use serde::{Deserialize, Serialize};

/// Marker on a [`Column`](super::column::Column) saying "only extract me
/// when the named parent column's value satisfies `predicate`".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalSpec {
    /// The id of the column whose value gates this child.
    pub parent_col: ColumnId,
    /// The predicate evaluated against the parent cell's value.
    pub predicate: Predicate,
}

/// Predicate over a JSON cell value. Tagged with `op` so a TOML template
/// can carry it as `{ op = "equals", value = "..." }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Predicate {
    /// Parent value equals the supplied literal (JSON-compared).
    Equals {
        /// The literal to compare against.
        value: serde_json::Value,
    },
    /// Parent value is not equal to the supplied literal.
    NotEquals {
        /// The literal to compare against.
        value: serde_json::Value,
    },
    /// Parent cell is non-null (any value extracted, of any type).
    NonNull,
    /// Parent value (coerced to string) matches the regex.
    Matches {
        /// Regex source string (Rust `regex` crate syntax).
        regex: String,
    },
}
