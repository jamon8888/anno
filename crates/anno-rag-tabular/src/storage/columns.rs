//! `tabular_columns` table — open + per-review column CRUD.
//!
//! One row per column-in-review. `cell_type` and (optionally)
//! `conditional` are serialised through serde_json so the schema can
//! grow new variants without a Lance migration — the table only sees
//! `Utf8` for those two fields.
//!
//! The `schema_version` bump on `add` that the parent review needs in
//! order to detect drift lives one layer up (Task 19); this module is
//! intentionally just the columns-table CRUD.

use crate::error::Result;
use crate::ids::{ColumnId, ReviewId};
use crate::schema::column::ExtractionSpec;
use crate::schema::Column;
use crate::storage::arrow_schema::columns_schema;
use crate::storage::reviews::ReviewsTable;
use crate::storage::util::uuid_to_filter_lit;
use arrow::error::ArrowError;
use arrow_array::{
    Array, BooleanArray, FixedSizeBinaryArray, RecordBatch, RecordBatchIterator, StringArray,
    UInt32Array,
};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use std::sync::Arc;

/// Physical Lance table name. The `tabular_` prefix keeps tabular-review
/// state from colliding with the v1.0 chunks / memories tables that
/// share the same dataset directory.
pub const TABLE_NAME: &str = "tabular_columns";

/// Handle to the `tabular_columns` LanceDB table. Cheap to clone —
/// [`lancedb::Table`] is itself a reference-counted handle.
#[derive(Clone)]
pub struct ColumnsTable {
    pub(crate) tbl: Table,
}

impl ColumnsTable {
    /// Open the `tabular_columns` table, creating it from
    /// [`columns_schema`] if it does not yet exist. Idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Lance`] if `table_names`,
    /// `open_table`, or `create_table` fails.
    pub async fn open(conn: Arc<Connection>) -> Result<Self> {
        let names = conn.table_names().execute().await?;
        let tbl = if names.iter().any(|n| n == TABLE_NAME) {
            conn.open_table(TABLE_NAME).execute().await?
        } else {
            let schema = columns_schema();
            let empty = RecordBatchIterator::new(
                std::iter::empty::<std::result::Result<RecordBatch, ArrowError>>(),
                schema,
            );
            let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(empty);
            conn.create_table(TABLE_NAME, reader).execute().await?
        };
        Ok(Self { tbl })
    }

    /// Append one column row for the given review.
    ///
    /// `cell_type` and `conditional` are persisted as JSON strings so
    /// new [`CellType`](crate::schema::CellType) variants don't force a
    /// table migration. The caller owns the review-side
    /// `schema_version` bump — keeping it out of here lets callers add
    /// multiple columns inside one logical schema update.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Arrow`] if the batch fails to
    /// build (schema/array mismatch, a bug not a user error),
    /// [`crate::error::Error::Json`] if serialising `cell_type` or
    /// `conditional` fails (only possible on a buggy `Serialize` impl),
    /// or [`crate::error::Error::Lance`] on table write failure.
    pub async fn add(&self, review_id: ReviewId, col: &Column) -> Result<()> {
        let schema = columns_schema();
        let id_b =
            FixedSizeBinaryArray::try_from_iter(std::iter::once(col.id.0.as_bytes().to_vec()))?;
        let rid_b =
            FixedSizeBinaryArray::try_from_iter(std::iter::once(review_id.0.as_bytes().to_vec()))?;
        let name_a = StringArray::from(vec![col.name.clone()]);
        let prompt_a = StringArray::from(vec![col.prompt.clone()]);
        let ttype_json = serde_json::to_string(&col.cell_type)?;
        let ttype_a = StringArray::from(vec![ttype_json]);
        // `Option::map` + `?` would need a fallible closure; `match` is
        // cleaner here and keeps the JSON error in the regular `?` path.
        let cond_json: Option<String> = match col.conditional.as_ref() {
            Some(c) => Some(serde_json::to_string(c)?),
            None => None,
        };
        let cond_a = StringArray::from(vec![cond_json]);
        let extraction_json = serde_json::to_string(&col.extraction)?;
        let extraction_a = StringArray::from(vec![Some(extraction_json)]);
        let manual_a = BooleanArray::from(vec![col.manual]);
        let order_a = UInt32Array::from(vec![col.order]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_b),
                Arc::new(rid_b),
                Arc::new(name_a),
                Arc::new(prompt_a),
                Arc::new(ttype_a),
                Arc::new(cond_a),
                Arc::new(extraction_a),
                Arc::new(manual_a),
                Arc::new(order_a),
            ],
        )?;

        // Bind to a named `reader` of the trait-object type — passing
        // `Box::new(iter)` directly into `self.tbl.add(...)` has been
        // observed to ICE rustc on lancedb 0.29 (see T14 notes).
        let iter = RecordBatchIterator::new(std::iter::once(Ok(batch)), schema);
        let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(iter);
        self.tbl.add(reader).execute().await?;
        Ok(())
    }

    /// Add a column and bump the owning review's `schema_version` in
    /// one step. Returns the new `schema_version`.
    ///
    /// This is the convenience wrapper most callers want; the extraction
    /// engine relies on the bump to detect drift and re-run missing
    /// cells. The lower-level [`Self::add`] stays available for the rare
    /// case where multiple columns land inside a single logical schema
    /// update — then the caller batches the bump itself.
    ///
    /// # Errors
    ///
    /// Propagates errors from [`Self::add`] (see that method) and from
    /// [`ReviewsTable::bump_schema_version`] (notably
    /// [`crate::error::Error::TemplateNotFound`] if `review_id` does
    /// not exist).
    pub async fn add_with_bump(
        &self,
        reviews: &ReviewsTable,
        review_id: ReviewId,
        col: &Column,
    ) -> Result<u32> {
        self.add(review_id, col).await?;
        reviews.bump_schema_version(review_id).await
    }

    /// List every column belonging to `review_id`, sorted by `order`.
    ///
    /// Lance returns batches in arbitrary order, so we sort in-process —
    /// the grid display layer treats `order` as the authoritative axis,
    /// and stable ordering here keeps tests deterministic.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Lance`] on query failure,
    /// [`crate::error::Error::Arrow`] if a batch fails to decode, or
    /// [`crate::error::Error::Json`] if a stored `cell_type_json` /
    /// `conditional_json` no longer round-trips (schema drift across
    /// crate versions).
    pub async fn list_for_review(&self, review_id: ReviewId) -> Result<Vec<Column>> {
        let hex = uuid_to_filter_lit(review_id.0);
        let stream = self
            .tbl
            .query()
            .only_if(format!("review_id = X'{hex}'"))
            .execute()
            .await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        let mut out = Vec::new();
        for b in batches {
            for i in 0..b.num_rows() {
                out.push(row_to_column(&b, i)?);
            }
        }
        out.sort_by_key(|c| c.order);
        Ok(out)
    }
}

/// Decode row `i` of a `tabular_columns` batch into a [`Column`].
///
/// Downcasts use `.expect` because the schema is fixed by
/// [`columns_schema`] — if a column has the wrong physical type at this
/// point, the schema and decoder are out of sync and panicking is the
/// honest signal. Json decode goes through `?` because *that* really
/// can fail at runtime if a row was written by an older crate version
/// with an incompatible serde shape.
fn row_to_column(b: &RecordBatch, i: usize) -> Result<Column> {
    let id_a = b
        .column(0)
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .expect("column 0 (id) is FixedSizeBinaryArray by schema");
    let name_a = b
        .column(2)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 2 (name) is StringArray by schema");
    let prompt_a = b
        .column(3)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 3 (prompt) is StringArray by schema");
    let ttype_a = b
        .column(4)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 4 (cell_type_json) is StringArray by schema");
    let cond_a = b
        .column(5)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 5 (conditional_json) is StringArray by schema");
    // Column 6 is extraction_json (nullable Utf8). Tables written before
    // this field existed will not have it — fall back to ExtractionSpec::default().
    let extraction_a = b.schema().field_with_name("extraction_json").ok().and_then(|_| {
        b.column_by_name("extraction_json")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
    });
    let manual_a = b
        .column_by_name("manual")
        .and_then(|c| c.as_any().downcast_ref::<BooleanArray>())
        .expect("manual is BooleanArray by schema");
    let order_a = b
        .column_by_name("order_idx")
        .and_then(|c| c.as_any().downcast_ref::<UInt32Array>())
        .expect("order_idx is UInt32Array by schema");

    let id_bytes: [u8; 16] = id_a
        .value(i)
        .try_into()
        .expect("id column is FixedSizeBinary(16) by schema");
    let conditional = if cond_a.is_null(i) {
        None
    } else {
        Some(serde_json::from_str(cond_a.value(i))?)
    };
    let extraction: ExtractionSpec = extraction_a
        .and_then(|a| if a.is_null(i) { None } else { Some(a.value(i)) })
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_default();
    Ok(Column {
        id: ColumnId(uuid::Uuid::from_bytes(id_bytes)),
        name: name_a.value(i).to_string(),
        prompt: prompt_a.value(i).to_string(),
        cell_type: serde_json::from_str(ttype_a.value(i))?,
        conditional,
        extraction,
        manual: manual_a.value(i),
        order: order_a.value(i),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{CellType, ColumnBuilder};
    use crate::storage::reviews::Review;
    use chrono::Utc;
    use tempfile::TempDir;

    async fn fresh_both() -> (TempDir, ReviewsTable, ColumnsTable) {
        let dir = TempDir::new().expect("tempdir");
        let conn = Arc::new(
            lancedb::connect(dir.path().to_str().expect("utf8 path"))
                .execute()
                .await
                .expect("lancedb connect"),
        );
        let r = ReviewsTable::open(conn.clone())
            .await
            .expect("open reviews");
        let c = ColumnsTable::open(conn).await.expect("open columns");
        (dir, r, c)
    }

    async fn fresh_table() -> (TempDir, ColumnsTable) {
        let dir = TempDir::new().expect("tempdir");
        let conn = Arc::new(
            lancedb::connect(dir.path().to_str().expect("utf8 path"))
                .execute()
                .await
                .expect("lancedb connect"),
        );
        let t = ColumnsTable::open(conn).await.expect("open columns");
        (dir, t)
    }

    #[tokio::test]
    async fn add_then_list_preserves_order() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        // Insert in scrambled order; `list_for_review` must sort by
        // `order` so the grid sees a deterministic column sequence.
        for name in ["c2", "c0", "c1"] {
            let order = match name {
                "c0" => 0,
                "c1" => 1,
                "c2" => 2,
                _ => unreachable!(),
            };
            let c = ColumnBuilder::new(r, name, "x", CellType::Text)
                .order(order)
                .build();
            t.add(r, &c).await.expect("add column");
        }
        let cols = t.list_for_review(r).await.expect("list");
        let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["c0", "c1", "c2"]);
    }

    #[tokio::test]
    async fn list_filters_by_review_id() {
        let (_dir, t) = fresh_table().await;
        let r1 = ReviewId::new();
        let r2 = ReviewId::new();
        t.add(
            r1,
            &ColumnBuilder::new(r1, "a", "p", CellType::Text).build(),
        )
        .await
        .expect("add r1");
        t.add(
            r2,
            &ColumnBuilder::new(r2, "b", "p", CellType::Text).build(),
        )
        .await
        .expect("add r2");
        let only_r1 = t.list_for_review(r1).await.expect("list r1");
        assert_eq!(only_r1.len(), 1);
        assert_eq!(only_r1[0].name, "a");
    }

    #[tokio::test]
    async fn add_column_bumps_review_schema_version() {
        let (_dir, reviews, columns) = fresh_both().await;
        let review = Review {
            id: ReviewId::new(),
            name: "Deal Acme NDAs".into(),
            project_id: None,
            template_id: None,
            scope_folder: None,
            created_at: Utc::now(),
            schema_version: 1,
        };
        reviews.create(&review).await.expect("create review");
        let col = ColumnBuilder::new(review.id, "governing_law", "p", CellType::Text).build();
        let new_v = columns
            .add_with_bump(&reviews, review.id, &col)
            .await
            .expect("add_with_bump");
        assert_eq!(new_v, 2);
        let refetched = reviews
            .get(review.id)
            .await
            .expect("get review")
            .expect("review row should still exist after update");
        assert_eq!(refetched.schema_version, 2);
    }
}
