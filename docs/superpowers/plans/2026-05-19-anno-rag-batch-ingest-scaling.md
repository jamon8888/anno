# anno-rag Batch Ingest Scaling Implementation Plan

> **Status update — 2026-05-19:** superseded after measurement. The A″ NER
> instance pool and B document fan-out failed the throughput acceptance check
> on the target hardware, so they are intentionally removed from the product
> path. Keep the plan below as historical evidence; the active salvage is
> deterministic/resumable ingest only: content-hashed `doc_id`, same-content
> skip, and `source_path` orphan cleanup.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `Pipeline::ingest_folder` ingest ~1000 documents on a laptop materially faster and, critically, resumably/idempotently — via a bounded NER engine pool (A″), bounded document fan-out (B), and deterministic content-hashed `doc_id` (C).

**Architecture:** No `anno`-crate changes. (A″) A small generic `Pool<T>` holds N independent `Detector` instances (each its own mutexed ONNX session) so NER runs on N engines at once. (B) `ingest_folder` drives docs through `futures::buffer_unordered` bounded concurrency (no `Arc<Self>`/spawn needed — single task, concurrent futures over `&self`). (C) `doc_id` becomes `Uuid::new_v5(NAMESPACE_OID, file_bytes)` so the *existing* `merge_insert(&["doc_id","chunk_idx"])` is idempotent across runs; a cheap `Store::doc_exists` skips already-ingested files; a delete-by-`source_path` clears orphans when content changes.

**Tech Stack:** Rust, tokio, `futures` (`StreamExt::buffer_unordered`), LanceDB 0.27 (`merge_insert`/`delete`/`count_rows`), `uuid` v5, the existing `anno` gliner2-fastino backend (unchanged).

**Spec:** `docs/superpowers/specs/2026-05-18-anno-rag-batch-ingest-scaling-design.md` (rev. 2, local commit `6ada1e05`).

**Grounding facts (verified — do not re-derive):**
- `crates/anno-rag/src/pipeline.rs`: `struct Pipeline` field `detector: OnceCell<Arc<Detector>>` (line ~17), init `detector: OnceCell::new()` (~46), `fn detector_get_or_init(&self) -> Result<&Arc<Detector>>` (~65, sync). `pub async fn ingest_one(&self, path: &Path, output_dir: &Path) -> Result<()>` (~111): `let doc_id = Uuid::now_v7();` (~112), then `for chunk in &extracted.chunks { let entities = self.detector_get_or_init()?.detect(&chunk.text)?; let pseudo = self.vault.pseudonymize(&chunk.text, &entities).await?; ... }` (~120-124), then `embed_batch`, build `ChunkRecord`s with `doc_id`, `self.store.upsert(records).await?`, write `<stem>.anon.md`. `pub async fn ingest_folder(&self, folder: &Path, recursive: bool, output_dir: &Path) -> Result<usize>` (~167): walkdir filter by extension, sequential `for entry { match self.ingest_one(path, output_dir).await {Ok=>count+=1, Err=>warn} }` (~206-211), then `self.store.maybe_build_index(...)` + `self.store.maybe_build_fts_index()`, `Ok(count)`.
- `crates/anno-rag/src/detect.rs`: `pub struct Detector { ner: GLiNER2Fastino }`; `pub fn new() -> Result<Self>` (loads model via `GLiNER2Fastino::from_pretrained(NER_MODEL_ID)`); `pub fn detect(&self, text: &str) -> Result<Vec<DetectedEntity>>` (`&self`). Multiple instances are independent.
- `crates/anno-rag/src/store.rs`: `pub async fn upsert(&self, records: Vec<ChunkRecord>) -> Result<()>` uses `self.tbl.merge_insert(&["doc_id","chunk_idx"])` (~738/746). `chunk_uuid(doc_id, chunk_idx) = Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("{doc_id}::{chunk_idx}").as_bytes())` (~1215). Delete idiom: `self.tbl.delete(&filter).await` where filter is a SQL string (memory path: `format!("id = '{}'", id)`). The chunks table handle is `self.tbl`.
- Bounded-concurrency idiom in repo: `crates/anno-rag-tabular/src/fanout.rs:162` `Arc<tokio::sync::Semaphore::new(n)>` + `tokio::spawn` + `acquire_owned`. This plan uses the lighter `futures::StreamExt::buffer_unordered` (no spawn/Arc<Self> — `ingest_one` borrows `&self` across concurrent futures in one task).
- `crates/anno-rag/src/config.rs`: serde struct `AnnoRagConfig`, `fn default_*()` helpers, `impl Default for AnnoRagConfig` (~174). Add fields following the existing `#[serde(default="default_x")]` + helper + Default-literal pattern (same as the recently-added `rerank_*` fields at ~161-170).
- `futures` and `uuid` are existing deps of `anno-rag`. `std::thread::available_parallelism()` (std) gives core count — no `num_cpus` dep.
- Worktree: this plan executes on a branch off the latest `main` (post PR#10). First cold `cargo` build of the workspace ~10 min; use `CARGO_INCREMENTAL=0` and single-process cargo (the Windows host shows LNK1318 PDB / rlib-format races under concurrent or incremental cargo). Heavy model/LanceDB tests are `#[ignore]`; run with `-- --ignored --test-threads=1`.
- Cargo single-test: `cargo test -p anno-rag --lib <path> -- --exact`. Integration: `cargo test -p anno-rag --test ingest_scale -- --ignored --test-threads=1`.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/anno-rag/src/config.rs` | Modify | `ingest_concurrency`, `ingest_ner_pool` config + defaults |
| `crates/anno-rag/src/pool.rs` | Create | Generic bounded `Pool<T>` + RAII `PoolGuard` (unit-testable with `T=u32`) |
| `crates/anno-rag/src/lib.rs` | Modify | `mod pool;` declaration |
| `crates/anno-rag/src/store.rs` | Modify | `Store::doc_exists(doc_id)`, `Store::delete_doc_rows(source_path)` |
| `crates/anno-rag/src/pipeline.rs` | Modify | deterministic `doc_id` helper; C1/C2/orphan wiring in `ingest_one`; `detector_pool` field + lazy build; pooled detect in `ingest_one`; `buffer_unordered` fan-out in `ingest_folder` |
| `crates/anno-rag/tests/ingest_scale.rs` | Create | Idempotency/resume + concurrency-safety + throughput-smoke integration tests |

---

## Task 1: Config — `ingest_concurrency` + `ingest_ner_pool`

**Files:**
- Modify: `crates/anno-rag/src/config.rs`
- Test: `crates/anno-rag/src/config.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Add to `#[cfg(test)] mod tests` in `crates/anno-rag/src/config.rs`:

```rust
#[test]
fn ingest_scaling_defaults_are_sane() {
    let c = AnnoRagConfig::default();
    assert!(c.ingest_concurrency >= 1, "concurrency >= 1");
    assert!(c.ingest_ner_pool >= 1, "ner pool >= 1");
    assert!(
        c.ingest_ner_pool <= 4,
        "ner pool capped at 4 for RSS budget, got {}",
        c.ingest_ner_pool
    );
    assert!(
        c.ingest_concurrency >= c.ingest_ner_pool,
        "concurrency ({}) must be >= ner pool ({})",
        c.ingest_concurrency,
        c.ingest_ner_pool
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag --lib config::tests::ingest_scaling_defaults_are_sane -- --exact`
Expected: FAIL — `no field 'ingest_concurrency'`.

- [ ] **Step 3: Add fields, default fns, Default wiring**

In the `AnnoRagConfig` struct (after the `rerank_batch_size` field):

```rust
    /// Max documents processed concurrently by `ingest_folder`
    /// (non-NER stages overlap; NER bounded by `ingest_ner_pool`).
    /// Default: detected core count.
    #[serde(default = "default_ingest_concurrency")]
    pub ingest_concurrency: usize,

    /// Number of independent NER engines (each its own ONNX session)
    /// run in parallel during ingest. Each costs ~one gliner2 model
    /// resident in RAM — capped low for a laptop RSS budget.
    /// Default: min(cores, 4).
    #[serde(default = "default_ingest_ner_pool")]
    pub ingest_ner_pool: usize,
```

Near the other `default_*` fns:

```rust
fn detected_cores() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
}
fn default_ingest_concurrency() -> usize {
    detected_cores().max(1)
}
fn default_ingest_ner_pool() -> usize {
    detected_cores().clamp(1, 4)
}
```

In the `impl Default for AnnoRagConfig` `Self { ... }` literal (after `rerank_batch_size: default_rerank_batch_size(),`):

```rust
            ingest_concurrency: default_ingest_concurrency(),
            ingest_ner_pool: default_ingest_ner_pool(),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p anno-rag --lib config::tests::ingest_scaling_defaults_are_sane -- --exact`
Expected: PASS. Also run `cargo test -p anno-rag --lib config::` — all config tests green (serde round-trip / v0.1-compat must still pass with the new `#[serde(default)]` fields).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/config.rs
git commit -m "feat(ingest): ingest_concurrency + ingest_ner_pool config"
```

---

## Task 2: Deterministic `doc_id` from file hash (Lever C1)

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs` (add `doc_uuid` helper; change `ingest_one`'s `doc_id`)
- Test: `crates/anno-rag/src/pipeline.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Add to `#[cfg(test)] mod tests` in `crates/anno-rag/src/pipeline.rs`:

```rust
#[test]
fn doc_uuid_is_deterministic_and_content_sensitive() {
    let a1 = super::doc_uuid(b"hello world");
    let a2 = super::doc_uuid(b"hello world");
    let b = super::doc_uuid(b"hello world!");
    assert_eq!(a1, a2, "same bytes => same doc_id");
    assert_ne!(a1, b, "different bytes => different doc_id");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag --lib pipeline::tests::doc_uuid_is_deterministic_and_content_sensitive -- --exact`
Expected: FAIL — `cannot find function 'doc_uuid'`.

- [ ] **Step 3: Add the helper and use it in `ingest_one`**

In `crates/anno-rag/src/pipeline.rs`, add a module-level fn (near the top, after imports):

```rust
/// Deterministic document id: UUID v5 (OID namespace) of the raw file
/// bytes. Same file content ⇒ same `doc_id` ⇒ the existing
/// `merge_insert(&["doc_id","chunk_idx"])` overwrites its own rows
/// instead of duplicating across `ingest_folder` runs.
#[must_use]
pub(crate) fn doc_uuid(file_bytes: &[u8]) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, file_bytes)
}
```

In `ingest_one`, replace:

```rust
        let extracted = ingest::extract(path, &self.cfg).await?;
        let doc_id = Uuid::now_v7();
```

with:

```rust
        let file_bytes = std::fs::read(path).map_err(Error::from)?;
        let doc_id = doc_uuid(&file_bytes);
        let extracted = ingest::extract(path, &self.cfg).await?;
```

(`Uuid` is already imported in pipeline.rs. Reading the file is cheap relative to extraction+NER and is needed for the stable id; extraction still reads it again via kreuzberg — acceptable at the 1000-doc scale.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p anno-rag --lib pipeline::tests::doc_uuid_is_deterministic_and_content_sensitive -- --exact`
Expected: PASS. Run `cargo check -p anno-rag` — clean.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/pipeline.rs
git commit -m "feat(ingest): deterministic doc_id = uuid_v5(file bytes) for idempotent re-ingest"
```

---

## Task 3: `Store::doc_exists` + skip-already-ingested (Lever C2)

**Files:**
- Modify: `crates/anno-rag/src/store.rs` (new `doc_exists`)
- Modify: `crates/anno-rag/src/pipeline.rs` (`ingest_one` early-skip)
- Test: `crates/anno-rag/src/store.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Add to `#[cfg(test)] mod tests` in `crates/anno-rag/src/store.rs` (use the existing `fresh_cfg` helper):

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag --lib store::tests::doc_exists_false_then_true_after_upsert -- --ignored --exact`
Expected: FAIL — `no method named 'doc_exists'`.

- [ ] **Step 3: Implement `Store::doc_exists`**

In `crates/anno-rag/src/store.rs` `impl Store`, near `memory_row_count`:

```rust
    /// Whether any chunk row exists for `doc_id`. Cheap filtered count.
    /// Used by `ingest_one` to skip files already ingested (same
    /// content hash ⇒ same `doc_id`).
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the LanceDB count fails.
    pub async fn doc_exists(&self, doc_id: Uuid) -> Result<bool> {
        let filter = format!("doc_id = '{doc_id}'");
        let n = self
            .tbl
            .count_rows(Some(filter))
            .await
            .map_err(|e| Error::Store(format!("doc_exists count_rows: {e}")))?;
        Ok(n > 0)
    }
```

> Filter form note: the chunks `doc_id` column is written through
> `merge_insert(&["doc_id","chunk_idx"])`; the string-uuid predicate
> `doc_id = '<uuid>'` matches the same encoding the memory path uses
> (`id = '<uuid>'`). If `count_rows` rejects the predicate (column-type
> mismatch), confirm the `doc_id` Arrow column type in `chunks_schema`
> and adjust the literal (e.g. unhyphenated, or `X'..'` binary) — keep
> the method signature identical.

- [ ] **Step 4: Add the skip to `ingest_one`**

In `ingest_one`, immediately after `let doc_id = doc_uuid(&file_bytes);` and **before** `ingest::extract`:

```rust
        if self.store.doc_exists(doc_id).await? {
            tracing::info!(path = %path.display(), "skip: already ingested (same content)");
            return Ok(());
        }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p anno-rag --lib store::tests::doc_exists_false_then_true_after_upsert -- --ignored --exact`
Expected: PASS. `cargo check -p anno-rag` clean.

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag/src/store.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat(ingest): Store::doc_exists + skip already-ingested files"
```

---

## Task 4: `Store::delete_doc_rows` + orphan cleanup on content change (Lever C1 completeness)

**Files:**
- Modify: `crates/anno-rag/src/store.rs` (new `delete_doc_rows`)
- Modify: `crates/anno-rag/src/pipeline.rs` (`ingest_one`: clear stale rows for this `source_path` before upsert)
- Test: `crates/anno-rag/src/store.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

```rust
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
    store.upsert(vec![mk("a", "A.txt"), mk("b", "B.txt")]).await.expect("up");
    store.delete_doc_rows("A.txt").await.expect("del");
    assert!(
        !store
            .doc_exists(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"a"))
            .await
            .unwrap()
    );
    assert!(
        store
            .doc_exists(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"b"))
            .await
            .unwrap()
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag --lib store::tests::delete_doc_rows_removes_only_that_source -- --ignored --exact`
Expected: FAIL — `no method named 'delete_doc_rows'`.

- [ ] **Step 3: Implement `Store::delete_doc_rows`**

In `impl Store` (mirror the memory delete idiom):

```rust
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
```

- [ ] **Step 4: Wire into `ingest_one`**

In `ingest_one`, the skip in Task 3 already returns early when the *same* content is present. When we do NOT skip (new or changed content), clear any stale rows for this path before upserting. Add immediately before `self.store.upsert(records).await?`:

```rust
        // Content changed (or first ingest): remove any prior rows for
        // this source_path so a superseded doc_id doesn't orphan.
        self.store
            .delete_doc_rows(&extracted.source_path)
            .await?;
```

(`extracted.source_path` is the same value placed in `ChunkRecord.source_path`. Confirm the field name on `extracted` matches what `ChunkRecord { source_path: extracted.source_path.clone(), .. }` already uses in `ingest_one`.)

- [ ] **Step 5: Run tests**

Run: `cargo test -p anno-rag --lib store::tests::delete_doc_rows_removes_only_that_source -- --ignored --exact`
Expected: PASS. `cargo check -p anno-rag` clean.

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag/src/store.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat(ingest): delete_doc_rows clears orphans on content change"
```

---

## Task 5: Generic bounded `Pool<T>` (Lever A″ mechanics)

**Files:**
- Create: `crates/anno-rag/src/pool.rs`
- Modify: `crates/anno-rag/src/lib.rs` (`mod pool;`)
- Test: `crates/anno-rag/src/pool.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag/src/pool.rs`:

```rust
//! Bounded pool of reusable heavy resources (e.g. NER engines). An
//! `acquire().await` hands out an RAII guard; dropping it returns the
//! item to the pool. Concurrency is bounded by the pool size.

use std::sync::Mutex;
use tokio::sync::Semaphore;

/// Fixed-size pool of `T`. `acquire` blocks when all are checked out.
pub struct Pool<T> {
    items: Mutex<Vec<T>>,
    sem: Semaphore,
}

/// RAII handle; returns the item to the pool on drop.
pub struct PoolGuard<'a, T> {
    pool: &'a Pool<T>,
    val: Option<T>,
}

impl<T> Pool<T> {
    /// Build a pool from `items` (must be non-empty).
    #[must_use]
    pub fn new(items: Vec<T>) -> Self {
        let n = items.len();
        Self {
            items: Mutex::new(items),
            sem: Semaphore::new(n),
        }
    }

    /// Number of resources in the pool.
    #[must_use]
    pub fn size(&self) -> usize {
        self.sem.available_permits() + {
            // permits in use can't be counted directly; size is fixed
            // at construction — track via items capacity instead.
            0
        }
    }

    /// Acquire one resource, awaiting a free slot. Never panics.
    pub async fn acquire(&self) -> PoolGuard<'_, T> {
        let permit = self
            .sem
            .acquire()
            .await
            .expect("pool semaphore never closed");
        permit.forget(); // permit lifetime tied to guard drop instead
        let val = self
            .items
            .lock()
            .expect("pool mutex poisoned")
            .pop()
            .expect("permit implies an available item");
        PoolGuard {
            pool: self,
            val: Some(val),
        }
    }
}

impl<T> std::ops::Deref for PoolGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.val.as_ref().expect("guard holds a value until drop")
    }
}

impl<T> Drop for PoolGuard<'_, T> {
    fn drop(&mut self) {
        if let Some(v) = self.val.take() {
            self.pool
                .items
                .lock()
                .expect("pool mutex poisoned")
                .push(v);
            self.pool.sem.add_permits(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn acquire_returns_distinct_items_and_bounds_concurrency() {
        let pool = Pool::new(vec![1u32, 2u32]);
        let a = pool.acquire().await;
        let b = pool.acquire().await;
        assert_ne!(*a, *b, "two acquires give the two distinct items");

        // Third acquire must block until one is released.
        let blocked = tokio::time::timeout(Duration::from_millis(50), pool.acquire()).await;
        assert!(blocked.is_err(), "3rd acquire blocks while 2 are out");

        drop(a);
        let c = tokio::time::timeout(Duration::from_millis(200), pool.acquire())
            .await
            .expect("acquire succeeds after a release");
        assert!(*c == 1 || *c == 2);
    }
}
```

In `crates/anno-rag/src/lib.rs`, add alongside the other `mod` lines:

```rust
mod pool;
```

- [ ] **Step 2: Run test to verify it fails, then passes**

Run: `cargo test -p anno-rag --lib pool::tests::acquire_returns_distinct_items_and_bounds_concurrency -- --exact`
Expected: first run before adding `mod pool;`/file → compile FAIL; after Step 1 content in place → **PASS**. (Remove the dead `size()` body if clippy flags it: replace its body with `self.items.lock().expect("pool mutex poisoned").len() + self.sem.available_permits()` is *not* correct under contention — instead store an explicit `size: usize` field set in `new` and return it. Apply that simplification now: add `size: usize` to the struct, set in `new`, `size()` returns it.)

- [ ] **Step 3: Apply the `size` simplification**

Replace the `size` field/method as noted: add `size: usize` to `struct Pool<T>`, set `size: n` in `new`, and:

```rust
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }
```

- [ ] **Step 4: Re-run + clippy**

Run: `cargo test -p anno-rag --lib pool:: -- --exact` → PASS.
Run: `cargo clippy -p anno-rag --lib -- -D warnings` → clean.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/pool.rs crates/anno-rag/src/lib.rs
git commit -m "feat(ingest): generic bounded Pool<T> with RAII guard"
```

---

## Task 6: `detector_pool` in `Pipeline`; pooled NER in `ingest_one` (Lever A″ wiring)

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs` (field + lazy build + use in `ingest_one`)
- Test: covered by Task 8's heavy integration (pool correctness is exercised end-to-end); a light unit test here asserts lazy-build sizing.

- [ ] **Step 1: Add the field + lazy builder**

In `struct Pipeline`, after the existing `detector: OnceCell<Arc<Detector>>` field:

```rust
    detector_pool: OnceCell<crate::pool::Pool<Detector>>,
```

In `Pipeline::new`'s `Ok(Self { ... })` literal, after `detector: OnceCell::new(),`:

```rust
            detector_pool: OnceCell::new(),
```

Add a lazy initializer near `detector_get_or_init` (async; building N models is slow, do it once):

```rust
    /// Lazy-build the NER engine pool (`cfg.ingest_ner_pool` independent
    /// `Detector`s, each its own ONNX session). Built on first ingest,
    /// not at `Pipeline::new`, to keep non-ingest startup RSS low.
    ///
    /// # Errors
    /// [`Error::Detect`] if any engine fails to load.
    async fn detector_pool(&self) -> Result<&crate::pool::Pool<Detector>> {
        if let Some(p) = self.detector_pool.get() {
            return Ok(p);
        }
        let n = self.cfg.ingest_ner_pool.max(1);
        let mut engines = Vec::with_capacity(n);
        for _ in 0..n {
            engines.push(Detector::new()?);
        }
        let _ = self.detector_pool.set(crate::pool::Pool::new(engines));
        Ok(self.detector_pool.get().expect("just set"))
    }
```

- [ ] **Step 2: Use the pool in `ingest_one`**

Replace the per-chunk detect loop:

```rust
        let mut pseudo_chunks: Vec<String> = Vec::with_capacity(extracted.chunks.len());
        for chunk in &extracted.chunks {
            let entities = self.detector_get_or_init()?.detect(&chunk.text)?;
            let pseudo = self.vault.pseudonymize(&chunk.text, &entities).await?;
            pseudo_chunks.push(pseudo);
        }
```

with (acquire one engine for this document's whole chunk loop — holding it for the doc keeps engine count = concurrency-bounded and avoids per-chunk acquire churn):

```rust
        let mut pseudo_chunks: Vec<String> = Vec::with_capacity(extracted.chunks.len());
        {
            let engine = self.detector_pool().await?.acquire().await;
            for chunk in &extracted.chunks {
                let entities = engine.detect(&chunk.text)?;
                let pseudo = self.vault.pseudonymize(&chunk.text, &entities).await?;
                pseudo_chunks.push(pseudo);
            }
        }
```

(`engine` derefs to `&Detector` via `PoolGuard`. The guard drops at the block's end, returning the engine. `detector_get_or_init` stays for the non-ingest `Pipeline::detect`/search path — unchanged.)

- [ ] **Step 3: Light unit test (sizing)**

Add to `pipeline.rs` `#[cfg(test)] mod tests`:

```rust
#[tokio::test]
#[ignore = "loads N gliner2 models"]
async fn detector_pool_builds_configured_size() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        ingest_ner_pool: 2,
        ..Default::default()
    };
    let p = Pipeline::new(cfg, [0u8; 32]).await.expect("pipeline");
    let pool = p.detector_pool().await.expect("pool");
    assert_eq!(pool.size(), 2);
}
```

- [ ] **Step 4: Verify**

Run: `cargo check -p anno-rag` and `cargo check -p anno-rag --features rerank` → clean.
Run: `cargo test -p anno-rag --lib pipeline::tests::detector_pool_builds_configured_size -- --ignored --exact` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/pipeline.rs
git commit -m "feat(ingest): per-doc pooled NER engine (Lever A'')"
```

---

## Task 7: Bounded fan-out in `ingest_folder` (Lever B)

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs` (`ingest_folder` loop → `buffer_unordered`)

- [ ] **Step 1: Replace the sequential loop with bounded concurrency**

In `ingest_folder`, the eligible-path filtering stays. Collect the eligible paths into a `Vec<std::path::PathBuf>` first (the walkdir + extension `match` unchanged — just push `path.to_path_buf()` into a vec instead of calling `ingest_one` inline). Then replace the sequential `for entry { ingest_one }` with:

```rust
        use futures::StreamExt;
        let conc = self.cfg.ingest_concurrency.max(1);
        let count = futures::stream::iter(paths.into_iter())
            .map(|p| async move {
                match self.ingest_one(&p, output_dir).await {
                    Ok(()) => 1usize,
                    Err(e) => {
                        tracing::warn!(path = %p.display(), error = %e, "ingest skipped");
                        0
                    }
                }
            })
            .buffer_unordered(conc)
            .fold(0usize, |acc, n| async move { acc + n })
            .await;
```

Keep the post-loop `maybe_build_index` / `maybe_build_fts_index` calls and `Ok(count)` exactly as they are (index build still happens once after all docs). `futures` is already a dep; `StreamExt` brings `map`/`buffer_unordered`/`fold`. No `Arc<Self>`/`tokio::spawn` — `ingest_one` borrows `&self` across the concurrent futures within this single task, which is sound because `ingest_one` is `&self` and all shared state (`store`, `embedder`, `vault`, `detector_pool`) is interior-synchronized.

- [ ] **Step 2: Verify it compiles + default tests**

Run: `cargo check -p anno-rag` → clean.
Run: `cargo test -p anno-rag --lib -- --exact` (fast unit tests) → green.
Run: `cargo clippy -p anno-rag --all-targets -- -D warnings` → clean (watch for `clippy::redundant_closure` / lifetime nits in the stream chain; adjust closure form if flagged, keep behavior).

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/src/pipeline.rs
git commit -m "feat(ingest): bounded document fan-out in ingest_folder (Lever B)"
```

---

## Task 8: Heavy integration — idempotency, resume, concurrency-safety, throughput

**Files:**
- Create: `crates/anno-rag/tests/ingest_scale.rs`

- [ ] **Step 1: Write the integration tests**

Create `crates/anno-rag/tests/ingest_scale.rs`:

```rust
//! End-to-end ingest scaling: idempotent re-ingest (no duplication),
//! resume/skip, content-change supersede, concurrency-safety, and a
//! recorded throughput smoke. Heavy (LanceDB + N NER models); ignored
//! by default.

use anno_rag::{AnnoRagConfig, Pipeline};

fn cfg(dir: &std::path::Path, conc: usize, pool: usize) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        ingest_concurrency: conc,
        ingest_ner_pool: pool,
        ..Default::default()
    }
}

fn write_corpus(dir: &std::path::Path, n: usize) {
    std::fs::create_dir_all(dir).unwrap();
    for i in 0..n {
        std::fs::write(
            dir.join(format!("doc_{i}.txt")),
            format!("Contrat numéro {i}. Responsabilité contractuelle du débiteur."),
        )
        .unwrap();
    }
}

#[tokio::test]
#[ignore = "LanceDB + NER models; heavy"]
async fn reingest_is_idempotent_and_resumable() {
    let tmp = tempfile::tempdir().unwrap();
    let p = Pipeline::new(cfg(tmp.path(), 4, 2), [0u8; 32])
        .await
        .expect("pipeline");
    let corpus = tmp.path().join("corpus");
    write_corpus(&corpus, 5);
    let out = tmp.path().join("out");

    let n1 = p.ingest_folder(&corpus, false, &out).await.expect("ingest 1");
    assert_eq!(n1, 5, "first run ingests all 5");

    // Re-run: every file's content unchanged ⇒ same doc_id ⇒ all skipped.
    let n2 = p.ingest_folder(&corpus, false, &out).await.expect("ingest 2");
    assert_eq!(n2, 0, "second run skips all 5 (resume/idempotent)");

    // Add one file ⇒ only it is ingested.
    std::fs::write(corpus.join("doc_new.txt"), "Nouveau contrat de bail.").unwrap();
    let n3 = p.ingest_folder(&corpus, false, &out).await.expect("ingest 3");
    assert_eq!(n3, 1, "third run ingests only the new file");

    // Change a file's content ⇒ it re-ingests (new doc_id), old rows
    // for that source_path are cleared (no orphan duplication).
    std::fs::write(
        corpus.join("doc_0.txt"),
        "Contenu modifié: clause de non-concurrence.",
    )
    .unwrap();
    let n4 = p.ingest_folder(&corpus, false, &out).await.expect("ingest 4");
    assert_eq!(n4, 1, "fourth run re-ingests only the changed file");

    // Searching the changed doc's new content returns it; the old
    // content must be gone (no orphan rows).
    let hits = p
        .search("clause de non-concurrence", 5)
        .await
        .expect("search");
    assert!(
        hits.iter().any(|h| h.source_path.ends_with("doc_0.txt")),
        "changed doc_0 is findable by its new content"
    );
    let stale = p.search("Contrat numéro 0", 5).await.expect("search2");
    assert!(
        !stale.iter().any(|h| h.source_path.ends_with("doc_0.txt")),
        "old content of doc_0 must not orphan (delete_doc_rows worked)"
    );
}

#[tokio::test]
#[ignore = "LanceDB + NER models; heavy"]
async fn concurrent_ingest_matches_sequential_count() {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("c");
    write_corpus(&corpus, 12);
    let out = tmp.path().join("o");

    // Sequential baseline.
    let seq_dir = tmp.path().join("seq");
    let pseq = Pipeline::new(cfg(&seq_dir, 1, 1), [0u8; 32]).await.unwrap();
    let nseq = pseq.ingest_folder(&corpus, false, &out).await.unwrap();

    // Concurrent.
    let par_dir = tmp.path().join("par");
    let ppar = Pipeline::new(cfg(&par_dir, 6, 3), [0u8; 32]).await.unwrap();
    let npar = ppar.ingest_folder(&corpus, false, &out).await.unwrap();

    assert_eq!(nseq, 12);
    assert_eq!(npar, nseq, "concurrent ingest must ingest the same set");
    // Same query must return the same number of distinct docs both ways.
    let hs = pseq.search("responsabilité contractuelle", 20).await.unwrap();
    let hp = ppar.search("responsabilité contractuelle", 20).await.unwrap();
    assert_eq!(
        hs.len(),
        hp.len(),
        "concurrent run lost/dup'd rows vs sequential"
    );
}

#[tokio::test]
#[ignore = "throughput smoke — records timing, run manually"]
async fn throughput_smoke_parallel_faster_than_sequential() {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("c");
    write_corpus(&corpus, 50);
    let out = tmp.path().join("o");

    let s_dir = tmp.path().join("s");
    let ps = Pipeline::new(cfg(&s_dir, 1, 1), [0u8; 32]).await.unwrap();
    let t0 = std::time::Instant::now();
    ps.ingest_folder(&corpus, false, &out).await.unwrap();
    let seq = t0.elapsed();

    let p_dir = tmp.path().join("p");
    let pp = Pipeline::new(cfg(&p_dir, 8, 4), [0u8; 32]).await.unwrap();
    let t1 = std::time::Instant::now();
    pp.ingest_folder(&corpus, false, &out).await.unwrap();
    let par = t1.elapsed();

    eprintln!("INGEST_50 sequential={seq:?} parallel(conc8,pool4)={par:?}");
    assert!(
        par < seq,
        "parallel ({par:?}) should beat sequential ({seq:?})"
    );
}
```

> This `throughput_smoke_*` test is the spec §5 regression-tracking
> mechanism (it records `INGEST_50 sequential=… parallel=…`); it
> supersedes a separate `bench_ingest.rs` criterion harness — no extra
> bench target is added (YAGNI: the smoke test already measures the
> seq-vs-parallel delta the spec asked to track).

- [ ] **Step 2: Compile the test target**

Run: `cargo test -p anno-rag --test ingest_scale --no-run`
Expected: SUCCESS (remove `chunk_count` if it triggers `dead_code` under `-D warnings` in the test profile).

- [ ] **Step 3: Run the heavy suite (single-threaded, ignored)**

Run: `CARGO_INCREMENTAL=0 cargo test -p anno-rag --test ingest_scale -- --ignored --test-threads=1`
Expected: `reingest_is_idempotent_and_resumable` PASS, `concurrent_ingest_matches_sequential_count` PASS. Run the throughput smoke explicitly and record the printed `INGEST_50 sequential=… parallel=…` line in the commit message. If `concurrent_ingest_matches_sequential_count` fails (row count differs), `Store::upsert` is not concurrency-safe under `buffer_unordered` — serialize upsert (wrap the `self.tbl.merge_insert` call in a `tokio::sync::Mutex` held on `Store`, or collect records and flush after the stream) and re-run; do not weaken the assertion.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/tests/ingest_scale.rs
git commit -m "test(ingest): idempotency/resume + concurrency-safety + throughput smoke"
```

---

## Final verification

- [ ] `cargo check -p anno-rag` and `--features rerank` — clean
- [ ] `cargo clippy -p anno-rag --all-targets -- -D warnings` (and `--features rerank`) — clean
- [ ] `cargo fmt --all -- --check` — clean (else `cargo fmt --all` + `style:` commit)
- [ ] `cargo test -p anno-rag --lib -- --exact` (fast unit: config, doc_uuid, pool) — green
- [ ] Heavy, one machine, single-process: `CARGO_INCREMENTAL=0 cargo test -p anno-rag --test ingest_scale -- --ignored --test-threads=1` — idempotency + concurrency-safety pass; throughput line recorded
- [ ] Spec §4 out-of-scope respected: no `anno`-crate changes, no OCR work, no FTS-optimize change, no batched-ONNX (A′) attempt
- [ ] `Store::upsert` concurrency decision documented in the Task 8 commit (safe as-is, or serialized — state which)
