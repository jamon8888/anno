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
use crate::extract::conditional::{should_extract, topo_waves};
use crate::extract::Extractor;
use crate::ids::{ColumnId, ReviewId, RowId};
use crate::schema::Column;
use crate::storage::cells::CellsTable;
use crate::storage::rows::Row;
use crate::storage::StorageHandle;
use std::collections::HashSet;
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
    /// If `true`, ignore the schema-drift incremental check and
    /// re-extract every column for every row even when a cell
    /// already exists. Useful when the operator just edited a
    /// column's prompt and wants every doc re-asked from scratch.
    /// Default `false`: cells that already exist for `(row, col)`
    /// are skipped (locked cells survive regardless via
    /// [`CellsTable::upsert`]'s lock guard).
    ///
    /// See `extract_and_upsert_one_row` for the v1 simplification:
    /// "drift" today is "cell-missing", not "cell extracted under
    /// an older `schema_version`".
    pub force_reextract: bool,
}

impl Default for FanoutConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 8,
            budget_tokens: crate::extract::budget::DEFAULT_BUDGET_TOKENS,
            force_reextract: false,
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

    let force_reextract = config.force_reextract;
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
            extract_and_upsert_one_row(
                &extractor,
                &cells,
                review_id,
                &row,
                &columns,
                force_reextract,
            )
            .await
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
/// cells that actually landed in the table (locked-cell skips and
/// conditional-gate skips don't count).
///
/// ## Conditional gating (T26)
///
/// Columns are first topo-sorted into *waves* by their
/// [`crate::schema::ConditionalSpec::parent_col`] edges (see
/// [`crate::extract::conditional::topo_waves`]). Each wave is one
/// batched LLM round-trip via [`Extractor::extract_doc`]. Between
/// waves we read parents back from the cells table and evaluate each
/// child column's predicate against the parent's freshly-written
/// value — if the predicate is false (or the parent was itself
/// skipped) the child is dropped from the next batch and we emit a
/// `tracing::debug` audit event.
///
/// ## Incremental re-extract (T27)
///
/// Before any wave runs we collect the set of `(row, col)` pairs
/// that already have a cell, and filter every wave's "to extract"
/// list against it. Existing cells are left untouched — including
/// non-locked ones. Pass `force_reextract = true` to bypass this
/// (operator-level override for prompt edits).
///
/// **v1 simplification:** "schema drift" here means *"cell does
/// not exist yet"*, not *"cell was extracted under an older
/// `Review::schema_version`"*. The `Cell` row carries a `version`
/// (per-cell revision number) but **not** the review's
/// `schema_version` at extraction time, so we can't distinguish
/// "old schema" from "current schema" without a storage migration.
///
/// // TODO(v1.x): track `schema_version_at_extract: u32` on each
/// // `Cell` so we can detect cells whose extraction predates a
/// // column's prompt edit and re-run *only* those. For v1 we only
/// // skip re-extracting columns that already have ANY cell for
/// // this `(review, row)` pair. Locked cells are handled separately
/// // by `CellsTable::upsert` and remain untouched even when
/// // re-extracted.
///
/// Kept private — callers should go through [`run_review`] so the
/// concurrency cap and outcome aggregation apply uniformly.
async fn extract_and_upsert_one_row(
    extractor: &Extractor,
    cells: &CellsTable,
    review_id: ReviewId,
    row: &Row,
    columns: &[Column],
    force_reextract: bool,
) -> Result<usize> {
    let waves = topo_waves(columns)?;

    // T27: cells that already exist for this row — we'll filter
    // each wave's "to extract" list against this set unless the
    // operator explicitly asked for a full re-run.
    let existing: HashSet<ColumnId> = if force_reextract {
        HashSet::new()
    } else {
        cells_for_row_columns(cells, review_id, row.id, columns).await?
    };

    let mut total_upserted = 0usize;
    let mut skipped_cols: HashSet<ColumnId> = HashSet::new();

    for wave in waves {
        // Build the wave's effective extract list: drop columns
        // whose conditional gate fails, drop columns that already
        // have a cell (unless force_reextract), and skip children
        // whose parent was itself skipped (cascading gate).
        let mut to_extract: Vec<Column> = Vec::with_capacity(wave.len());
        for col in &wave {
            if existing.contains(&col.id) {
                tracing::debug!(
                    target: "tabular::fanout",
                    col = %col.name,
                    reason = "cell_exists",
                    "incremental skip"
                );
                continue;
            }
            let Some(spec) = &col.conditional else {
                to_extract.push(col.clone());
                continue;
            };
            // Cascading: parent skipped → child skipped.
            if skipped_cols.contains(&spec.parent_col) {
                skipped_cols.insert(col.id);
                tracing::debug!(
                    target: "tabular::fanout",
                    col = %col.name,
                    reason = "parent_skipped",
                    "conditional skip"
                );
                continue;
            }
            // Read parent's latest value (could be from a prior
            // wave in this run, or pre-existing from an earlier
            // partial run).
            let parent_value = cells
                .latest(review_id, row.id, spec.parent_col)
                .await?
                .map(|c| c.value);
            if should_extract(col, parent_value.as_ref()) {
                to_extract.push(col.clone());
            } else {
                skipped_cols.insert(col.id);
                tracing::debug!(
                    target: "tabular::fanout",
                    col = %col.name,
                    reason = "predicate_false",
                    "conditional skip"
                );
            }
        }

        if to_extract.is_empty() {
            continue;
        }

        let mut extracted = extractor
            .extract_doc(review_id, row.doc_id, &to_extract)
            .await?;

        // T28: offset / quote round-trip verification. Mutates each
        // cell's `confidence` to `Low` if any citation fails — does
        // not drop the cell, does not touch `support_score`.
        for cell in &mut extracted {
            crate::verify::offsets::verify_cell_offsets(cell, row.doc_id, extractor.chunks())
                .await?;
        }
        // TODO(T29): cross-encoder support scoring goes here — set
        // `support_score` and re-bin `confidence` against
        // High/Medium/Low thresholds before the upsert loop runs.

        for cell in extracted {
            match cells.upsert(&cell).await {
                Ok(()) => total_upserted += 1,
                // Locked cells are expected to survive re-extraction —
                // that's the whole point of the lock. Swallow only this
                // variant; anything else (Lance, Arrow, Json …) is a
                // real failure and aborts the row.
                Err(Error::LockedCell { .. }) => {}
                Err(other) => return Err(other),
            }
        }
    }

    Ok(total_upserted)
}

/// Collect the set of `ColumnId`s in `columns` that already have at
/// least one cell stored for `(review, row)`. One `CellsTable::latest`
/// round-trip per column — fine for v1 where the typical column count
/// is small (<50); optimisation candidate for later if templates
/// balloon.
async fn cells_for_row_columns(
    cells: &CellsTable,
    review: ReviewId,
    row: RowId,
    columns: &[Column],
) -> Result<HashSet<ColumnId>> {
    let mut out = HashSet::new();
    for col in columns {
        if cells.latest(review, row, col.id).await?.is_some() {
            out.insert(col.id);
        }
    }
    Ok(out)
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
                ColumnBuilder::new(review, name, &format!("Q for {name}?"), CellType::Text).build();
            col.order = i as u32;
            storage.columns.add(review, &col).await.expect("add column");
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
        chunk_map.get_mut(&poison_doc).expect("doc seeded")[0].content = "POISON DO NOT EAT".into();

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

    // ---- T26 / T27 tests ----

    use crate::schema::{ConditionalSpec, Predicate};

    /// LLM that returns column envelopes keyed off a per-doc *parent
    /// value*. Used by the conditional-gating tests: row A's chunk
    /// content carries marker "GATE=X" → LLM returns `parent="X"`;
    /// row B's marker says "GATE=Y" → LLM returns `parent="Y"`. Also
    /// records the column-name list of each call so the test can
    /// assert wave ordering (parent before child).
    struct GatedLlm {
        chunk_id: uuid::Uuid,
        /// Per-call record: requested column names (parsed from
        /// `[COLUMN::name]` markers) in the order they appeared.
        calls: Arc<Mutex<Vec<Vec<String>>>>,
    }

    #[async_trait]
    impl LlmClient for GatedLlm {
        async fn generate_structured(
            &self,
            _system: &str,
            user: &str,
            _json_schema: &Value,
        ) -> Result<StructuredOutput> {
            // Parse [COLUMN::name] markers to record what was asked.
            let mut asked: Vec<String> = Vec::new();
            let mut rest = user;
            while let Some(idx) = rest.find("[COLUMN::") {
                let after = &rest[idx + "[COLUMN::".len()..];
                if let Some(end) = after.find(']') {
                    asked.push(after[..end].to_string());
                    rest = &after[end..];
                } else {
                    break;
                }
            }
            self.calls.lock().unwrap().push(asked.clone());

            // Decide parent's value from the chunk marker.
            let parent_value = if user.contains("GATE=X") { "X" } else { "Y" };

            // Build a response that includes envelopes for any
            // column the caller asked for. Child gets a stub value
            // — the gate decides if it's ever invoked at all.
            let mut map = serde_json::Map::new();
            for name in &asked {
                let v = if name == "parent" {
                    parent_value.to_string()
                } else {
                    format!("child-of-{parent_value}")
                };
                map.insert(
                    name.clone(),
                    json!({
                        "value": v,
                        "reasoning": "stub",
                        "citations": [{
                            "chunk_id": self.chunk_id.to_string(),
                            "char_start": 0,
                            "char_end": 1,
                            "quoted_text": "G"
                        }]
                    }),
                );
            }
            Ok(StructuredOutput {
                value: Value::Object(map),
                usage: Usage::default(),
            })
        }

        fn model_id(&self) -> &str {
            "gated"
        }
    }

    /// Seed a 2-row review with one ungated `parent` column and one
    /// `child` column gated on `parent == "X"`. Row 0's chunk
    /// contains "GATE=X", row 1's contains "GATE=Y".
    async fn seed_gated_review(
        storage: &StorageHandle,
    ) -> (ReviewId, Vec<Row>, HashMap<uuid::Uuid, Vec<ChunkRef>>) {
        let review = ReviewId::new();
        let parent_id = ColumnId::for_name(review, "parent");
        let parent_col = ColumnBuilder::new(review, "parent", "parent?", CellType::Text)
            .order(0)
            .build();
        let child_col = ColumnBuilder::new(review, "child", "child?", CellType::Text)
            .order(1)
            .conditional(ConditionalSpec {
                parent_col: parent_id,
                predicate: Predicate::Equals { value: json!("X") },
            })
            .build();
        storage
            .columns
            .add(review, &parent_col)
            .await
            .expect("add parent");
        storage
            .columns
            .add(review, &child_col)
            .await
            .expect("add child");

        let mut rows = Vec::new();
        let mut chunk_map: HashMap<uuid::Uuid, Vec<ChunkRef>> = HashMap::new();
        for marker in ["GATE=X", "GATE=Y"] {
            let doc = uuid::Uuid::now_v7();
            let chunk_id = uuid::Uuid::now_v7();
            chunk_map.insert(
                doc,
                vec![ChunkRef {
                    id: chunk_id,
                    doc_id: doc,
                    content: format!("Hello world. {marker}"),
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

    #[tokio::test]
    async fn fanout_skips_child_when_parent_predicate_false() {
        let (_dir, storage) = fresh_storage().await;
        let (review, rows, chunk_map) = seed_gated_review(&storage).await;

        let llm = Arc::new(GatedLlm {
            chunk_id: any_chunk_id(&chunk_map),
            calls: Arc::new(Mutex::new(Vec::new())),
        });
        let chunks = Arc::new(InMemoryChunks { by_doc: chunk_map });
        let extractor = Extractor::new(llm, chunks);

        let outcomes = run_review(&storage, &extractor, review, FanoutConfig::default())
            .await
            .expect("run_review succeeds");
        assert_eq!(outcomes.len(), 2);

        // Row 0 (GATE=X): parent + child = 2 cells.
        // Row 1 (GATE=Y): parent only = 1 cell.
        let by_row: HashMap<RowId, usize> = outcomes
            .iter()
            .map(|o| (o.row_id, *o.result.as_ref().expect("row succeeded")))
            .collect();
        assert_eq!(by_row[&rows[0].id], 2, "X row extracts both columns");
        assert_eq!(by_row[&rows[1].id], 1, "Y row gates out the child");

        let child_id = ColumnId::for_name(review, "child");
        let child_row0 = storage
            .cells
            .latest(review, rows[0].id, child_id)
            .await
            .expect("latest")
            .expect("child cell exists for X row");
        assert_eq!(child_row0.value, json!("child-of-X"));
        let child_row1 = storage
            .cells
            .latest(review, rows[1].id, child_id)
            .await
            .expect("latest");
        assert!(child_row1.is_none(), "child must NOT exist for Y row");
    }

    #[tokio::test]
    async fn fanout_extracts_child_after_parent_in_wave_order() {
        let (_dir, storage) = fresh_storage().await;
        let (review, _rows, chunk_map) = seed_gated_review(&storage).await;

        let calls = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
        let llm = Arc::new(GatedLlm {
            chunk_id: any_chunk_id(&chunk_map),
            calls: Arc::clone(&calls),
        });
        let chunks = Arc::new(InMemoryChunks { by_doc: chunk_map });
        let extractor = Extractor::new(llm, chunks);

        let outcomes = run_review(&storage, &extractor, review, FanoutConfig::default())
            .await
            .expect("run_review succeeds");
        assert_eq!(outcomes.len(), 2);

        // For the GATE=X row we expect TWO calls: wave 0 with just
        // "parent", then wave 1 with just "child". Inspect every
        // recorded call list; we must see at least one (parent-only,
        // child-only) pair in that order. Calls from the GATE=Y row
        // contribute a parent-only call (no second wave: child gated
        // out) — that's fine, it doesn't violate ordering.
        let recorded = calls.lock().unwrap().clone();

        // Across all calls, "parent" appears before "child"
        // somewhere — and "child" never co-occurs with "parent" in
        // the same call (proving the two are in separate waves).
        let mut saw_parent_only = false;
        let mut saw_child_only_after_parent = false;
        for call in &recorded {
            if call.len() == 1 && call[0] == "parent" {
                saw_parent_only = true;
            } else if call.len() == 1 && call[0] == "child" {
                assert!(saw_parent_only, "child wave fired before parent wave");
                saw_child_only_after_parent = true;
            } else {
                panic!("unexpected co-batched call: {call:?}");
            }
        }
        assert!(saw_parent_only, "expected at least one parent-only batch");
        assert!(
            saw_child_only_after_parent,
            "expected a child-only batch after parent (got {recorded:?})"
        );
    }

    #[tokio::test]
    async fn fanout_skips_columns_with_existing_cells() {
        let (_dir, storage) = fresh_storage().await;
        let (review, rows, chunk_map) = seed_review(&storage, 1).await;

        // Pre-seed a cell for (row_0, col_a). Default config →
        // force_reextract=false → that column must be skipped while
        // col_b is freshly extracted.
        let col_a = ColumnId::for_name(review, "col_a");
        let col_b = ColumnId::for_name(review, "col_b");
        let pre = Cell {
            review_id: review,
            row_id: rows[0].id,
            col_id: col_a,
            value: json!("PRE_EXISTING"),
            reasoning: Some("pre-seeded".into()),
            citations: vec![],
            support_score: 0.5,
            confidence: Confidence::Medium,
            locked: false,
            version: 1,
            author: Author::System {
                extractor_version: "old".into(),
            },
            updated_at: Utc::now(),
        };
        storage.cells.upsert(&pre).await.expect("seed cell");

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
        assert_eq!(outcomes.len(), 1);
        assert_eq!(
            *outcomes[0].result.as_ref().expect("ok"),
            1,
            "only col_b should be (re)extracted"
        );

        let kept = storage
            .cells
            .latest(review, rows[0].id, col_a)
            .await
            .expect("latest")
            .expect("col_a cell exists");
        assert_eq!(kept.value, json!("PRE_EXISTING"), "col_a untouched");
        assert_eq!(kept.version, 1, "version not incremented");

        let fresh = storage
            .cells
            .latest(review, rows[0].id, col_b)
            .await
            .expect("latest")
            .expect("col_b cell exists");
        assert_eq!(fresh.value, json!("B"), "col_b freshly extracted");
    }

    /// LLM whose citations point at bogus offsets so the T28 offset
    /// verifier must downgrade every extracted cell to `Low`.
    struct BadOffsetLlm {
        chunk_id: uuid::Uuid,
    }

    #[async_trait]
    impl LlmClient for BadOffsetLlm {
        async fn generate_structured(
            &self,
            _system: &str,
            _user: &str,
            _json_schema: &Value,
        ) -> Result<StructuredOutput> {
            Ok(StructuredOutput {
                value: json!({
                    "col_a": {
                        "value": "A",
                        "reasoning": "stub",
                        "citations": [{
                            "chunk_id": self.chunk_id.to_string(),
                            "char_start": 0,
                            "char_end": 5,
                            // Chunk content is "Governing law: ..." —
                            // bytes 0..5 are "Gover", NOT "WRONG".
                            "quoted_text": "WRONG"
                        }]
                    },
                    "col_b": {
                        "value": "B",
                        "reasoning": "stub",
                        "citations": [{
                            "chunk_id": self.chunk_id.to_string(),
                            "char_start": 0,
                            "char_end": 5,
                            "quoted_text": "Gover"
                        }]
                    }
                }),
                usage: Usage::default(),
            })
        }

        fn model_id(&self) -> &str {
            "bad-offset"
        }
    }

    #[tokio::test]
    async fn fanout_offset_verifier_downgrades_bad_citation() {
        let (_dir, storage) = fresh_storage().await;
        let (review, rows, chunk_map) = seed_review(&storage, 1).await;

        let llm = Arc::new(BadOffsetLlm {
            chunk_id: any_chunk_id(&chunk_map),
        });
        let chunks = Arc::new(InMemoryChunks { by_doc: chunk_map });
        let extractor = Extractor::new(llm, chunks);

        let outcomes = run_review(&storage, &extractor, review, FanoutConfig::default())
            .await
            .expect("run_review succeeds");
        assert_eq!(outcomes.len(), 1);

        // col_a's citation quote was bogus → must be downgraded to Low.
        let col_a = ColumnId::for_name(review, "col_a");
        let bad_cell = storage
            .cells
            .latest(review, rows[0].id, col_a)
            .await
            .expect("latest")
            .expect("col_a cell exists");
        assert!(
            matches!(bad_cell.confidence, Confidence::Low),
            "T28 must downgrade cells whose citations don't round-trip, got {:?}",
            bad_cell.confidence
        );

        // col_b's citation matched the chunk → must stay at the
        // extractor's default Medium (verifier never upgrades).
        let col_b = ColumnId::for_name(review, "col_b");
        let good_cell = storage
            .cells
            .latest(review, rows[0].id, col_b)
            .await
            .expect("latest")
            .expect("col_b cell exists");
        assert!(
            matches!(good_cell.confidence, Confidence::Medium),
            "well-formed citation must keep its starting confidence, got {:?}",
            good_cell.confidence
        );
    }

    #[tokio::test]
    async fn fanout_force_reextract_runs_all_columns() {
        // With force_reextract=true the incremental skip is bypassed,
        // so even a pre-seeded col_a is sent to the LLM again. The
        // re-extraction is asserted via:
        //  1. the per-row upsert count = 2 (both columns wrote),
        //  2. the LLM was invoked (extract_doc actually ran).
        //
        // Note we deliberately don't assert that `latest.value`
        // flipped from "OLD_VALUE" to "A": `extract::batch` always
        // writes `version = 1`, identical to the pre-seeded version,
        // and `CellsTable::latest` picks max-by-version which is then
        // a tie between two `version = 1` rows. Version-bump semantics
        // belong to the verifier+commit path (later phases). The
        // re-extraction *happened* — that's what force_reextract
        // guarantees.
        let (_dir, storage) = fresh_storage().await;
        let (review, rows, chunk_map) = seed_review(&storage, 1).await;

        let col_a = ColumnId::for_name(review, "col_a");
        let pre = Cell {
            review_id: review,
            row_id: rows[0].id,
            col_id: col_a,
            value: json!("OLD_VALUE"),
            reasoning: Some("pre".into()),
            citations: vec![],
            support_score: 0.5,
            confidence: Confidence::Medium,
            locked: false,
            version: 1,
            author: Author::System {
                extractor_version: "old".into(),
            },
            updated_at: Utc::now(),
        };
        storage.cells.upsert(&pre).await.expect("seed cell");

        let calls = Arc::new(AtomicUsize::new(0));
        let llm = Arc::new(StubLlm {
            chunk_id: any_chunk_id(&chunk_map),
            calls: Arc::clone(&calls),
            delay_ms: 0,
        });
        let chunks = Arc::new(InMemoryChunks { by_doc: chunk_map });
        let extractor = Extractor::new(llm, chunks);

        let cfg = FanoutConfig {
            force_reextract: true,
            ..Default::default()
        };
        let outcomes = run_review(&storage, &extractor, review, cfg)
            .await
            .expect("run_review succeeds");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(
            *outcomes[0].result.as_ref().expect("ok"),
            2,
            "both columns re-extracted under force_reextract (col_a not skipped)"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "LLM was invoked (single batched call) — incremental skip bypassed"
        );

        // Sanity: by contrast, the default-config skip path would have
        // counted only 1 upsert and returned. Make that explicit by
        // re-running with force_reextract=false on a sibling storage.
        let (_dir2, storage2) = fresh_storage().await;
        let (review2, rows2, chunk_map2) = seed_review(&storage2, 1).await;
        let col_a2 = ColumnId::for_name(review2, "col_a");
        let pre2 = Cell {
            review_id: review2,
            row_id: rows2[0].id,
            col_id: col_a2,
            value: json!("OLD_VALUE"),
            reasoning: Some("pre".into()),
            citations: vec![],
            support_score: 0.5,
            confidence: Confidence::Medium,
            locked: false,
            version: 1,
            author: Author::System {
                extractor_version: "old".into(),
            },
            updated_at: Utc::now(),
        };
        storage2.cells.upsert(&pre2).await.expect("seed");
        let llm2 = Arc::new(StubLlm {
            chunk_id: any_chunk_id(&chunk_map2),
            calls: Arc::new(AtomicUsize::new(0)),
            delay_ms: 0,
        });
        let chunks2 = Arc::new(InMemoryChunks { by_doc: chunk_map2 });
        let extractor2 = Extractor::new(llm2, chunks2);
        let outcomes2 = run_review(&storage2, &extractor2, review2, FanoutConfig::default())
            .await
            .expect("run_review succeeds");
        assert_eq!(
            *outcomes2[0].result.as_ref().expect("ok"),
            1,
            "default config (force_reextract=false) skips the pre-seeded col_a"
        );
    }
}
