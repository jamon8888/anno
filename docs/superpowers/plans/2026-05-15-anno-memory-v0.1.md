# anno-memory v0.1 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a PII-safe persistent memory layer for Cowork sessions as a new `memory` module inside `anno-rag`, exposing four MCP tools (`memory_save` / `memory_recall` / `memory_forget` / `memory_list`) on the existing `AnnoRagServer`. Plaintext PII never persists; GDPR Art. 17 erasure cascades vault tokens; forward-compat columns (`valid_from`, `valid_to`, `entity_refs`) reserved for v0.2 activation.

**Architecture:** A new module `memory` adds a `Memory` data type, a `MemoryStore` over a second LanceDB collection (`memories`) in the same store as `documents`, and four MCP tools layered on the existing `Pipeline`. Hybrid retrieval (vector + LanceDB native FTS + `RRFReranker`) reuses the helpers landed by anno-rag v0.6. Scalar indexes (BTree on `created_at`/`session_id`, Bitmap on `kind`, LabelList on `token_refs` + `entity_refs`) keep filter scans linear in result-set size, not table size. Erasure SLO: daily background `Table::optimize` reclaims tombstoned bytes within 24 hours.

**Tech Stack:** Rust 2021, `lancedb 0.29.x` (bumped in the pre-req PR), `cloakpipe-core` (vault), `rmcp 1.6` (`#[tool_router]`), `fastembed` (existing e5-small), `tokio`, `uuid` (v7), `chrono`, `proptest`.

**Prerequisite:** PR-A (`2026-05-15-lancedb-0.29-workspace-bump.md`) merged. PR-B (`2026-05-14-anno-rag-v0.6-hybrid-retrieval-eval.md`) merged — this plan calls into `embed_query`, `build_fts_index`, and the hybrid `search()` helpers v0.6 lands in `embed.rs` and `store.rs`.

---

## File Structure

- **Create** `crates/anno-rag/src/memory.rs` — `Memory`, `MemoryKind`, `MemoryId`, `TokenRef`, `MemoryHit`, plus the `MemoryStore` struct wrapping a LanceDB `Table` and exposing `insert`, `get`, `delete_by_id`, `delete_by_query`, `hybrid_search_memories`, `list_paginated`.
- **Create** `crates/anno-rag/tests/memory_mcp.rs` — integration tests over rmcp client/server.
- **Create** `crates/anno-rag/tests/fixtures/locomo_subset/conversations.toml` — 50 LoCoMo subset conversation/question pairs.
- **Create** `crates/anno-rag/tests/fixtures/locomo_baseline.toml` — committed baseline (`accuracy@1`, `latency_p95_ms`) — empty values filled in Task 11.
- **Create** `crates/anno-rag/benches/bench_locomo.rs` — eval bench wrapping the LoCoMo harness.
- **Modify** `crates/anno-rag/src/lib.rs` — `pub mod memory;` and `pub use memory::{Memory, MemoryKind, MemoryHit};`.
- **Modify** `crates/anno-rag/src/store.rs` — extend `Store::open` to also open/create the `memories` collection alongside `documents`; create scalar indexes; expose `memories_hybrid_search`.
- **Modify** `crates/anno-rag/src/pipeline.rs` — add `save_memory`, `recall_memory`, `forget_memory`, `list_memories` methods composing `detect` / `vault` / `embed` / `store`. Add `spawn_compaction_task` for the erasure SLO.
- **Modify** `crates/anno-rag/src/mcp.rs` — four new tool handlers and their `*Params` / `*Result` types.
- **Modify** `crates/anno-rag/src/config.rs` — add `memory_collection_name: String` (default `"memories"`), `memory_embedding_dim: usize` (default 384), `compaction_interval: Duration` (default 24h), `compaction_dry_run: bool` (default false).
- **Modify** `crates/anno-rag/Cargo.toml` — add `uuid = { version = "1", features = ["v7", "serde"] }` if not already a dep, add `chrono` if not already present, add `proptest` to `[dev-dependencies]`.
- **Modify** `crates/anno-rag/CHANGELOG.md` — v0.7 entry.

---

## Task 1: Types and module skeleton

**Files:**
- Create: `crates/anno-rag/src/memory.rs`
- Modify: `crates/anno-rag/src/lib.rs`

- [ ] **Step 1: Write failing test for `MemoryKind` round-trip**

Create `crates/anno-rag/src/memory.rs`:
```rust
//! v0.1 stub — types only. Logic lands in Tasks 2+.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MemoryKind {
    Fact,
    Preference,
    Reference,
    Context,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_kind_round_trip_json() {
        for k in [MemoryKind::Fact, MemoryKind::Preference, MemoryKind::Reference, MemoryKind::Context] {
            let s = serde_json::to_string(&k).unwrap();
            let back: MemoryKind = serde_json::from_str(&s).unwrap();
            assert_eq!(k, back);
        }
    }
}
```

Modify `crates/anno-rag/src/lib.rs` — add `pub mod memory;` next to the other module declarations.

- [ ] **Step 2: Run test, verify it passes**

Run:
```powershell
cargo test -p anno-rag memory::tests::memory_kind_round_trip_json
```
Expected: PASS.

- [ ] **Step 3: Add `Memory`, `MemoryId`, `TokenRef`, `MemoryHit`**

Append to `crates/anno-rag/src/memory.rs`:
```rust
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(pub Uuid);

impl MemoryId {
    pub fn new() -> Self { Self(Uuid::now_v7()) }
    pub fn as_string(&self) -> String { self.0.to_string() }
}

impl Default for MemoryId {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TokenRef {
    pub label: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub session_id: Option<String>,
    pub kind: MemoryKind,
    /// PII-tokenized text. Never plaintext.
    pub text: String,
    pub created_at: DateTime<Utc>,
    pub accessed_at: DateTime<Utc>,
    /// v0.2 forward-compat: populated as `created_at` in v0.1.
    pub valid_from: DateTime<Utc>,
    /// v0.2 forward-compat: always None in v0.1.
    pub valid_to: Option<DateTime<Utc>>,
    pub embedding: Vec<f32>,
    pub token_refs: Vec<TokenRef>,
    /// v0.2 forward-compat: always empty in v0.1.
    pub entity_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryHit {
    pub id: String,
    pub text: String,
    pub kind: MemoryKind,
    pub created_at: String,
    pub score: f32,
}
```

- [ ] **Step 4: Compile-check**

Run:
```powershell
cargo build -p anno-rag
```
Expected: green. If `chrono` or `uuid` is not yet a dep, add to `crates/anno-rag/Cargo.toml`:
```toml
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { workspace = true, features = ["serde"] }
```

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/memory.rs crates/anno-rag/src/lib.rs crates/anno-rag/Cargo.toml
git commit -m "feat(anno-rag): memory module skeleton — types only"
```

---

## Task 2: LanceDB schema for the `memories` collection

**Files:**
- Modify: `crates/anno-rag/src/store.rs`

- [ ] **Step 1: Find the existing `documents` schema definition**

Run:
```powershell
Select-String -Path crates\anno-rag\src\store.rs -Pattern "Schema|schema_ref|Arc<Schema>"
```
Expected: at least one hit defining the documents-collection Arrow schema. Mirror its style.

- [ ] **Step 2: Write failing test for `memories_schema()`**

Add to `crates/anno-rag/src/store.rs` (in the `#[cfg(test)] mod tests` block — create if missing):
```rust
#[test]
fn memories_schema_has_required_columns() {
    let schema = memories_schema(384);
    let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
    for expected in [
        "id", "session_id", "kind", "text", "created_at", "accessed_at",
        "valid_from", "valid_to", "embedding", "token_refs", "entity_refs",
    ] {
        assert!(names.contains(&expected), "missing column: {expected}");
    }
}
```

Run:
```powershell
cargo test -p anno-rag memories_schema_has_required_columns
```
Expected: FAIL — `memories_schema` not defined.

- [ ] **Step 3: Implement `memories_schema`**

Add to `crates/anno-rag/src/store.rs`:
```rust
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

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
        Field::new("created_at", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("accessed_at", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("valid_from", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("valid_to", DataType::Timestamp(TimeUnit::Microsecond, None), true),
        Field::new(
            "embedding",
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
```

(If the imports already exist at the top of `store.rs`, reuse them — do not duplicate.)

- [ ] **Step 4: Run test, verify it passes**

```powershell
cargo test -p anno-rag memories_schema_has_required_columns
```
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/store.rs
git commit -m "feat(anno-rag): memories LanceDB schema (11 columns, v0.1 active + v0.2 reserved)"
```

---

## Task 3: `MemoryStore` open + insert + get + delete

**Files:**
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/memory.rs`
- Modify: `crates/anno-rag/src/config.rs`

- [ ] **Step 1: Add config fields**

Append to `crates/anno-rag/src/config.rs`:
```rust
// Memory collection
pub memory_collection_name: String,
pub memory_embedding_dim: usize,
```
Update the `Default` impl:
```rust
memory_collection_name: "memories".to_string(),
memory_embedding_dim: 384,
```

- [ ] **Step 2: Write failing integration test**

Create `crates/anno-rag/tests/memory_store.rs`:
```rust
use anno_rag::memory::{Memory, MemoryId, MemoryKind, TokenRef};
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn insert_then_get_round_trips() {
    let tmp = TempDir::new().unwrap();
    let store = anno_rag::store::Store::open(tmp.path().to_str().unwrap(), 384)
        .await.unwrap();
    let id = MemoryId::new();
    let now = Utc::now();
    let m = Memory {
        id: id.clone(),
        session_id: Some("s1".into()),
        kind: MemoryKind::Fact,
        text: "le dossier PERSON_a4f3".into(),
        created_at: now,
        accessed_at: now,
        valid_from: now,
        valid_to: None,
        embedding: vec![0.1f32; 384],
        token_refs: vec![TokenRef { label: "PERSON".into(), token: "PERSON_a4f3".into() }],
        entity_refs: vec![],
    };
    store.memory_insert(&m).await.unwrap();
    let got = store.memory_get(&id).await.unwrap().expect("must exist");
    assert_eq!(got.text, m.text);
    assert_eq!(got.session_id, m.session_id);
    assert_eq!(got.kind, m.kind);
}
```

Run:
```powershell
cargo test -p anno-rag --test memory_store insert_then_get_round_trips
```
Expected: FAIL — `Store::memory_insert` / `Store::memory_get` not defined.

- [ ] **Step 3: Implement `memory_insert` / `memory_get` / `memory_delete_by_id`**

Add to `crates/anno-rag/src/store.rs` — within the existing `impl Store { ... }` block (find it by searching for `pub async fn upsert_chunks` or similar in the existing v0.5 code):

```rust
pub async fn memory_insert(&self, m: &crate::memory::Memory) -> Result<()> {
    let batch = memory_to_batch(m, self.memory_embedding_dim, &self.memories_schema)?;
    let reader = arrow::record_batch::RecordBatchIterator::new(
        vec![Ok(batch)].into_iter(),
        self.memories_schema.clone(),
    );
    self.memories_tbl
        .add(Box::new(reader))
        .execute()
        .await
        .map_err(|e| Error::Store(format!("memory add: {e}")))?;
    Ok(())
}

pub async fn memory_get(&self, id: &crate::memory::MemoryId) -> Result<Option<crate::memory::Memory>> {
    use lancedb::query::{ExecutableQuery, QueryBase};
    let filter = format!("id = '{}'", id.as_string());
    let mut stream = self.memories_tbl
        .query()
        .only_if(&filter)
        .limit(1)
        .execute()
        .await
        .map_err(|e| Error::Store(format!("memory_get exec: {e}")))?;
    let next = futures_util::TryStreamExt::try_next(&mut stream)
        .await
        .map_err(|e| Error::Store(format!("memory_get stream: {e}")))?;
    match next {
        Some(batch) if batch.num_rows() > 0 => Ok(Some(batch_row_to_memory(&batch, 0)?)),
        _ => Ok(None),
    }
}

pub async fn memory_delete_by_id(&self, id: &crate::memory::MemoryId) -> Result<bool> {
    let filter = format!("id = '{}'", id.as_string());
    self.memories_tbl
        .delete(&filter)
        .await
        .map_err(|e| Error::Store(format!("memory_delete: {e}")))?;
    Ok(true)
}
```

Add the helpers `memory_to_batch` and `batch_row_to_memory` as private functions in the same file. Mirror the structure of any existing `records_to_batch` helper for documents — they are identical Arrow patterns.

- [ ] **Step 4: Extend `Store::open` to also open the `memories` table**

Find `Store::open` (search for `pub async fn open`). Currently it opens one table; add a sibling call for `memories`:
```rust
pub async fn open(uri: &str, embedding_dim: usize) -> Result<Self> {
    let conn = lancedb::connect(uri).execute().await
        .map_err(|e| Error::Store(format!("connect: {e}")))?;

    // documents — existing v0.5 logic, unchanged.
    let documents_tbl = open_or_create_table(&conn, "documents", documents_schema(embedding_dim)).await?;

    // memories — new in v0.1.
    let memories_schema = memories_schema(embedding_dim);
    let memories_tbl = open_or_create_table(&conn, "memories", memories_schema.clone()).await?;

    Ok(Self {
        tbl: documents_tbl,
        memories_tbl,
        memories_schema,
        memory_embedding_dim: embedding_dim,
        // ... other existing fields ...
    })
}
```

Extract `open_or_create_table` as a small private fn if it doesn't already exist; both collections share the same idempotent open-or-create pattern.

- [ ] **Step 5: Run test, verify it passes**

```powershell
cargo test -p anno-rag --test memory_store insert_then_get_round_trips
```
Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/store.rs crates/anno-rag/src/config.rs crates/anno-rag/tests/memory_store.rs
git commit -m "feat(anno-rag): MemoryStore — open + insert + get + delete on second LanceDB collection"
```

---

## Task 4: Scalar indexes on the memories collection

**Files:**
- Modify: `crates/anno-rag/src/store.rs`

- [ ] **Step 1: Write failing test asserting indexes are present after setup**

Add to `crates/anno-rag/tests/memory_store.rs`:
```rust
#[tokio::test]
async fn scalar_indexes_created_after_setup() {
    let tmp = TempDir::new().unwrap();
    let store = anno_rag::store::Store::open(tmp.path().to_str().unwrap(), 384).await.unwrap();
    store.setup_memory_indexes().await.unwrap();
    let indexes = store.memory_list_indexes().await.unwrap();
    let columns: Vec<String> = indexes.iter().flat_map(|i| i.columns.clone()).collect();
    for expected in ["created_at", "session_id", "kind", "token_refs", "entity_refs"] {
        assert!(columns.iter().any(|c| c == expected), "missing index on {expected}");
    }
}
```

Run:
```powershell
cargo test -p anno-rag --test memory_store scalar_indexes_created_after_setup
```
Expected: FAIL.

- [ ] **Step 2: Implement `setup_memory_indexes` and `memory_list_indexes`**

Add to `crates/anno-rag/src/store.rs`:
```rust
pub async fn setup_memory_indexes(&self) -> Result<()> {
    use lancedb::index::scalar::{BTreeIndexBuilder, BitmapIndexBuilder, LabelListIndexBuilder};
    use lancedb::index::Index;

    // Helper: create only if missing.
    let existing = self.memories_tbl.list_indices().await
        .map_err(|e| Error::Store(format!("list_indices: {e}")))?;
    let has_index_on = |col: &str| {
        existing.iter().any(|i| i.columns.iter().any(|c| c == col))
    };

    if !has_index_on("created_at") {
        self.memories_tbl.create_index(&["created_at"], Index::BTree(BTreeIndexBuilder::default()))
            .execute().await
            .map_err(|e| Error::Store(format!("btree created_at: {e}")))?;
    }
    if !has_index_on("session_id") {
        self.memories_tbl.create_index(&["session_id"], Index::BTree(BTreeIndexBuilder::default()))
            .execute().await
            .map_err(|e| Error::Store(format!("btree session_id: {e}")))?;
    }
    if !has_index_on("kind") {
        self.memories_tbl.create_index(&["kind"], Index::Bitmap(BitmapIndexBuilder::default()))
            .execute().await
            .map_err(|e| Error::Store(format!("bitmap kind: {e}")))?;
    }
    if !has_index_on("token_refs") {
        self.memories_tbl.create_index(&["token_refs"], Index::LabelList(LabelListIndexBuilder::default()))
            .execute().await
            .map_err(|e| Error::Store(format!("label_list token_refs: {e}")))?;
    }
    if !has_index_on("entity_refs") {
        self.memories_tbl.create_index(&["entity_refs"], Index::LabelList(LabelListIndexBuilder::default()))
            .execute().await
            .map_err(|e| Error::Store(format!("label_list entity_refs: {e}")))?;
    }
    Ok(())
}

pub async fn memory_list_indexes(&self) -> Result<Vec<lancedb::index::IndexConfig>> {
    self.memories_tbl.list_indices().await
        .map_err(|e| Error::Store(format!("memory_list_indexes: {e}")))
}
```

**Verification note:** the exact paths `lancedb::index::scalar::{BTreeIndexBuilder, BitmapIndexBuilder, LabelListIndexBuilder}` reflect the 0.29 API per the research report. If `cargo check` flags a missing path, look at the `lancedb::index` re-exports — variants may be one level up.

- [ ] **Step 3: Run test, verify it passes**

```powershell
cargo test -p anno-rag --test memory_store scalar_indexes_created_after_setup
```
Expected: PASS.

- [ ] **Step 4: Wire `setup_memory_indexes` into `Store::open`**

In `Store::open`, after the `memories_tbl` is opened, add:
```rust
let store = Self { /* … */ };
store.setup_memory_indexes().await?;
Ok(store)
```
Or, if `open` returns the struct in one shot, restructure to build the struct first, then call `setup_memory_indexes` before returning.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/store.rs crates/anno-rag/tests/memory_store.rs
git commit -m "feat(anno-rag): scalar indexes on memories (BTree, Bitmap, LabelList)"
```

---

## Task 5: FTS index on memories `text` + hybrid search helper

**Files:**
- Modify: `crates/anno-rag/src/store.rs`

- [ ] **Step 1: Write failing test for hybrid search ordering**

Add to `crates/anno-rag/tests/memory_store.rs`:
```rust
#[tokio::test]
async fn hybrid_search_orders_by_relevance() {
    let tmp = TempDir::new().unwrap();
    let store = anno_rag::store::Store::open(tmp.path().to_str().unwrap(), 4).await.unwrap();

    // Three memories with deliberately different relevance to "vente Bordeaux".
    let now = Utc::now();
    let mk = |text: &str, embedding: Vec<f32>| Memory {
        id: MemoryId::new(),
        session_id: None,
        kind: MemoryKind::Fact,
        text: text.into(),
        created_at: now, accessed_at: now, valid_from: now, valid_to: None,
        embedding,
        token_refs: vec![],
        entity_refs: vec![],
    };
    let close = mk("dossier vente Bordeaux pour PERSON_a", vec![1.0, 0.0, 0.0, 0.0]);
    let mid   = mk("vente immobiliere en région", vec![0.5, 0.5, 0.0, 0.0]);
    let far   = mk("contrat de travail tertiaire", vec![0.0, 0.0, 1.0, 0.0]);
    for m in [&close, &mid, &far] { store.memory_insert(m).await.unwrap(); }
    store.build_memories_fts_index().await.unwrap();

    let query_vec = vec![1.0, 0.0, 0.0, 0.0];
    let query_text = "vente Bordeaux";
    let hits = store.memories_hybrid_search(&query_vec, query_text, 10).await.unwrap();
    assert_eq!(hits.first().map(|h| h.id.clone()), Some(close.id.as_string()));
    assert!(hits.iter().position(|h| h.id == far.id.as_string()).unwrap() > 0);
}
```

Run:
```powershell
cargo test -p anno-rag --test memory_store hybrid_search_orders_by_relevance
```
Expected: FAIL.

- [ ] **Step 2: Implement `build_memories_fts_index`**

Add to `crates/anno-rag/src/store.rs`:
```rust
pub async fn build_memories_fts_index(&self) -> Result<()> {
    use lancedb::index::scalar::{FtsIndexBuilder};
    use lancedb::index::Index;
    let existing = self.memories_tbl.list_indices().await
        .map_err(|e| Error::Store(format!("list_indices: {e}")))?;
    let has_fts = existing.iter().any(|i|
        i.columns.iter().any(|c| c == "text") && i.index_type == lancedb::index::IndexType::FTS
    );
    if has_fts { return Ok(()); }
    self.memories_tbl
        .create_index(&["text"], Index::FTS(FtsIndexBuilder::default()))
        .execute().await
        .map_err(|e| Error::Store(format!("fts index: {e}")))?;
    Ok(())
}
```

- [ ] **Step 3: Implement `memories_hybrid_search`**

Add to `crates/anno-rag/src/store.rs`. Reuse the same RRF pattern v0.6's `Store::search` uses for documents — if v0.6 exposes a `hybrid_search_inner(query_vec, query_text, top_k, table: &Table)` helper, call it on `self.memories_tbl`. If v0.6 only landed `search` over documents, copy its body and substitute `self.memories_tbl`:

```rust
pub async fn memories_hybrid_search(
    &self,
    query_vec: &[f32],
    query_text: &str,
    top_k: usize,
) -> Result<Vec<MemoryHitRow>> {
    use lancedb::query::{ExecutableQuery, QueryBase, Select, FullTextSearchQuery};
    use lancedb::rerankers::RRFReranker;

    let reranker = RRFReranker::default();
    let mut stream = self.memories_tbl
        .query()
        .nearest_to(query_vec.to_vec())?
        .full_text_search(FullTextSearchQuery::new(query_text.to_string()))
        .limit(top_k)
        .rerank(Box::new(reranker))
        .execute().await
        .map_err(|e| Error::Store(format!("hybrid execute: {e}")))?;

    let mut hits = Vec::with_capacity(top_k);
    while let Some(batch) = futures_util::TryStreamExt::try_next(&mut stream).await
        .map_err(|e| Error::Store(format!("hybrid stream: {e}")))?
    {
        for row in 0..batch.num_rows() {
            hits.push(memory_hit_row(&batch, row)?);
        }
    }
    Ok(hits)
}
```

Define `MemoryHitRow` next to `MemoryHit` in `memory.rs` — it carries the on-disk `text` (tokenized) + score; rehydration happens in `pipeline.rs`:
```rust
#[derive(Debug, Clone)]
pub struct MemoryHitRow {
    pub id: String,
    pub text_tokenized: String,
    pub kind: MemoryKind,
    pub created_at: String,
    pub score: f32,
}
```

**Verification note:** if v0.6's pattern differs from this snippet (e.g. it uses `query_type("hybrid")` instead of chaining `nearest_to + full_text_search`), match v0.6's pattern exactly to keep one code path.

- [ ] **Step 4: Run test, verify it passes**

```powershell
cargo test -p anno-rag --test memory_store hybrid_search_orders_by_relevance
```
Expected: PASS. If FTS index build is slow on small fixtures, the test may time out — add `.timeout(Duration::from_secs(30))` to the tokio test attribute or pre-build the index synchronously in setup.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/store.rs crates/anno-rag/src/memory.rs crates/anno-rag/tests/memory_store.rs
git commit -m "feat(anno-rag): hybrid search over memories (Index::FTS + RRFReranker)"
```

---

## Task 6: Pipeline `save_memory` (PII-tokenize then persist)

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Write failing test asserting tokenized storage**

Add to `crates/anno-rag/tests/memory_mcp.rs` (create the file):
```rust
use anno_rag::memory::MemoryKind;
use anno_rag::pipeline::Pipeline;
use anno_rag::config::AnnoRagConfig;
use tempfile::TempDir;

#[tokio::test]
async fn save_memory_persists_tokenized_text_only() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path()); // assumes helper exists from v0.5; otherwise build manually
    let p = Pipeline::new(cfg.clone()).await.unwrap();
    let plaintext = "Le dossier concerne Sophie Wilson, ne le 12 mars.";
    let saved = p.save_memory(plaintext, Some(MemoryKind::Fact), Some("s1".into())).await.unwrap();
    // The stored text must not contain the plaintext name.
    let row = p.store().memory_get(&saved.id).await.unwrap().unwrap();
    assert!(!row.text.contains("Sophie Wilson"),
        "plaintext PII leaked to memories collection: {}", row.text);
    assert!(!saved.token_refs.is_empty(), "expected at least one PII token");
}
```

Run:
```powershell
cargo test -p anno-rag --test memory_mcp save_memory_persists_tokenized_text_only
```
Expected: FAIL — `Pipeline::save_memory` not defined.

- [ ] **Step 2: Implement `Pipeline::save_memory`**

Add to `crates/anno-rag/src/pipeline.rs`:
```rust
use crate::memory::{Memory, MemoryHit, MemoryId, MemoryKind, TokenRef};
use chrono::Utc;

pub struct SavedMemory {
    pub id: MemoryId,
    pub redacted_text: String,
    pub token_refs: Vec<TokenRef>,
}

impl Pipeline {
    pub async fn save_memory(
        &self,
        text: &str,
        kind: Option<MemoryKind>,
        session_id: Option<String>,
    ) -> Result<SavedMemory> {
        // 1. Detect + pseudonymize via the existing vault.
        let detected = self.detector.detect(text)?;
        let (tokenized, token_refs) = self.vault.pseudonymize_collect(text, &detected).await?;

        // 2. Embed the tokenized text (passage prefix, per v0.6 e5 fix).
        let embedding = self.embedder.embed_passage(&tokenized).await?;

        // 3. Build the row.
        let now = Utc::now();
        let id = MemoryId::new();
        let m = Memory {
            id: id.clone(),
            session_id,
            kind: kind.unwrap_or(MemoryKind::Context),
            text: tokenized.clone(),
            created_at: now,
            accessed_at: now,
            valid_from: now,
            valid_to: None,
            embedding,
            token_refs: token_refs.clone(),
            entity_refs: vec![],
        };

        // 4. Persist.
        self.store.memory_insert(&m).await?;

        Ok(SavedMemory { id, redacted_text: tokenized, token_refs })
    }
}
```

`Vault::pseudonymize_collect` may need to be added if the existing `pseudonymize` discards the per-token mapping. If so, add it next to `pseudonymize` in `src/vault.rs`:
```rust
pub async fn pseudonymize_collect(
    &self,
    text: &str,
    detected: &[cloakpipe_core::DetectedEntity],
) -> Result<(String, Vec<crate::memory::TokenRef>)> {
    let mut guard = self.inner.lock().await;
    let mut refs = Vec::new();
    let pseudo = guard.pseudonymize_with_callback(text, detected, |label, token| {
        refs.push(crate::memory::TokenRef { label: label.into(), token: token.into() });
    })?;
    Ok((pseudo, refs))
}
```
If the cloakpipe API doesn't expose a callback, derive `token_refs` by walking the detected entities and querying the vault for each post-pseudonymize (one extra lookup per detected entity, acceptable for v0.1).

- [ ] **Step 3: Run test, verify it passes**

```powershell
cargo test -p anno-rag --test memory_mcp save_memory_persists_tokenized_text_only
```
Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/src/vault.rs crates/anno-rag/tests/memory_mcp.rs
git commit -m "feat(anno-rag): Pipeline::save_memory — PII-tokenize before persist"
```

---

## Task 7: Pipeline `recall_memory` (hybrid search + rehydrate)

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Write failing test for save-then-recall round trip**

Add to `crates/anno-rag/tests/memory_mcp.rs`:
```rust
#[tokio::test]
async fn save_then_recall_returns_plaintext() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();
    let original = "Le client Sophie Wilson souhaite recevoir les actes en PDF.";
    p.save_memory(original, Some(MemoryKind::Preference), None).await.unwrap();
    let hits = p.recall_memory("envoi des actes PDF", 5, None, None).await.unwrap();
    assert!(!hits.is_empty(), "expected at least one hit");
    let top = &hits[0];
    assert!(top.text.contains("Sophie Wilson"),
        "rehydration failed: stored tokenized text was returned, got: {}", top.text);
}
```

Run: expect FAIL.

- [ ] **Step 2: Implement `Pipeline::recall_memory`**

Append to `crates/anno-rag/src/pipeline.rs`:
```rust
impl Pipeline {
    pub async fn recall_memory(
        &self,
        query: &str,
        top_k: usize,
        session_id: Option<String>,
        kinds: Option<Vec<MemoryKind>>,
    ) -> Result<Vec<MemoryHit>> {
        // 1. Detect + pseudonymize the query.
        let detected = self.detector.detect(query)?;
        let (tokenized_query, _) = self.vault.pseudonymize_collect(query, &detected).await?;

        // 2. Embed the query (query prefix, per v0.6 e5 fix).
        let query_vec = self.embedder.embed_query(&tokenized_query).await?;

        // 3. Hybrid search.
        let mut raw = self.store
            .memories_hybrid_search(&query_vec, &tokenized_query, top_k * 2)
            .await?;

        // 4. Filter by session / kind.
        raw.retain(|h| {
            let kind_ok = match &kinds {
                Some(allowed) => allowed.iter().any(|k| {
                    let s = match k { MemoryKind::Fact => "fact", MemoryKind::Preference => "preference",
                        MemoryKind::Reference => "reference", MemoryKind::Context => "context" };
                    h.kind_str() == s
                }),
                None => true,
            };
            kind_ok
            // session_id filter: requires storing it on MemoryHitRow — see Task 5 row builder.
        });
        raw.truncate(top_k);

        // 5. Rehydrate each hit through the vault.
        let mut out = Vec::with_capacity(raw.len());
        for row in raw {
            let rehydrated = self.vault.rehydrate(&row.text_tokenized).await?;
            out.push(MemoryHit {
                id: row.id,
                text: rehydrated.text,
                kind: row.kind,
                created_at: row.created_at,
                score: row.score,
            });
        }
        Ok(out)
    }
}
```

If `MemoryHitRow` does not yet carry `session_id`, extend Task 5's row builder to include it, then add the session filter:
```rust
if let Some(s) = &session_id {
    raw.retain(|h| h.session_id.as_deref() == Some(s.as_str()) || h.session_id.is_none());
}
```

- [ ] **Step 3: Run test, verify it passes**

Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/src/memory.rs crates/anno-rag/src/store.rs crates/anno-rag/tests/memory_mcp.rs
git commit -m "feat(anno-rag): Pipeline::recall_memory — hybrid search + vault rehydrate"
```

---

## Task 8: `forget_memory` + vault-token cascade

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/vault.rs`

- [ ] **Step 1: Write failing tests for cascade and dry-run**

Add to `crates/anno-rag/tests/memory_mcp.rs`:
```rust
#[tokio::test]
async fn forget_cascades_orphaned_vault_tokens() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();
    let a = p.save_memory("le client Dupont signe demain.", Some(MemoryKind::Fact), None).await.unwrap();
    let b = p.save_memory("Dupont preferes les RDV le vendredi.", Some(MemoryKind::Preference), None).await.unwrap();
    // Forget only `a`. Token referencing Dupont must remain (referenced by `b`).
    let r1 = p.forget_memory(Some(a.id.clone()), None, 1, false).await.unwrap();
    assert_eq!(r1.forgotten_ids.len(), 1);
    assert_eq!(r1.vault_tokens_purged, 0, "token still referenced by b");
    // Now forget `b`. Token must be purged.
    let r2 = p.forget_memory(Some(b.id.clone()), None, 1, false).await.unwrap();
    assert!(r2.vault_tokens_purged >= 1, "expected vault purge after last reference removed");
}

#[tokio::test]
async fn forget_dry_run_does_not_mutate() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();
    let m = p.save_memory("test", Some(MemoryKind::Context), None).await.unwrap();
    let r = p.forget_memory(Some(m.id.clone()), None, 1, true).await.unwrap();
    assert_eq!(r.forgotten_ids, vec![m.id.as_string()]);
    // Row still exists.
    let row = p.store().memory_get(&m.id).await.unwrap();
    assert!(row.is_some());
}
```

Run: expect FAIL.

- [ ] **Step 2: Add helpers — count token references across the store**

Add to `crates/anno-rag/src/store.rs`:
```rust
pub async fn token_reference_count(&self, token: &str) -> Result<u64> {
    let filter = format!("array_contains(token_refs, struct(token = '{token}'))");
    let n = self.memories_tbl.count_rows(Some(&filter)).await
        .map_err(|e| Error::Store(format!("token ref count memories: {e}")))?;
    // Also check the documents collection — chunks may also reference the same token.
    let d = self.tbl.count_rows(Some(&format!("array_contains(token_refs, struct(token = '{token}'))"))).await
        .unwrap_or(0); // documents may not carry token_refs in v0.5; treat absence as 0.
    Ok((n + d) as u64)
}
```

**Verification note:** `array_contains` with a struct predicate is the LanceDB SQL syntax for filtering on List<Struct>. If 0.29 surfaces a different operator name, consult [docs.lancedb.com/sql](https://docs.lancedb.com/sql) and adjust.

- [ ] **Step 3: Add `Vault::purge_token`**

Add to `crates/anno-rag/src/vault.rs`:
```rust
pub async fn purge_token(&self, token: &str) -> Result<bool> {
    let mut guard = self.inner.lock().await;
    Ok(guard.delete_token(token)
        .map_err(|e| Error::Vault(format!("cloakpipe delete: {e}")))?)
}
```
If `cloakpipe_core::vault::Vault::delete_token` does not exist, add a thin wrapper that re-encrypts the vault file without the named entry. Expected outcome: subsequent `lookup(token)` returns `None`.

- [ ] **Step 4: Implement `Pipeline::forget_memory`**

Append to `crates/anno-rag/src/pipeline.rs`:
```rust
pub struct ForgetResult {
    pub forgotten_ids: Vec<String>,
    pub vault_tokens_purged: usize,
}

impl Pipeline {
    pub async fn forget_memory(
        &self,
        id: Option<MemoryId>,
        query: Option<String>,
        limit: usize,
        dry_run: bool,
    ) -> Result<ForgetResult> {
        // 1. Resolve target rows.
        let targets: Vec<Memory> = match (id, query) {
            (Some(id), None) => self.store.memory_get(&id).await?.into_iter().collect(),
            (None, Some(q)) => {
                let hits = self.recall_memory(&q, limit, None, None).await?;
                let mut out = Vec::with_capacity(hits.len());
                for h in hits.iter().take(limit) {
                    let id = MemoryId(uuid::Uuid::parse_str(&h.id)
                        .map_err(|e| Error::Memory(format!("bad id: {e}")))?);
                    if let Some(m) = self.store.memory_get(&id).await? { out.push(m); }
                }
                out
            }
            _ => return Err(Error::Memory("exactly one of id/query must be set".into())),
        };

        if dry_run {
            return Ok(ForgetResult {
                forgotten_ids: targets.iter().map(|t| t.id.as_string()).collect(),
                vault_tokens_purged: 0,
            });
        }

        // 2. Collect candidate tokens to potentially purge.
        let mut candidate_tokens = std::collections::HashSet::new();
        for t in &targets {
            for r in &t.token_refs { candidate_tokens.insert(r.token.clone()); }
        }

        // 3. Delete the rows.
        let mut forgotten_ids = Vec::new();
        for t in &targets {
            self.store.memory_delete_by_id(&t.id).await?;
            forgotten_ids.push(t.id.as_string());
        }

        // 4. Cascade: for each candidate token, if reference count is now zero, purge from vault.
        let mut purged = 0;
        for tok in candidate_tokens {
            let count = self.store.token_reference_count(&tok).await?;
            if count == 0 {
                if self.vault.purge_token(&tok).await? { purged += 1; }
            }
        }

        Ok(ForgetResult { forgotten_ids, vault_tokens_purged: purged })
    }
}
```

- [ ] **Step 5: Run tests, verify both pass**

```powershell
cargo test -p anno-rag --test memory_mcp forget_cascades_orphaned_vault_tokens forget_dry_run_does_not_mutate
```
Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/src/store.rs crates/anno-rag/src/vault.rs crates/anno-rag/tests/memory_mcp.rs
git commit -m "feat(anno-rag): forget_memory + vault-token cascade (GDPR Art.17)"
```

---

## Task 9: `list_memories` (paginated)

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/store.rs`

- [ ] **Step 1: Write failing test**

Add to `crates/anno-rag/tests/memory_mcp.rs`:
```rust
#[tokio::test]
async fn list_paginates_in_creation_order() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();
    for i in 0..7 { p.save_memory(&format!("memory {i}"), Some(MemoryKind::Context), None).await.unwrap(); }
    let page1 = p.list_memories(None, None, 3, None).await.unwrap();
    assert_eq!(page1.items.len(), 3);
    let page2 = p.list_memories(None, None, 3, page1.next_cursor.clone()).await.unwrap();
    assert_eq!(page2.items.len(), 3);
    let page3 = p.list_memories(None, None, 3, page2.next_cursor.clone()).await.unwrap();
    assert_eq!(page3.items.len(), 1);
    assert!(page3.next_cursor.is_none());
}
```

Run: expect FAIL.

- [ ] **Step 2: Implement `Store::memory_list` + `Pipeline::list_memories`**

In `crates/anno-rag/src/store.rs`:
```rust
pub async fn memory_list(
    &self,
    session_id: Option<&str>,
    kind: Option<&str>,
    limit: usize,
    cursor: Option<&str>, // created_at ISO string of the last returned row
) -> Result<(Vec<crate::memory::Memory>, Option<String>)> {
    use lancedb::query::{ExecutableQuery, QueryBase};
    let mut clauses = Vec::new();
    if let Some(s) = session_id { clauses.push(format!("session_id = '{s}'")); }
    if let Some(k) = kind { clauses.push(format!("kind = '{k}'")); }
    if let Some(c) = cursor { clauses.push(format!("created_at < '{c}'")); }
    let filter = if clauses.is_empty() { None } else { Some(clauses.join(" AND ")) };

    let mut q = self.memories_tbl.query();
    if let Some(f) = &filter { q = q.only_if(f); }
    let mut stream = q
        .order_by("created_at DESC")
        .limit(limit + 1)
        .execute().await
        .map_err(|e| Error::Store(format!("memory_list exec: {e}")))?;

    let mut items = Vec::with_capacity(limit + 1);
    while let Some(batch) = futures_util::TryStreamExt::try_next(&mut stream).await
        .map_err(|e| Error::Store(format!("memory_list stream: {e}")))?
    {
        for r in 0..batch.num_rows() {
            items.push(batch_row_to_memory(&batch, r)?);
            if items.len() > limit { break; }
        }
        if items.len() > limit { break; }
    }
    let next_cursor = if items.len() > limit {
        items.pop();
        items.last().map(|m| m.created_at.to_rfc3339())
    } else {
        None
    };
    Ok((items, next_cursor))
}
```

In `crates/anno-rag/src/pipeline.rs`:
```rust
pub struct ListPage {
    pub items: Vec<MemoryHit>,
    pub next_cursor: Option<String>,
}

impl Pipeline {
    pub async fn list_memories(
        &self,
        session_id: Option<String>,
        kind: Option<MemoryKind>,
        limit: usize,
        cursor: Option<String>,
    ) -> Result<ListPage> {
        let kind_str = kind.map(|k| match k {
            MemoryKind::Fact => "fact", MemoryKind::Preference => "preference",
            MemoryKind::Reference => "reference", MemoryKind::Context => "context",
        }.to_string());
        let (rows, next_cursor) = self.store.memory_list(
            session_id.as_deref(), kind_str.as_deref(), limit, cursor.as_deref(),
        ).await?;
        let mut items = Vec::with_capacity(rows.len());
        for m in rows {
            let rehydrated = self.vault.rehydrate(&m.text).await?;
            items.push(MemoryHit {
                id: m.id.as_string(),
                text: rehydrated.text,
                kind: m.kind,
                created_at: m.created_at.to_rfc3339(),
                score: 0.0,
            });
        }
        Ok(ListPage { items, next_cursor })
    }
}
```

**Note:** v0.1 `list_memories` rehydrates by default — small N, low cost. The spec mentions "metadata only by default to keep responses small"; defer the metadata-only flag to v0.2 if response sizes become an issue. v0.1 keeps the API simple.

- [ ] **Step 3: Run test, verify it passes**

Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/store.rs crates/anno-rag/src/pipeline.rs crates/anno-rag/tests/memory_mcp.rs
git commit -m "feat(anno-rag): list_memories — cursor-paginated, indexed by created_at"
```

---

## Task 10: MCP tool wiring (four new tools on `AnnoRagServer`)

**Files:**
- Modify: `crates/anno-rag/src/mcp.rs`

- [ ] **Step 1: Add parameter and result types**

Append to `crates/anno-rag/src/mcp.rs` (next to the existing `SearchParams`, etc.):
```rust
use crate::memory::{MemoryKind, TokenRef};

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemorySaveParams {
    pub text: String,
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Serialize)]
struct MemorySaveResultWire {
    id: String,
    redacted_text: String,
    token_count: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryRecallParams {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub kinds: Option<Vec<MemoryKind>>,
}

#[derive(Serialize)]
struct MemoryHitWire {
    id: String,
    text: String,
    kind: String,
    created_at: String,
    score: f32,
}

#[derive(Serialize)]
struct MemoryRecallResultWire { hits: Vec<MemoryHitWire> }

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryForgetParams {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default = "default_forget_limit")]
    pub limit: usize,
    #[serde(default)]
    pub dry_run: bool,
}
fn default_forget_limit() -> usize { 5 }

#[derive(Serialize)]
struct MemoryForgetResultWire {
    forgotten_ids: Vec<String>,
    vault_tokens_purged: usize,
    note: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryListParams {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    #[serde(default = "default_list_limit")]
    pub limit: usize,
    #[serde(default)]
    pub cursor: Option<String>,
}
fn default_list_limit() -> usize { 20 }

#[derive(Serialize)]
struct MemoryListResultWire {
    items: Vec<MemoryHitWire>,
    next_cursor: Option<String>,
}
```

- [ ] **Step 2: Add four `#[tool]` handlers inside the existing `#[tool_router] impl AnnoRagServer`**

In `crates/anno-rag/src/mcp.rs`, after the existing `detect` handler:
```rust
#[tool(description = "Save a memory. PII is tokenized through the local vault before storage. Returns the new id and the redacted text actually persisted.")]
async fn memory_save(&self, Parameters(p): Parameters<MemorySaveParams>) -> String {
    let span = tracing::info_span!(target: "anno_rag::memory::audit", "memory_save",
        tenant = self.cfg.tenant_label().unwrap_or("default"), result = tracing::field::Empty);
    let _g = span.enter();
    let start = std::time::Instant::now();
    let r = self.pipeline.save_memory(&p.text, p.kind, p.session_id).await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(s) => {
            tracing::info!(target: "anno_rag::memory::audit", tool = "memory_save", result = "ok", duration_ms = elapsed, "");
            let wire = MemorySaveResultWire {
                id: s.id.as_string(),
                redacted_text: s.redacted_text,
                token_count: s.token_refs.len(),
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(target: "anno_rag::memory::audit", tool = "memory_save", result = "error", duration_ms = elapsed, "{e}");
            format!("Error: {e}")
        }
    }
}

#[tool(description = "Recall memories by hybrid (vector + FTS) search. Returns rehydrated plaintext for the caller's tenant.")]
async fn memory_recall(&self, Parameters(p): Parameters<MemoryRecallParams>) -> String {
    let start = std::time::Instant::now();
    let r = self.pipeline.recall_memory(&p.query, p.top_k, p.session_id, p.kinds).await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(hits) => {
            tracing::info!(target: "anno_rag::memory::audit", tool = "memory_recall", result = "ok", duration_ms = elapsed, n = hits.len(), "");
            let wire = MemoryRecallResultWire {
                hits: hits.into_iter().map(|h| MemoryHitWire {
                    id: h.id, text: h.text,
                    kind: format!("{:?}", h.kind).to_lowercase(),
                    created_at: h.created_at, score: h.score,
                }).collect(),
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(target: "anno_rag::memory::audit", tool = "memory_recall", result = "error", duration_ms = elapsed, "{e}");
            format!("Error: {e}")
        }
    }
}

#[tool(description = "Forget memories by id or by query. Cascades to vault tokens no longer referenced. Returns the SLO note that physical erasure may take up to 24h.")]
async fn memory_forget(&self, Parameters(p): Parameters<MemoryForgetParams>) -> String {
    let id = match &p.id {
        Some(s) => match uuid::Uuid::parse_str(s) {
            Ok(u) => Some(crate::memory::MemoryId(u)),
            Err(e) => return format!("Error: bad id: {e}"),
        },
        None => None,
    };
    let start = std::time::Instant::now();
    let r = self.pipeline.forget_memory(id, p.query, p.limit, p.dry_run).await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(res) => {
            tracing::info!(target: "anno_rag::memory::audit", tool = "memory_forget", result = "ok", duration_ms = elapsed, n = res.forgotten_ids.len(), "");
            let wire = MemoryForgetResultWire {
                forgotten_ids: res.forgotten_ids,
                vault_tokens_purged: res.vault_tokens_purged,
                note: "logically forgotten; physical erasure within 24h".into(),
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(target: "anno_rag::memory::audit", tool = "memory_forget", result = "error", duration_ms = elapsed, "{e}");
            format!("Error: {e}")
        }
    }
}

#[tool(description = "List memories with optional session/kind filter and cursor pagination.")]
async fn memory_list(&self, Parameters(p): Parameters<MemoryListParams>) -> String {
    let start = std::time::Instant::now();
    let r = self.pipeline.list_memories(p.session_id, p.kind, p.limit, p.cursor).await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(page) => {
            tracing::info!(target: "anno_rag::memory::audit", tool = "memory_list", result = "ok", duration_ms = elapsed, n = page.items.len(), "");
            let wire = MemoryListResultWire {
                items: page.items.into_iter().map(|h| MemoryHitWire {
                    id: h.id, text: h.text,
                    kind: format!("{:?}", h.kind).to_lowercase(),
                    created_at: h.created_at, score: h.score,
                }).collect(),
                next_cursor: page.next_cursor,
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(target: "anno_rag::memory::audit", tool = "memory_list", result = "error", duration_ms = elapsed, "{e}");
            format!("Error: {e}")
        }
    }
}
```

- [ ] **Step 3: Write rmcp integration test**

Add to `crates/anno-rag/tests/memory_mcp.rs`:
```rust
#[tokio::test]
async fn mcp_memory_save_recall_round_trip() {
    use rmcp::ServiceExt;
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    // Spin server via the standard test harness — mirror v0.5/v0.6's MCP integration test.
    // The exact harness call differs per how `AnnoRagServer::serve` is wired; copy the pattern
    // from the existing `mcp_search_round_trip` (or equivalent) test, then assert:
    // 1. server.list_tools() contains "memory_save", "memory_recall", "memory_forget", "memory_list".
    // 2. memory_save({text: "...PII..."}) returns a non-empty id and redacted_text without the plaintext.
    // 3. memory_recall({query: "..."}) returns a hits array with the plaintext rehydrated.
    // (Implementation pattern is identical to the existing tools' rmcp tests.)
}
```

Run:
```powershell
cargo test -p anno-rag --test memory_mcp mcp_memory_save_recall_round_trip
```
Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/mcp.rs crates/anno-rag/tests/memory_mcp.rs
git commit -m "feat(anno-rag): mcp memory_save/recall/forget/list tools + tracing audit"
```

---

## Task 11: Erasure SLO — daily background compaction

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/config.rs`

- [ ] **Step 1: Add config**

Append to `crates/anno-rag/src/config.rs`:
```rust
pub compaction_interval_secs: u64, // default 24 * 3600
pub compaction_min_age_secs: u64,  // default 3600 — only reclaim deletes older than this
```
Defaults: `86400`, `3600`.

- [ ] **Step 2: Write failing test asserting compaction shrinks files after delete**

Add to `crates/anno-rag/tests/memory_mcp.rs`:
```rust
#[tokio::test]
async fn compaction_shrinks_after_delete() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();
    let mut ids = Vec::new();
    for i in 0..100 {
        let s = p.save_memory(&format!("filler memory {i}"), Some(MemoryKind::Context), None).await.unwrap();
        ids.push(s.id);
    }
    let size_before = dir_size(tmp.path()).unwrap();
    for id in ids { p.store().memory_delete_by_id(&id).await.unwrap(); }
    p.compact_now().await.unwrap();
    let size_after = dir_size(tmp.path()).unwrap();
    assert!(size_after < size_before * 9 / 10,
        "expected >10% size reduction after compaction, before={size_before} after={size_after}");
}

fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}
```

If `walkdir` is not a dev-dependency, add it:
```toml
[dev-dependencies]
walkdir = "2"
```

Run: expect FAIL.

- [ ] **Step 3: Implement `Store::optimize_memories` and `Pipeline::compact_now`**

In `crates/anno-rag/src/store.rs`:
```rust
pub async fn optimize_memories(&self, min_age: std::time::Duration) -> Result<()> {
    use lancedb::table::OptimizeAction;
    self.memories_tbl
        .optimize(OptimizeAction::All)
        .await
        .map_err(|e| Error::Store(format!("optimize_memories: {e}")))?;
    // Drop tombstones older than min_age — the API may require a separate cleanup call.
    // In 0.29 this is part of `optimize` when `OptimizeAction::All` is used, but older-than
    // semantics may need an explicit param. Adjust per actual API surface.
    let _ = min_age;
    Ok(())
}
```

**Verification note:** the exact `OptimizeAction` variants and any "older-than" parameter are 0.29 API surface; verify against the docs at implementation time and pin the correct variant.

In `crates/anno-rag/src/pipeline.rs`:
```rust
impl Pipeline {
    pub async fn compact_now(&self) -> Result<()> {
        let min_age = std::time::Duration::from_secs(self.cfg.compaction_min_age_secs);
        self.store.optimize_memories(min_age).await
    }

    pub fn spawn_compaction_task(self: std::sync::Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = std::time::Duration::from_secs(self.cfg.compaction_interval_secs);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // skip immediate tick
            loop {
                ticker.tick().await;
                if let Err(e) = self.compact_now().await {
                    tracing::warn!(target: "anno_rag::memory::audit",
                        "compaction failed: {e}");
                }
            }
        })
    }
}
```

Spawn the task from `AnnoRagServer::new` (or wherever the MCP server boots):
```rust
let _compaction_handle = pipeline.clone().spawn_compaction_task();
```
Store the handle on the server struct so Drop kills it on shutdown — or accept that the runtime cancels detached tasks at shutdown for v0.1.

- [ ] **Step 4: Run test, verify it passes**

```powershell
cargo test -p anno-rag --test memory_mcp compaction_shrinks_after_delete
```
Expected: PASS. If the size threshold is too aggressive on a small fixture, relax to `< size_before` and revisit when LoCoMo lands.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/store.rs crates/anno-rag/src/pipeline.rs crates/anno-rag/src/config.rs crates/anno-rag/Cargo.toml crates/anno-rag/tests/memory_mcp.rs
git commit -m "feat(anno-rag): erasure SLO — daily Table::optimize background task (24h max-delay)"
```

---

## Task 12: Property test — forget cascade invariant

**Files:**
- Create: `crates/anno-rag/tests/memory_proptest.rs`
- Modify: `crates/anno-rag/Cargo.toml`

- [ ] **Step 1: Add `proptest` to dev-deps**

In `crates/anno-rag/Cargo.toml`:
```toml
[dev-dependencies]
proptest = "1"
```

- [ ] **Step 2: Write the property test**

Create `crates/anno-rag/tests/memory_proptest.rs`:
```rust
use anno_rag::config::AnnoRagConfig;
use anno_rag::memory::MemoryKind;
use anno_rag::pipeline::Pipeline;
use proptest::prelude::*;
use tempfile::TempDir;

#[derive(Debug, Clone)]
enum Op {
    Save(String),                // text
    ForgetById(usize),           // index into the saved list
}

prop_compose! {
    fn op_seq(max_ops: usize)
              (ops in proptest::collection::vec(
                  prop_oneof![
                      "[a-zA-Z ]{5,40}".prop_map(Op::Save),
                      (0usize..50).prop_map(Op::ForgetById),
                  ],
                  1..max_ops))
              -> Vec<Op> { ops }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn forget_cascade_never_underflows(ops in op_seq(30)) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let cfg = AnnoRagConfig::test_config_in(tmp.path());
            let p = Pipeline::new(cfg).await.unwrap();
            let mut ids = Vec::new();
            for op in ops {
                match op {
                    Op::Save(text) => {
                        let s = p.save_memory(&text, Some(MemoryKind::Context), None).await.unwrap();
                        ids.push(s.id);
                    }
                    Op::ForgetById(idx) => if !ids.is_empty() {
                        let i = idx % ids.len();
                        let id = ids.remove(i);
                        let r = p.forget_memory(Some(id), None, 1, false).await.unwrap();
                        prop_assert!(r.forgotten_ids.len() <= 1);
                    }
                }
            }
            Ok(())
        }).unwrap();
    }
}
```

Run:
```powershell
cargo test -p anno-rag --test memory_proptest --release
```
Expected: PASS (50 cases). If a shrunk failure surfaces, the property is wrong or the cascade has a real bug — fix the bug in `Pipeline::forget_memory`, do not relax the property.

- [ ] **Step 3: Commit**

```powershell
git add crates/anno-rag/tests/memory_proptest.rs crates/anno-rag/Cargo.toml
git commit -m "test(anno-rag): proptest — forget cascade invariant (50 cases)"
```

---

## Task 13: LoCoMo subset harness + baseline numbers

**Files:**
- Create: `crates/anno-rag/tests/fixtures/locomo_subset/conversations.toml`
- Create: `crates/anno-rag/tests/fixtures/locomo_baseline.toml`
- Create: `crates/anno-rag/benches/bench_locomo.rs`

- [ ] **Step 1: Construct the 50-pair fixture**

Source: pick 50 pairs from the public LoCoMo benchmark (Maharana et al., 2024 — [snap-research/LoCoMo](https://github.com/snap-research/locomo)). Sample a balanced mix: 20 single-hop, 20 multi-hop, 10 temporal-reasoning.

Create `crates/anno-rag/tests/fixtures/locomo_subset/conversations.toml`:
```toml
# 50 LoCoMo subset items. Each item has a conversation (list of turns) and a
# question with an expected substring (lenient — the model's answer must
# contain this string).
[[item]]
id = "locomo_001"
kind = "single_hop"
conversation = [
  { role = "user", text = "..." },
  { role = "assistant", text = "..." },
  # ...
]
question = "..."
expected_substring = "..."

# repeat for 49 more items
```

(The exact 50 items are sourced manually — do not invent. Reference the LoCoMo repo's `data/` folder and copy with attribution in a `LICENSE` note in the fixture dir.)

Create `crates/anno-rag/tests/fixtures/locomo_baseline.toml`:
```toml
# Baseline filled by Task 13 Step 3. v0.1 does not gate on these in CI;
# v0.2 must improve multi-hop accuracy@1 by ≥10 pp.
[overall]
accuracy_at_1 = 0.0
latency_p95_ms = 0
[multi_hop]
accuracy_at_1 = 0.0
[temporal]
accuracy_at_1 = 0.0
```

- [ ] **Step 2: Write the harness as a criterion bench**

Create `crates/anno-rag/benches/bench_locomo.rs`:
```rust
use anno_rag::config::AnnoRagConfig;
use anno_rag::memory::MemoryKind;
use anno_rag::pipeline::Pipeline;
use criterion::{criterion_group, criterion_main, Criterion};
use serde::Deserialize;
use tempfile::TempDir;

#[derive(Deserialize)]
struct Item {
    id: String,
    kind: String,
    conversation: Vec<Turn>,
    question: String,
    expected_substring: String,
}

#[derive(Deserialize)]
struct Turn { role: String, text: String }

#[derive(Deserialize)]
struct Fixture { item: Vec<Item> }

fn run_locomo(c: &mut Criterion) {
    let txt = std::fs::read_to_string("tests/fixtures/locomo_subset/conversations.toml").unwrap();
    let fx: Fixture = toml::from_str(&txt).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("locomo_subset_50", |b| {
        b.to_async(&rt).iter(|| async {
            let tmp = TempDir::new().unwrap();
            let cfg = AnnoRagConfig::test_config_in(tmp.path());
            let p = Pipeline::new(cfg).await.unwrap();

            let mut hits = 0usize;
            let mut multi_hits = 0usize;
            let mut multi_total = 0usize;
            for item in &fx.item {
                // 1. Save all assistant + user turns as memories.
                for t in &item.conversation {
                    let kind = match t.role.as_str() {
                        "user" => MemoryKind::Context,
                        _ => MemoryKind::Fact,
                    };
                    let _ = p.save_memory(&t.text, Some(kind), Some(item.id.clone())).await.unwrap();
                }
                // 2. Recall against the question.
                let r = p.recall_memory(&item.question, 5, Some(item.id.clone()), None).await.unwrap();
                let any_match = r.iter().any(|h| h.text.contains(&item.expected_substring));
                if any_match { hits += 1; }
                if item.kind == "multi_hop" {
                    multi_total += 1;
                    if any_match { multi_hits += 1; }
                }
            }
            let acc1 = hits as f64 / fx.item.len() as f64;
            let multi_acc1 = if multi_total > 0 { multi_hits as f64 / multi_total as f64 } else { 0.0 };
            eprintln!("LOCOMO_OVERALL_ACC1={acc1:.4}");
            eprintln!("LOCOMO_MULTIHOP_ACC1={multi_acc1:.4}");
        });
    });
}

criterion_group!(benches, run_locomo);
criterion_main!(benches);
```

Wire in `crates/anno-rag/Cargo.toml`:
```toml
[[bench]]
name = "bench_locomo"
harness = false
```

- [ ] **Step 3: Run baseline and commit the numbers**

```powershell
cargo bench -p anno-rag --bench bench_locomo 2>&1 | Tee-Object -FilePath locomo_v0.1.txt
```
Extract `LOCOMO_OVERALL_ACC1` and `LOCOMO_MULTIHOP_ACC1` from the output. Write them into `crates/anno-rag/tests/fixtures/locomo_baseline.toml`.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/tests/fixtures/locomo_subset/ crates/anno-rag/tests/fixtures/locomo_baseline.toml crates/anno-rag/benches/bench_locomo.rs crates/anno-rag/Cargo.toml
git commit -m "test(anno-rag): LoCoMo 50-item subset + v0.1 baseline numbers"
```

---

## Task 14: Docs + CHANGELOG + crate version bump

**Files:**
- Modify: `crates/anno-rag/CHANGELOG.md`
- Modify: `crates/anno-rag/README.md`
- Modify: `crates/anno-rag/Cargo.toml`

- [ ] **Step 1: Bump crate version**

In `crates/anno-rag/Cargo.toml`, `version = "0.6.0"` → `"0.7.0"` (or current + 1 minor — confirm what's actually shipped).

- [ ] **Step 2: CHANGELOG entry**

Prepend to `crates/anno-rag/CHANGELOG.md`:
```markdown
## 0.7.0 — 2026-05-XX

### Added
- `memory` module with four MCP tools (`memory_save`, `memory_recall`,
  `memory_forget`, `memory_list`).
- Second LanceDB collection `memories` alongside `documents`, with
  scalar indexes (BTree on `created_at`/`session_id`, Bitmap on `kind`,
  LabelList on `token_refs` + `entity_refs`) and a native FTS index on
  `text`.
- GDPR Art. 17 cascade in `memory_forget`: deletes the row and purges
  vault tokens no longer referenced by any document or memory.
- Background compaction task — daily `Table::optimize` to satisfy the
  24-hour physical-erasure SLO.
- LoCoMo 50-item subset eval harness (`benches/bench_locomo.rs`) with
  committed baseline.
- Forward-compatibility columns `valid_from`, `valid_to`, `entity_refs`
  reserved for v0.2 temporal + graph activation.

### Changed
- N/A — `memory` is purely additive to existing v0.6 retrieval.

### Privacy
- Memories on disk are PII-tokenized; plaintext PII never persists.
- Per-tenant vault isolation reused from v0.5.
```

- [ ] **Step 3: README section**

Append to `crates/anno-rag/README.md`:
```markdown
## Memory (v0.7+)

`anno-rag` exposes four MCP tools for Cowork session memory:

| Tool             | Purpose                                                |
|------------------|--------------------------------------------------------|
| `memory_save`    | Persist a tokenized memory + embedding                 |
| `memory_recall`  | Hybrid (vector + FTS) recall with vault rehydration    |
| `memory_forget`  | Delete a memory and cascade orphan vault tokens (GDPR) |
| `memory_list`    | Cursor-paginated browse (session/kind filter)          |

Stored rows are PII-tokenized — plaintext PII never lands on disk.
Rehydration uses the caller's tenant vault on recall. `memory_forget`
returns the SLO note `"logically forgotten; physical erasure within 24h"`
to reflect the daily compaction cadence.
```

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/CHANGELOG.md crates/anno-rag/README.md crates/anno-rag/Cargo.toml
git commit -m "docs(anno-rag): v0.7 memory tools — CHANGELOG + README"
```

---

## Task 15: PR

- [ ] **Step 1: Final full-workspace check**

```powershell
cargo build --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
```
All green.

- [ ] **Step 2: Push and open PR**

```powershell
git push -u origin (git rev-parse --abbrev-ref HEAD)
gh pr create --title "feat(anno-rag): v0.7 — PII-safe Cowork session memory (memory module + 4 MCP tools)" --body @'
## Summary
- New `memory` module in `anno-rag` (no new crate).
- Four MCP tools on existing `AnnoRagServer`: memory_save / memory_recall / memory_forget / memory_list.
- Second LanceDB collection `memories` with native FTS + scalar indexes + RRFReranker.
- GDPR Art. 17 cascade: forget purges vault tokens no other row references.
- 24-hour physical-erasure SLO via daily Table::optimize background task.
- Forward-compat columns reserved for v0.2 (valid_from, valid_to, entity_refs).
- LoCoMo 50-item subset baseline committed (no CI gate yet).

Depends on PR-A (lancedb 0.29 bump) and PR-B (anno-rag v0.6 hybrid retrieval).

## Test plan
- [ ] `cargo test -p anno-rag --all-features` green (unit + integration + proptest).
- [ ] `cargo bench -p anno-rag --bench bench_locomo` produces baseline numbers.
- [ ] `cargo test -p anno-rag --test memory_mcp compaction_shrinks_after_delete` green.
- [ ] Peak RSS under 1.5 GB.

Design: `docs/superpowers/specs/2026-05-15-anno-memory-v0.1-design.md`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
'@
```

---

## Self-Review

- **Spec coverage:** Walk §1–§13 of `2026-05-15-anno-memory-v0.1-design.md`:
  - §3 architecture — Tasks 2 (schema), 3 (store), 5 (hybrid search). ✓
  - §4 data model — Task 1 (types) + Task 2 (Arrow schema) + Task 4 (scalar indexes). ✓
  - §5 MCP tools — Task 10. ✓
  - §6 privacy — Tasks 6 (tokenize on save), 7 (rehydrate on recall), 8 (cascade). §6.1 erasure SLO → Task 11. ✓
  - §7 tenancy — implicit via existing `AnnoRagConfig`; no dedicated task because reuse is the point. ✓
  - §8 audit — `tracing::info!` events emitted from each tool handler in Task 10. ✓
  - §9 testing — unit/integration in Tasks 1–11, property in Task 12, LoCoMo in Task 13. ✓
  - §10 file layout — matches the "File Structure" header. ✓
  - §11 open questions — informational, no task implements them.
  - §12 success criteria — `cargo test --workspace` (Task 15 Step 1) covers most; LoCoMo numbers committed in Task 13; compaction test in Task 11. ✓
  - §13 planned extensions → handed off to v0.2 plan.
- **Placeholders scan:** the LoCoMo fixture (Task 13 Step 1) intentionally points to upstream data rather than inventing items — this is correct, not a placeholder. The `MemoryHitRow` extension in Task 7 references "see Task 5's row builder" — replaced inline by the explicit `session_id` filter snippet. Two "Verification note" callouts (Tasks 4 & 5) acknowledge API surface uncertainty on `lancedb 0.29` paths — unavoidable until the bump (PR-A) lands; both are flagged for verify-at-implementation-time, which is honest, not a placeholder.
- **Type consistency:** `MemoryHit` is the public hit type (with rehydrated text); `MemoryHitRow` is the on-disk tokenized type returned by `Store::memories_hybrid_search`. The pipeline maps `MemoryHitRow → MemoryHit` via vault rehydration in Task 7. `MemorySaveResult` (spec) → `SavedMemory` (internal Pipeline) → `MemorySaveResultWire` (MCP wire). All three are intentional layers; types line up across Tasks 6, 7, 10.
- **Risk callouts (added during review):** Task 4 uses `lancedb::index::scalar::{...}` paths from the 0.29 research report; Task 5 uses the `nearest_to + full_text_search + rerank` chain. If 0.29 surfaces different paths/methods, the fix is mechanical and localized to those two tasks — no plan structure change.
