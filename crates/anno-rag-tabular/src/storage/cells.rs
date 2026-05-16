//! `tabular_cells` table — open/create handle.
//!
//! Cell versioning, locked-cell enforcement, and append-only writes
//! land across Tasks 17/18. Today this module owns only the idempotent
//! open path so [`crate::storage::StorageHandle::open`] can wire it up.

use crate::error::Result;
use crate::storage::arrow_schema::cells_schema;
use arrow::error::ArrowError;
use arrow_array::{RecordBatch, RecordBatchIterator};
use lancedb::{Connection, Table};
use std::sync::Arc;

/// Physical Lance table name. The `tabular_` prefix keeps tabular-review
/// state from colliding with the v1.0 chunks / memories tables that
/// share the same dataset directory.
pub const TABLE_NAME: &str = "tabular_cells";

/// Handle to the `tabular_cells` LanceDB table.
#[derive(Clone)]
pub struct CellsTable {
    pub(crate) tbl: Table,
}

impl CellsTable {
    /// Open the `tabular_cells` table, creating it from [`cells_schema`]
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
            let schema = cells_schema();
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
