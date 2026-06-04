//! `tabular_rows` table — open + per-review row CRUD.
//!
//! One row per `(review, document)` pair. The grid's row axis.
//! `folder_path` denormalises the source document's folder so scope-
//! filter queries don't need a chunks-table join.

use crate::error::Result;
use crate::ids::{ReviewId, RowId};
use crate::storage::arrow_schema::rows_schema;
use crate::storage::util::{opt_str, uuid_to_filter_lit};
use arrow::error::ArrowError;
use arrow_array::{
    FixedSizeBinaryArray, RecordBatch, RecordBatchIterator, StringArray, TimestampMicrosecondArray,
};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Physical Lance table name. The `tabular_` prefix keeps tabular-review
/// state from colliding with the v1.0 chunks / memories tables that
/// share the same dataset directory.
pub const TABLE_NAME: &str = "tabular_rows";

/// Handle to the `tabular_rows` LanceDB table. Cheap to clone —
/// [`lancedb::Table`] is itself a reference-counted handle.
#[derive(Clone)]
pub struct RowsTable {
    pub(crate) tbl: Table,
}

/// In-memory representation of one row of `tabular_rows`.
///
/// Mirrors the Arrow schema field-for-field. `doc_id` is the document's
/// UUID as used throughout `anno-rag::store` — the workspace has no
/// separate `DocId` newtype, so we hold raw [`uuid::Uuid`] here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    /// Stable deterministic id (UUID v5 from `(review_id, doc_id)`).
    pub id: RowId,
    /// Owning review.
    pub review_id: ReviewId,
    /// Source document id (raw `uuid::Uuid`; matches `anno-rag::store`).
    pub doc_id: uuid::Uuid,
    /// Denormalised folder path for fast scope filters. `None` when the
    /// review is workspace-wide.
    pub folder_path: Option<String>,
    /// Creation timestamp, microsecond UTC.
    pub created_at: DateTime<Utc>,
}

impl RowsTable {
    /// Open the `tabular_rows` table, creating it from [`rows_schema`]
    /// if it does not yet exist. Idempotent.
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
            let schema = rows_schema();
            let empty = RecordBatchIterator::new(
                std::iter::empty::<std::result::Result<RecordBatch, ArrowError>>(),
                schema,
            );
            let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(empty);
            conn.create_table(TABLE_NAME, reader).execute().await?
        };
        Ok(Self { tbl })
    }

    /// Append one row.
    ///
    /// Lance is append-only at the table layer; there is no update path
    /// here (a row's identity is immutable — re-running ingestion over
    /// the same `(review, doc)` recomputes the same `RowId` and would
    /// duplicate, which is by design: dedupe is a query-time concern).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Arrow`] if the batch fails to
    /// build (schema/array mismatch — a bug, not an input error), or
    /// [`crate::error::Error::Lance`] on table write failure.
    pub async fn add(&self, row: &Row) -> Result<()> {
        let schema = rows_schema();
        let id_arr =
            FixedSizeBinaryArray::try_from_iter(std::iter::once(row.id.0.as_bytes().to_vec()))?;
        let rid_arr = FixedSizeBinaryArray::try_from_iter(std::iter::once(
            row.review_id.0.as_bytes().to_vec(),
        ))?;
        let did_arr =
            FixedSizeBinaryArray::try_from_iter(std::iter::once(row.doc_id.as_bytes().to_vec()))?;
        let folder_arr = StringArray::from(vec![row.folder_path.clone()]);
        // Schema declares `Timestamp(Microsecond, None)`; we attach no
        // timezone here so the RecordBatch matches by-type.
        let ts_arr = TimestampMicrosecondArray::from(vec![row.created_at.timestamp_micros()]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_arr),
                Arc::new(rid_arr),
                Arc::new(did_arr),
                Arc::new(folder_arr),
                Arc::new(ts_arr),
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

    /// List every row belonging to `review_id`. Order is whatever Lance
    /// hands back — callers that need a stable order should sort by
    /// `created_at`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Lance`] on query failure.
    pub async fn list_for_review(&self, review_id: ReviewId) -> Result<Vec<Row>> {
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
                out.push(row_to_row(&b, i));
            }
        }
        Ok(out)
    }

    /// Delete every row belonging to `review_id`.
    ///
    /// # Errors
    /// Returns [`crate::error::Error::Lance`] on delete failure.
    pub async fn delete_for_review(&self, review_id: ReviewId) -> Result<()> {
        let hex = uuid_to_filter_lit(review_id.0);
        self.tbl.delete(&format!("review_id = X'{hex}'")).await?;
        Ok(())
    }

    /// Look up a row by id. Returns `Ok(None)` if no row matches —
    /// missing rows are not an error here, only IO is.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Lance`] on query failure.
    pub async fn get(&self, id: RowId) -> Result<Option<Row>> {
        let id_hex = uuid_to_filter_lit(id.0);
        let stream = self
            .tbl
            .query()
            .only_if(format!("id = X'{id_hex}'"))
            .limit(1)
            .execute()
            .await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        for b in batches {
            if b.num_rows() > 0 {
                return Ok(Some(row_to_row(&b, 0)));
            }
        }
        Ok(None)
    }
}

/// Decode row `i` of a `tabular_rows` batch into a [`Row`].
///
/// Downcasts use `.expect` because the schema is fixed by
/// [`rows_schema`] — if a column has the wrong physical type at this
/// point, the schema and decoder are out of sync and panicking is the
/// honest signal.
fn row_to_row(b: &RecordBatch, i: usize) -> Row {
    let id_a = b
        .column(0)
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .expect("column 0 (id) is FixedSizeBinaryArray by schema");
    let rid_a = b
        .column(1)
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .expect("column 1 (review_id) is FixedSizeBinaryArray by schema");
    let did_a = b
        .column(2)
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .expect("column 2 (doc_id) is FixedSizeBinaryArray by schema");
    let folder_a = b
        .column(3)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 3 (folder_path) is StringArray by schema");
    let ts_a = b
        .column(4)
        .as_any()
        .downcast_ref::<TimestampMicrosecondArray>()
        .expect("column 4 (created_at) is TimestampMicrosecondArray by schema");

    let id_bytes: [u8; 16] = id_a
        .value(i)
        .try_into()
        .expect("id column is FixedSizeBinary(16) by schema");
    let rid_bytes: [u8; 16] = rid_a
        .value(i)
        .try_into()
        .expect("review_id column is FixedSizeBinary(16) by schema");
    let did_bytes: [u8; 16] = did_a
        .value(i)
        .try_into()
        .expect("doc_id column is FixedSizeBinary(16) by schema");
    Row {
        id: RowId(uuid::Uuid::from_bytes(id_bytes)),
        review_id: ReviewId(uuid::Uuid::from_bytes(rid_bytes)),
        doc_id: uuid::Uuid::from_bytes(did_bytes),
        folder_path: opt_str(folder_a, i),
        created_at: DateTime::<Utc>::from_timestamp_micros(ts_a.value(i)).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn fresh_table() -> (TempDir, RowsTable) {
        let dir = TempDir::new().expect("tempdir");
        let conn = Arc::new(
            lancedb::connect(dir.path().to_str().expect("utf8 path"))
                .execute()
                .await
                .expect("lancedb connect"),
        );
        let t = RowsTable::open(conn).await.expect("open rows");
        (dir, t)
    }

    fn mk_row(review_id: ReviewId, doc_id: uuid::Uuid) -> Row {
        Row {
            id: RowId::for_doc(review_id, doc_id),
            review_id,
            doc_id,
            folder_path: Some("Deal_Acme/01_NDA".into()),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn add_three_rows_list_returns_three() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        for _ in 0..3 {
            let row = mk_row(r, uuid::Uuid::now_v7());
            t.add(&row).await.expect("add row");
        }
        let all = t.list_for_review(r).await.expect("list");
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn get_returns_specific_row() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        let row = mk_row(r, uuid::Uuid::now_v7());
        t.add(&row).await.expect("add");
        let got = t.get(row.id).await.expect("get").expect("present");
        assert_eq!(got.id, row.id);
        assert_eq!(got.doc_id, row.doc_id);
        assert_eq!(got.folder_path, row.folder_path);
    }

    #[tokio::test]
    async fn get_unknown_returns_none() {
        let (_dir, t) = fresh_table().await;
        let unknown = RowId::for_doc(ReviewId::new(), uuid::Uuid::now_v7());
        assert!(t.get(unknown).await.expect("get").is_none());
    }

    #[tokio::test]
    async fn delete_for_review_removes_only_matching_rows() {
        let (_dir, t) = fresh_table().await;
        let r1 = ReviewId::new();
        let r2 = ReviewId::new();
        t.add(&mk_row(r1, uuid::Uuid::now_v7()))
            .await
            .expect("add r1");
        t.add(&mk_row(r1, uuid::Uuid::now_v7()))
            .await
            .expect("add r1 second");
        t.add(&mk_row(r2, uuid::Uuid::now_v7()))
            .await
            .expect("add r2");

        t.delete_for_review(r1).await.expect("delete r1");

        assert!(t.list_for_review(r1).await.expect("list r1").is_empty());
        assert_eq!(t.list_for_review(r2).await.expect("list r2").len(), 1);
    }
}
