# Anno Knowledge — Vector Projection & Semantic Search (Phase 3) Design

**Date:** 2026-06-02
**Status:** Draft for review
**Scope:** Add a LanceDB vector projection (`knowledge_chunks_v1`) alongside the Phase 2 SQLite FTS plane, extend `knowledge_sync` to embed pseudonymized chunks after the FTS write, and implement `knowledge_search(mode=semantic)` as RRF fusion over FTS + vectors. After this phase the knowledge index supports both keyword and meaning-based queries with the same privacy guarantees.

**Parent spec:** [`2026-05-29-anno-local-knowledge-service-multisource-design.md`](2026-05-29-anno-local-knowledge-service-multisource-design.md)
**Builds on:** [`2026-06-01-anno-knowledge-local-folder-source-phase2-design.md`](2026-06-01-anno-knowledge-local-folder-source-phase2-design.md)

---

## 1. Prerequisite

Phase 1 (foundation + 3 MCP tools) and Phase 2 (local folder source + privacy API) are merged on `main` (commit `87aeff2b` or later). The `Pipeline::pseudonymize_knowledge_object` API, the `KnowledgeIndexer`, and `knowledge_sync` MCP tool are in place. The knowledge index currently contains pseudonymized chunks reachable only through FTS5 keyword search.

## 2. Goal

Today, `knowledge_search(mode=semantic)` silently degrades to fast/FTS — there are no vectors to search. The Phase 2 architecture left explicit hooks for this: an `ObjectState::VectorIndexed` variant, `KnowledgeSearchMode::Semantic`, a `Vec<KnowledgeSearchHit>` shape that already carries `score`, and a `knowledge_chunks_v1` LanceDB target named in the parent spec but unimplemented.

Phase 3 closes that loop:

```text
knowledge_sync       -> walks → extracts → pseudonymizes → FTS write [Phase 2]
                                          ↘ embeds pseudonymized chunks → LanceDB vectors  [NEW]

knowledge_search(semantic)
                     -> pseudonymize query → embed query → parallel FTS + LanceDB cosine
                     -> RRF merge (k=60)   → SQLite hydrate snippets → pseudo hits
```

The product promise stays intact: raw text and the user query are pseudonymized through the local vault before reaching either index, and Claude receives pseudonymized snippets only.

## 3. Decisions Locked In Brainstorming

These were chosen explicitly and are not open for re-litigation in the plan:

- **Vectors are the next slice (option A).** Phase 2 shipped indexable content but `knowledge_search(fast)` is the only working mode. Adding more data sources (Outlook) without semantic search would scale the index without scaling its value.
- **Extended sync (option A again).** `knowledge_sync` runs the vector pass synchronously after the FTS write, in the same call. One UX command, one progress summary. No separate `knowledge_vectorize` tool.
- **Hybrid RRF (option A).** `knowledge_search(mode=semantic)` runs FTS and vector search in parallel, then merges with Reciprocal Rank Fusion (k=60). Pure semantic loses exact-match precision; FTS-as-filter risks dropping good conceptual matches.
- **State machine extension.** Phase 2's terminal state `fts_ready` becomes an intermediate state. Phase 3 adds `vector_pending` and `vector_ready` (the parent spec §9.9 lists both).
- **Search-side privacy.** The user query must be pseudonymized *before* embedding. A raw-text query vector lives in a different semantic space than pseudonymized chunks (recall collapse) and leaks PII to LanceDB. This is mandatory, not optional.
- **No new MCP tools.** All three Phase 1/2 tools (`knowledge_search`, `knowledge_sync`, `knowledge_status`) gain semantics; surface count is unchanged.

## 4. Non-Goals

- No `Deep` rerank mode. `Deep` queries map to `Semantic` for now (graceful degradation); a local reranker arrives in a later phase.
- No `Auto` mode. Default remains `Fast` so no implicit model load happens for casual queries.
- No re-embed pipeline when the embedding model changes. We record `embedding_model` + `embedding_fingerprint` on each row so a future phase can detect stale vectors; we do not yet *act* on the detection.
- No background job queue or worker daemon. Sync stays synchronous bounded (S1 from Phase 2). Vector pass joins the same call.
- No async embedding I/O optimization. `embedder.embed_batch` is synchronous CPU work; we run it inside the existing sync path.
- No changes to the existing LanceDB `chunks` table, `Pipeline::ingest_folder`, `Pipeline::search`, or any `legal_*` path.

## 5. Architecture

The new code reuses three existing patterns:

| Existing | Reused For | Reference |
|------|------------|-----------|
| `Embedder` lazy init via `Pipeline::embedder()` | Both batch embed (chunks) and query embed | `crates/anno-rag/src/pipeline.rs:151` |
| `GLiNER2Fastino` PII subset detection via `detect_for_ingest` | `pseudonymize_query` (single-shot, no chunks) | `crates/anno-rag/src/detect.rs:235` |
| `anno-rag-tabular::storage` autonomous scoped LanceDB store | `KnowledgeVectorStore` shape (own connection, own schema, own batch types, no coupling to `anno-rag::Store`) | `crates/anno-rag-tabular/src/storage/` |

### 5.1 Components

| Unit | Crate | Responsibility | Depends on |
|------|-------|----------------|-----------|
| `KnowledgeVectorStore` | `anno-knowledge-store` *(addition)* | LanceDB connection, schema, `upsert_vectors`, `search(query_vec, top_k)`, `delete_object`. Autonomous — does NOT import `anno-rag::Store`. | `lancedb`, `arrow-array`, `arrow-schema`, `anno-knowledge-core` |
| `knowledge_chunks_schema` | `anno-knowledge-store` *(addition)* | Arrow schema for `knowledge_chunks_v1` (separate file, pure constructor function) | `arrow-schema` |
| `KnowledgeControlStore::mark_vector_ready` / `mark_vector_pending` / `pending_vector_objects` / `vector_lag_count` | `anno-knowledge-store` *(addition)* | State machine transitions and backfill lookup | `rusqlite` (already in crate) |
| `Pipeline::embed_pseudonymized_chunks` | `anno-rag` *(addition to `knowledge_privacy.rs`)* | Thin wrapper over `embedder().embed_batch(&[String])` — applies the e5 `"passage: "` prefix | existing `embed.rs` |
| `Pipeline::pseudonymize_query` | `anno-rag` *(addition to `knowledge_privacy.rs`)* | Single-shot pseudo of user query string via PII subset of GLiNER2 + vault. No offset map, no chunks. | existing `detect.rs` + `vault.rs` |
| `KnowledgeIndexer::vector_pass` *(extension)* | `anno-rag-mcp` | Called after `commit_object` per object: try embed + upsert, else mark vector_pending | new `KnowledgeVectorStore` + new `Pipeline` APIs |
| `KnowledgeService::search` *(extension)* | `anno-rag-mcp` | New `search_semantic` path: pseudonymize query → embed → parallel FTS + vec → RRF → SQLite hydrate | both stores + `Pipeline` |

### 5.2 Why an autonomous vector store

The `anno-rag-tabular` crate sets the precedent: a workflow-specific subsystem owns its own LanceDB connection, its own Arrow schema, and its own batch types. The main `Store` is for the legal/ingest_folder path. Following that pattern keeps `KnowledgeVectorStore` independently testable, avoids growing `Store`'s already large blast radius, and matches the parent spec §8 dependency direction (`anno-knowledge-store` depends on `rusqlite` + `lancedb`, not on `anno-rag`).

## 6. `knowledge_chunks_v1` LanceDB table

Path: `cfg.index_path() / "knowledge_chunks_v1"`. Separate from `chunks` — zero migration risk.

```rust
// crates/anno-knowledge-store/src/vector_schema.rs
use std::sync::Arc;
use arrow_schema::{DataType, Field, Schema, TimeUnit};

pub const TABLE_NAME: &str = "knowledge_chunks_v1";
pub const EMBED_DIM: i32 = 384; // matches AnnoRagConfig::embed_dim (e5-small)

pub fn knowledge_chunks_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("chunk_id",              DataType::FixedSizeBinary(16), false),
        Field::new("object_id",             DataType::FixedSizeBinary(16), false),
        Field::new("revision_id",           DataType::FixedSizeBinary(16), false),
        Field::new("source_kind",           DataType::Utf8, false),
        Field::new("object_type",           DataType::Utf8, false),
        Field::new("title_pseudo",          DataType::Utf8, true),
        Field::new("indexed_at",            DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("embedding_model",       DataType::Utf8, false),
        Field::new("embedding_fingerprint", DataType::Utf8, false),
        Field::new("vector", DataType::FixedSizeList(
            Arc::new(Field::new("item", DataType::Float32, false)),
            EMBED_DIM,
        ), false),
    ]))
}
```

Design rationale:

- **No `text_pseudo` / `metadata_pseudo` in LanceDB.** SQLite already stores them for FTS. Storing again in LanceDB would duplicate bytes and complicate `forget`. Hydration at search time joins on `chunk_id` against SQLite.
- **`title_pseudo` *is* in LanceDB.** Short, already pseudonymized, materially improves result inspection at zero cost.
- **`object_id` + `revision_id` carried.** Enables filter pushdown (e.g., "search only within source X") in a later phase without a SQLite round-trip.
- **`embedding_model` + `embedding_fingerprint`.** Stale vector detection is a future phase; recording them now is cheap and avoids a schema migration later.
- **`acl_hash` from parent spec §11.3 is omitted.** ACLs are an Outlook-era concern; we add them when the source connector that needs them lands.

## 7. State machine

Phase 2 currently writes objects directly to state `fts_ready`. Phase 3 promotes `fts_ready` to an intermediate state with two terminal paths:

```text
discovered → extracted → pseudonymized → fts_ready ┬→ vector_ready    (embedder available)
                                                   └→ vector_pending → (re-sync) → vector_ready
```

State transitions are owned by `KnowledgeControlStore`. `commit_object` still ends at `fts_ready`. The new `vector_pass` then calls one of:

- `mark_vector_ready(object_id)` after a successful upsert
- `mark_vector_pending(object_id)` after an embedder or upsert failure

Backfill query: `pending_vector_objects(scope_id, limit)` returns objects in state `fts_ready` *or* `vector_pending` that have no row in the current revision of `knowledge_chunks_v1`. The indexer ratchets through them on every sync until the embedder is reachable.

## 8. Privacy primitives (anno-rag additions)

```rust
// crates/anno-rag/src/knowledge_privacy.rs (additions)

impl Pipeline {
    /// Embed pseudonymized chunks with the e5 "passage: " prefix.
    /// Loads the embedder on demand; vectors are L2-normalized.
    ///
    /// # Errors
    /// Returns `Error::Embed` on tokenization or forward-pass failure;
    /// returns embedder-load errors when the model is unavailable.
    pub async fn embed_pseudonymized_chunks(
        &self,
        chunks: &[PseudonymizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        let embedder = self.embedder().await?;
        let texts: Vec<String> = chunks.iter().map(|c| c.text_pseudo.clone()).collect();
        embedder.embed_batch(&texts)
    }

    /// Pseudonymize a user query for semantic search.
    /// Single-shot: no chunk machinery, no offset map. Returns the pseudonymized string.
    /// Uses PII subset only (empty legal_labels) — same precedent as `pseudonymize_knowledge_object`.
    ///
    /// # Errors
    /// Returns detector or vault errors.
    pub async fn pseudonymize_query(&self, query: &str) -> Result<String> {
        let detector = self.detector_get_or_init()?;
        let no_legal: Vec<crate::legal::LegalLabel> = Vec::new();
        let no_thresholds: std::collections::HashMap<&'static str, f32> = Default::default();
        let bundle = detector.detect_for_ingest(query, &no_legal, &no_thresholds)?;
        let (pseudo, _map) = self.vault.pseudonymize_with_map(query, &bundle.pii).await?;
        Ok(pseudo)
    }
}
```

The e5 prefix discipline is mandatory: `embed_batch` already prepends `"passage: "`, `embed_query` prepends `"query: "`. Mixing them measurably degrades retrieval. Phase 3 uses `embed_batch` for chunk indexing (correct) and `embed_query` for the user query at search time (correct).

## 9. Vector store API (`anno-knowledge-store`)

```rust
// crates/anno-knowledge-store/src/vector_store.rs

pub struct KnowledgeVectorStore {
    table: lancedb::Table,
    embed_dim: usize,
}

#[derive(Debug, Clone)]
pub struct VectorUpsertBatch {
    pub object_id: ObjectId,
    pub revision_id: RevisionId,
    pub source_kind: SourceKind,
    pub object_type: ObjectType,
    pub title_pseudo: Option<String>,
    pub chunks: Vec<CommitChunk>,  // for chunk_id, chunk_idx
    pub vectors: Vec<Vec<f32>>,    // same len as chunks
    pub embedding_model: String,
    pub embedding_fingerprint: String,
}

impl KnowledgeVectorStore {
    /// Open or create the LanceDB table at `path` with the knowledge schema.
    pub async fn open(path: impl AsRef<Path>, embed_dim: usize) -> Result<Self>;

    /// Replace all vectors for the given object_id (delete + insert).
    /// Atomic at the LanceDB transaction level.
    pub async fn upsert_vectors(&self, batch: VectorUpsertBatch) -> Result<()>;

    /// Cosine top-k. Vectors are L2-normalized so LanceDB's default L2 metric
    /// produces the same ordering as cosine similarity.
    pub async fn search(&self, query_vec: &[f32], top_k: usize)
        -> Result<Vec<(ChunkId, f32)>>;

    /// Delete all chunks for an object_id. Called from forget paths.
    pub async fn delete_object(&self, object_id: &ObjectId) -> Result<u64>;
}
```

Schema-aware design: `upsert_vectors` requires `chunks.len() == vectors.len()` and that each vector has length `embed_dim`. Mismatches return errors before LanceDB sees the batch.

## 10. Extended sync flow

```rust
// crates/anno-rag-mcp/src/indexer.rs — after the Phase 2 commit_object block

// 5. NEW: vector pass (best-effort)
match pipeline.embed_pseudonymized_chunks(&pseudo).await {
    Ok(vectors) => {
        let batch = VectorUpsertBatch {
            object_id, revision_id,
            source_kind: SourceKind::LocalFolder,
            object_type: obj.object_type,
            title_pseudo: pseudo.first().and_then(|p| p.title_pseudo.clone()),
            chunks: chunks.clone(),
            vectors,
            embedding_model: cfg.embed_model.clone(),
            embedding_fingerprint: embedding_fingerprint(&cfg.embed_model),
        };
        match vector_store.upsert_vectors(batch).await {
            Ok(()) => {
                store.mark_vector_ready(&object_id)?;
                summary.vector_ready += 1;
            }
            Err(e) => {
                tracing::warn!(error = %e, "vector upsert failed");
                store.mark_vector_pending(&object_id)?;
                summary.vector_pending += 1;
            }
        }
    }
    Err(_) => {
        // Embedder unavailable (model not downloaded yet) — FTS already works
        store.mark_vector_pending(&object_id)?;
        summary.vector_pending += 1;
    }
}
```

`embedding_fingerprint` is a deterministic helper (e.g. `format!("{}@v1", model_name)` or a hash of model weights once available). Each row records which model produced it.

Extended `SyncSummary`:
```rust
#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncSummary {
    // Phase 2 fields unchanged
    pub seen: u64,
    pub skipped_unchanged: u64,
    pub extracted: u64,
    pub pseudonymized: u64,
    pub fts_ready: u64,
    pub forgotten: u64,
    pub failed: u64,
    pub truncated: bool,
    // Phase 3 additions
    pub vector_ready: u64,
    pub vector_pending: u64,
}
```

Backfill on next sync: before the discover loop, the indexer calls `pending_vector_objects(scope, limit)` and replays the vector pass for each — re-embedding chunks read back from SQLite without re-extracting. Once the embedder becomes available, a single subsequent `knowledge_sync` brings the index to full `vector_ready` state.

## 11. Search flow (semantic)

```rust
async fn search_semantic(
    &self,
    pipeline: &Pipeline,
    params: KnowledgeSearchParams,
) -> Result<KnowledgeSearchResponse> {
    // 1. Pseudonymize query (critical: same space as indexed chunks)
    let pseudo = pipeline.pseudonymize_query(&params.query).await?;

    // 2. Embed with "query: " prefix
    let embedder = pipeline.embedder().await?;
    let query_vec = embedder.embed_query(&pseudo)?;

    // 3. Parallel FTS + vector top-N (N >> top_k for RRF pool)
    let over_fetch = (params.top_k * 5).max(20);
    let req = KnowledgeSearchRequest::new(pseudo.clone()).with_top_k(over_fetch);
    let (fts_hits, vec_hits) = tokio::join!(
        async { self.store.search_fast(&req) },
        async { self.vector_store.as_ref().unwrap().search(&query_vec, over_fetch).await },
    );
    let fts_hits = fts_hits?;
    let vec_hits = vec_hits?;

    // 4. RRF merge (k=60, the standard constant)
    let ranked_ids = rrf_merge(&fts_hits, &vec_hits, 60, params.top_k);

    // 5. SQLite hydrate (title_pseudo + snippet) for the merged chunk_ids
    let hits = self.store.hydrate_chunks(&ranked_ids, &pseudo)?;

    Ok(KnowledgeSearchResponse { mode: "semantic".into(), hits })
}

fn rrf_merge(
    fts: &[KnowledgeSearchHit],
    vec: &[(ChunkId, f32)],
    k: usize,
    top_k: usize,
) -> Vec<ChunkId> {
    use std::collections::HashMap;
    let mut scores: HashMap<ChunkId, f32> = HashMap::new();
    for (rank, h) in fts.iter().enumerate() {
        *scores.entry(h.chunk_id).or_insert(0.0) += 1.0 / (k as f32 + rank as f32 + 1.0);
    }
    for (rank, (cid, _)) in vec.iter().enumerate() {
        *scores.entry(*cid).or_insert(0.0) += 1.0 / (k as f32 + rank as f32 + 1.0);
    }
    let mut ranked: Vec<(ChunkId, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked.into_iter().take(top_k).map(|(c, _)| c).collect()
}
```

A new `KnowledgeControlStore::hydrate_chunks(&[ChunkId], &str)` rebuilds the snippets from the same FTS expression on the merged candidate set. This keeps snippet quality consistent across modes.

`Fast` mode is unchanged. `Deep` mode dispatches to `search_semantic` (rerank lands later).

## 12. `KnowledgeService` lifecycle changes

```rust
pub struct KnowledgeService {
    store: KnowledgeControlStore,
    vector_store: tokio::sync::OnceCell<Arc<KnowledgeVectorStore>>,
}
```

Lazy init via a private helper:
```rust
async fn vector_store(&self, cfg: &AnnoRagConfig)
    -> Result<&KnowledgeVectorStore>
{
    self.vector_store
        .get_or_try_init(|| async {
            KnowledgeVectorStore::open(cfg.index_path(), cfg.embed_dim)
                .await.map(Arc::new)
        })
        .await
        .map(|arc| arc.as_ref())
}
```
- `add_local_folder`, `status`, `search(fast)`, `forget_source` never call this helper — Phase 1/2 behavior preserved
- `sync`, `search(semantic)`, and the new `forget` cascade call it on demand

Open errors surface as `vector_store_unavailable` in summary/response (LanceDB filesystem issues, not model issues — those become `models_missing`).

## 13. Status surface

```rust
// crates/anno-knowledge-core/src/status.rs (extension)
pub struct KnowledgeStatus {
    pub sources: u64,
    pub accounts: u64,
    pub scopes: u64,
    pub objects: u64,
    pub chunks: u64,
    pub failures: u64,
    pub models_loaded: bool,
    // Phase 3 additions
    pub vector_ready: u64,
    pub vector_pending: u64,
    pub embedding_model: Option<String>,
}
```

`embedding_model` is populated from the most recent row in `knowledge_chunks_v1`; `None` means no vectors yet.

## 14. Forget path consistency

`knowledge_forget` (Phase 2) currently cascades SQLite and FTS. Phase 3 extends it: when forgetting an object/scope/source, also call `vector_store.delete_object(object_id)` for each affected object. If the vector store is unreachable, the SQLite cascade still completes and the dangling LanceDB rows are reaped by a future sync. The `forget` outcome reports `vector_rows_removed`.

## 15. Error categories

In addition to the Phase 2 categories:

```text
embedder_missing       -- equivalent to models_missing for the vector path
vector_store_io        -- LanceDB filesystem / schema issue
vector_dimension       -- vector length != embed_dim
```

`knowledge_sync` continues to return `models_missing` as the user-facing error code when the *detector* is unavailable (FTS itself cannot proceed). When only the *embedder* is missing, `sync` returns success with `vector_pending` counts — no error, just degraded state.

## 16. Test strategy

| Test | Crate | Model-gated | Covers |
|------|-------|:-----------:|--------|
| `vector_store_open_creates_table` | `anno-knowledge-store` | no | LanceDB connect + schema |
| `upsert_then_search_returns_nearest` | `anno-knowledge-store` | no | cosine top-k with synthetic vectors |
| `upsert_replaces_prior_vectors_for_object` | `anno-knowledge-store` | no | revision replacement |
| `delete_object_removes_all_chunks` | `anno-knowledge-store` | no | scope of delete |
| `mark_vector_ready_transitions_state` | `anno-knowledge-store` | no | SQLite state |
| `pending_vector_objects_returns_fts_ready_without_vectors` | `anno-knowledge-store` | no | backfill lookup |
| `vector_lag_count_matches_pending` | `anno-knowledge-store` | no | status sanity |
| `embed_pseudonymized_chunks_dim_matches_config` | `anno-rag` | yes | embedder API; skip if model absent |
| `pseudonymize_query_removes_pii` | `anno-rag` | yes | privacy critical |
| `embed_query_uses_query_prefix` | `anno-rag` | yes | e5 prefix discipline |
| `vector_pass_marks_ready_when_embedder_present` | `anno-rag-mcp` | yes | sync extension happy path |
| `vector_pass_marks_pending_when_embedder_absent` | `anno-rag-mcp` | no (mock embedder fail) | graceful degradation |
| `next_sync_backfills_vector_pending` | `anno-rag-mcp` | yes | resume after model arrival |
| `semantic_search_returns_rrf_fusion` | `anno-rag-mcp` | yes | quality of merged ordering |
| `forget_cascades_to_vector_store` | `anno-rag-mcp` | no | cross-store consistency |

Non-regression set (all must remain green):
- `knowledge_search(mode=fast)` returns Phase 2 results with the same shape
- `knowledge_status` works without any model directory
- `knowledge_add_local_folder` / `knowledge_forget` work without embedder
- `Pipeline::ingest_folder` and `legal_search` are byte-identical to their Phase 2 behavior

## 17. Acceptance criteria

- New crate-internal type `KnowledgeVectorStore` in `anno-knowledge-store`. No import of `anno-rag::Store` anywhere in `anno-knowledge-store`.
- `knowledge_chunks_v1` LanceDB table at `cfg.index_path()`, separate from existing `chunks`. Schema as in §6.
- `knowledge_sync` writes FTS *and* vectors when the embedder is reachable, in a single call.
- `knowledge_sync` with embedder unavailable: writes FTS, increments `vector_pending`, returns success (not an error).
- A subsequent `knowledge_sync` once the embedder is available backfills the `vector_pending` objects without re-extracting source files.
- `knowledge_search(mode=semantic)`: query is pseudonymized through the local vault before embedding, then RRF-merges FTS + LanceDB results, then hydrates snippets from SQLite.
- `knowledge_search(mode=fast)` behavior is byte-identical to Phase 2.
- `knowledge_status` reports `vector_ready`, `vector_pending`, and `embedding_model`.
- `knowledge_forget` cascades to LanceDB (`delete_object`) for each affected object.
- Embedder reuses `Pipeline::embedder()` lazy init — only one Bert instance in RSS.
- Zero changes to `crates/anno-rag/src/store.rs`, `Pipeline::ingest_folder`, `Pipeline::search`, `legal_*`, or `anno-rag-tabular`.

## 18. Deferred to later phases

All items deferred from this spec are tracked in [`docs/product/roadmap.md`](../../product/roadmap.md):

- `Deep` mode with a local reranker
- `Auto` mode (Fast/Semantic auto-routing)
- Re-embed pipeline on `embedding_model` change
- `acl_hash` column population (Outlook era)
- Outlook / Microsoft Graph connector
- Background job queue / daemon split
- Filter pushdown in `search_semantic`
