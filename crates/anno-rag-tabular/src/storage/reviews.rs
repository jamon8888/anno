//! `tabular_reviews` table — open/create handle.
//!
//! CRUD operations (create / get / list) land in Task 15. Today this
//! module owns only the idempotent open path so `StorageHandle::open`
//! can wire it up.

use crate::error::Result;
use crate::storage::arrow_schema::reviews_schema;
use arrow_array::{RecordBatch, RecordBatchIterator};
use arrow::error::ArrowError;
use lancedb::{Connection, Table};
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
}
