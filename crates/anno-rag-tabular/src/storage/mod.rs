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
pub mod cells;
pub mod columns;
pub mod lock;
pub mod reviews;
pub mod rows;

pub use cells::CellsTable;
pub use columns::ColumnsTable;
pub use reviews::ReviewsTable;
pub use rows::RowsTable;

use crate::error::Result;
use lancedb::Connection;
use std::sync::Arc;

/// One handle, four tables. Opened against an existing LanceDB
/// `Connection` (the same `~/.anno-rag/index.lance/` directory as the
/// v1.0 chunks/memories tables) so a single Lance dataset directory
/// hosts both the RAG corpus and the tabular-review state.
///
/// Cloning is cheap — every inner [`lancedb::Table`] is itself a
/// reference-counted handle.
#[derive(Clone)]
pub struct StorageHandle {
    /// `tabular_reviews` — one row per review.
    pub reviews: ReviewsTable,
    /// `tabular_columns` — one row per column definition (per review).
    pub columns: ColumnsTable,
    /// `tabular_rows` — one row per (review, document) pairing.
    pub rows: RowsTable,
    /// `tabular_cells` — one row per (review, row, column, version).
    pub cells: CellsTable,
}

impl StorageHandle {
    /// Open or create the four tabular tables on the given connection.
    /// Idempotent across process restarts and across repeated calls in
    /// the same process — each per-table `open` is a `table_names`
    /// check followed by either `open_table` or `create_table`.
    ///
    /// `conn` is wrapped in [`Arc`] so the four sibling opens can share
    /// the same connection without cloning the underlying handle.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::Error::Lance`] if any of the four
    /// `table_names` / `open_table` / `create_table` calls fail.
    pub async fn open(conn: Arc<Connection>) -> Result<Self> {
        let reviews = ReviewsTable::open(conn.clone()).await?;
        let columns = ColumnsTable::open(conn.clone()).await?;
        let rows = RowsTable::open(conn.clone()).await?;
        let cells = CellsTable::open(conn).await?;
        Ok(Self {
            reviews,
            columns,
            rows,
            cells,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn open_creates_4_tables_on_empty_db() {
        let dir = TempDir::new().expect("tempdir");
        let uri = dir.path().to_str().expect("utf8 tempdir path");
        let conn = Arc::new(
            lancedb::connect(uri)
                .execute()
                .await
                .expect("lancedb connect"),
        );
        let _h = StorageHandle::open(conn.clone())
            .await
            .expect("open storage");
        let names: Vec<String> = conn
            .table_names()
            .execute()
            .await
            .expect("list table names");
        for must in [
            "tabular_reviews",
            "tabular_columns",
            "tabular_rows",
            "tabular_cells",
        ] {
            assert!(
                names.contains(&must.to_string()),
                "missing table {must}: {names:?}"
            );
        }
    }

    #[tokio::test]
    async fn open_is_idempotent() {
        let dir = TempDir::new().expect("tempdir");
        let uri = dir.path().to_str().expect("utf8 tempdir path");
        let conn = Arc::new(
            lancedb::connect(uri)
                .execute()
                .await
                .expect("lancedb connect"),
        );
        let _h1 = StorageHandle::open(conn.clone())
            .await
            .expect("first open");
        let _h2 = StorageHandle::open(conn.clone())
            .await
            .expect("second open");
        let count = conn
            .table_names()
            .execute()
            .await
            .expect("list table names")
            .len();
        assert_eq!(count, 4);
    }
}
