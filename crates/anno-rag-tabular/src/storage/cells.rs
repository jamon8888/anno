//! `tabular_cells` table — append-only versioned cell storage.
//!
//! Cells are immutable: every re-extraction or human edit writes a new
//! row with `version = previous + 1`. `locked = true` blocks the
//! auto-overwrite path: a `System`-authored upsert on a previously
//! locked cell returns [`Error::LockedCell`]; only a `Human`-authored
//! upsert may pass.
//!
//! LanceDB has no native `ORDER BY x DESC LIMIT 1`, so `latest` /
//! `all_for_review_latest` pull all matching rows and pick the max
//! `version` client-side. Fast enough for v1 (cell counts are in the
//! thousands, not millions) and keeps the query path straightforward.

use crate::error::{Error, Result};
use crate::ids::{ColumnId, ReviewId, RowId};
use crate::storage::arrow_schema::cells_schema;
use crate::storage::util::{opt_str, uuid_to_filter_lit};
use arrow::error::ArrowError;
use arrow_array::{
    BooleanArray, FixedSizeBinaryArray, Float32Array, RecordBatch, RecordBatchIterator,
    StringArray, TimestampMicrosecondArray, UInt32Array,
};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Physical Lance table name. The `tabular_` prefix keeps tabular-review
/// state from colliding with the v1.0 chunks / memories tables that
/// share the same dataset directory.
pub const TABLE_NAME: &str = "tabular_cells";

/// Per-cell verifier confidence bucket. Encoded as the variant name
/// (`"High"|"Medium"|"Low"`) into the `confidence` Utf8 column — a tiny
/// hand-rolled encode/decode pair is cheaper here than dragging
/// `serde_json` through one of three constant strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    /// Cross-encoder support score above the "high" threshold.
    High,
    /// Score in the mid band — usable but worth flagging in review UI.
    Medium,
    /// Below threshold — the engine kept the cell but the verifier was
    /// not happy. Surface for human inspection.
    Low,
}

impl Confidence {
    fn as_str(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

/// Author of a cell version. The `system:v…` / `human:<user_id>` Utf8
/// encoding lives in [`Author::encode`] / [`Author::decode`]; serde is
/// not used because the on-disk form is a single colon-split string,
/// not JSON — round-tripping through serde_json would just add `"\""`
/// noise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Author {
    /// Extracted automatically. `extractor_version` is the pipeline
    /// version (e.g. `"v1"`) so re-extractions with a newer engine can
    /// be told apart in history.
    System {
        /// Extractor pipeline version string.
        extractor_version: String,
    },
    /// Human edit. `user_id` is whatever opaque identifier the host app
    /// chooses (anno-rag is content-agnostic about user identity).
    Human {
        /// Opaque user identifier from the host application.
        user_id: String,
    },
}

impl Author {
    fn encode(&self) -> String {
        match self {
            Self::System { extractor_version } => format!("system:{extractor_version}"),
            Self::Human { user_id } => format!("human:{user_id}"),
        }
    }

    fn decode(s: &str) -> Result<Self> {
        // `split_once` keeps the value half intact even if it contains
        // further colons (e.g. a future "human:tenant:user" shape).
        let (kind, rest) = s.split_once(':').ok_or_else(|| {
            use serde::de::Error as _;
            Error::Json(serde_json::Error::custom(format!(
                "author '{s}' missing ':' separator"
            )))
        })?;
        match kind {
            "system" => Ok(Self::System {
                extractor_version: rest.to_string(),
            }),
            "human" => Ok(Self::Human {
                user_id: rest.to_string(),
            }),
            other => {
                use serde::de::Error as _;
                Err(Error::Json(serde_json::Error::custom(format!(
                    "unknown author kind '{other}'"
                ))))
            }
        }
    }
}

/// Single citation supporting a cell value — chunk reference plus the
/// exact character range and quoted text. Persisted as part of the
/// `citations_json` Utf8 column (entire `Vec<Citation>` serialised
/// together — one JSON write per cell version, not per citation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// Chunk this citation points into. Raw [`uuid::Uuid`] — the
    /// workspace has no separate `ChunkId` newtype.
    pub chunk_id: uuid::Uuid,
    /// Inclusive start offset within the chunk's text.
    pub byte_start: u32,
    /// Exclusive end offset within the chunk's text.
    pub byte_end: u32,
    /// Verbatim quoted text. Stored alongside offsets so the verifier
    /// can detect chunk text drift between extract-time and review-time.
    pub quoted_text: String,
    /// Page number for paginated sources; `None` for free-form text.
    pub page: Option<u32>,
}

/// One **version** of a `(review, row, col)` cell. Append-only: re-
/// extractions and human edits both produce new rows; the version
/// counter is the caller's responsibility (see
/// [`CellsTable::upsert`] docs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    /// Owning review.
    pub review_id: ReviewId,
    /// Owning row.
    pub row_id: RowId,
    /// Owning column.
    pub col_id: ColumnId,
    /// Extracted/edited value. Free-form JSON so the schema doesn't
    /// have to change when [`crate::schema::CellType`] gains a variant.
    pub value: serde_json::Value,
    /// Optional model reasoning trace for audit.
    pub reasoning: Option<String>,
    /// Supporting citations (may be empty for manual / non-grounded cells).
    pub citations: Vec<Citation>,
    /// Cross-encoder verifier score (0.0–1.0).
    pub support_score: f32,
    /// Confidence bucket derived from `support_score`.
    pub confidence: Confidence,
    /// `true` when a human has locked the cell — auto-overwrites by a
    /// `System` author will be rejected.
    pub locked: bool,
    /// 1-based version counter. Caller picks `previous_latest + 1`.
    pub version: u32,
    /// Who wrote this version.
    pub author: Author,
    /// Write timestamp, microsecond UTC.
    pub updated_at: DateTime<Utc>,
}

/// Handle to the `tabular_cells` LanceDB table. Cheap to clone —
/// [`lancedb::Table`] is itself a reference-counted handle.
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

    /// Append a new cell version. **Caller picks `version = last + 1`**
    /// — this method does not auto-increment, by design: the engine
    /// often batches version selection with other writes and a silent
    /// re-read here would race those.
    ///
    /// Locked-cell semantics: if the latest existing version is locked
    /// AND the incoming author is [`Author::System`], the write is
    /// rejected with [`Error::LockedCell`]. Human edits always pass.
    ///
    /// # Errors
    ///
    /// - [`Error::LockedCell`] on auto-overwrite of a locked cell.
    /// - [`Error::Json`] if value/citations fail to serialise (only on
    ///   a buggy `Serialize` impl).
    /// - [`Error::Arrow`] if the batch fails to build.
    /// - [`Error::Lance`] on table read or write failure.
    pub async fn upsert(&self, cell: &Cell) -> Result<()> {
        // Check the lock *before* writing — Lance is append-only so a
        // rejected write must not produce a row. We fetch `latest` once
        // here (we'll typically need it for the caller's `version + 1`
        // computation too) and hand it to the sync core of the lock
        // check so the lock layer doesn't re-read the table.
        let prev = self
            .latest(cell.review_id, cell.row_id, cell.col_id)
            .await?;
        crate::storage::lock::deny_if_locked(prev.as_ref(), cell)?;

        let schema = cells_schema();
        let rid_arr = FixedSizeBinaryArray::try_from_iter(std::iter::once(
            cell.review_id.0.as_bytes().to_vec(),
        ))?;
        let row_arr = FixedSizeBinaryArray::try_from_iter(std::iter::once(
            cell.row_id.0.as_bytes().to_vec(),
        ))?;
        let col_arr = FixedSizeBinaryArray::try_from_iter(std::iter::once(
            cell.col_id.0.as_bytes().to_vec(),
        ))?;
        let value_arr = StringArray::from(vec![serde_json::to_string(&cell.value)?]);
        let reasoning_arr = StringArray::from(vec![cell.reasoning.clone()]);
        // `citations_json` is non-null in the schema; serialise the
        // whole Vec into one JSON array so the cost is one parse per
        // cell version, not per citation.
        let citations_arr = StringArray::from(vec![serde_json::to_string(&cell.citations)?]);
        let support_arr = Float32Array::from(vec![cell.support_score]);
        let conf_arr = StringArray::from(vec![cell.confidence.as_str().to_string()]);
        let locked_arr = BooleanArray::from(vec![cell.locked]);
        let ver_arr = UInt32Array::from(vec![cell.version]);
        let author_arr = StringArray::from(vec![cell.author.encode()]);
        let ts_arr = TimestampMicrosecondArray::from(vec![cell.updated_at.timestamp_micros()]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(rid_arr),
                Arc::new(row_arr),
                Arc::new(col_arr),
                Arc::new(value_arr),
                Arc::new(reasoning_arr),
                Arc::new(citations_arr),
                Arc::new(support_arr),
                Arc::new(conf_arr),
                Arc::new(locked_arr),
                Arc::new(ver_arr),
                Arc::new(author_arr),
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

    /// Return the highest-`version` row for `(review, row, col)`, or
    /// `None` if no version exists yet.
    ///
    /// LanceDB has no native `ORDER BY … DESC LIMIT 1`; we pull all
    /// matching rows and pick the max client-side. Per-cell history
    /// stays bounded (handful of versions), so the scan is cheap.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Lance`] on query failure, [`Error::Arrow`] /
    /// [`Error::Json`] on decode failure.
    pub async fn latest(
        &self,
        review: ReviewId,
        row: RowId,
        col: ColumnId,
    ) -> Result<Option<Cell>> {
        let rows = self.history(review, row, col).await?;
        // `history` is descending; the first element is the latest.
        Ok(rows.into_iter().next())
    }

    /// Return every version for `(review, row, col)`, **descending by
    /// `version`** (latest first).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Lance`] on query failure, [`Error::Arrow`] /
    /// [`Error::Json`] on decode failure.
    pub async fn history(&self, review: ReviewId, row: RowId, col: ColumnId) -> Result<Vec<Cell>> {
        let rh = uuid_to_filter_lit(review.0);
        let row_h = uuid_to_filter_lit(row.0);
        let col_h = uuid_to_filter_lit(col.0);
        let stream = self
            .tbl
            .query()
            .only_if(format!(
                "review_id = X'{rh}' AND row_id = X'{row_h}' AND col_id = X'{col_h}'"
            ))
            .execute()
            .await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        let mut out = Vec::new();
        for b in batches {
            for i in 0..b.num_rows() {
                out.push(row_to_cell(&b, i)?);
            }
        }
        out.sort_by_key(|b| std::cmp::Reverse(b.version));
        Ok(out)
    }

    /// Return the latest version of every cell in `review`, one entry
    /// per `(row_id, col_id)` pair.
    ///
    /// Pulls every matching row and keeps the max-version per
    /// `(row, col)` group in a `HashMap`. v1 cell counts are small
    /// enough that this is fine; if a later phase grows beyond
    /// ~100k cells per review we can revisit.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Lance`] on query failure, [`Error::Arrow`] /
    /// [`Error::Json`] on decode failure.
    pub async fn all_for_review_latest(&self, review: ReviewId) -> Result<Vec<Cell>> {
        let rh = uuid_to_filter_lit(review.0);
        let stream = self
            .tbl
            .query()
            .only_if(format!("review_id = X'{rh}'"))
            .execute()
            .await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        let mut best: HashMap<(RowId, ColumnId), Cell> = HashMap::new();
        for b in batches {
            for i in 0..b.num_rows() {
                let c = row_to_cell(&b, i)?;
                let key = (c.row_id, c.col_id);
                match best.get(&key) {
                    Some(prev) if prev.version >= c.version => {}
                    _ => {
                        best.insert(key, c);
                    }
                }
            }
        }
        Ok(best.into_values().collect())
    }
}

/// Decode row `i` of a `tabular_cells` batch into a [`Cell`].
///
/// Downcasts use `.expect` because the schema is fixed by
/// [`cells_schema`] — if a column has the wrong physical type at this
/// point, the schema and decoder are out of sync and panicking is the
/// honest signal. Json/author decodes go through `?` because *those*
/// really can fail at runtime if a row was written by an older crate
/// version with an incompatible serde shape.
fn row_to_cell(b: &RecordBatch, i: usize) -> Result<Cell> {
    let rid_a = b
        .column(0)
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .expect("column 0 (review_id) is FixedSizeBinaryArray by schema");
    let row_a = b
        .column(1)
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .expect("column 1 (row_id) is FixedSizeBinaryArray by schema");
    let col_a = b
        .column(2)
        .as_any()
        .downcast_ref::<FixedSizeBinaryArray>()
        .expect("column 2 (col_id) is FixedSizeBinaryArray by schema");
    let val_a = b
        .column(3)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 3 (value_json) is StringArray by schema");
    let reasoning_a = b
        .column(4)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 4 (reasoning) is StringArray by schema");
    let cites_a = b
        .column(5)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 5 (citations_json) is StringArray by schema");
    let support_a = b
        .column(6)
        .as_any()
        .downcast_ref::<Float32Array>()
        .expect("column 6 (support_score) is Float32Array by schema");
    let conf_a = b
        .column(7)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 7 (confidence) is StringArray by schema");
    let locked_a = b
        .column(8)
        .as_any()
        .downcast_ref::<BooleanArray>()
        .expect("column 8 (locked) is BooleanArray by schema");
    let ver_a = b
        .column(9)
        .as_any()
        .downcast_ref::<UInt32Array>()
        .expect("column 9 (version) is UInt32Array by schema");
    let author_a = b
        .column(10)
        .as_any()
        .downcast_ref::<StringArray>()
        .expect("column 10 (author) is StringArray by schema");
    let ts_a = b
        .column(11)
        .as_any()
        .downcast_ref::<TimestampMicrosecondArray>()
        .expect("column 11 (updated_at) is TimestampMicrosecondArray by schema");

    let rid_bytes: [u8; 16] = rid_a
        .value(i)
        .try_into()
        .expect("review_id column is FixedSizeBinary(16) by schema");
    let row_bytes: [u8; 16] = row_a
        .value(i)
        .try_into()
        .expect("row_id column is FixedSizeBinary(16) by schema");
    let col_bytes: [u8; 16] = col_a
        .value(i)
        .try_into()
        .expect("col_id column is FixedSizeBinary(16) by schema");

    let confidence = match conf_a.value(i) {
        "High" => Confidence::High,
        "Medium" => Confidence::Medium,
        "Low" => Confidence::Low,
        other => {
            use serde::de::Error as _;
            return Err(Error::Json(serde_json::Error::custom(format!(
                "unknown confidence '{other}'"
            ))));
        }
    };

    Ok(Cell {
        review_id: ReviewId(uuid::Uuid::from_bytes(rid_bytes)),
        row_id: RowId(uuid::Uuid::from_bytes(row_bytes)),
        col_id: ColumnId(uuid::Uuid::from_bytes(col_bytes)),
        value: serde_json::from_str(val_a.value(i))?,
        reasoning: opt_str(reasoning_a, i),
        citations: serde_json::from_str(cites_a.value(i))?,
        support_score: support_a.value(i),
        confidence,
        locked: locked_a.value(i),
        version: ver_a.value(i),
        author: Author::decode(author_a.value(i))?,
        updated_at: DateTime::<Utc>::from_timestamp_micros(ts_a.value(i)).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn fresh_table() -> (TempDir, CellsTable) {
        let dir = TempDir::new().expect("tempdir");
        let conn = Arc::new(
            lancedb::connect(dir.path().to_str().expect("utf8 path"))
                .execute()
                .await
                .expect("lancedb connect"),
        );
        let t = CellsTable::open(conn).await.expect("open cells");
        (dir, t)
    }

    fn mk_cell(
        review: ReviewId,
        row: RowId,
        col: ColumnId,
        version: u32,
        locked: bool,
        author: Author,
    ) -> Cell {
        Cell {
            review_id: review,
            row_id: row,
            col_id: col,
            value: serde_json::json!({"text": format!("v{version}")}),
            reasoning: Some(format!("because v{version}")),
            citations: vec![Citation {
                chunk_id: uuid::Uuid::now_v7(),
                byte_start: 0,
                byte_end: 10,
                quoted_text: "hello, sir".into(),
                page: Some(1),
            }],
            support_score: 0.91,
            confidence: Confidence::High,
            locked,
            version,
            author,
            updated_at: Utc::now(),
        }
    }

    fn system_v1() -> Author {
        Author::System {
            extractor_version: "v1".into(),
        }
    }

    fn human_alice() -> Author {
        Author::Human {
            user_id: "alice".into(),
        }
    }

    #[tokio::test]
    async fn upsert_increments_version_when_history_already_exists() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        let row = RowId::for_doc(r, uuid::Uuid::now_v7());
        let col = ColumnId::for_name(r, "governing_law");
        t.upsert(&mk_cell(r, row, col, 1, false, system_v1()))
            .await
            .expect("v1 write");
        // Caller does the +1 — that's the contract.
        t.upsert(&mk_cell(r, row, col, 2, false, system_v1()))
            .await
            .expect("v2 write");
        let latest = t.latest(r, row, col).await.expect("latest").expect("some");
        assert_eq!(latest.version, 2);
    }

    #[tokio::test]
    async fn locked_cell_cannot_be_overwritten_by_system_author() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        let row = RowId::for_doc(r, uuid::Uuid::now_v7());
        let col = ColumnId::for_name(r, "governing_law");
        // Human locks v1.
        t.upsert(&mk_cell(r, row, col, 1, true, human_alice()))
            .await
            .expect("v1 locked write");
        // System tries to overwrite → must be rejected.
        let err = t
            .upsert(&mk_cell(r, row, col, 2, false, system_v1()))
            .await
            .expect_err("system overwrite must be rejected");
        assert!(matches!(err, Error::LockedCell { .. }), "got {err:?}");
    }

    #[tokio::test]
    async fn locked_cell_can_be_overwritten_by_human_author() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        let row = RowId::for_doc(r, uuid::Uuid::now_v7());
        let col = ColumnId::for_name(r, "governing_law");
        t.upsert(&mk_cell(r, row, col, 1, true, human_alice()))
            .await
            .expect("v1 locked write");
        // Another human edit must pass.
        t.upsert(&mk_cell(r, row, col, 2, true, human_alice()))
            .await
            .expect("human overwrite must pass");
        let latest = t.latest(r, row, col).await.expect("latest").expect("some");
        assert_eq!(latest.version, 2);
    }

    #[tokio::test]
    async fn history_returns_all_versions_descending() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        let row = RowId::for_doc(r, uuid::Uuid::now_v7());
        let col = ColumnId::for_name(r, "governing_law");
        for v in 1..=3 {
            t.upsert(&mk_cell(r, row, col, v, false, system_v1()))
                .await
                .expect("write");
        }
        let h = t.history(r, row, col).await.expect("history");
        let versions: Vec<u32> = h.iter().map(|c| c.version).collect();
        assert_eq!(versions, vec![3, 2, 1]);
    }

    #[tokio::test]
    async fn all_for_review_latest_returns_one_per_row_col() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        let row_a = RowId::for_doc(r, uuid::Uuid::now_v7());
        let row_b = RowId::for_doc(r, uuid::Uuid::now_v7());
        let col_x = ColumnId::for_name(r, "x");
        let col_y = ColumnId::for_name(r, "y");
        // Two rows × two cols = four (row, col) pairs; multiple
        // versions per pair must collapse to one entry.
        for (row, col) in [
            (row_a, col_x),
            (row_a, col_y),
            (row_b, col_x),
            (row_b, col_y),
        ] {
            t.upsert(&mk_cell(r, row, col, 1, false, system_v1()))
                .await
                .expect("v1");
            t.upsert(&mk_cell(r, row, col, 2, false, system_v1()))
                .await
                .expect("v2");
        }
        let latest = t.all_for_review_latest(r).await.expect("latest");
        assert_eq!(latest.len(), 4);
        assert!(
            latest.iter().all(|c| c.version == 2),
            "every (row, col) pair should resolve to v2"
        );
    }
}
