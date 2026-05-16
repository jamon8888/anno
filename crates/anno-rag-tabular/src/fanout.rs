//! Per-review fan-out — turn one `ReviewId` into N concurrent
//! per-row extraction tasks, each writing its own cells back into the
//! `tabular_cells` table.
//!
//! ## Why a fan-out layer at all?
//!
//! [`crate::extract::Extractor::extract_doc`] handles **one** document.
//! A review almost always has many rows (one per document in scope),
//! and each row is independent: same column set, same extractor, but
//! a different `doc_id` and therefore a different chunk corpus. That
//! shape is the classic case for `tokio::spawn` + a [`Semaphore`] to
//! bound concurrency — fully parallel within a budget, no inter-task
//! coordination needed.
//!
//! ## Failure policy
//!
//! A single row's failure must not sink the entire review (a 200-doc
//! NDA review where one PDF is malformed should still finish the
//! other 199). We collect a [`RowOutcome`] per row so the caller can
//! surface partial failures in the UI. Only fatal *pre-fan-out*
//! errors — failure to list columns or rows — propagate as an outer
//! `Err`.
//!
//! ## Locked-cell handling
//!
//! [`crate::storage::cells::CellsTable::upsert`] rejects auto
//! (`Author::System`) overwrites of human-locked cells with
//! [`crate::error::Error::LockedCell`]. That's the *intended*
//! behaviour during re-extraction — the human edit must survive — so
//! this module **silently swallows** that specific error variant.
//! All other upsert failures abort the row and surface as `Err` in
//! that row's [`RowOutcome::result`].

use crate::error::{Error, Result};
use crate::extract::Extractor;
use crate::ids::{ReviewId, RowId};
use crate::schema::Column;
use crate::storage::cells::CellsTable;
use crate::storage::rows::Row;
use crate::storage::StorageHandle;
use std::sync::Arc;

/// Configuration for a fan-out run.
///
/// Defaults are tuned for an Anthropic-backed extractor against a
/// laptop-class workstation: 8 concurrent rows keeps the LLM
/// in-flight while staying well under the typical 50 req/s rate
/// limit, and the 80k token budget is the same default used by
/// [`crate::extract::Extractor`].
#[derive(Debug, Clone, Copy)]
pub struct FanoutConfig {
    /// Max concurrent extraction tasks. Default 8.
    pub max_concurrency: usize,
    /// Per-call LLM token budget passed through to
    /// [`Extractor::with_budget`].
    pub budget_tokens: usize,
}

impl Default for FanoutConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 8,
            budget_tokens: crate::extract::budget::DEFAULT_BUDGET_TOKENS,
        }
    }
}

/// Outcome of one row's extraction within a fan-out run.
///
/// `result` is `Ok(count_of_cells_upserted)` on success — that count
/// excludes any cells silently skipped because of a pre-existing
/// human lock (see module docs). On failure the boxed error is the
/// per-row error; the surrounding [`run_review`] call still returns
/// `Ok(outcomes)` so the caller sees the partial result.
#[derive(Debug)]
pub struct RowOutcome {
    /// Row whose extraction this outcome describes.
    pub row_id: RowId,
    /// Document the row pointed at (denormalised for caller
    /// ergonomics — avoids a second `RowsTable::get`).
    pub doc_id: uuid::Uuid,
    /// `Ok(n)` with the count of upserted cells (locked-cell skips
    /// don't count), or `Err` carrying the per-row failure.
    pub result: Result<usize>,
}

/// Run a full review extraction with bounded concurrency.
///
/// Per-row tasks fan out via [`tokio::spawn`] guarded by a
/// [`tokio::sync::Semaphore`] of size [`FanoutConfig::max_concurrency`].
/// Each task fetches the row's chunks via the extractor's
/// [`crate::extract::ChunkSource`], runs the LLM round-trip, and
/// upserts the resulting [`crate::storage::cells::Cell`]s.
///
/// Returns one [`RowOutcome`] per row in arbitrary order (spawn /
/// join ordering is not deterministic). If the upstream column or
/// row list is empty the run short-circuits with `Ok(vec![])` — no
/// tasks spawned, no LLM calls made.
///
/// # Errors
///
/// Returns `Err` only on fatal pre-fan-out failure:
/// - [`Error::Lance`] if listing columns or rows fails.
///
/// Per-row failures surface inside the returned `Vec<RowOutcome>`
/// rather than as the outer `Err`.
pub async fn run_review(
    storage: &StorageHandle,
    extractor: &Extractor,
    review_id: ReviewId,
    config: FanoutConfig,
) -> Result<Vec<RowOutcome>> {
    let columns = storage.columns.list_for_review(review_id).await?;
    let rows = storage.rows.list_for_review(review_id).await?;

    // Empty schema or empty grid → no work. Returning early keeps the
    // caller from having to special-case an empty Vec downstream.
    if columns.is_empty() || rows.is_empty() {
        return Ok(Vec::new());
    }

    // Wrap the shared per-run state in Arcs so each spawned task can
    // hold a cheap cloneable view without lifetime gymnastics. The
    // extractor's budget override is applied once here, not per row.
    let sem = Arc::new(tokio::sync::Semaphore::new(config.max_concurrency));
    let columns = Arc::new(columns);
    let extractor = Arc::new(extractor.clone().with_budget(config.budget_tokens));
    let cells_table = storage.cells.clone();

    let mut handles = Vec::with_capacity(rows.len());
    for row in rows {
        let sem = Arc::clone(&sem);
        let columns = Arc::clone(&columns);
        let extractor = Arc::clone(&extractor);
        let cells = cells_table.clone();
        let row_id = row.id;
        let doc_id = row.doc_id;
        let handle = tokio::spawn(async move {
            // `acquire_owned` so the permit can outlive the borrow on
            // `sem` — the permit is dropped at task end, releasing
            // the slot for the next queued row.
            let _permit = sem
                .acquire_owned()
                .await
                .expect("fanout semaphore must stay open for the lifetime of run_review");
            extract_and_upsert_one_row(&extractor, &cells, review_id, &row, &columns).await
        });
        handles.push((row_id, doc_id, handle));
    }

    let mut outcomes = Vec::with_capacity(handles.len());
    for (row_id, doc_id, handle) in handles {
        let result = match handle.await {
            Ok(inner) => inner,
            // A `JoinError` here means the task panicked or was
            // cancelled — wrap it as Extract so the caller has a
            // typed error to surface.
            Err(join_err) => Err(Error::Extract {
                doc: doc_id.to_string(),
                col: "?".into(),
                source: Box::new(join_err),
            }),
        };
        outcomes.push(RowOutcome {
            row_id,
            doc_id,
            result,
        });
    }
    Ok(outcomes)
}

/// Extract one row's cells and upsert them. Returns the count of
/// cells that actually landed in the table (locked-cell skips don't
/// count).
///
/// Kept private — callers should go through [`run_review`] so the
/// concurrency cap and outcome aggregation apply uniformly.
async fn extract_and_upsert_one_row(
    extractor: &Extractor,
    cells: &CellsTable,
    review_id: ReviewId,
    row: &Row,
    columns: &[Column],
) -> Result<usize> {
    let extracted = extractor.extract_doc(review_id, row.doc_id, columns).await?;

    // TODO(T28-T30): run verifier on each cell — set support_score
    // + confidence based on the cross-encoder support check before
    // upserting. Today every cell hits the table with the
    // placeholder `support_score = 0.0` / `Confidence::Medium`
    // assigned by `extract::batch`.

    let mut written = 0usize;
    for cell in extracted {
        match cells.upsert(&cell).await {
            Ok(()) => written += 1,
            // Locked cells are expected to survive re-extraction —
            // that's the whole point of the lock. Swallow only this
            // variant; anything else (Lance, Arrow, Json …) is a
            // real failure and aborts the row.
            Err(Error::LockedCell { .. }) => {}
            Err(other) => return Err(other),
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{ChunkRef, ChunkSource};
    use crate::ids::ColumnId;
    use crate::llm::{LlmClient, StructuredOutput, Usage};
    use crate::schema::column::ColumnBuilder;
    use crate::schema::CellType;
    use crate::storage::cells::{Author, Cell, Citation, Confidence};
    use crate::storage::rows::Row;
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use tempfile::TempDir;

    // ---- Test fixtures ----

    /// In-memory chunk source — identical shape to the one in
    /// `extract::tests`, duplicated here so the test module stays
    /// self-contained (the upstream one is private).
    struct InMemoryChunks {
        by_doc: HashMap<uuid::Uuid, Vec<ChunkRef>>,
    }

    #[async_trait]
    impl ChunkSource for InMemoryChunks {
        async fn chunks_for_doc(&self, doc_id: uuid::Uuid) -> Result<Vec<ChunkRef>> {
            Ok(self.by_doc.get(&doc_id).cloned().unwrap_or_default())
        }
    }

    /// Mock LLM that always returns the same envelope for the two
    /// `col_a` / `col_b` columns the tests stage. Keeps the test
    /// fixture small — production extraction logic is exercised in
    /// `extract::tests`.
    struct StubLlm {
        chunk_id: uuid::Uuid,
        /// Optional counter incremented on every call. Used by the
        /// concurrency test to observe the in-flight count.
        calls: Arc<AtomicUsize>,
        /// Optional delay inserted at the start of each call to make
        /// the concurrency cap observable in wall time.
        delay_ms: u64,
    }

    #[async_trait]
    impl LlmClient for StubLlm {
        async fn generate_structured(
            &self,
            _system: &str,
            _user: &str,
            _json_schema: &Value,
        ) -> Result<StructuredOutput> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
            }
            let v = json!({
                "col_a": {
                    "value": "A",
                    "reasoning": "stub",
                    "citations": [{
                        "chunk_id": self.chunk_id.to_string(),
                        "char_start": 0,
                        "char_end": 1,
                        "quoted_text": "G"
                    }]
                },
                "col_b": {
                    "value": "B",
                    "reasoning": "stub",
                    "citations": [{
                        "chunk_id": self.chunk_id.to_string(),
                        "char_start": 0,
                        "char_end": 1,
                        "quoted_text": "G"
                    }]
                }
            });
            Ok(StructuredOutput {
                value: v,
                usage: Usage::default(),
            })
        }

        fn model_id(&self) -> &str {
            "stub"
        }
    }

    /// LLM that errors for a specific doc_id (matched by chunk
    /// content) and otherwise returns a normal envelope. Used by
    /// the per-row failure test.
    struct FailingLlm {
        good_chunk_id: uuid::Uuid,
        /// Substring that, when found in the user prompt, triggers
        /// an error response.
        poison_marker: String,
    }

    #[async_trait]
    impl LlmClient for FailingLlm {
        async fn generate_structured(
            &self,
            _system: &str,
            user: &str,
            _json_schema: &Value,
        ) -> Result<StructuredOutput> {
            if user.contains(&self.poison_marker) {
                return Err(Error::Extract {
                    doc: "poisoned".into(),
                    col: "col_a".into(),
                    source: "stub-failure".into(),
                });
            }
            Ok(StructuredOutput {
                value: json!({
                    "col_a": {
                        "value": "A",
                        "reasoning": "stub",
                        "citations": [{
                            "chunk_id": self.good_chunk_id.to_string(),
                            "char_start": 0,
                            "char_end": 1,
                            "quoted_text": "G"
                        }]
                    },
                    "col_b": {
                        "value": "B",
                        "reasoning": "stub",
                        "citations": [{
                            "chunk_id": self.good_chunk_id.to_string(),
                            "char_start": 0,
                            "char_end": 1,
                            "quoted_text": "G"
                        }]
                    }
                }),
                usage: Usage::default(),
            })
        }

        fn model_id(&self) -> &str {
            "failing"
        }
    }

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

    /// Seed N rows + 2 columns for a review. Returns (review, rows,
    /// chunk_ids per row).
    async fn seed_review(
        storage: &StorageHandle,
        n_rows: usize,
    ) -> (ReviewId, Vec<Row>, HashMap<uuid::Uuid, Vec<ChunkRef>>) {
        let review = ReviewId::new();
        for (i, name) in ["col_a", "col_b"].iter().enumerate() {
            let mut col =
                ColumnBuilder::new(review, name, &format!("Q for {name}?"), CellType::Text)
                    .build();
            col.order = i as u32;
            storage
                .columns
                .add(review, &col)
                .await
                .expect("add column");
        }
        let mut rows = Vec::new();
        let mut chunk_map: HashMap<uuid::Uuid, Vec<ChunkRef>> = HashMap::new();
        for _ in 0..n_rows {
            let doc = uuid::Uuid::now_v7();
            let chunk_id = uuid::Uuid::now_v7();
            chunk_map.insert(
                doc,
                vec![ChunkRef {
                    id: chunk_id,
                    doc_id: doc,
                    content: "Governing law: France. Term: 24 months.".into(),
                    page: Some(1),
                }],
            );
            let row = Row {
                id: RowId::for_doc(review, doc),
                review_id: review,
                doc_id: doc,
                folder_path: None,
                created_at: Utc::now(),
            };
            storage.rows.add(&row).await.expect("add row");
            rows.push(row);
        }
        (review, rows, chunk_map)
    }

    /// First chunk_id encountered in the map — handy for stubs that
    /// only need *some* valid chunk id to cite.
    fn any_chunk_id(map: &HashMap<uuid::Uuid, Vec<ChunkRef>>) -> uuid::Uuid {
        map.values()
            .next()
            .and_then(|v| v.first())
            .map(|c| c.id)
            .expect("at least one chunk seeded")
    }

    // ---- Tests ----

    #[tokio::test]
    async fn fanout_extracts_all_rows() {
        let (_dir, storage) = fresh_storage().await;
        let (review, rows, chunk_map) = seed_review(&storage, 3).await;

        let llm = Arc::new(StubLlm {
            chunk_id: any_chunk_id(&chunk_map),
            calls: Arc::new(AtomicUsize::new(0)),
            delay_ms: 0,
        });
        let chunks = Arc::new(InMemoryChunks { by_doc: chunk_map });
        let extractor = Extractor::new(llm, chunks);

        let outcomes = run_review(&storage, &extractor, review, FanoutConfig::default())
            .await
            .expect("run_review succeeds");
        assert_eq!(outcomes.len(), 3, "one outcome per row");
        for o in &outcomes {
            let n = o.result.as_ref().expect("row succeeded");
            assert_eq!(*n, 2, "two columns per row");
        }
        let cells = storage
            .cells
            .all_for_review_latest(review)
            .await
            .expect("latest cells");
        assert_eq!(cells.len(), rows.len() * 2);
    }

    #[tokio::test]
    async fn fanout_with_empty_columns_returns_empty() {
        let (_dir, storage) = fresh_storage().await;
        // No columns added, no rows added.
        let review = ReviewId::new();
        let llm = Arc::new(StubLlm {
            chunk_id: uuid::Uuid::now_v7(),
            calls: Arc::new(AtomicUsize::new(0)),
            delay_ms: 0,
        });
        let chunks = Arc::new(InMemoryChunks {
            by_doc: HashMap::new(),
        });
        let extractor = Extractor::new(llm.clone(), chunks);

        let outcomes = run_review(&storage, &extractor, review, FanoutConfig::default())
            .await
            .expect("run_review succeeds");
        assert!(outcomes.is_empty(), "no columns → no outcomes");
        assert_eq!(
            llm.calls.load(Ordering::SeqCst),
            0,
            "no LLM calls when grid is empty"
        );
    }

    #[tokio::test]
    async fn fanout_respects_max_concurrency() {
        // 4 rows, max_concurrency=2, 60ms per LLM call → at least
        // 2 waves of 60ms = 120ms wall time. The bound is loose on
        // purpose — Windows + tokio scheduling can add jitter, but
        // a fully-serial run would be ≥240ms and a fully-parallel
        // run would be ≥60ms, so 100ms cleanly distinguishes the
        // "bounded at 2" case.
        let (_dir, storage) = fresh_storage().await;
        let (review, _rows, chunk_map) = seed_review(&storage, 4).await;

        let calls = Arc::new(AtomicUsize::new(0));
        let llm = Arc::new(StubLlm {
            chunk_id: any_chunk_id(&chunk_map),
            calls: Arc::clone(&calls),
            delay_ms: 60,
        });
        let chunks = Arc::new(InMemoryChunks { by_doc: chunk_map });
        let extractor = Extractor::new(llm, chunks);

        let cfg = FanoutConfig {
            max_concurrency: 2,
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let outcomes = run_review(&storage, &extractor, review, cfg)
            .await
            .expect("run_review succeeds");
        let elapsed = start.elapsed();

        assert_eq!(outcomes.len(), 4);
        assert_eq!(calls.load(Ordering::SeqCst), 4, "every row hit the LLM");
        assert!(
            elapsed >= std::time::Duration::from_millis(100),
            "expected ≥100ms wall time with 4 rows / max_concurrency=2 / 60ms per call, got {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn fanout_continues_when_one_row_fails() {
        let (_dir, storage) = fresh_storage().await;
        let (review, rows, mut chunk_map) = seed_review(&storage, 3).await;

        // Mark row 1's chunk content with a "POISON" marker the
        // failing LLM will detect and reject. The other two rows
        // keep the normal content.
        let poison_doc = rows[1].doc_id;
        chunk_map.get_mut(&poison_doc).expect("doc seeded")[0].content =
            "POISON DO NOT EAT".into();

        let good_chunk_id = chunk_map
            .iter()
            .filter(|(d, _)| **d != poison_doc)
            .next()
            .and_then(|(_, v)| v.first())
            .map(|c| c.id)
            .expect("a non-poisoned chunk");

        let llm = Arc::new(FailingLlm {
            good_chunk_id,
            poison_marker: "POISON".into(),
        });
        let chunks = Arc::new(InMemoryChunks { by_doc: chunk_map });
        let extractor = Extractor::new(llm, chunks);

        let outcomes = run_review(&storage, &extractor, review, FanoutConfig::default())
            .await
            .expect("run_review succeeds overall");
        assert_eq!(outcomes.len(), 3);

        let mut ok_count = 0;
        let mut err_count = 0;
        for o in &outcomes {
            if o.doc_id == poison_doc {
                assert!(o.result.is_err(), "poisoned row must fail");
                err_count += 1;
            } else {
                assert!(o.result.is_ok(), "non-poisoned row must succeed");
                ok_count += 1;
            }
        }
        assert_eq!(ok_count, 2);
        assert_eq!(err_count, 1);

        // The 2 successful rows × 2 cols = 4 cells should be in the
        // table; the failed row contributed nothing.
        let cells = storage
            .cells
            .all_for_review_latest(review)
            .await
            .expect("latest cells");
        assert_eq!(cells.len(), 4);
    }

    #[tokio::test]
    async fn fanout_silently_skips_locked_cells() {
        let (_dir, storage) = fresh_storage().await;
        let (review, rows, chunk_map) = seed_review(&storage, 2).await;

        // Pre-seed a human-locked cell on (row_0, col_a). The
        // fan-out's System-authored upsert must hit
        // Error::LockedCell, be swallowed, and leave the original
        // value intact. The other 3 cells (row_0/col_b, row_1/col_a,
        // row_1/col_b) get upserted normally.
        let locked_col = ColumnId::for_name(review, "col_a");
        let locked_cell = Cell {
            review_id: review,
            row_id: rows[0].id,
            col_id: locked_col,
            value: json!("HUMAN_LOCKED_VALUE"),
            reasoning: Some("set by reviewer".into()),
            citations: vec![Citation {
                chunk_id: uuid::Uuid::now_v7(),
                char_start: 0,
                char_end: 5,
                quoted_text: "hello".into(),
                page: None,
            }],
            support_score: 1.0,
            confidence: Confidence::High,
            locked: true,
            version: 1,
            author: Author::Human {
                user_id: "alice".into(),
            },
            updated_at: Utc::now(),
        };
        storage
            .cells
            .upsert(&locked_cell)
            .await
            .expect("seed locked cell");

        let llm = Arc::new(StubLlm {
            chunk_id: any_chunk_id(&chunk_map),
            calls: Arc::new(AtomicUsize::new(0)),
            delay_ms: 0,
        });
        let chunks = Arc::new(InMemoryChunks { by_doc: chunk_map });
        let extractor = Extractor::new(llm, chunks);

        let outcomes = run_review(&storage, &extractor, review, FanoutConfig::default())
            .await
            .expect("run_review succeeds");
        assert_eq!(outcomes.len(), 2);

        // Per-row counts: row 0 wrote 1 cell (col_b) and skipped
        // col_a (locked); row 1 wrote both.
        let by_row: HashMap<RowId, usize> = outcomes
            .iter()
            .map(|o| (o.row_id, *o.result.as_ref().expect("row succeeded")))
            .collect();
        assert_eq!(by_row[&rows[0].id], 1, "locked col_a skipped on row 0");
        assert_eq!(by_row[&rows[1].id], 2, "row 1 unaffected");

        // The locked value must be the latest version on (row_0,
        // col_a). The extraction would have written "A"; it must
        // not be there.
        let latest = storage
            .cells
            .latest(review, rows[0].id, locked_col)
            .await
            .expect("latest")
            .expect("locked cell exists");
        assert_eq!(latest.value, json!("HUMAN_LOCKED_VALUE"));
        assert!(latest.locked);
    }

    // Silences `dead_code` on `Mutex` if no test in the future
    // happens to use it; kept around because it was the original
    // mechanism considered for the concurrency observer before
    // `AtomicUsize` proved sufficient.
    #[allow(dead_code)]
    fn _phantom_mutex() -> Mutex<()> {
        Mutex::new(())
    }
}
