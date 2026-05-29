//! Markdown export — serialise a review as a GitHub-Flavored Markdown
//! table.
//!
//! ## Output shape
//!
//! ```text
//! | Doc | Col1 | Col2 |
//! |-----|------|------|
//! | doc.pdf | value | value |
//! ```
//!
//! ## Special rendering
//!
//! - Pipe characters (`|`) inside cell values are escaped as `\|`.
//! - Cells with [`Confidence::Low`] get a `⚠️ ` prefix so reviewers
//!   can spot uncertain extractions at a glance.

use crate::error::Result;
use crate::ids::ReviewId;
use crate::storage::cells::Confidence;
use crate::storage::StorageHandle;
use std::collections::HashMap;

/// Export `review_id` as a GitHub-Flavored Markdown table string.
///
/// # Errors
///
/// Returns [`crate::error::Error::Lance`] when the storage queries fail
/// or [`crate::error::Error::Json`] when a cell value fails to serialise.
pub async fn export_markdown(storage: &StorageHandle, review_id: ReviewId) -> Result<String> {
    let columns = storage.columns.list_for_review(review_id).await?;
    let mut rows = storage.rows.list_for_review(review_id).await?;
    let cells = storage.cells.all_for_review_latest(review_id).await?;

    rows.sort_by(|a, b| crate::export::csv::doc_label(a).cmp(&crate::export::csv::doc_label(b)));

    let cell_map: HashMap<_, _> = cells.iter().map(|c| ((c.row_id, c.col_id), c)).collect();

    let mut out = String::new();

    // Header row: | Doc | Col1 | Col2 | …
    out.push_str("| Doc");
    for col in &columns {
        out.push_str(" | ");
        out.push_str(&pipe_escape(&col.name));
    }
    out.push_str(" |\n");

    // Separator row: |-----|------|
    out.push_str("|-----");
    for _ in &columns {
        out.push_str("|------");
    }
    out.push_str("|\n");

    // Data rows.
    for row in &rows {
        out.push_str("| ");
        out.push_str(&pipe_escape(&crate::export::csv::doc_label(row)));
        for col in &columns {
            out.push_str(" | ");
            if let Some(cell) = cell_map.get(&(row.id, col.id)) {
                let rendered = crate::export::csv::render_value(&cell.value)?;
                let escaped = pipe_escape(&rendered);
                if cell.confidence == Confidence::Low {
                    out.push_str("⚠️ ");
                }
                out.push_str(&escaped);
            }
        }
        out.push_str(" |\n");
    }

    Ok(out)
}

/// Escape pipe characters inside a Markdown table cell so they do not
/// break the table structure.
fn pipe_escape(s: &str) -> String {
    s.replace('|', "\\|")
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
            name: "Markdown Review".into(),
            project_id: None,
            template_id: None,
            scope_folder: None,
            created_at: Utc::now(),
            schema_version: 1,
        }
    }

    fn mk_row(review_id: crate::ids::ReviewId, folder: &str) -> Row {
        let doc_id = uuid::Uuid::now_v7();
        Row {
            id: RowId::for_doc(review_id, doc_id),
            review_id,
            doc_id,
            folder_path: Some(folder.into()),
            created_at: Utc::now(),
        }
    }

    fn mk_cell_with_conf(
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
            support_score: if confidence == Confidence::Low {
                0.2
            } else {
                0.9
            },
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
    async fn markdown_has_header_separator() {
        let (_dir, storage) = fresh_storage().await;
        let review_id = crate::ids::ReviewId::new();
        storage
            .reviews
            .create(&mk_review(review_id))
            .await
            .expect("create review");

        let col = ColumnBuilder::new(review_id, "Parties", "Who?", CellType::Text)
            .order(0)
            .build();
        storage.columns.add(review_id, &col).await.expect("add col");

        let md = export_markdown(&storage, review_id)
            .await
            .expect("export md");

        let lines: Vec<&str> = md.lines().collect();
        assert!(lines.len() >= 2, "must have at least header + separator");

        // Header line.
        assert!(
            lines[0].starts_with("| Doc"),
            "header must start with '| Doc', got: {:?}",
            lines[0]
        );
        assert!(
            lines[0].contains("Parties"),
            "header must contain column name"
        );

        // Separator line: only `|`, `-`, and spaces.
        let sep = lines[1];
        assert!(
            sep.chars().all(|c| c == '|' || c == '-' || c == ' '),
            "separator must contain only |, -, and spaces, got: {sep:?}"
        );
    }

    #[tokio::test]
    async fn markdown_low_confidence_has_warning_emoji() {
        let (_dir, storage) = fresh_storage().await;
        let review_id = crate::ids::ReviewId::new();
        storage
            .reviews
            .create(&mk_review(review_id))
            .await
            .expect("create review");

        let col = ColumnBuilder::new(review_id, "Clause", "Key clause?", CellType::Text)
            .order(0)
            .build();
        storage.columns.add(review_id, &col).await.expect("add col");

        let row = mk_row(review_id, "docs/contract.pdf");
        storage.rows.add(&row).await.expect("add row");

        let cell = mk_cell_with_conf(
            review_id,
            row.id,
            col.id,
            serde_json::Value::String("uncertain value".into()),
            Confidence::Low,
        );
        storage.cells.upsert(&cell).await.expect("upsert cell");

        let md = export_markdown(&storage, review_id)
            .await
            .expect("export md");

        assert!(
            md.contains("⚠️"),
            "low-confidence cell must have warning emoji, got:\n{md}"
        );
    }
}
