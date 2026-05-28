//! Arrow schemas for the four tabular-review tables.
//!
//! Column order is **stable / append-only** — bumping a schema means
//! adding fields at the end, never reshuffling. The Lance writer paths
//! depend on positional column ordering in `RecordBatch` construction.
//!
//! UUIDs are stored as `FixedSizeBinary(16)` (raw 128-bit) rather than
//! `Utf8` — saves ~24B/row and matches the chunks table's convention.
//! Timestamps use microsecond precision (UTC-naive) for parity with the
//! v1.0 chunks table.

use arrow_schema::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

/// `tabular_reviews` — one row per review. Holds the review metadata
/// (project scope, optional template id, schema-version counter that
/// gets bumped on every column add).
#[must_use]
pub fn reviews_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::FixedSizeBinary(16), false), // UUID v7
        Field::new("name", DataType::Utf8, false),
        Field::new("project_id", DataType::Utf8, true),
        Field::new("template_id", DataType::Utf8, true),
        Field::new("scope_folder", DataType::Utf8, true),
        Field::new(
            "created_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("schema_version", DataType::UInt32, false), // bumped on add_column
    ]))
}

/// `tabular_columns` — one row per column-in-review. `cell_type_json`
/// is the serialised [`CellType`](crate::schema::CellType) (carries the
/// `kind` discriminant + variant payload). `conditional_json` is null
/// when no gate is set.
#[must_use]
pub fn columns_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::FixedSizeBinary(16), false), // UUID v5
        Field::new("review_id", DataType::FixedSizeBinary(16), false),
        Field::new("name", DataType::Utf8, false),
        Field::new("prompt", DataType::Utf8, false),
        Field::new("cell_type_json", DataType::Utf8, false), // serde_json CellType
        Field::new("conditional_json", DataType::Utf8, true),
        Field::new("extraction_json", DataType::Utf8, true),
        Field::new("manual", DataType::Boolean, false),
        Field::new("order_idx", DataType::UInt32, false),
    ]))
}

/// `tabular_rows` — one row per (review, document) pair. The grid's
/// row axis. `folder_path` denormalises the document's folder for fast
/// scope-filter queries without a chunks-table join.
#[must_use]
pub fn rows_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::FixedSizeBinary(16), false), // UUID v5
        Field::new("review_id", DataType::FixedSizeBinary(16), false),
        Field::new("doc_id", DataType::FixedSizeBinary(16), false),
        Field::new("folder_path", DataType::Utf8, true),
        Field::new(
            "created_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
    ]))
}

/// `tabular_cells` — one row per **version** of a `(review, row, col)`
/// cell. Cells are immutable: re-extraction or human edits write a new
/// row with `version = previous + 1`. `locked = true` blocks the
/// auto-overwrite path; only the explicit override path may write past
/// a locked cell.
#[must_use]
pub fn cells_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("review_id", DataType::FixedSizeBinary(16), false),
        Field::new("row_id", DataType::FixedSizeBinary(16), false),
        Field::new("col_id", DataType::FixedSizeBinary(16), false),
        Field::new("value_json", DataType::Utf8, false), // serde_json
        Field::new("reasoning", DataType::Utf8, true),
        Field::new("citations_json", DataType::Utf8, false), // Vec<Citation>
        Field::new("support_score", DataType::Float32, false),
        Field::new("confidence", DataType::Utf8, false), // High|Medium|Low
        Field::new("locked", DataType::Boolean, false),
        Field::new("version", DataType::UInt32, false),
        Field::new("author", DataType::Utf8, false), // "system:v1" or "human:user_id"
        Field::new(
            "updated_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_schemas_have_unique_field_names() {
        for s in [
            reviews_schema(),
            columns_schema(),
            rows_schema(),
            cells_schema(),
        ] {
            let names: Vec<_> = s.fields().iter().map(|f| f.name().as_str()).collect();
            let mut sorted = names.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                names.len(),
                "duplicate field name in {names:?}"
            );
        }
    }

    #[test]
    fn cells_has_versioning_columns() {
        let s = cells_schema();
        assert!(s.field_with_name("version").is_ok());
        assert!(s.field_with_name("locked").is_ok());
        assert!(s.field_with_name("author").is_ok());
    }

    #[test]
    fn cells_uses_float32_for_support_score() {
        let s = cells_schema();
        let f = s
            .field_with_name("support_score")
            .expect("support_score must exist");
        assert_eq!(f.data_type(), &DataType::Float32);
    }

    #[test]
    fn uuid_fields_are_fixed_size_binary_16() {
        // Spot-check: every id-like column must be 16-byte binary, not Utf8.
        let cells = cells_schema();
        for f in ["review_id", "row_id", "col_id"] {
            let dt = cells.field_with_name(f).expect("present").data_type();
            assert_eq!(dt, &DataType::FixedSizeBinary(16), "{f} dtype");
        }
    }
}
