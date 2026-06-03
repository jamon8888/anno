//! `tabular_reviews` table — open + CRUD for the top-level review row.
//!
//! One row per review (the "spreadsheet"). Columns, rows, and cells all
//! hang off the `ReviewId` stored here. This module owns the Arrow
//! encode/decode pair and the idempotent `open` so the rest of the
//! crate never has to touch raw `RecordBatch` shapes.

use crate::error::{Error, Result};
use crate::ids::ReviewId;
use crate::storage::arrow_schema::reviews_schema;
use crate::storage::util::{opt_str, uuid_to_filter_lit};
use arrow::error::ArrowError;
use arrow_array::{
    FixedSizeBinaryArray, RecordBatch, RecordBatchIterator, StringArray, TimestampMicrosecondArray,
    UInt32Array,
};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Physical Lance table name. Prefixed with `tabular_` so the
/// tabular-review state never collides with the v1.0 chunks / memories
/// tables that share the same dataset directory.
pub const TABLE_NAME: &str = "tabular_reviews";

/// Handle to the `tabular_reviews` LanceDB table. Cheap to clone —
/// [`lancedb::Table`] is itself a reference-counted handle.
#[derive(Clone)]
pub struct ReviewsTable {
    pub(crate) tbl: Table,
}

/// In-memory representation of one row of `tabular_reviews`.
///
/// Mirrors the Arrow schema field-for-field. `created_at` is microsecond
/// UTC to round-trip cleanly through Lance's `Timestamp(Microsecond,
/// UTC)` type; finer precision would silently truncate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    /// Stable UUID v7 identifying this review.
    pub id: ReviewId,
    /// Human-readable display name (e.g. "Deal Acme NDAs").
    pub name: String,
    /// Optional scoping project — `None` means workspace-wide.
    pub project_id: Option<String>,
    /// Template the review was instantiated from, if any. Kept as the
    /// template's stable name (not id) so re-imports survive.
    pub template_id: Option<String>,
    /// Optional folder filter applied to document ingestion.
    pub scope_folder: Option<String>,
    /// Creation timestamp, microsecond UTC.
    pub created_at: DateTime<Utc>,
    /// Bumped whenever a column is added/removed/edited so re-extracts
    /// can detect drift against previously-written cells.
    pub schema_version: u32,
}

impl ReviewsTable {
    /// Open the `tabular_reviews` table, creating it from the
    /// [`reviews_schema`] if it does not yet exist on disk. Idempotent:
    /// safe to call across process restarts and repeated in-process.
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
            let schema = reviews_schema();
            // `RecordBatchIterator::new` needs the iterator item type
            // (`Result<RecordBatch, ArrowError>`) pinned — `iter::empty`
            // alone is too ambiguous for inference.
            let empty = RecordBatchIterator::new(
                std::iter::empty::<std::result::Result<RecordBatch, ArrowError>>(),
                schema,
            );
            let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(empty);
            conn.create_table(TABLE_NAME, reader).execute().await?
        };
        Ok(Self { tbl })
    }

    /// Append a single review row.
    ///
    /// LanceDB is append-only at the table layer; `create` is the
    /// constructor and there is no `update` path here today (column
    /// mutations land in T16's `add_column` which separately bumps
    /// `schema_version`).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Arrow`] if the batch fails to
    /// build (only possible if the schema and column arrays disagree —
    /// a bug, not an input error), or
    /// [`crate::error::Error::Lance`] on table write failure.
    pub async fn create(&self, review: &Review) -> Result<()> {
        let schema = reviews_schema();
        let id_bytes: [u8; 16] = *review.id.0.as_bytes();
        let id_arr = FixedSizeBinaryArray::try_from_iter(std::iter::once(id_bytes.to_vec()))?;
        let name_arr = StringArray::from(vec![review.name.clone()]);
        let project_arr = StringArray::from(vec![review.project_id.clone()]);
        let template_arr = StringArray::from(vec![review.template_id.clone()]);
        let folder_arr = StringArray::from(vec![review.scope_folder.clone()]);
        // Schema declares `Timestamp(Microsecond, None)` (see
        // `arrow_schema::reviews_schema`); we attach no timezone here
        // so the RecordBatch matches by-type.
        let ts_arr = TimestampMicrosecondArray::from(vec![review.created_at.timestamp_micros()]);
        let sv_arr = UInt32Array::from(vec![review.schema_version]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_arr),
                Arc::new(name_arr),
                Arc::new(project_arr),
                Arc::new(template_arr),
                Arc::new(folder_arr),
                Arc::new(ts_arr),
                Arc::new(sv_arr),
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

    /// Look up a review by id. Returns `Ok(None)` if no row matches —
    /// missing rows are not an error here, only IO is.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Lance`] on query failure, or
    /// [`crate::error::Error::Arrow`] if a returned batch fails to
    /// decode against the schema.
    pub async fn get(&self, id: ReviewId) -> Result<Option<Review>> {
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
                return Ok(Some(row_to_review(&b, 0)));
            }
        }
        Ok(None)
    }

    /// Delete a review row by id.
    ///
    /// This only removes the top-level review row. Callers that need a
    /// cascading cleanup must delete dependent rows in sibling tables
    /// explicitly before calling this method.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Lance`] on delete failure.
    pub async fn delete(&self, id: ReviewId) -> Result<()> {
        let id_hex = uuid_to_filter_lit(id.0);
        self.tbl.delete(&format!("id = X'{id_hex}'")).await?;
        Ok(())
    }

    /// Increment the review's `schema_version` by one and return the
    /// new value. Called by [`crate::storage::ColumnsTable::add_with_bump`]
    /// whenever a column is added so the extraction engine can detect
    /// schema drift against previously-written cells and re-run only
    /// the missing ones.
    ///
    /// Implementation note: `lancedb` 0.29 exposes a SQL-style
    /// `update().only_if(...).column(name, expr).execute()` builder; the
    /// expression is parsed by Lance as SQL, so a `u32` literal is
    /// passed as its decimal string form.
    ///
    /// # Errors
    ///
    /// - [`Error::TemplateNotFound`] when no review with `id` exists.
    ///   (The variant name doesn't quite match the situation — the plan
    ///   reuses it deliberately to avoid adding a new error variant for
    ///   one call site; the `name` field carries the stringified id.)
    /// - [`Error::Lance`] on `get` or `update` failure, propagated.
    pub async fn bump_schema_version(&self, id: ReviewId) -> Result<u32> {
        let prev = self.get(id).await?.ok_or_else(|| Error::TemplateNotFound {
            name: id.0.to_string(),
        })?;
        let new_version = prev.schema_version + 1;
        let id_hex = uuid_to_filter_lit(id.0);
        self.tbl
            .update()
            .only_if(format!("id = X'{id_hex}'"))
            .column("schema_version", new_version.to_string())
            .execute()
            .await?;
        Ok(new_version)
    }

    /// Return every review row. Order is whatever Lance hands back —
    /// callers that need a stable order should sort by `created_at`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Lance`] on query failure.
    pub async fn list(&self) -> Result<Vec<Review>> {
        let stream = self.tbl.query().execute().await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        let mut out = Vec::new();
        for b in batches {
            for i in 0..b.num_rows() {
                out.push(row_to_review(&b, i));
            }
        }
        Ok(out)
    }
}

/// Decode row `i` of a `tabular_reviews` batch into a [`Review`].
///
/// The downcasts use `.expect` because the schema is fixed by
/// [`reviews_schema`] — if a column has the wrong physical type at this
/// point, the schema and decoder are out of sync and panicking is the
/// honest signal.
fn row_to_review(b: &RecordBatch, i: usize) -> Review {
    let id_arr = b
        .column(0)
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .expect("column 0 (id) is FixedSizeBinaryArray by schema");
    let name_arr = b
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 1 (name) is StringArray by schema");
    let project_arr = b
        .column(2)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 2 (project_id) is StringArray by schema");
    let template_arr = b
        .column(3)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 3 (template_id) is StringArray by schema");
    let folder_arr = b
        .column(4)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 4 (scope_folder) is StringArray by schema");
    let ts_arr = b
        .column(5)
        .as_any()
        .downcast_ref::<TimestampMicrosecondArray>()
        .expect("column 5 (created_at) is TimestampMicrosecondArray by schema");
    let sv_arr = b
        .column(6)
        .as_any()
        .downcast_ref::<UInt32Array>()
        .expect("column 6 (schema_version) is UInt32Array by schema");

    let id_bytes: [u8; 16] = id_arr
        .value(i)
        .try_into()
        .expect("id column is FixedSizeBinary(16) by schema");
    Review {
        id: ReviewId(uuid::Uuid::from_bytes(id_bytes)),
        name: name_arr.value(i).to_string(),
        project_id: opt_str(project_arr, i),
        template_id: opt_str(template_arr, i),
        scope_folder: opt_str(folder_arr, i),
        created_at: DateTime::<Utc>::from_timestamp_micros(ts_arr.value(i)).unwrap_or_default(),
        schema_version: sv_arr.value(i),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn fresh_table() -> (TempDir, ReviewsTable) {
        let dir = TempDir::new().expect("tempdir");
        let conn = Arc::new(
            lancedb::connect(dir.path().to_str().expect("utf8 path"))
                .execute()
                .await
                .expect("lancedb connect"),
        );
        let t = ReviewsTable::open(conn).await.expect("open reviews");
        (dir, t)
    }

    #[tokio::test]
    async fn create_then_get_roundtrips() {
        let (_dir, t) = fresh_table().await;
        let r = Review {
            id: ReviewId::new(),
            name: "Deal Acme NDAs".into(),
            project_id: Some("Deal_Acme".into()),
            template_id: Some("nda-v1".into()),
            scope_folder: Some("Deal_Acme/01_NDA".into()),
            created_at: Utc::now(),
            schema_version: 1,
        };
        t.create(&r).await.expect("create");
        let got = t.get(r.id).await.expect("get").expect("should exist");
        assert_eq!(got.name, r.name);
        assert_eq!(got.template_id, r.template_id);
        assert_eq!(got.project_id, r.project_id);
        assert_eq!(got.scope_folder, r.scope_folder);
        assert_eq!(got.schema_version, r.schema_version);
    }

    #[tokio::test]
    async fn list_returns_all() {
        let (_dir, t) = fresh_table().await;
        for i in 0..3 {
            let r = Review {
                id: ReviewId::new(),
                name: format!("Review {i}"),
                project_id: None,
                template_id: None,
                scope_folder: None,
                created_at: Utc::now(),
                schema_version: 1,
            };
            t.create(&r).await.expect("create");
        }
        let all = t.list().await.expect("list");
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn delete_removes_review() {
        let (_dir, t) = fresh_table().await;
        let r = Review {
            id: ReviewId::new(),
            name: "Review to delete".into(),
            project_id: None,
            template_id: None,
            scope_folder: None,
            created_at: Utc::now(),
            schema_version: 1,
        };
        t.create(&r).await.expect("create");

        t.delete(r.id).await.expect("delete");

        assert!(t.get(r.id).await.expect("get").is_none());
        let all = t.list().await.expect("list");
        assert!(!all.iter().any(|review| review.id == r.id));
    }

    #[tokio::test]
    async fn get_unknown_returns_none() {
        let (_dir, t) = fresh_table().await;
        let unknown = ReviewId::new();
        assert!(t.get(unknown).await.expect("get").is_none());
    }
}
