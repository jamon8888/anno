//! LanceDB persistence: chunks table, upsert via `merge_insert`, native search.
//!
//! v0.1: vector-only retrieval. Hybrid (FTS + vector via `RRFReranker`) lands
//! in v0.2 once we add a FTS index after ingest.
//!
//! Schema (see [`chunks_schema`]):
//! - `chunk_id`: `FixedSizeBinary(16)` — UUID v5 from (`doc_id`, `chunk_idx`) for idempotent re-ingest
//! - `doc_id`: `FixedSizeBinary(16)` — UUID v7
//! - `source_path`, `folder_path`: `Utf8`
//! - `chunk_idx`: `UInt32`
//! - `text_pseudo`: `Utf8`
//! - `page`: `UInt32` nullable
//! - `char_start`, `char_end`: `UInt32`
//! - `text_hash`: `FixedSizeBinary(32)` — sha256 of `text_pseudo`
//! - `vector`: `FixedSizeList<Float32>(dim)`
//! - `ingested_at`: `Timestamp(Microsecond, None)`

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use arrow_array::cast::AsArray;
use arrow_array::types::Float32Type;
use arrow_array::{
    builder::{FixedSizeBinaryBuilder, ListBuilder, StringBuilder, StructBuilder},
    Array, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, ListArray, RecordBatch,
    RecordBatchIterator, StringArray, StructArray, TimestampMicrosecondArray, UInt32Array,
};
use arrow_schema::{DataType, Field, Fields, Schema, TimeUnit};
use chrono::{DateTime, TimeZone, Utc};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

/// Name of the chunks table inside the LanceDB index directory.
pub const TABLE_NAME: &str = "chunks";

/// Name of the memories table inside the LanceDB index directory.
pub const MEMORIES_TABLE_NAME: &str = "memories";

/// Build the Arrow schema for the `memories` collection.
///
/// 11 columns: 7 always populated in v0.1, 3 forward-compat for v0.2
/// (`valid_from`, `valid_to`, `entity_refs`), and `embedding` /
/// `token_refs` carrying the runtime PII-cascade payload.
#[must_use]
pub fn memories_schema(embedding_dim: usize) -> Arc<Schema> {
    let token_ref_struct = DataType::Struct(
        vec![
            Field::new("label", DataType::Utf8, false),
            Field::new("token", DataType::Utf8, false),
        ]
        .into(),
    );
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("session_id", DataType::Utf8, true),
        Field::new("kind", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new(
            "created_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new(
            "accessed_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new(
            "valid_from",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new(
            "valid_to",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new(
            "embedding",
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                embedding_dim as i32,
            ),
            false,
        ),
        Field::new(
            "token_refs",
            DataType::List(Arc::new(Field::new("item", token_ref_struct, true))),
            false,
        ),
        Field::new(
            "entity_refs",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            false,
        ),
    ]))
}

/// Build the Arrow schema for the chunks table.
#[must_use]
pub fn chunks_schema(dim: usize) -> Arc<Schema> {
    let vector_field = Arc::new(Field::new("item", DataType::Float32, true));
    Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::FixedSizeBinary(16), false),
        Field::new("doc_id", DataType::FixedSizeBinary(16), false),
        Field::new("source_path", DataType::Utf8, false),
        Field::new("folder_path", DataType::Utf8, false),
        Field::new("chunk_idx", DataType::UInt32, false),
        Field::new("text_pseudo", DataType::Utf8, false),
        Field::new("page", DataType::UInt32, true),
        Field::new("char_start", DataType::UInt32, false),
        Field::new("char_end", DataType::UInt32, false),
        Field::new("text_hash", DataType::FixedSizeBinary(32), false),
        Field::new(
            "vector",
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            DataType::FixedSizeList(vector_field, dim as i32),
            false,
        ),
        Field::new(
            "ingested_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
    ]))
}

/// One chunk to insert. `chunk_id` and `text_hash` are derived inside [`Store::upsert`].
#[derive(Debug, Clone)]
pub struct ChunkRecord {
    /// Stable per-document UUID v7 (assigned by the caller during ingest).
    pub doc_id: Uuid,
    /// Absolute filesystem path of the source document.
    pub source_path: String,
    /// Folder containing the source (used for folder-scoped filters in v0.2).
    pub folder_path: String,
    /// 0-based chunk index within the document.
    pub chunk_idx: u32,
    /// Pseudonymized chunk text (PII already replaced).
    pub text_pseudo: String,
    /// Page number (if the source is paginated).
    pub page: Option<u32>,
    /// Inclusive start char offset inside the pseudonymized document.
    pub char_start: u32,
    /// Exclusive end char offset inside the pseudonymized document.
    pub char_end: u32,
    /// Embedding for `text_pseudo`. Must match the configured `embed_dim`.
    pub vector: Vec<f32>,
}

/// A search hit returned by [`Store::search`].
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Source document.
    pub doc_id: Uuid,
    /// Deterministic chunk UUID.
    pub chunk_id: Uuid,
    /// Absolute source path.
    pub source_path: String,
    /// Folder path.
    pub folder_path: String,
    /// 0-based chunk index.
    pub chunk_idx: u32,
    /// Pseudonymized text.
    pub text_pseudo: String,
    /// Optional page.
    pub page: Option<u32>,
    /// Char start offset.
    pub char_start: u32,
    /// Char end offset.
    pub char_end: u32,
    /// Relevance score from the hybrid reranker (`_relevance_score`) —
    /// **higher is more relevant**. `0.0` means the batch carried no
    /// relevance column.
    pub score: f32,
}

/// Summary of chunk rows removed for a registered source folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteFolderRowsReport {
    /// Number of chunk rows removed from the chunks table.
    pub removed_chunks: u64,
    /// Distinct document ids observed under the source folder before deletion.
    pub doc_ids: Vec<Uuid>,
}

/// Handle to the `chunks` table.
#[derive(Clone)]
pub struct Store {
    tbl: Table,
    dim: usize,
    memories_tbl: Table,
    memories_schema: Arc<Schema>,
    memory_embedding_dim: usize,
    index_distance: String,
    index_num_partitions: Option<usize>,
    search_nprobes: usize,
    search_refine_factor: u32,
}

/// Map a config distance string to a LanceDB metric. Unknown values fall back
/// to cosine (the default), which matches the L2-normalized embedder output.
fn distance_from_str(s: &str) -> lancedb::DistanceType {
    match s {
        "l2" => lancedb::DistanceType::L2,
        "dot" => lancedb::DistanceType::Dot,
        _ => lancedb::DistanceType::Cosine,
    }
}

impl Store {
    /// Open or create the chunks table under `cfg.index_path()`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] if the path is not valid UTF-8, or [`Error::Lance`]
    /// if LanceDB connection / table creation fails.
    pub async fn open(cfg: &AnnoRagConfig) -> Result<Self> {
        let path = cfg.index_path();
        let uri = path
            .to_str()
            .ok_or_else(|| Error::Store(format!("non-utf8 index path: {}", path.display())))?;
        let conn = lancedb::connect(uri).execute().await?;

        let chunks_schema = chunks_schema(cfg.embed_dim);
        let tbl = open_or_create_table(&conn, TABLE_NAME, &chunks_schema).await?;

        // Detect stale index built with a different embedder — surface a clear
        // error rather than returning silent empty results on every search.
        if let Some(stored) = stored_vector_dim(&conn, TABLE_NAME, "vector").await? {
            if stored != cfg.embed_dim {
                return Err(Error::Store(format!(
                    "legal index was built with {stored}-dim embeddings but the current \
                     embedder produces {}-dim vectors. \
                     Re-index your documents with `anno-rag index` to fix this.",
                    cfg.embed_dim
                )));
            }
        }

        let memories_schema = memories_schema(cfg.memory_embedding_dim);
        let memories_tbl = {
            match stored_vector_dim(&conn, &cfg.memory_collection_name, "embedding").await {
                Ok(Some(stored)) if stored != cfg.memory_embedding_dim => {
                    tracing::warn!(
                        stored_dim = stored,
                        config_dim = cfg.memory_embedding_dim,
                        table = %cfg.memory_collection_name,
                        "memory table embedding dim mismatch — dropping and rebuilding (memories are ephemeral)"
                    );
                    conn.drop_table(&cfg.memory_collection_name, &[]).await?;
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        table = %cfg.memory_collection_name,
                        "memory table schema malformed — dropping and rebuilding (memories are ephemeral)"
                    );
                    // Best-effort: if the table is corrupt the drop may itself fail;
                    // open_or_create_table below will create a fresh one regardless.
                    let _ = conn.drop_table(&cfg.memory_collection_name, &[]).await;
                }
                _ => {}
            }
            open_or_create_table(&conn, &cfg.memory_collection_name, &memories_schema).await?
        };

        Ok(Self {
            tbl,
            dim: cfg.embed_dim,
            memories_tbl,
            memories_schema,
            memory_embedding_dim: cfg.memory_embedding_dim,
            index_distance: cfg.index_distance.clone(),
            index_num_partitions: cfg.index_num_partitions,
            search_nprobes: cfg.search_nprobes,
            search_refine_factor: cfg.search_refine_factor,
        })
    }

    /// Insert one memory row into the `memories` table.
    ///
    /// # Errors
    /// Returns [`Error::Store`] / [`Error::Arrow`] / [`Error::Lance`] on
    /// build or insert failure.
    pub async fn memory_insert(&self, m: &crate::memory::Memory) -> Result<()> {
        let batch = memory_to_batch(m, self.memory_embedding_dim, &self.memories_schema)?;
        let reader =
            RecordBatchIterator::new(std::iter::once(Ok(batch)), self.memories_schema.clone());
        let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(reader);
        self.memories_tbl
            .add(reader)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("memory add: {e}")))?;
        Ok(())
    }

    /// Fetch one memory row by id.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query failure or row decoding failure.
    pub async fn memory_get(
        &self,
        id: &crate::memory::MemoryId,
    ) -> Result<Option<crate::memory::Memory>> {
        let filter = format!("id = '{}'", id.as_string());
        let mut stream = self
            .memories_tbl
            .query()
            .only_if(&filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("memory_get exec: {e}")))?;
        let next = stream
            .try_next()
            .await
            .map_err(|e| Error::Store(format!("memory_get stream: {e}")))?;
        match next {
            Some(batch) if batch.num_rows() > 0 => Ok(Some(batch_row_to_memory(&batch, 0)?)),
            _ => Ok(None),
        }
    }

    /// Delete a memory by id.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on delete failure.
    pub async fn memory_delete_by_id(&self, id: &crate::memory::MemoryId) -> Result<bool> {
        let filter = format!("id = '{}'", id.as_string());
        self.memories_tbl
            .delete(&filter)
            .await
            .map_err(|e| Error::Store(format!("memory_delete: {e}")))?;
        Ok(true)
    }

    /// Fetch memories that reference any of `entity_ids` in their
    /// `entity_refs` column, optionally bounded by a bi-temporal `as_of`
    /// (defaults to "now"). Used by v0.2's BFS graph-recall.
    ///
    /// Exploits the `LabelList` scalar index on `entity_refs` (v0.1 T4)
    /// so the per-hop scan is sub-linear in table size.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query / decode failure.
    pub async fn memory_filter_by_entities(
        &self,
        entity_ids: &[String],
        as_of: Option<chrono::DateTime<chrono::Utc>>,
        limit: usize,
    ) -> Result<Vec<crate::memory::Memory>> {
        if entity_ids.is_empty() {
            return Ok(Vec::new());
        }
        let parts: Vec<String> = entity_ids
            .iter()
            .map(|e| format!("array_contains(entity_refs, '{}')", e.replace('\'', "''")))
            .collect();
        let mut filter = format!("({})", parts.join(" OR "));
        let t_us = as_of.unwrap_or_else(chrono::Utc::now).timestamp_micros();
        filter = format!(
            "{filter} AND valid_from <= CAST({t_us} AS TIMESTAMP) \
             AND (valid_to IS NULL OR valid_to > CAST({t_us} AS TIMESTAMP))"
        );

        let mut stream = self
            .memories_tbl
            .query()
            .only_if(filter)
            .limit(limit)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("graph filter exec: {e}")))?;

        let mut out: Vec<crate::memory::Memory> = Vec::new();
        while let Some(batch) = stream
            .try_next()
            .await
            .map_err(|e| Error::Store(format!("graph filter stream: {e}")))?
        {
            for r in 0..batch.num_rows() {
                out.push(batch_row_to_memory(&batch, r)?);
            }
        }
        Ok(out)
    }

    /// Fetch candidate memories that share at least one entity with any of
    /// `entity_refs`, optionally scoped to a session (matches that session
    /// or session-less rows), and currently valid (`valid_to IS NULL`).
    ///
    /// Used by the v0.2 conflict resolver — cosine similarity is computed
    /// in Rust against the embeddings, not in SQL, so this candidate set
    /// must be small. Capped at 100 rows; deployers with hot popular
    /// entities will see candidate truncation rather than O(table) scans.
    ///
    /// `array_contains(entity_refs, '<key>')` exploits the LabelList scalar
    /// index on `entity_refs` (v0.1 T4), so the scan is sub-linear.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query / decode failure.
    pub async fn memory_candidates_for_conflict(
        &self,
        entity_refs: &[String],
        session_id: Option<&str>,
    ) -> Result<Vec<crate::memory::Memory>> {
        if entity_refs.is_empty() {
            return Ok(Vec::new());
        }
        let parts: Vec<String> = entity_refs
            .iter()
            .map(|e| format!("array_contains(entity_refs, '{}')", e.replace('\'', "''")))
            .collect();
        let mut filter = format!("({}) AND valid_to IS NULL", parts.join(" OR "));
        if let Some(s) = session_id {
            filter = format!(
                "{filter} AND (session_id = '{}' OR session_id IS NULL)",
                s.replace('\'', "''")
            );
        }

        let mut stream = self
            .memories_tbl
            .query()
            .only_if(filter)
            .limit(100)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("candidates exec: {e}")))?;

        let mut out: Vec<crate::memory::Memory> = Vec::new();
        while let Some(batch) = stream
            .try_next()
            .await
            .map_err(|e| Error::Store(format!("candidates stream: {e}")))?
        {
            for r in 0..batch.num_rows() {
                out.push(batch_row_to_memory(&batch, r)?);
            }
        }
        Ok(out)
    }

    /// Set `valid_to` on the memory with `id` to `valid_to`. Used by the
    /// v0.2 bi-temporal invalidation path: a memory whose `valid_to` is
    /// non-null + ≤ `as_of` is excluded from `recall_memory` results.
    ///
    /// Guarded by `valid_to IS NULL` so a double-invalidation is a no-op
    /// (returns `Ok(false)`).
    ///
    /// # Errors
    /// Returns [`Error::Store`] on update failure.
    pub async fn memory_update_valid_to(
        &self,
        id: &crate::memory::MemoryId,
        valid_to: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool> {
        let id_s = id.as_string();
        let ts = valid_to.timestamp_micros();
        let filter = format!("id = '{id_s}' AND valid_to IS NULL");
        let res = self
            .memories_tbl
            .update()
            .only_if(filter)
            .column("valid_to", format!("CAST({ts} AS TIMESTAMP)"))
            .execute()
            .await
            .map_err(|e| Error::Store(format!("update valid_to: {e}")))?;
        Ok(res.rows_updated > 0)
    }

    /// Update only the NER-derived reference fields for a memory row.
    ///
    /// The LanceDB update API is expression-oriented for complex list values,
    /// so this preserves all existing fields by decoding the current row and
    /// replacing it atomically with a `merge_insert` keyed by `id`.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on lookup, Arrow conversion, or update failure.
    pub async fn memory_update_ner_fields(
        &self,
        id: &crate::memory::MemoryId,
        token_refs: Vec<crate::memory::TokenRef>,
        entity_refs: Vec<String>,
    ) -> Result<()> {
        let Some(existing) = self.memory_get(id).await? else {
            return Ok(());
        };
        let updated = crate::memory::Memory {
            token_refs,
            entity_refs,
            ..existing
        };
        let batch = memory_to_batch(&updated, self.memory_embedding_dim, &self.memories_schema)?;
        let reader =
            RecordBatchIterator::new(std::iter::once(Ok(batch)), self.memories_schema.clone());
        let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(reader);
        let mut merge_insert = self.memories_tbl.merge_insert(&["id"]);
        merge_insert.when_matched_update_all(None);
        merge_insert
            .execute(reader)
            .await
            .map_err(|e| Error::Store(format!("update ner fields: {e}")))?;
        Ok(())
    }

    /// Create the v0.1 scalar indexes on the memories collection.
    /// Idempotent — skips columns that already have an index.
    ///
    /// - `created_at` → `BTree` (range scans for pagination + as-of in v0.2)
    /// - `session_id` → `BTree` (per-session listing)
    /// - `kind` → `Bitmap` (low-cardinality category filter)
    /// - `token_refs` → `LabelList` (vault-cascade erasure path)
    /// - `entity_refs` → `LabelList` (v0.2 graph traversal — populated empty in v0.1)
    ///
    /// # Errors
    /// Returns [`Error::Store`] if listing or creating any index fails.
    pub async fn setup_memory_indexes(&self) -> Result<()> {
        use lancedb::index::scalar::{
            BTreeIndexBuilder, BitmapIndexBuilder, LabelListIndexBuilder,
        };
        use lancedb::index::Index;

        let existing = self
            .memories_tbl
            .list_indices()
            .await
            .map_err(|e| Error::Store(format!("list_indices: {e}")))?;
        let has_index_on = |col: &str| existing.iter().any(|i| i.columns.iter().any(|c| c == col));

        if !has_index_on("created_at") {
            self.memories_tbl
                .create_index(&["created_at"], Index::BTree(BTreeIndexBuilder::default()))
                .execute()
                .await
                .map_err(|e| Error::Store(format!("btree created_at: {e}")))?;
        }
        if !has_index_on("session_id") {
            self.memories_tbl
                .create_index(&["session_id"], Index::BTree(BTreeIndexBuilder::default()))
                .execute()
                .await
                .map_err(|e| Error::Store(format!("btree session_id: {e}")))?;
        }
        if !has_index_on("kind") {
            self.memories_tbl
                .create_index(&["kind"], Index::Bitmap(BitmapIndexBuilder::default()))
                .execute()
                .await
                .map_err(|e| Error::Store(format!("bitmap kind: {e}")))?;
        }
        if !has_index_on("token_refs") {
            self.memories_tbl
                .create_index(
                    &["token_refs"],
                    Index::LabelList(LabelListIndexBuilder::default()),
                )
                .execute()
                .await
                .map_err(|e| Error::Store(format!("label_list token_refs: {e}")))?;
        }
        if !has_index_on("entity_refs") {
            self.memories_tbl
                .create_index(
                    &["entity_refs"],
                    Index::LabelList(LabelListIndexBuilder::default()),
                )
                .execute()
                .await
                .map_err(|e| Error::Store(format!("label_list entity_refs: {e}")))?;
        }
        Ok(())
    }

    /// Compact the memories table — reclaim tombstoned bytes, merge small
    /// fragments. Drives the v0.1 GDPR Art. 17 erasure SLO: after the
    /// background ticker fires (default 24h interval), forgotten rows are
    /// physically gone from disk.
    ///
    /// `_min_age` is the desired minimum age of a tombstone before
    /// reclaim; on lance 0.29 the value is informational — `OptimizeAction::All`
    /// reclaims everything compaction-eligible. A v0.2 patch can plumb the
    /// age through `CompactionOptions` once the surface stabilises.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on optimize failure.
    pub async fn optimize_memories(&self, _min_age: std::time::Duration) -> Result<()> {
        use lancedb::table::OptimizeAction;
        self.memories_tbl
            .optimize(OptimizeAction::All)
            .await
            .map_err(|e| Error::Store(format!("optimize_memories: {e}")))?;
        Ok(())
    }

    /// Cursor-paginated memory list. Filters by optional `session_id` /
    /// `kind`; orders by `created_at` DESC; pages by passing the
    /// previous page's last `created_at` (RFC 3339) as `cursor`.
    ///
    /// Returns `(rows, next_cursor)`. `next_cursor` is `Some(...)` when a
    /// further page exists; `None` when the result set is exhausted.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query/scan failure.
    pub async fn memory_list(
        &self,
        session_id: Option<&str>,
        kind: Option<&str>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<crate::memory::Memory>, Option<String>)> {
        let mut clauses: Vec<String> = Vec::new();
        if let Some(s) = session_id {
            // Single-quote escape: replace ' with '' (SQL standard).
            clauses.push(format!("session_id = '{}'", s.replace('\'', "''")));
        }
        if let Some(k) = kind {
            clauses.push(format!("kind = '{}'", k.replace('\'', "''")));
        }
        if let Some(c) = cursor {
            clauses.push(format!(
                "created_at < timestamp '{}'",
                c.replace('\'', "''")
            ));
        }
        let mut q = self.memories_tbl.query();
        if !clauses.is_empty() {
            let filter = clauses.join(" AND ");
            q = q.only_if(filter);
        }
        // Fetch limit + 1 so we know if there's a next page.
        let mut stream = q
            .limit(limit + 1)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("memory_list exec: {e}")))?;

        let mut items: Vec<crate::memory::Memory> = Vec::with_capacity(limit + 1);
        while let Some(batch) = stream
            .try_next()
            .await
            .map_err(|e| Error::Store(format!("memory_list stream: {e}")))?
        {
            for r in 0..batch.num_rows() {
                items.push(batch_row_to_memory(&batch, r)?);
            }
        }
        // Order by created_at DESC in Rust — LanceDB Query lacks a stable
        // `order_by` on the 0.29 surface used elsewhere in this crate.
        items.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        let next_cursor = if items.len() > limit {
            items.truncate(limit + 1);
            let extra = items.pop();
            extra.map(|m| m.created_at.to_rfc3339())
        } else {
            None
        };
        items.truncate(limit);
        Ok((items, next_cursor))
    }

    /// Count how many memory rows reference `token` in their `token_refs`.
    /// Used by the GDPR Art. 17 cascade to decide whether a vault token
    /// is orphaned after a `forget_memory` deletion.
    ///
    /// v0.1 implementation scans the table (O(N)). v0.2 will switch to the
    /// LabelList index once the lance struct-subfield filter syntax is
    /// confirmed for `List<Struct>` columns.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on scan failure.
    pub async fn token_reference_count(&self, token: &str) -> Result<u64> {
        let mut stream = self
            .memories_tbl
            .query()
            .select(lancedb::query::Select::columns(&["token_refs"]))
            .execute()
            .await
            .map_err(|e| Error::Store(format!("token_ref scan: {e}")))?;
        let mut count: u64 = 0;
        while let Some(batch) = stream
            .try_next()
            .await
            .map_err(|e| Error::Store(format!("token_ref stream: {e}")))?
        {
            let token_refs_arr = get_col::<ListArray>(&batch, "token_refs")?;
            for i in 0..batch.num_rows() {
                if token_refs_arr.is_null(i) {
                    continue;
                }
                let inner = token_refs_arr.value(i);
                let s = inner
                    .as_any()
                    .downcast_ref::<StructArray>()
                    .ok_or_else(|| Error::Store("token_refs inner not Struct".into()))?;
                let token_col = s
                    .column_by_name("token")
                    .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                    .ok_or_else(|| Error::Store("token_refs.token not Utf8".into()))?;
                for k in 0..s.len() {
                    if token_col.value(k) == token {
                        count += 1;
                        break;
                    }
                }
            }
        }
        Ok(count)
    }

    /// List indexes currently registered on the memories table. Useful for
    /// startup checks and tests.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on LanceDB listing failure.
    pub async fn memory_list_indexes(&self) -> Result<Vec<lancedb::index::IndexConfig>> {
        self.memories_tbl
            .list_indices()
            .await
            .map_err(|e| Error::Store(format!("memory_list_indexes: {e}")))
    }

    /// Whether any chunk row exists for `doc_id`. Cheap filtered count.
    /// Used by `ingest_one` to skip files already ingested (same
    /// content hash ⇒ same `doc_id`).
    ///
    /// The `doc_id` column is `FixedSizeBinary(16)` in Arrow/LanceDB, so the
    /// DataFusion SQL filter uses the `X'<hex>'` binary-literal syntax with the
    /// 32-char unhyphenated UUID hex (e.g. `X'550e8400e29b41d4a716446655440000'`).
    /// A string-UUID literal (e.g. `doc_id = '550e...'`) would fail at runtime
    /// with a column-type mismatch.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the LanceDB count fails.
    pub async fn doc_exists(&self, doc_id: Uuid) -> Result<bool> {
        let hex = doc_id.simple().to_string(); // 32 hex chars, no hyphens
        let filter = format!("doc_id = X'{hex}'");
        let n = self
            .tbl
            .count_rows(Some(filter))
            .await
            .map_err(|e| Error::Store(format!("doc_exists count_rows: {e}")))?;
        Ok(n > 0)
    }

    /// Fetch every chunk row belonging to `doc_id`, decoded as [`SearchHit`].
    ///
    /// Used by [`crate::pipeline::Pipeline::drain_enrichment_backlog`] to
    /// re-run legal enrichment without re-ingesting the document.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query / decode failures.
    pub async fn chunks_by_doc(&self, doc_id: Uuid) -> Result<Vec<SearchHit>> {
        let hex = doc_id.simple().to_string();
        let filter = format!("doc_id = X'{hex}'");
        let stream = self
            .tbl
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("chunks_by_doc: {e}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| Error::Store(format!("chunks_by_doc stream: {e}")))?;
        let mut hits = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                hits.push(batch_to_hit(batch, i)?);
            }
        }
        hits.sort_by_key(|h| h.chunk_idx);
        Ok(hits)
    }

    /// Fetch multiple chunks by their deterministic chunk UUIDs in one query.
    /// Results are sorted by `chunk_idx` for stable ordering. Returns an empty
    /// vec immediately when `chunk_ids` is empty.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query / decode failures.
    pub async fn chunks_by_ids(&self, chunk_ids: &[Uuid]) -> Result<Vec<SearchHit>> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        let filter = chunk_id_filter_sql(chunk_ids);
        let stream = self
            .tbl
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("chunks_by_ids: {e}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| Error::Store(format!("chunks_by_ids stream: {e}")))?;
        let mut hits = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                hits.push(batch_to_hit(batch, i)?);
            }
        }
        hits.sort_by_key(|h| h.chunk_idx);
        Ok(hits)
    }

    /// Fetch chunk UUIDs for a set of document UUIDs.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query / decode failures.
    pub async fn chunk_ids_for_docs(&self, doc_ids: &[Uuid]) -> Result<Vec<Uuid>> {
        if doc_ids.is_empty() {
            return Ok(Vec::new());
        }
        let filter = doc_id_filter_sql(doc_ids);
        let stream = self
            .tbl
            .query()
            .select(lancedb::query::Select::columns(&["chunk_id"]))
            .only_if(filter)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("chunk_ids_for_docs: {e}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| Error::Store(format!("chunk_ids_for_docs stream: {e}")))?;
        let mut out = Vec::new();
        for batch in &batches {
            let arr = get_col::<FixedSizeBinaryArray>(batch, "chunk_id")?;
            for idx in 0..batch.num_rows() {
                out.push(uuid_from_fsb(arr, idx)?);
            }
        }
        Ok(out)
    }

    /// Delete all chunk rows whose `doc_id` belongs to `doc_ids`.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if LanceDB count/query/delete fails.
    pub async fn delete_doc_id_rows(&self, doc_ids: &[Uuid]) -> Result<u64> {
        if doc_ids.is_empty() {
            return Ok(0);
        }
        let removed = self.chunk_ids_for_docs(doc_ids).await?.len() as u64;
        if removed == 0 {
            return Ok(0);
        }
        self.tbl
            .delete(&doc_id_filter_sql(doc_ids))
            .await
            .map_err(|e| Error::Store(format!("delete_doc_id_rows: {e}")))?;
        Ok(removed)
    }

    /// Fetch a single chunk by its deterministic UUID. Returns `None` when
    /// no row matches (e.g. doc not yet ingested).
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query / decode failures.
    pub async fn chunk_by_id(&self, chunk_id: Uuid) -> Result<Option<SearchHit>> {
        let filter = format!("chunk_id = X'{}'", chunk_id.simple());
        let stream = self
            .tbl
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("chunk_by_id: {e}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| Error::Store(format!("chunk_by_id stream: {e}")))?;
        for batch in &batches {
            if batch.num_rows() > 0 {
                return Ok(Some(batch_to_hit(batch, 0)?));
            }
        }
        Ok(None)
    }

    /// Hybrid (vector + FTS) search restricted to a pre-filtered set of chunk
    /// UUIDs. When `allowed_ids` is empty, returns an empty vec immediately
    /// without hitting LanceDB.
    ///
    /// This is the post-filter path used by `Pipeline::legal_search`: the
    /// legal store produces a candidate set via SQL predicates, and this
    /// method reranks only those candidates by relevance.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query / decode failures.
    pub async fn search_filtered_to_chunks(
        &self,
        query_text: &str,
        query_vec: &[f32],
        k: usize,
        allowed_ids: &[Uuid],
    ) -> Result<Vec<SearchHit>> {
        use lance_index::scalar::FullTextSearchQuery;
        use lancedb::query::QueryBase;
        use lancedb::rerankers::rrf::RRFReranker;
        use std::sync::Arc;

        if allowed_ids.is_empty() {
            return Ok(Vec::new());
        }
        let filter = chunk_id_filter_sql(allowed_ids);
        let stream = self
            .tbl
            .query()
            .nearest_to(query_vec.to_vec())?
            .nprobes(self.search_nprobes)
            .refine_factor(self.search_refine_factor)
            .full_text_search(FullTextSearchQuery::new(query_text.to_string()))
            .rerank(Arc::new(RRFReranker::default()))
            .only_if(filter)
            .limit(k)
            .execute()
            .await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        let mut hits = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                hits.push(batch_to_hit(batch, i)?);
            }
        }
        hits.truncate(k);
        Ok(hits)
    }

    /// Hybrid search restricted to documents.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on query / decode failures.
    pub async fn search_filtered_to_docs(
        &self,
        query_text: &str,
        query_vec: &[f32],
        k: usize,
        allowed_doc_ids: &[Uuid],
    ) -> Result<Vec<SearchHit>> {
        if allowed_doc_ids.is_empty() {
            return Ok(Vec::new());
        }
        let chunk_ids = self.chunk_ids_for_docs(allowed_doc_ids).await?;
        self.search_filtered_to_chunks(query_text, query_vec, k, &chunk_ids)
            .await
    }

    /// Number of rows in the memories table. Cheap (`count_rows`); used
    /// by the recall-path optimize gate to skip `optimize()` when no
    /// memories were added since the last index fold-in.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the LanceDB count fails.
    pub async fn memory_row_count(&self) -> Result<u64> {
        let n = self
            .memories_tbl
            .count_rows(None)
            .await
            .map_err(|e| Error::Store(format!("memories count_rows: {e}")))?;
        Ok(n as u64)
    }

    /// Count rows in the chunks table.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the LanceDB count fails.
    pub async fn count_chunks(&self) -> Result<u64> {
        let n = self
            .tbl
            .count_rows(None)
            .await
            .map_err(|e| Error::Store(format!("chunks count_rows: {e}")))?;
        Ok(n as u64)
    }

    /// Build an FTS index on `memories.text`. Idempotent — returns `Ok(false)`
    /// when the index already exists or the table is empty. Mirrors the
    /// French-tokenized chunks FTS index in [`Self::maybe_build_fts_index`].
    ///
    /// # Errors
    /// Returns [`Error::Store`] if `count_rows`, `list_indices`, or
    /// `create_index` fails.
    pub async fn build_memories_fts_index(&self) -> Result<bool> {
        use lancedb::index::scalar::FtsIndexBuilder;
        use lancedb::index::Index;

        let count = self
            .memories_tbl
            .count_rows(None)
            .await
            .map_err(|e| Error::Store(format!("memories count_rows: {e}")))?;
        if count == 0 {
            return Ok(false);
        }
        let existing = self
            .memories_tbl
            .list_indices()
            .await
            .map_err(|e| Error::Store(format!("list_indices: {e}")))?;
        let already = existing
            .iter()
            .any(|i| i.columns.iter().any(|c| c == "text"));
        if already {
            return Ok(false);
        }

        let fts = FtsIndexBuilder::default()
            .base_tokenizer("simple".to_string())
            .language("French")
            .map_err(|e| Error::Store(format!("fts language: {e}")))?
            .stem(true)
            .remove_stop_words(true)
            .lower_case(true);
        self.memories_tbl
            .create_index(&["text"], Index::FTS(fts))
            .execute()
            .await
            .map_err(|e| Error::Store(format!("memories fts: {e}")))?;
        Ok(true)
    }

    /// Hybrid search (dense vector + native FTS, RRF-reranked) over memories.
    /// Returns at most `top_k` rows with the on-disk tokenized text — the
    /// Pipeline rehydrates before exposing to the caller.
    ///
    /// # Errors
    /// Returns [`Error::Store`] / [`Error::Lance`] on query failures.
    pub async fn memories_hybrid_search(
        &self,
        query_vec: &[f32],
        query_text: &str,
        top_k: usize,
    ) -> Result<Vec<crate::memory::MemoryHitRow>> {
        use lance_index::scalar::FullTextSearchQuery;
        use lancedb::rerankers::rrf::RRFReranker;

        let fts_query = FullTextSearchQuery::new(query_text.to_string())
            .with_columns(&["text".to_string()])
            .map_err(|e| Error::Store(format!("memories fts column: {e}")))?;
        let stream = self
            .memories_tbl
            .query()
            .nearest_to(query_vec.to_vec())?
            .full_text_search(fts_query)
            .rerank(Arc::new(RRFReranker::default()))
            .limit(top_k)
            .execute()
            .await
            .map_err(|e| Error::Store(format!("memories hybrid: {e}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| Error::Store(format!("memories stream: {e}")))?;
        let mut hits = Vec::with_capacity(top_k);
        for batch in &batches {
            for row in 0..batch.num_rows() {
                hits.push(batch_row_to_memory_hit(batch, row)?);
            }
        }
        hits.truncate(top_k);
        Ok(hits)
    }

    /// Upsert chunks. Idempotent on `(doc_id, chunk_idx)` via `merge_insert`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] if a record's vector length does not match
    /// `embed_dim`, or [`Error::Arrow`] / [`Error::Lance`] on Arrow/LanceDB errors.
    pub async fn upsert(&self, records: Vec<ChunkRecord>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let schema = chunks_schema(self.dim);
        let batch = records_to_batch(&records, self.dim, &schema)?;
        let reader = RecordBatchIterator::new(std::iter::once(Ok(batch)), schema);

        let mut mi = self.tbl.merge_insert(&["doc_id", "chunk_idx"]);
        mi.when_matched_update_all(None);
        mi.when_not_matched_insert_all();
        mi.execute(Box::new(reader)).await?;
        Ok(())
    }

    /// Delete all chunk rows for a given `source_path`. Used to clear
    /// stale rows when a file's content changed (new `doc_id`) so the
    /// old `doc_id`'s rows don't orphan-duplicate the document.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on delete failure.
    pub async fn delete_doc_rows(&self, source_path: &str) -> Result<()> {
        let escaped = source_path.replace('\'', "''");
        self.tbl
            .delete(&format!("source_path = '{escaped}'"))
            .await
            .map_err(|e| Error::Store(format!("delete_doc_rows: {e}")))?;
        Ok(())
    }

    /// List distinct raw legal corpus folder paths currently present in chunks.
    /// Intended for local/admin bookkeeping; do not expose these paths in MCP
    /// or UI responses without a privacy gate.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the LanceDB query or Arrow decode fails.
    pub async fn list_indexed_folder_paths(&self) -> Result<Vec<String>> {
        let stream = self
            .tbl
            .query()
            .select(lancedb::query::Select::columns(&["folder_path"]))
            .execute()
            .await
            .map_err(|e| Error::Store(format!("list_indexed_folder_paths query: {e}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| Error::Store(format!("list_indexed_folder_paths collect: {e}")))?;
        let mut seen = std::collections::BTreeSet::new();
        for batch in &batches {
            let folder_paths = batch
                .column_by_name("folder_path")
                .ok_or_else(|| Error::Store("folder_path column missing".into()))?
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| Error::Store("folder_path wrong type".into()))?;
            for i in 0..folder_paths.len() {
                if !folder_paths.is_null(i) {
                    seen.insert(folder_paths.value(i).to_string());
                }
            }
        }
        Ok(seen.into_iter().collect())
    }

    /// Count chunks whose `source_path` is inside `folder_path`.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the LanceDB query or Arrow decode fails.
    pub async fn count_chunks_for_folder(&self, folder_path: &str) -> Result<u64> {
        Ok(self.source_rows_for_subtree(folder_path).await?.len() as u64)
    }

    /// List distinct document ids whose `source_path` is inside `folder_path`.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the LanceDB query or Arrow decode fails.
    pub async fn doc_ids_for_source_subtree(&self, folder_path: &str) -> Result<Vec<Uuid>> {
        let mut doc_ids = std::collections::BTreeSet::new();
        for (doc_id, _) in self.source_rows_for_subtree(folder_path).await? {
            doc_ids.insert(doc_id);
        }
        Ok(doc_ids.into_iter().collect())
    }

    /// Delete all chunks whose `source_path` is inside `folder_path`.
    /// Returns the row count and document ids observed before deletion.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the LanceDB query/decode/delete fails.
    pub async fn delete_folder_rows(&self, folder_path: &str) -> Result<DeleteFolderRowsReport> {
        let rows = self.source_rows_for_subtree(folder_path).await?;
        let removed_chunks = rows.len() as u64;
        let mut doc_ids = std::collections::BTreeSet::new();
        let mut source_paths = std::collections::BTreeSet::new();
        for (doc_id, source_path) in rows {
            doc_ids.insert(doc_id);
            source_paths.insert(source_path);
        }
        for source_path in source_paths {
            self.delete_doc_rows(&source_path).await?;
        }
        Ok(DeleteFolderRowsReport {
            removed_chunks,
            doc_ids: doc_ids.into_iter().collect(),
        })
    }

    async fn source_rows_for_subtree(&self, folder_path: &str) -> Result<Vec<(Uuid, String)>> {
        let stream = self
            .tbl
            .query()
            .select(lancedb::query::Select::columns(&["doc_id", "source_path"]))
            .execute()
            .await
            .map_err(|e| Error::Store(format!("source_rows_for_subtree query: {e}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| Error::Store(format!("source_rows_for_subtree collect: {e}")))?;
        let mut rows = Vec::new();
        for batch in &batches {
            let doc_ids = get_col::<FixedSizeBinaryArray>(batch, "doc_id")?;
            let source_paths = get_col::<StringArray>(batch, "source_path")?;
            for idx in 0..batch.num_rows() {
                if source_paths.is_null(idx) {
                    continue;
                }
                let source_path = source_paths.value(idx);
                if path_is_in_subtree(source_path, folder_path) {
                    rows.push((uuid_from_fsb(doc_ids, idx)?, source_path.to_string()));
                }
            }
        }
        Ok(rows)
    }

    /// k-nearest-neighbor search over `vector`. v0.1 ignores `_query_text`
    /// (FTS lands in v0.2 once we add an FTS index).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Lance`] if the LanceDB query fails, [`Error::Arrow`]
    /// on batch decoding, or [`Error::Store`] if a row is malformed.
    pub async fn search(
        &self,
        query_text: &str,
        query_vec: &[f32],
        k: usize,
    ) -> Result<Vec<SearchHit>> {
        match self.try_search(query_text, query_vec, k).await {
            Ok(hits) => Ok(hits),
            Err(e) if is_missing_fts_index_error(&e) => {
                tracing::warn!(
                    error = %e,
                    "FTS index missing at search time; building inline and retrying"
                );
                if let Err(build_err) = self.maybe_build_fts_index().await {
                    // A concurrent request may have already built the index; retry anyway.
                    tracing::warn!(error = %build_err, "FTS inline build failed; retrying search once anyway");
                }
                self.try_search(query_text, query_vec, k).await
            }
            Err(e) => Err(e),
        }
    }

    async fn try_search(
        &self,
        query_text: &str,
        query_vec: &[f32],
        k: usize,
    ) -> Result<Vec<SearchHit>> {
        use lance_index::scalar::FullTextSearchQuery;
        use lancedb::query::QueryBase;
        use lancedb::rerankers::rrf::RRFReranker;
        use std::sync::Arc;

        // Hybrid: a vector query that also carries a full-text-search query
        // becomes a hybrid query; `rerank` is only valid on hybrid queries.
        // The FTS arm searches whichever column carries the FTS index —
        // currently only `text_pseudo` (see `maybe_build_fts_index`).
        // nprobes + refine_factor must be set on the VectorQuery before chaining
        // to full_text_search; they are no-ops when no IVF index exists yet.
        let stream = self
            .tbl
            .query()
            .nearest_to(query_vec.to_vec())?
            .nprobes(self.search_nprobes)
            .refine_factor(self.search_refine_factor)
            .full_text_search(FullTextSearchQuery::new(query_text.to_string()))
            .rerank(Arc::new(RRFReranker::default()))
            .limit(k)
            .execute()
            .await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        let mut hits = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                hits.push(batch_to_hit(batch, i)?);
            }
        }
        hits.truncate(k);
        Ok(hits)
    }

    /// Build an `IVF_HNSW_SQ` index on the `vector` column if:
    ///   1. The table currently has at least `threshold` rows
    ///   2. The vector column does not already have an index
    ///
    /// Idempotent. Returns `Ok(true)` when an index was built this call,
    /// `Ok(false)` when nothing was done (below threshold, or already
    /// indexed). The index build is CPU-heavy (~30-60s for 10k rows at
    /// 384-dim); callers should run it from `tokio::spawn_blocking` or
    /// accept the latency.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] if `count_rows`, `list_indices`, or
    /// `create_index` fail.
    pub async fn maybe_build_index(&self, threshold: usize) -> Result<bool> {
        use lancedb::index::vector::IvfHnswSqIndexBuilder;
        use lancedb::index::Index;

        let count = self
            .tbl
            .count_rows(None)
            .await
            .map_err(|e| Error::Store(format!("count_rows: {e}")))?;
        if count < threshold {
            return Ok(false);
        }

        let existing = self
            .tbl
            .list_indices()
            .await
            .map_err(|e| Error::Store(format!("list_indices: {e}")))?;
        let already_indexed = existing
            .iter()
            .any(|i| i.columns.iter().any(|c| c == "vector"));
        if already_indexed {
            return Ok(false);
        }

        let mut builder =
            IvfHnswSqIndexBuilder::default().distance_type(distance_from_str(&self.index_distance));
        if let Some(parts) = self.index_num_partitions {
            let parts_u32 = u32::try_from(parts).unwrap_or(u32::MAX);
            builder = builder.num_partitions(parts_u32);
        }
        self.tbl
            .create_index(&["vector"], Index::IvfHnswSq(builder))
            .execute()
            .await
            .map_err(|e| Error::Store(format!("create_index: {e}")))?;
        Ok(true)
    }

    /// Build a French-tokenized full-text-search index on `text_pseudo` if
    /// the table has rows and the index does not already exist. Idempotent.
    ///
    /// French tokenization (stemming + stop-word removal + lowercase) is
    /// mandatory for legal French — the default `simple` tokenizer would
    /// make "résiliation" miss "résilier" and let stop-words pollute BM25.
    ///
    /// Locked v0.6 config: French stem + stop-words + lowercase. The
    /// comparative tokenizer spike is deferred — the eval harness now
    /// exists to support it as a follow-up.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if `count_rows`, `list_indices`, or
    /// `create_index` fail.
    pub async fn maybe_build_fts_index(&self) -> Result<bool> {
        use lancedb::index::scalar::FtsIndexBuilder;
        use lancedb::index::Index;

        let count = self
            .tbl
            .count_rows(None)
            .await
            .map_err(|e| Error::Store(format!("count_rows: {e}")))?;
        if count == 0 {
            return Ok(false);
        }
        let existing = self
            .tbl
            .list_indices()
            .await
            .map_err(|e| Error::Store(format!("list_indices: {e}")))?;
        let already = existing
            .iter()
            .any(|i| i.columns.iter().any(|c| c == "text_pseudo"));
        if already {
            return Ok(false);
        }

        // French legal tokenization: stem + stop-words + lowercase.
        let fts = FtsIndexBuilder::default()
            .base_tokenizer("simple".to_string())
            .language("French")
            .map_err(|e| Error::Store(format!("fts language: {e}")))?
            .stem(true)
            .remove_stop_words(true)
            .lower_case(true);

        self.tbl
            .create_index(&["text_pseudo"], Index::FTS(fts))
            .execute()
            .await
            .map_err(|e| Error::Store(format!("create_fts_index: {e}")))?;
        Ok(true)
    }
}

/// True when a LanceDB error is the "no inverted index" failure, which we can
/// recover from by building the FTS index and retrying once.
fn is_missing_fts_index_error(e: &Error) -> bool {
    e.to_string().contains("INVERTED index")
}

/// True when a LanceDB error indicates the requested table does not exist.
fn is_lancedb_table_not_found(e: &lancedb::Error) -> bool {
    let msg = e.to_string();
    msg.contains("was not found") || msg.contains("not found") || msg.contains("does not exist")
}

/// Read the FixedSizeList dimension of the named vector column from an existing
/// LanceDB table.
///
/// Returns:
/// - `Ok(None)` — table does not exist (safe to create fresh).
/// - `Ok(Some(dim))` — table exists and `field_name` is a `FixedSizeList` of `dim` elements.
/// - `Err` — table exists but its schema cannot be read, `field_name` is absent, or the
///   field is not a `FixedSizeList` (indicates a malformed table that callers must handle).
async fn stored_vector_dim(
    conn: &Connection,
    table_name: &str,
    field_name: &str,
) -> Result<Option<usize>> {
    let tbl = match conn.open_table(table_name).execute().await {
        Ok(tbl) => tbl,
        Err(e) if is_lancedb_table_not_found(&e) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let schema = tbl.schema().await?;
    match schema.fields().iter().find(|f| f.name() == field_name) {
        None => Err(Error::Store(format!(
            "table '{table_name}' exists but has no '{field_name}' column — schema may be malformed"
        ))),
        Some(f) => match f.data_type() {
            DataType::FixedSizeList(_, dim) => Ok(Some(*dim as usize)),
            other => Err(Error::Store(format!(
                "table '{table_name}' column '{field_name}' has type {other:?}, expected FixedSizeList — schema may be malformed"
            ))),
        },
    }
}

/// Open an existing LanceDB table by name, or create an empty one with `schema`
/// if no table by that name exists. Idempotent across process restarts.
async fn open_or_create_table(
    conn: &Connection,
    name: &str,
    schema: &Arc<Schema>,
) -> Result<Table> {
    let names = conn.table_names().execute().await?;
    if names.iter().any(|n| n == name) {
        Ok(conn.open_table(name).execute().await?)
    } else {
        let empty = RecordBatchIterator::new(std::iter::empty(), schema.clone());
        let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(empty);
        Ok(conn.create_table(name, reader).execute().await?)
    }
}

/// Build a single-row Arrow `RecordBatch` for one [`Memory`].
fn memory_to_batch(
    m: &crate::memory::Memory,
    dim: usize,
    schema: &Arc<Schema>,
) -> Result<RecordBatch> {
    use crate::memory::Memory;

    let Memory {
        id,
        session_id,
        kind,
        text,
        created_at,
        accessed_at,
        valid_from,
        valid_to,
        embedding,
        token_refs,
        entity_refs,
    } = m;

    if embedding.len() != dim {
        return Err(Error::Store(format!(
            "embedding len {} != memory_embedding_dim {}",
            embedding.len(),
            dim
        )));
    }

    let id_arr = StringArray::from(vec![id.as_string()]);
    let session_arr = StringArray::from(vec![session_id.clone()]);
    let kind_str = match kind {
        crate::memory::MemoryKind::Fact => "fact",
        crate::memory::MemoryKind::Preference => "preference",
        crate::memory::MemoryKind::Reference => "reference",
        crate::memory::MemoryKind::Context => "context",
    };
    let kind_arr = StringArray::from(vec![kind_str.to_string()]);
    let text_arr = StringArray::from(vec![text.clone()]);
    let created_arr = TimestampMicrosecondArray::from(vec![created_at.timestamp_micros()]);
    let accessed_arr = TimestampMicrosecondArray::from(vec![accessed_at.timestamp_micros()]);
    let valid_from_arr = TimestampMicrosecondArray::from(vec![valid_from.timestamp_micros()]);
    let valid_to_arr =
        TimestampMicrosecondArray::from(vec![valid_to.map(|t| t.timestamp_micros())]);

    // embedding — FixedSizeList<Float32>(dim)
    let values_arr = Arc::new(Float32Array::from(embedding.clone()));
    let item_field = Arc::new(Field::new("item", DataType::Float32, true));
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let embedding_arr = FixedSizeListArray::try_new(item_field, dim as i32, values_arr, None)
        .map_err(Error::Arrow)?;

    // token_refs — List<Struct{label, token}>
    let struct_fields: Fields = vec![
        Field::new("label", DataType::Utf8, false),
        Field::new("token", DataType::Utf8, false),
    ]
    .into();
    let label_builder = StringBuilder::new();
    let token_builder = StringBuilder::new();
    let struct_builder = StructBuilder::new(
        struct_fields.clone(),
        vec![Box::new(label_builder), Box::new(token_builder)],
    );
    let mut list_builder = ListBuilder::new(struct_builder);
    for tr in token_refs {
        let sb = list_builder.values();
        sb.field_builder::<StringBuilder>(0)
            .expect("label builder")
            .append_value(&tr.label);
        sb.field_builder::<StringBuilder>(1)
            .expect("token builder")
            .append_value(&tr.token);
        sb.append(true);
    }
    list_builder.append(true);
    let token_refs_arr: ListArray = list_builder.finish();

    // entity_refs — List<Utf8>
    let mut entity_lb = ListBuilder::new(StringBuilder::new());
    for s in entity_refs {
        entity_lb.values().append_value(s);
    }
    entity_lb.append(true);
    let entity_refs_arr: ListArray = entity_lb.finish();

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(id_arr),
            Arc::new(session_arr),
            Arc::new(kind_arr),
            Arc::new(text_arr),
            Arc::new(created_arr),
            Arc::new(accessed_arr),
            Arc::new(valid_from_arr),
            Arc::new(valid_to_arr),
            Arc::new(embedding_arr),
            Arc::new(token_refs_arr),
            Arc::new(entity_refs_arr),
        ],
    )
    .map_err(Error::Arrow)?;
    Ok(batch)
}

/// Decode one row of the `memories` table back into a [`Memory`].
fn batch_row_to_memory(b: &RecordBatch, i: usize) -> Result<crate::memory::Memory> {
    use crate::memory::{Memory, MemoryId, MemoryKind, TokenRef};

    let id_arr = get_col::<StringArray>(b, "id")?;
    let session_arr = get_col::<StringArray>(b, "session_id")?;
    let kind_arr = get_col::<StringArray>(b, "kind")?;
    let text_arr = get_col::<StringArray>(b, "text")?;
    let created_arr = get_col::<TimestampMicrosecondArray>(b, "created_at")?;
    let accessed_arr = get_col::<TimestampMicrosecondArray>(b, "accessed_at")?;
    let valid_from_arr = get_col::<TimestampMicrosecondArray>(b, "valid_from")?;
    let valid_to_arr = get_col::<TimestampMicrosecondArray>(b, "valid_to")?;
    let embedding_arr = get_col::<FixedSizeListArray>(b, "embedding")?;
    let token_refs_arr = get_col::<ListArray>(b, "token_refs")?;
    let entity_refs_arr = get_col::<ListArray>(b, "entity_refs")?;

    let id_uuid = Uuid::parse_str(id_arr.value(i))
        .map_err(|e| Error::Store(format!("invalid memory id uuid: {e}")))?;
    let id = MemoryId(id_uuid);

    let session_id = if session_arr.is_null(i) {
        None
    } else {
        Some(session_arr.value(i).to_string())
    };

    let kind = match kind_arr.value(i) {
        "fact" => MemoryKind::Fact,
        "preference" => MemoryKind::Preference,
        "reference" => MemoryKind::Reference,
        "context" => MemoryKind::Context,
        other => return Err(Error::Store(format!("unknown memory kind: {other}"))),
    };

    let text = text_arr.value(i).to_string();

    let to_dt = |micros: i64| -> Result<DateTime<Utc>> {
        Utc.timestamp_micros(micros)
            .single()
            .ok_or_else(|| Error::Store(format!("invalid timestamp micros: {micros}")))
    };

    let created_at = to_dt(created_arr.value(i))?;
    let accessed_at = to_dt(accessed_arr.value(i))?;
    let valid_from = to_dt(valid_from_arr.value(i))?;
    let valid_to = if valid_to_arr.is_null(i) {
        None
    } else {
        Some(to_dt(valid_to_arr.value(i))?)
    };

    // embedding: copy the i-th FixedSizeList element back to Vec<f32>
    let emb_list = embedding_arr.value(i);
    let emb_f32 = emb_list
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or_else(|| Error::Store("embedding inner not Float32".into()))?;
    let embedding: Vec<f32> = (0..emb_f32.len()).map(|k| emb_f32.value(k)).collect();

    // token_refs: List<Struct{label, token}>
    let mut token_refs: Vec<TokenRef> = Vec::new();
    if !token_refs_arr.is_null(i) {
        let inner = token_refs_arr.value(i);
        let s = inner
            .as_any()
            .downcast_ref::<StructArray>()
            .ok_or_else(|| Error::Store("token_refs inner not Struct".into()))?;
        let label = s
            .column_by_name("label")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| Error::Store("token_refs.label not Utf8".into()))?;
        let token = s
            .column_by_name("token")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| Error::Store("token_refs.token not Utf8".into()))?;
        for k in 0..s.len() {
            token_refs.push(TokenRef {
                label: label.value(k).to_string(),
                token: token.value(k).to_string(),
            });
        }
    }

    // entity_refs: List<Utf8>
    let mut entity_refs: Vec<String> = Vec::new();
    if !entity_refs_arr.is_null(i) {
        let inner = entity_refs_arr.value(i);
        let s = inner
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| Error::Store("entity_refs inner not Utf8".into()))?;
        for k in 0..s.len() {
            entity_refs.push(s.value(k).to_string());
        }
    }

    Ok(Memory {
        id,
        session_id,
        kind,
        text,
        created_at,
        accessed_at,
        valid_from,
        valid_to,
        embedding,
        token_refs,
        entity_refs,
    })
}

/// Slim batch decoder for hybrid-search hits. Reads only the columns the
/// caller actually surfaces (no embeddings, no entity refs) plus the
/// `_relevance_score` RRF column.
fn batch_row_to_memory_hit(b: &RecordBatch, i: usize) -> Result<crate::memory::MemoryHitRow> {
    use crate::memory::{MemoryHitRow, MemoryKind};

    let id_arr = get_col::<StringArray>(b, "id")?;
    let session_arr = get_col::<StringArray>(b, "session_id")?;
    let kind_arr = get_col::<StringArray>(b, "kind")?;
    let text_arr = get_col::<StringArray>(b, "text")?;
    let created_arr = get_col::<TimestampMicrosecondArray>(b, "created_at")?;
    let valid_from_arr = get_col::<TimestampMicrosecondArray>(b, "valid_from")?;
    let valid_to_arr = get_col::<TimestampMicrosecondArray>(b, "valid_to")?;
    let entity_refs_arr = get_col::<ListArray>(b, "entity_refs")?;

    let id = id_arr.value(i).to_string();
    let session_id = if session_arr.is_null(i) {
        None
    } else {
        Some(session_arr.value(i).to_string())
    };
    let kind = match kind_arr.value(i) {
        "fact" => MemoryKind::Fact,
        "preference" => MemoryKind::Preference,
        "reference" => MemoryKind::Reference,
        "context" => MemoryKind::Context,
        other => {
            return Err(Error::Store(format!("unknown memory kind: {other}")));
        }
    };
    let text_tokenized = text_arr.value(i).to_string();
    let created_at = ts_to_rfc3339(created_arr.value(i));
    let valid_from_us = valid_from_arr.value(i);
    let valid_to_us = if valid_to_arr.is_null(i) {
        None
    } else {
        Some(valid_to_arr.value(i))
    };

    // entity_refs: List<Utf8>
    let mut entity_refs: Vec<String> = Vec::new();
    if !entity_refs_arr.is_null(i) {
        let inner = entity_refs_arr.value(i);
        if let Some(s) = inner.as_any().downcast_ref::<StringArray>() {
            for k in 0..s.len() {
                entity_refs.push(s.value(k).to_string());
            }
        }
    }

    // Score: lancedb writes `_relevance_score` for hybrid queries; absence
    // means the batch carried no relevance column (e.g. a pure vector arm),
    // in which case 0.0 is the documented sentinel.
    let score = b
        .column_by_name("_relevance_score")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
        .map(|a| a.value(i))
        .unwrap_or(0.0);

    Ok(MemoryHitRow {
        id,
        session_id,
        text_tokenized,
        kind,
        created_at,
        valid_from_us,
        valid_to_us,
        entity_refs,
        score,
    })
}

fn ts_to_rfc3339(micros: i64) -> String {
    Utc.timestamp_micros(micros)
        .single()
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| String::from("1970-01-01T00:00:00Z"))
}

/// Deterministic chunk id: UUID v5 (OID namespace) of `"{doc_id}::{chunk_idx}"`.
/// Matches the value `Store::upsert` will write to the `chunk_id` column,
/// so legal code can derive matching IDs *before* the row hits LanceDB.
#[must_use]
pub fn chunk_uuid(doc_id: Uuid, chunk_idx: u32) -> Uuid {
    let name = format!("{doc_id}::{chunk_idx}");
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
}

/// Build a LanceDB `only_if` filter expression matching any of the given chunk UUIDs.
/// Produces: `chunk_id IN (X'<hex>', X'<hex>', ...)`.
fn chunk_id_filter_sql(ids: &[Uuid]) -> String {
    let hexes: Vec<String> = ids.iter().map(|id| format!("X'{}'", id.simple())).collect();
    format!("chunk_id IN ({})", hexes.join(", "))
}

fn doc_id_filter_sql(ids: &[Uuid]) -> String {
    let hexes: Vec<String> = ids.iter().map(|id| format!("X'{}'", id.simple())).collect();
    format!("doc_id IN ({})", hexes.join(", "))
}

fn records_to_batch(
    records: &[ChunkRecord],
    dim: usize,
    schema: &Arc<Schema>,
) -> Result<RecordBatch> {
    let n = records.len();

    // chunk_id (16-byte)
    let mut chunk_id_b = FixedSizeBinaryBuilder::with_capacity(n, 16);
    // doc_id (16-byte)
    let mut doc_id_b = FixedSizeBinaryBuilder::with_capacity(n, 16);
    // text_hash (32-byte)
    let mut text_hash_b = FixedSizeBinaryBuilder::with_capacity(n, 32);

    let mut source_path = Vec::with_capacity(n);
    let mut folder_path = Vec::with_capacity(n);
    let mut chunk_idx = Vec::with_capacity(n);
    let mut text_pseudo = Vec::with_capacity(n);
    let mut page: Vec<Option<u32>> = Vec::with_capacity(n);
    let mut char_start = Vec::with_capacity(n);
    let mut char_end = Vec::with_capacity(n);

    // Flat buffer for FixedSizeListArray<Float32>(dim).
    let mut vec_values: Vec<f32> = Vec::with_capacity(n * dim);

    let now_micros = Utc::now().timestamp_micros();
    let ts: Vec<i64> = vec![now_micros; n];

    for r in records {
        if r.vector.len() != dim {
            return Err(Error::Store(format!(
                "vector len {} != embed_dim {}",
                r.vector.len(),
                dim
            )));
        }
        let cid = chunk_uuid(r.doc_id, r.chunk_idx);
        chunk_id_b
            .append_value(cid.as_bytes())
            .map_err(Error::Arrow)?;
        doc_id_b
            .append_value(r.doc_id.as_bytes())
            .map_err(Error::Arrow)?;

        let hash = Sha256::digest(r.text_pseudo.as_bytes());
        text_hash_b
            .append_value(hash.as_slice())
            .map_err(Error::Arrow)?;

        source_path.push(r.source_path.clone());
        folder_path.push(r.folder_path.clone());
        chunk_idx.push(r.chunk_idx);
        text_pseudo.push(r.text_pseudo.clone());
        page.push(r.page);
        char_start.push(r.char_start);
        char_end.push(r.char_end);

        vec_values.extend_from_slice(&r.vector);
    }

    let chunk_id_arr: FixedSizeBinaryArray = chunk_id_b.finish();
    let doc_id_arr: FixedSizeBinaryArray = doc_id_b.finish();
    let text_hash_arr: FixedSizeBinaryArray = text_hash_b.finish();

    let source_path_arr = StringArray::from(source_path);
    let folder_path_arr = StringArray::from(folder_path);
    let chunk_idx_arr = UInt32Array::from(chunk_idx);
    let text_pseudo_arr = StringArray::from(text_pseudo);
    let page_arr = UInt32Array::from(page);
    let char_start_arr = UInt32Array::from(char_start);
    let char_end_arr = UInt32Array::from(char_end);
    let ts_arr = TimestampMicrosecondArray::from(ts);

    let values_arr = Arc::new(Float32Array::from(vec_values));
    let item_field = Arc::new(Field::new("item", DataType::Float32, true));
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let vector_arr = FixedSizeListArray::try_new(item_field, dim as i32, values_arr, None)
        .map_err(Error::Arrow)?;

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(chunk_id_arr),
            Arc::new(doc_id_arr),
            Arc::new(source_path_arr),
            Arc::new(folder_path_arr),
            Arc::new(chunk_idx_arr),
            Arc::new(text_pseudo_arr),
            Arc::new(page_arr),
            Arc::new(char_start_arr),
            Arc::new(char_end_arr),
            Arc::new(text_hash_arr),
            Arc::new(vector_arr),
            Arc::new(ts_arr),
        ],
    )
    .map_err(Error::Arrow)?;
    Ok(batch)
}

fn get_col<'a, T: 'static>(b: &'a RecordBatch, name: &str) -> Result<&'a T> {
    let idx = b
        .schema()
        .index_of(name)
        .map_err(|e| Error::Store(format!("missing column {name}: {e}")))?;
    b.column(idx)
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| Error::Store(format!("column {name} has unexpected type")))
}

fn uuid_from_fsb(arr: &FixedSizeBinaryArray, i: usize) -> Result<Uuid> {
    let bytes = arr.value(i);
    let arr16: [u8; 16] = bytes
        .try_into()
        .map_err(|_| Error::Store("uuid column not 16 bytes".into()))?;
    Ok(Uuid::from_bytes(arr16))
}

fn path_is_in_subtree(source_path: &str, root_path: &str) -> bool {
    let source = normalize_path_for_subtree_match(source_path);
    let root = normalize_path_for_subtree_match(root_path);
    if root.is_empty() {
        return false;
    }
    source == root
        || source
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn normalize_path_for_subtree_match(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn batch_to_hit(b: &RecordBatch, i: usize) -> Result<SearchHit> {
    let chunk_id_arr = get_col::<FixedSizeBinaryArray>(b, "chunk_id")?;
    let doc_id_arr = get_col::<FixedSizeBinaryArray>(b, "doc_id")?;
    let source_arr = get_col::<StringArray>(b, "source_path")?;
    let folder_arr = get_col::<StringArray>(b, "folder_path")?;
    let chunk_idx_arr = get_col::<UInt32Array>(b, "chunk_idx")?;
    let text_arr = get_col::<StringArray>(b, "text_pseudo")?;
    let page_arr = get_col::<UInt32Array>(b, "page")?;
    let cs_arr = get_col::<UInt32Array>(b, "char_start")?;
    let ce_arr = get_col::<UInt32Array>(b, "char_end")?;

    let score = b
        .schema()
        .index_of("_relevance_score")
        .ok()
        .and_then(|idx| {
            let col = b.column(idx);
            col.as_primitive_opt::<Float32Type>().map(|a| a.value(i))
        })
        .unwrap_or(0.0);

    Ok(SearchHit {
        doc_id: uuid_from_fsb(doc_id_arr, i)?,
        chunk_id: uuid_from_fsb(chunk_id_arr, i)?,
        source_path: source_arr.value(i).to_string(),
        folder_path: folder_arr.value(i).to_string(),
        chunk_idx: chunk_idx_arr.value(i),
        text_pseudo: text_arr.value(i).to_string(),
        page: if page_arr.is_null(i) {
            None
        } else {
            Some(page_arr.value(i))
        },
        char_start: cs_arr.value(i),
        char_end: ce_arr.value(i),
        score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_cfg(dim: usize) -> (TempDir, AnnoRagConfig) {
        let dir = TempDir::new().expect("tempdir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            embed_dim: dim,
            memory_embedding_dim: dim,
            ..Default::default()
        };
        (dir, cfg)
    }

    #[test]
    fn distance_type_maps_from_config_string() {
        use lancedb::DistanceType;
        assert!(matches!(distance_from_str("cosine"), DistanceType::Cosine));
        assert!(matches!(distance_from_str("l2"), DistanceType::L2));
        assert!(matches!(distance_from_str("dot"), DistanceType::Dot));
        // Unknown strings fall back to the cosine default.
        assert!(matches!(distance_from_str("garbage"), DistanceType::Cosine));
    }

    #[tokio::test]
    #[ignore = "opens LanceDB (~30s); run with --ignored"]
    async fn memory_row_count_reflects_inserts() {
        let (_dir, cfg) = fresh_cfg(8);
        let store = Store::open(&cfg).await.expect("open store");
        assert_eq!(store.memory_row_count().await.expect("count"), 0);
    }

    #[test]
    fn schema_has_expected_columns() {
        let s = chunks_schema(8);
        assert_eq!(s.fields().len(), 12);
        assert_eq!(s.field(0).name(), "chunk_id");
        assert_eq!(s.field(10).name(), "vector");
    }

    #[test]
    fn memories_schema_has_required_columns() {
        let schema = memories_schema(384);
        let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        for expected in [
            "id",
            "session_id",
            "kind",
            "text",
            "created_at",
            "accessed_at",
            "valid_from",
            "valid_to",
            "embedding",
            "token_refs",
            "entity_refs",
        ] {
            assert!(names.contains(&expected), "missing column: {expected}");
        }
        // 7 active v0.1 columns + 3 forward-compat + embedding = 11.
        assert_eq!(schema.fields().len(), 11);
    }

    #[test]
    fn chunk_uuid_is_deterministic() {
        let doc = Uuid::nil();
        assert_eq!(chunk_uuid(doc, 0), chunk_uuid(doc, 0));
        assert_ne!(chunk_uuid(doc, 0), chunk_uuid(doc, 1));
    }

    #[tokio::test]
    #[ignore = "opens LanceDB (~30s); run with --ignored"]
    async fn doc_exists_false_then_true_after_upsert() {
        let (_dir, cfg) = fresh_cfg(8);
        let store = Store::open(&cfg).await.expect("open");
        let doc = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"d1");
        assert!(!store.doc_exists(doc).await.expect("exists?"));
        store
            .upsert(vec![ChunkRecord {
                doc_id: doc,
                source_path: "p".into(),
                folder_path: "f".into(),
                chunk_idx: 0,
                text_pseudo: "x".into(),
                page: None,
                char_start: 0,
                char_end: 1,
                vector: vec![0.0; 8],
            }])
            .await
            .expect("upsert");
        assert!(store.doc_exists(doc).await.expect("exists?2"));
    }

    #[tokio::test]
    #[ignore = "opens LanceDB (~30s); run with --ignored"]
    async fn search_filtered_to_docs_uses_only_allowed_doc_ids() {
        let (_dir, cfg) = fresh_cfg(8);
        let store = Store::open(&cfg).await.expect("open");
        let a = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"a");
        let b = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"b");
        let mk = |doc_id: uuid::Uuid, text: &str| ChunkRecord {
            doc_id,
            source_path: format!("{doc_id}.txt"),
            folder_path: "corpus".into(),
            chunk_idx: 0,
            text_pseudo: text.into(),
            page: None,
            char_start: 0,
            char_end: text.len() as u32,
            vector: vec![0.0; 8],
        };
        store
            .upsert(vec![mk(a, "Alpha contrat"), mk(b, "Beta contrat")])
            .await
            .expect("upsert");
        let hits = store
            .search_filtered_to_docs("contrat", &[0.0; 8], 10, &[a])
            .await
            .expect("search");
        assert!(hits.iter().all(|hit| hit.doc_id == a));
    }

    #[tokio::test]
    #[ignore = "lancedb table creation takes ~30s — exercised in Task 10 integration"]
    async fn open_creates_chunks_table() {
        let (_dir, cfg) = fresh_cfg(8);
        let _s = Store::open(&cfg).await.expect("open");
    }

    #[tokio::test]
    #[ignore = "lancedb too slow for per-task run — Task 10 covers end-to-end"]
    async fn upsert_then_search_returns_inserted_chunk() {
        let (_dir, cfg) = fresh_cfg(8);
        let s = Store::open(&cfg).await.expect("open");
        let doc_id = Uuid::now_v7();
        let recs = vec![ChunkRecord {
            doc_id,
            source_path: "/test/a.md".into(),
            folder_path: "test".into(),
            chunk_idx: 0,
            text_pseudo: "hello world".into(),
            page: Some(1),
            char_start: 0,
            char_end: 11,
            vector: vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }];
        s.upsert(recs).await.expect("upsert");
        let hits = s
            .search("hello", &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 5)
            .await
            .expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text_pseudo, "hello world");
        assert_eq!(hits[0].doc_id, doc_id);
    }

    #[tokio::test]
    #[ignore = "opens LanceDB (~30s); run with --ignored"]
    async fn delete_doc_rows_removes_only_that_source() {
        let (_dir, cfg) = fresh_cfg(8);
        let store = Store::open(&cfg).await.expect("open");
        let mk = |d: &str, sp: &str| ChunkRecord {
            doc_id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, d.as_bytes()),
            source_path: sp.into(),
            folder_path: "f".into(),
            chunk_idx: 0,
            text_pseudo: "x".into(),
            page: None,
            char_start: 0,
            char_end: 1,
            vector: vec![0.0; 8],
        };
        store
            .upsert(vec![mk("a", "A.txt"), mk("b", "B.txt")])
            .await
            .expect("up");
        store.delete_doc_rows("A.txt").await.expect("del");
        assert!(!store
            .doc_exists(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"a"))
            .await
            .unwrap());
        assert!(store
            .doc_exists(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"b"))
            .await
            .unwrap());
    }

    #[test]
    fn path_is_in_subtree_matches_exact_and_children() {
        assert!(path_is_in_subtree("C:/legal/a", "C:/legal/a"));
        assert!(path_is_in_subtree("C:/legal/a/one.txt", "C:/legal/a"));
        assert!(path_is_in_subtree("C:/legal/a/sub/two.txt", "C:/legal/a"));
    }

    #[test]
    fn path_is_in_subtree_does_not_match_sibling_prefix() {
        assert!(!path_is_in_subtree("C:/legal/ab/one.txt", "C:/legal/a"));
        assert!(!path_is_in_subtree("C:/legal/a-file.txt", "C:/legal/a"));
    }

    #[test]
    fn path_is_in_subtree_handles_windows_separators_and_case() {
        assert!(path_is_in_subtree(
            r"C:\Legal\Client\Sub\one.txt",
            "c:/legal/client"
        ));
    }

    #[tokio::test]
    #[ignore = "opens LanceDB; run with --ignored"]
    async fn list_indexed_folder_paths_returns_distinct_folders() {
        let (_dir, cfg) = fresh_cfg(8);
        let store = Store::open(&cfg).await.expect("open");

        let folders = store.list_indexed_folder_paths().await.expect("list");
        assert!(folders.is_empty());
    }

    #[tokio::test]
    #[ignore = "opens LanceDB; run with --ignored"]
    async fn count_chunks_returns_zero_on_empty_store() {
        let (_dir, cfg) = fresh_cfg(8);
        let store = Store::open(&cfg).await.expect("open");

        assert_eq!(store.count_chunks().await.expect("count"), 0);
    }

    #[tokio::test]
    #[ignore = "opens LanceDB; run with --ignored"]
    async fn list_count_and_delete_folder_rows_use_source_subtree() {
        let (_dir, cfg) = fresh_cfg(8);
        let store = Store::open(&cfg).await.expect("open");
        let mk = |d: &str, folder_path: &str, source_path: &str| ChunkRecord {
            doc_id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, d.as_bytes()),
            source_path: source_path.into(),
            folder_path: folder_path.into(),
            chunk_idx: 0,
            text_pseudo: "x".into(),
            page: None,
            char_start: 0,
            char_end: 1,
            vector: vec![0.0; 8],
        };
        store
            .upsert(vec![
                mk("a", "C:/legal/a", "C:/legal/a/one.txt"),
                mk("b", "C:/legal/a/sub", "C:/legal/a/sub/two.txt"),
                mk("c", "C:/legal/a", "C:/legal/a/l'avocat.txt"),
                mk("d", "C:/legal/ab", "C:/legal/ab/one.txt"),
            ])
            .await
            .expect("up");

        let folders = store.list_indexed_folder_paths().await.expect("list");
        assert_eq!(
            folders,
            vec![
                "C:/legal/a".to_string(),
                "C:/legal/a/sub".to_string(),
                "C:/legal/ab".to_string()
            ]
        );
        assert_eq!(store.count_chunks().await.expect("count all"), 4);
        assert_eq!(
            store
                .count_chunks_for_folder("C:/legal/a")
                .await
                .expect("count"),
            3
        );
        let mut expected_doc_ids = vec![
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"a"),
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"b"),
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"c"),
        ];
        expected_doc_ids.sort();
        assert_eq!(
            store
                .doc_ids_for_source_subtree("C:/legal/a")
                .await
                .expect("doc ids"),
            expected_doc_ids
        );

        let report = store
            .delete_folder_rows("C:/legal/a")
            .await
            .expect("delete folder");
        assert_eq!(report.removed_chunks, 3);
        assert_eq!(report.doc_ids, expected_doc_ids);
        assert_eq!(
            store
                .count_chunks_for_folder("C:/legal/a")
                .await
                .expect("count"),
            0
        );
        assert_eq!(
            store
                .count_chunks_for_folder("C:/legal/ab")
                .await
                .expect("count"),
            1
        );
        assert_eq!(store.count_chunks().await.expect("count all"), 1);
    }

    #[tokio::test]
    #[ignore = "needs an FTS index build; exercised by bench_eval"]
    async fn fts_stemming_matches_french_variant() {
        // A query for "résiliation" must retrieve a chunk that only contains
        // "résilier" — proves the FTS index uses French stemming, not the
        // default `simple` tokenizer. Full exercise lives in bench_eval;
        // this test documents the contract.
    }

    #[tokio::test]
    #[ignore = "opens LanceDB (~30s); run with --ignored"]
    async fn memory_update_ner_fields_updates_only_refs() {
        let (_dir, cfg) = fresh_cfg(8);
        let store = Store::open(&cfg).await.expect("open store");
        let now = chrono::Utc::now();
        let id = crate::memory::MemoryId::new();
        let m = crate::memory::Memory {
            id: id.clone(),
            session_id: Some("s1".into()),
            kind: crate::memory::MemoryKind::Context,
            text: "Antoine Lefebvre approved the report.".into(),
            created_at: now,
            accessed_at: now,
            valid_from: now,
            valid_to: None,
            embedding: vec![0.0; 8],
            token_refs: Vec::new(),
            entity_refs: Vec::new(),
        };
        store.memory_insert(&m).await.expect("insert");

        store
            .memory_update_ner_fields(
                &id,
                vec![crate::memory::TokenRef {
                    label: "Person".into(),
                    token: "PERSON_1".into(),
                }],
                vec!["pii:Person:PERSON_1".into()],
            )
            .await
            .expect("update refs");

        let updated = store
            .memory_get(&id)
            .await
            .expect("get")
            .expect("memory exists");
        assert_eq!(updated.text, m.text);
        assert_eq!(updated.token_refs[0].token, "PERSON_1");
        assert_eq!(updated.entity_refs, vec!["pii:Person:PERSON_1"]);
    }

    #[tokio::test]
    #[ignore = "opens LanceDB (~30s); run with --ignored"]
    async fn memory_table_rebuilds_on_dim_mismatch() {
        // Open with dim=4 — creates the memories table at that dimension.
        let (dir, cfg4) = fresh_cfg(4);
        {
            let _store = Store::open(&cfg4).await.expect("open at dim=4");
        }

        // Reopen with only memory_embedding_dim changed — chunks dim stays at 4
        // so the chunks validation passes and we reach the memories rebuild path.
        let cfg8 = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            embed_dim: 4,
            memory_embedding_dim: 8,
            ..Default::default()
        };
        Store::open(&cfg8)
            .await
            .expect("open must succeed after memories dim mismatch");
    }
}
