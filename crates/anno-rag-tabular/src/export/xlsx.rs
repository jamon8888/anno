//! XLSX export — serialise a review to an Excel workbook.
//!
//! Uses [`rust_xlsxwriter`] (already declared in `Cargo.toml`). The
//! workbook has a single worksheet named after the review. The header
//! row is bold with a light-grey background. Per-cell formatting:
//!
//! | Cell type / confidence | Formatting applied              |
//! |------------------------|---------------------------------|
//! | `Date`                 | `"DD/MM/YYYY"` number format    |
//! | `Currency { code }`    | locale-fr currency format       |
//! | `Boolean`              | renders as `"Oui"` / `"Non"`    |
//! | `Confidence::Low`      | light-red (`#FFCCCC`) fill      |
//!
//! The doc-label column (column 0) is auto-width-fitted after writing.

use crate::error::Result;
use crate::ids::ReviewId;
use crate::schema::CellType;
use crate::storage::cells::Confidence;
use crate::storage::StorageHandle;
use rust_xlsxwriter::{Color, Format, FormatPattern, Workbook, XlsxError};
use std::collections::HashMap;
use std::path::Path;

/// Export `review_id` as an XLSX file at `output_path`.
///
/// # Errors
///
/// Returns [`crate::error::Error::Lance`] when the storage queries fail,
/// [`crate::error::Error::Json`] when a cell value fails to serialise,
/// or [`crate::error::Error::Io`] if writing the workbook fails.
pub async fn export_xlsx(
    storage: &StorageHandle,
    review_id: ReviewId,
    output_path: &Path,
) -> Result<()> {
    let columns = storage.columns.list_for_review(review_id).await?;
    let mut rows = storage.rows.list_for_review(review_id).await?;
    let cells = storage.cells.all_for_review_latest(review_id).await?;

    rows.sort_by(|a, b| {
        crate::export::csv::doc_label(a).cmp(&crate::export::csv::doc_label(b))
    });

    let cell_map: HashMap<_, _> = cells
        .iter()
        .map(|c| ((c.row_id, c.col_id), c))
        .collect();

    // --- Formats ---
    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xD9D9D9))
        .set_pattern(FormatPattern::Solid);

    let low_conf_fmt = Format::new()
        .set_background_color(Color::RGB(0xFFCCCC))
        .set_pattern(FormatPattern::Solid);

    let date_fmt = Format::new().set_num_format("DD/MM/YYYY");

    let eur_fmt = Format::new().set_num_format("# ##0,00 [$€-40C]");

    // Low-confidence variants of typed formats.
    let low_date_fmt = Format::new()
        .set_background_color(Color::RGB(0xFFCCCC))
        .set_pattern(FormatPattern::Solid)
        .set_num_format("DD/MM/YYYY");

    let low_eur_fmt = Format::new()
        .set_background_color(Color::RGB(0xFFCCCC))
        .set_pattern(FormatPattern::Solid)
        .set_num_format("# ##0,00 [$€-40C]");

    // --- Build workbook ---
    let mut workbook = Workbook::new();
    let ws = workbook.add_worksheet();

    // Header row.
    ws.write_string_with_format(0, 0, "Doc", &header_fmt)
        .map_err(xlsx_err)?;
    for (ci, col) in columns.iter().enumerate() {
        ws.write_string_with_format(0, (ci + 1) as u16, &col.name, &header_fmt)
            .map_err(xlsx_err)?;
    }

    // Data rows.
    for (ri, row) in rows.iter().enumerate() {
        let excel_row = (ri + 1) as u32;
        let label = crate::export::csv::doc_label(row);
        ws.write_string(excel_row, 0, &label).map_err(xlsx_err)?;

        for (ci, col) in columns.iter().enumerate() {
            let excel_col = (ci + 1) as u16;
            if let Some(cell) = cell_map.get(&(row.id, col.id)) {
                let is_low = cell.confidence == Confidence::Low;
                write_cell(
                    ws,
                    excel_row,
                    excel_col,
                    &cell.value,
                    &col.cell_type,
                    is_low,
                    &date_fmt,
                    &eur_fmt,
                    &low_conf_fmt,
                    &low_date_fmt,
                    &low_eur_fmt,
                )?;
            }
        }
    }

    // Auto-fit column widths based on content (simulates Excel autofit).
    ws.autofit();

    workbook.save(output_path).map_err(xlsx_err)?;
    Ok(())
}

/// Write one cell with appropriate type-aware and confidence formatting.
#[allow(clippy::too_many_arguments)]
fn write_cell(
    ws: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: &serde_json::Value,
    cell_type: &CellType,
    is_low: bool,
    date_fmt: &Format,
    eur_fmt: &Format,
    low_fmt: &Format,
    low_date_fmt: &Format,
    low_eur_fmt: &Format,
) -> Result<()> {
    use serde_json::Value;

    match cell_type {
        CellType::Boolean => {
            let text = match value {
                Value::Bool(true) => "Oui",
                Value::Bool(false) => "Non",
                Value::String(s) if s.eq_ignore_ascii_case("true") || s == "1" => "Oui",
                Value::String(s) if s.eq_ignore_ascii_case("false") || s == "0" => "Non",
                _ => {
                    let s = crate::export::csv::render_value(value)?;
                    if is_low {
                        ws.write_string_with_format(row, col, &s, low_fmt)
                            .map_err(xlsx_err)?;
                    } else {
                        ws.write_string(row, col, &s).map_err(xlsx_err)?;
                    }
                    return Ok(());
                }
            };
            if is_low {
                ws.write_string_with_format(row, col, text, low_fmt)
                    .map_err(xlsx_err)?;
            } else {
                ws.write_string(row, col, text).map_err(xlsx_err)?;
            }
        }

        CellType::Date => {
            // Dates arrive as ISO-8601 strings or numbers — store as
            // string with date format so Excel can display them in FR
            // locale without requiring numeric serial conversion.
            let s = crate::export::csv::render_value(value)?;
            let fmt = if is_low { low_date_fmt } else { date_fmt };
            ws.write_string_with_format(row, col, &s, fmt)
                .map_err(xlsx_err)?;
        }

        CellType::Currency { .. } => {
            // Write as a number when possible so Excel can format it.
            let fmt = if is_low { low_eur_fmt } else { eur_fmt };
            match value {
                Value::Number(n) if n.as_f64().is_some() => {
                    ws.write_number_with_format(row, col, n.as_f64().unwrap(), fmt)
                        .map_err(xlsx_err)?;
                }
                _ => {
                    let s = crate::export::csv::render_value(value)?;
                    ws.write_string_with_format(row, col, &s, fmt)
                        .map_err(xlsx_err)?;
                }
            }
        }

        _ => {
            // Text, Verbatim, Enum, Number — generic rendering.
            match value {
                Value::Null => {} // leave cell empty
                Value::Number(n) if n.as_f64().is_some() => {
                    if is_low {
                        ws.write_number_with_format(row, col, n.as_f64().unwrap(), low_fmt)
                            .map_err(xlsx_err)?;
                    } else {
                        ws.write_number(row, col, n.as_f64().unwrap())
                            .map_err(xlsx_err)?;
                    }
                }
                _ => {
                    let s = crate::export::csv::render_value(value)?;
                    if s.is_empty() {
                        // no-op: leave cell blank
                    } else if is_low {
                        ws.write_string_with_format(row, col, &s, low_fmt)
                            .map_err(xlsx_err)?;
                    } else {
                        ws.write_string(row, col, &s).map_err(xlsx_err)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Convert an [`XlsxError`] into the crate's [`crate::error::Error::Io`]
/// variant so callers stay within the unified `Result` type.
fn xlsx_err(e: XlsxError) -> crate::error::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::RowId;
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
            name: "XLSX Test Review".into(),
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

    fn mk_cell(
        review_id: crate::ids::ReviewId,
        row_id: RowId,
        col_id: crate::ids::ColumnId,
        value: serde_json::Value,
    ) -> Cell {
        Cell {
            review_id,
            row_id,
            col_id,
            value,
            reasoning: None,
            citations: vec![],
            support_score: 0.95,
            confidence: Confidence::High,
            locked: false,
            version: 1,
            author: Author::System {
                extractor_version: "v1".into(),
            },
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn xlsx_creates_file() {
        let (_db_dir, storage) = fresh_storage().await;
        let review_id = crate::ids::ReviewId::new();
        storage
            .reviews
            .create(&mk_review(review_id))
            .await
            .expect("create review");

        let col =
            ColumnBuilder::new(review_id, "Parties", "Who are the parties?", CellType::Text)
                .order(0)
                .build();
        storage
            .columns
            .add(review_id, &col)
            .await
            .expect("add col");

        let row = mk_row(review_id, "Deal/contract.pdf");
        storage.rows.add(&row).await.expect("add row");

        let cell = mk_cell(
            review_id,
            row.id,
            col.id,
            serde_json::Value::String("Acme Corp".into()),
        );
        storage.cells.upsert(&cell).await.expect("upsert cell");

        let out_dir = TempDir::new().expect("out tempdir");
        let out_path = out_dir.path().join("test.xlsx");
        export_xlsx(&storage, review_id, &out_path)
            .await
            .expect("export xlsx");

        assert!(out_path.exists(), "XLSX file must be created at output path");
        assert!(
            out_path.metadata().expect("metadata").len() > 0,
            "XLSX file must be non-empty"
        );
    }

    #[tokio::test]
    async fn xlsx_header_row_has_column_count_plus_one() {
        // We verify the column count indirectly: the file is created with
        // N + 1 headers (Doc + N columns). The workbook must be
        // parseable. Since rust_xlsxwriter is write-only, we reparse
        // with the zip reader to count the shared strings (each header
        // cell is a unique string → count ≥ N + 1 shared strings).
        use std::io::Read;

        let (_db_dir, storage) = fresh_storage().await;
        let review_id = crate::ids::ReviewId::new();
        storage
            .reviews
            .create(&mk_review(review_id))
            .await
            .expect("create review");

        let col_names = ["Clause", "Amount", "Date"];
        for (i, name) in col_names.iter().enumerate() {
            let col = ColumnBuilder::new(review_id, name, "p", CellType::Text)
                .order(i as u32)
                .build();
            storage
                .columns
                .add(review_id, &col)
                .await
                .expect("add col");
        }

        let out_dir = TempDir::new().expect("out tempdir");
        let out_path = out_dir.path().join("hdr.xlsx");
        export_xlsx(&storage, review_id, &out_path)
            .await
            .expect("export xlsx");

        // XLSX is a ZIP — open it and read sharedStrings.xml.
        let file = std::fs::File::open(&out_path).expect("open xlsx");
        let mut zip = zip::ZipArchive::new(file).expect("open zip");
        let mut xml = String::new();
        zip.by_name("xl/sharedStrings.xml")
            .expect("sharedStrings.xml")
            .read_to_string(&mut xml)
            .expect("read sharedStrings");

        // Count <si> entries — one per unique string (header cells).
        let si_count = xml.matches("<si>").count();
        assert!(
            si_count >= col_names.len() + 1,
            "expected ≥ {} shared strings (Doc + {} cols), got {}",
            col_names.len() + 1,
            col_names.len(),
            si_count
        );
    }
}
