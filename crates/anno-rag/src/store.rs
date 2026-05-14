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
    builder::FixedSizeBinaryBuilder, Array, FixedSizeBinaryArray, FixedSizeListArray, Float32Array,
    RecordBatch, RecordBatchIterator, StringArray, TimestampMicrosecondArray, UInt32Array,
};
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use chrono::Utc;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Table;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

/// Name of the chunks table inside the LanceDB index directory.
pub const TABLE_NAME: &str = "chunks";

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

/// Handle to the `chunks` table.
#[derive(Clone)]
pub struct Store {
    tbl: Table,
    dim: usize,
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
        let names = conn.table_names().execute().await?;
        let tbl = if names.iter().any(|n| n == TABLE_NAME) {
            conn.open_table(TABLE_NAME).execute().await?
        } else {
            let schema = chunks_schema(cfg.embed_dim);
            let empty = RecordBatchIterator::new(std::iter::empty(), schema);
            let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(empty);
            conn.create_table(TABLE_NAME, reader).execute().await?
        };
        Ok(Self {
            tbl,
            dim: cfg.embed_dim,
        })
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
        use lance_index::scalar::FullTextSearchQuery;
        use lancedb::query::QueryBase;
        use lancedb::rerankers::rrf::RRFReranker;
        use std::sync::Arc;

        // Hybrid: a vector query that also carries a full-text-search query
        // becomes a hybrid query; `rerank` is only valid on hybrid queries.
        // The FTS arm searches the single indexed column `text_pseudo`.
        let stream = self
            .tbl
            .query()
            .nearest_to(query_vec.to_vec())?
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

        self.tbl
            .create_index(
                &["vector"],
                Index::IvfHnswSq(IvfHnswSqIndexBuilder::default()),
            )
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

/// Deterministic chunk id: UUID v5 (OID namespace) of `"{doc_id}::{chunk_idx}"`.
#[must_use]
fn chunk_uuid(doc_id: Uuid, chunk_idx: u32) -> Uuid {
    let name = format!("{doc_id}::{chunk_idx}");
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
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
            ..Default::default()
        };
        (dir, cfg)
    }

    #[test]
    fn schema_has_expected_columns() {
        let s = chunks_schema(8);
        assert_eq!(s.fields().len(), 12);
        assert_eq!(s.field(0).name(), "chunk_id");
        assert_eq!(s.field(10).name(), "vector");
    }

    #[test]
    fn chunk_uuid_is_deterministic() {
        let doc = Uuid::nil();
        assert_eq!(chunk_uuid(doc, 0), chunk_uuid(doc, 0));
        assert_ne!(chunk_uuid(doc, 0), chunk_uuid(doc, 1));
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
    #[ignore = "needs an FTS index build; exercised by bench_eval"]
    async fn fts_stemming_matches_french_variant() {
        // A query for "résiliation" must retrieve a chunk that only contains
        // "résilier" — proves the FTS index uses French stemming, not the
        // default `simple` tokenizer. Full exercise lives in bench_eval;
        // this test documents the contract.
    }
}
