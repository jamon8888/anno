//! CSV export — serialise a review into RFC 4180 CSV.
//!
//! Uses the [`csv`] crate for correct quoting. The header row is
//! `"Doc"` followed by column names in `order` order. Each data row
//! starts with a doc label derived from the row's `folder_path` last
//! segment (or the `doc_id` UUID as a fallback) and then the cell
//! values.
//!
//! ## Cell value rendering
//!
//! | JSON shape          | CSV representation            |
//! |---------------------|-------------------------------|
//! | `null`              | empty string                  |
//! | `bool`              | `"Oui"` / `"Non"` (FR legal)  |
//! | `number`            | decimal string                |
//! | `string`            | the string as-is              |
//! | `array`             | elements joined by ` \| `     |
//! | other (object, …)   | `serde_json::to_string_pretty`|

use crate::error::Result;
use crate::ids::ReviewId;
use crate::storage::StorageHandle;
use std::collections::HashMap;

/// Export `review_id` as an RFC 4180 CSV string.
///
/// # Errors
///
/// Returns [`crate::error::Error::Lance`] when the storage queries fail,
/// [`crate::error::Error::Json`] when a cell value fails to serialise,
/// or [`crate::error::Error::Io`] if the in-memory CSV writer fails.
pub async fn export_csv(storage: &StorageHandle, review_id: ReviewId) -> Result<String> {
    let columns = storage.columns.list_for_review(review_id).await?;
    let mut rows = storage.rows.list_for_review(review_id).await?;
    let cells = storage.cells.all_for_review_latest(review_id).await?;

    // Sort rows by doc label for a stable, human-friendly order.
    rows.sort_by(|a, b| doc_label(a).cmp(&doc_label(b)));

    // O(1) cell lookup keyed on (row_id, col_id).
    let cell_map: HashMap<_, _> = cells
        .iter()
        .map(|c| ((c.row_id, c.col_id), c))
        .collect();

    let mut wtr = csv::Writer::from_writer(Vec::<u8>::new());

    // Header: "Doc" + column names.
    let mut header: Vec<String> = Vec::with_capacity(columns.len() + 1);
    header.push("Doc".into());
    for col in &columns {
        header.push(col.name.clone());
    }
    wtr.write_record(&header)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    // One data row per Row.
    for row in &rows {
        let mut record: Vec<String> = Vec::with_capacity(columns.len() + 1);
        record.push(doc_label(row));
        for col in &columns {
            let value = cell_map
                .get(&(row.id, col.id))
                .map(|c| render_value(&c.value))
                .transpose()?
                .unwrap_or_default();
            record.push(value);
        }
        wtr.write_record(&record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    }

    wtr.flush()?;
    let bytes = wtr.into_inner().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    })?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Derive a human-readable label for a row from its `folder_path`.
///
/// Takes the last `/`-separated segment of `folder_path` when present,
/// falling back to the stringified `doc_id` UUID.
pub fn doc_label(row: &crate::storage::rows::Row) -> String {
    row.folder_path
        .as_deref()
        .and_then(|p| p.rsplit('/').next())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| row.doc_id.to_string())
}

/// Render a [`serde_json::Value`] to a plain string for CSV / Markdown.
///
/// # Errors
///
/// Returns [`crate::error::Error::Json`] only for the `Object` fallback
/// path (`serde_json::to_string_pretty`); all other branches are
/// infallible.
pub(crate) fn render_value(v: &serde_json::Value) -> Result<String> {
    use serde_json::Value;
    match v {
        Value::Null => Ok(String::new()),
        Value::Bool(b) => Ok(if *b { "Oui" } else { "Non" }.into()),
        Value::Number(n) => Ok(n.to_string()),
        Value::String(s) => Ok(s.clone()),
        Value::Array(arr) => {
            let parts: Result<Vec<String>> = arr.iter().map(render_value).collect();
            Ok(parts?.join(" | "))
        }
        other => Ok(serde_json::to_string_pretty(other)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ColumnId, RowId};
    use crate::schema::{CellType, ColumnBuilder};
    use crate::storage::cells::{Author, Cell, Confidence};
    use crate::storage::reviews::Review;
    use crate::storage::rows::Row;
    use crate::storage::StorageHandle;
    use chrono::Utc;
    use lancedb::Connection;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn fresh_storage() -> (TempDir, StorageHandle) {
        let dir = TempDir::new().expect("tempdir");
        let conn = Arc::new(
            lancedb::connect(dir.path().to_str().expect("utf8 path"))
                .execute()
                .await
                .expect("lancedb connect"),
        );
        let h = StorageHandle::open(conn).await.expect("open storage");
        (dir, h)
    }

    fn mk_review(id: crate::ids::ReviewId) -> Review {
        Review {
            id,
            name: "Test Review".into(),
            project_id: None,
            template_id: None,
            scope_folder: None,
            created_at: Utc::now(),
            schema_version: 1,
        }
    }

    fn mk_row(review_id: crate::ids::ReviewId, doc_id: uuid::Uuid, folder: &str) -> Row {
        Row {
            id: RowId::for_doc(review_id, doc_id),
            review_id,
            doc_id,
            folder_path: Some(folder.into()),
            created_at: Utc::now(),
        }
    }

    fn mk_cell(
        review_id: crate::ids::ReviewId,
        row_id: RowId,
        col_id: ColumnId,
        value: serde_json::Value,
        confidence: Confidence,
    ) -> Cell {
        Cell {
            review_id,
            row_id,
            col_id,
            value,
            reasoning: None,
            citations: vec![],
            support_score: 0.9,
            confidence,
            locked: false,
            version: 1,
            author: Author::System {
                extractor_version: "v1".into(),
            },
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn csv_header_matches_columns() {
        let (_dir, storage) = fresh_storage().await;
        let review_id = crate::ids::ReviewId::new();
        storage
            .reviews
            .create(&mk_review(review_id))
            .await
            .expect("create review");

        let col_a = ColumnBuilder::new(review_id, "Parties", "Who?", CellType::Text)
            .order(0)
            .build();
        let col_b = ColumnBuilder::new(review_id, "Date", "When?", CellType::Date)
            .order(1)
            .build();
        storage
            .columns
            .add(review_id, &col_a)
            .await
            .expect("add col a");
        storage
            .columns
            .add(review_id, &col_b)
            .await
            .expect("add col b");

        let csv = export_csv(&storage, review_id).await.expect("export csv");
        let first_line = csv.lines().next().expect("header line");
        assert_eq!(first_line, "Doc,Parties,Date");
    }

    #[tokio::test]
    async fn csv_empty_cells_are_blank() {
        let (_dir, storage) = fresh_storage().await;
        let review_id = crate::ids::ReviewId::new();
        storage
            .reviews
            .create(&mk_review(review_id))
            .await
            .expect("create review");

        let col = ColumnBuilder::new(review_id, "Term", "What is the term?", CellType::Text)
            .order(0)
            .build();
        storage
            .columns
            .add(review_id, &col)
            .await
            .expect("add col");

        let doc_id = uuid::Uuid::now_v7();
        let row = mk_row(review_id, doc_id, "Deal/contract.pdf");
        storage.rows.add(&row).await.expect("add row");
        // No cell written — should produce an empty field.

        let csv = export_csv(&storage, review_id).await.expect("export csv");
        let data_line = csv.lines().nth(1).expect("data line");
        // "contract.pdf," — doc label + empty field
        assert!(
            data_line.ends_with(','),
            "empty cell should produce trailing comma, got: {data_line:?}"
        );
    }

    #[tokio::test]
    async fn csv_verbatim_with_commas_is_quoted() {
        let (_dir, storage) = fresh_storage().await;
        let review_id = crate::ids::ReviewId::new();
        storage
            .reviews
            .create(&mk_review(review_id))
            .await
            .expect("create review");

        let col =
            ColumnBuilder::new(review_id, "Clause", "Verbatim clause", CellType::Verbatim)
                .order(0)
                .build();
        storage
            .columns
            .add(review_id, &col)
            .await
            .expect("add col");

        let doc_id = uuid::Uuid::now_v7();
        let row = mk_row(review_id, doc_id, "docs/contract.pdf");
        storage.rows.add(&row).await.expect("add row");

        let cell = mk_cell(
            review_id,
            row.id,
            col.id,
            serde_json::Value::String("first clause, second clause".into()),
            Confidence::High,
        );
        storage.cells.upsert(&cell).await.expect("upsert cell");

        let csv = export_csv(&storage, review_id).await.expect("export csv");
        let data_line = csv.lines().nth(1).expect("data line");
        // The csv crate wraps fields containing commas in double-quotes.
        assert!(
            data_line.contains('"'),
            "value with comma must be quoted in RFC 4180, got: {data_line:?}"
        );
    }
}
