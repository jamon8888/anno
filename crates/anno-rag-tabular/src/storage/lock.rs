//! Locked-cell semantics.
//!
//! A `locked = true` cell version may only be overwritten by a
//! [`Author::Human`] write. Auto-overwrites from a [`Author::System`]
//! author are rejected with [`Error::LockedCell`] *before* anything is
//! appended to the Lance table — Lance is append-only, so a rejected
//! write must not produce a row.
//!
//! Two entry points live here:
//!
//! - [`check_lock_allows`] is the public API: it fetches the latest
//!   version itself and is the one external callers should use when
//!   they don't already hold the previous cell.
//! - [`deny_if_locked`] is the synchronous core, used internally by
//!   [`CellsTable::upsert`] which already had to fetch the previous
//!   version anyway. Re-using the same fetch keeps `upsert` from
//!   reading the table twice per write.

use crate::error::{Error, Result};
use crate::storage::cells::{Author, Cell, CellsTable};

/// Returns `Ok(())` if `incoming` may be appended to `table`, or
/// `Err(Error::LockedCell)` if the latest version of the same
/// `(review, row, col)` is locked and `incoming.author` is
/// [`Author::System`]. Human edits always pass.
///
/// Fetches the latest version itself; prefer [`deny_if_locked`] inside
/// `CellsTable::upsert` where the previous version is already in hand.
///
/// # Errors
///
/// - [`Error::LockedCell`] when a system author tries to overwrite a
///   locked cell.
/// - [`Error::Lance`] / [`Error::Arrow`] / [`Error::Json`] if the
///   underlying `latest()` query fails (propagated from
///   [`CellsTable::latest`]).
pub async fn check_lock_allows(table: &CellsTable, incoming: &Cell) -> Result<()> {
    let prev = table
        .latest(incoming.review_id, incoming.row_id, incoming.col_id)
        .await?;
    deny_if_locked(prev.as_ref(), incoming)
}

/// Synchronous core of the lock check. Given an optional previous
/// version and an incoming cell, decide whether the write is allowed.
///
/// Used inside [`CellsTable::upsert`] to avoid double-fetching the
/// latest version (the upsert path already needs it for the version+1
/// computation).
///
/// # Errors
///
/// Returns [`Error::LockedCell`] when `prev.locked` is true and
/// `incoming.author` is [`Author::System`].
pub(crate) fn deny_if_locked(prev: Option<&Cell>, incoming: &Cell) -> Result<()> {
    if let Some(p) = prev {
        if p.locked && matches!(incoming.author, Author::System { .. }) {
            return Err(Error::LockedCell {
                review: incoming.review_id.0.to_string(),
                row: incoming.row_id.0.to_string(),
                col: incoming.col_id.0.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ColumnId, ReviewId, RowId};
    use crate::storage::cells::{Citation, Confidence};
    use chrono::Utc;
    use std::sync::Arc;
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
            reasoning: None,
            citations: vec![Citation {
                chunk_id: uuid::Uuid::now_v7(),
                byte_start: 0,
                byte_end: 5,
                quoted_text: "hello".into(),
                page: None,
            }],
            support_score: 0.9,
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
    async fn check_lock_allows_blocks_system_overwrite_of_locked() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        let row = RowId::for_doc(r, uuid::Uuid::now_v7());
        let col = ColumnId::for_name(r, "governing_law");
        // Seed with a human-locked v1.
        t.upsert(&mk_cell(r, row, col, 1, true, human_alice()))
            .await
            .expect("v1 locked write");
        // System author trying to write v2 must be rejected.
        let incoming = mk_cell(r, row, col, 2, false, system_v1());
        let err = check_lock_allows(&t, &incoming)
            .await
            .expect_err("system overwrite must be rejected");
        assert!(matches!(err, Error::LockedCell { .. }), "got {err:?}");
    }

    #[tokio::test]
    async fn check_lock_allows_permits_human_overwrite_of_locked() {
        let (_dir, t) = fresh_table().await;
        let r = ReviewId::new();
        let row = RowId::for_doc(r, uuid::Uuid::now_v7());
        let col = ColumnId::for_name(r, "governing_law");
        t.upsert(&mk_cell(r, row, col, 1, true, human_alice()))
            .await
            .expect("v1 locked write");
        // Another human edit must pass.
        let incoming = mk_cell(r, row, col, 2, true, human_alice());
        check_lock_allows(&t, &incoming)
            .await
            .expect("human overwrite must pass");
    }
}
