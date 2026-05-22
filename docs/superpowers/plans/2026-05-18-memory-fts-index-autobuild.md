# Memory FTS-Index Auto-Build Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `Pipeline::recall_memory` (and the inheriting `recall_memory_reranked`) work through the public API by lazily ensuring the memories FTS index exists and folding in newly-saved rows, with no manual setup and no 24h staleness window.

**Architecture:** In `Pipeline::recall_memory`, before `store.memories_hybrid_search`, call the existing idempotent `Store::build_memories_fts_index()` (clears the "no INVERTED index" hard error) and a row-count-gated `Store::optimize_memories()` (folds rows appended since the last optimize into the existing index). A `Pipeline` `AtomicU64` watermark makes the optimize a no-op at steady state. No change to `save_memory`; no feature gate (memory is not feature-gated).

**Tech Stack:** Rust, LanceDB 0.27 (`memories_tbl.optimize(OptimizeAction::All)`), tokio, the existing `anno-rag` `Store`/`Pipeline`.

**Spec:** `docs/superpowers/specs/2026-05-18-memory-fts-index-autobuild-design.md` (local commit `65ef707a`).

**Grounding facts (verified in code — do not re-derive):**
- `Pipeline::recall_memory` (`crates/anno-rag/src/pipeline.rs:553`): signature `(&self, query: &str, top_k: usize, session_id: Option<String>, kinds: Option<Vec<crate::memory::MemoryKind>>, as_of: Option<chrono::DateTime<chrono::Utc>>, graph_expand: bool) -> Result<Vec<crate::memory::MemoryHit>>`. Body: `detect` → `pseudonymize_with_refs` → `embed_query` → **`let mut raw = self.store.memories_hybrid_search(&query_vec, &tokenized_query, top_k.saturating_mul(2)).await?;`** ← insertion point is immediately before this line.
- `Store::build_memories_fts_index(&self) -> Result<bool>` (`crates/anno-rag/src/store.rs:639`): idempotent; builds FTS on `memories_tbl` column `text` only if `count_rows > 0` and no `text` index exists; returns `Ok(true)` if it built, `Ok(false)` if skipped.
- `Store::optimize_memories(&self, _min_age: std::time::Duration) -> Result<()>` (`crates/anno-rag/src/store.rs:495`): `memories_tbl.optimize(OptimizeAction::All)` (folds new fragments into existing indices). `_min_age` is currently informational (lance 0.29) — pass `Duration::from_secs(self.cfg.compaction_min_age_secs)` for forward-compat, mirroring `Pipeline::do_compaction` (pipeline.rs:1021).
- `spawn_compaction_task` (pipeline.rs:1034) is **never called** from `anno-rag-bin` or `anno-rag-mcp` (verified by grep). So `optimize_memories` never runs anywhere except this new recall-path call → the gated optimize here is the *only* mechanism keeping memory FTS current. Mandatory, not optional.
- `Pipeline` struct (pipeline.rs:16-22): fields `detector: OnceCell<Arc<Detector>>`, `vault: Vault`, `embedder: OnceCell<Arc<Embedder>>`, `store: Store`, `cfg: AnnoRagConfig`, plus `#[cfg(feature = "rerank")] reranker: OnceCell<Arc<...>>`. Built in `Pipeline::new` (pipeline.rs:32-43) via `Ok(Self { detector: OnceCell::new(), vault, embedder: OnceCell::new(), store, cfg, #[cfg(feature="rerank")] reranker: OnceCell::new() })`. `Pipeline` is **not** `#[derive(Clone)]`.
- `recall_memory` is `&self`; `build_memories_fts_index`/`optimize_memories` are `&self`; `AtomicU64` mutates through `&self` with `Ordering::Relaxed`. No mutability/borrow problem.
- Cargo: run a single anno-rag test with `cargo test -p anno-rag --lib <path> -- --exact`; integration test `cargo test -p anno-rag --test rerank_integration <name> -- --ignored`. First cold build of the workspace is ~10 min; allow generous timeouts and wait. Concurrent cargo invocations on this Windows host can produce spurious rlib-format/incremental races — verify targets one at a time, single process.
- The `rerank` feature is unrelated to this fix but the regression test that exercises it (`reranked_memory_recall_returns_topk`) lives behind `#![cfg(feature = "rerank")]` in `crates/anno-rag/tests/rerank_integration.rs` — its commands need `--features rerank`.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/anno-rag/src/store.rs` | Modify | Add `Store::memory_row_count()` — public wrapper over `memories_tbl.count_rows(None)` for the watermark gate |
| `crates/anno-rag/src/pipeline.rs` | Modify | Add `memory_fts_watermark: std::sync::atomic::AtomicU64` field + init; add `ensure_memory_searchable()` helper; call it at the top of `recall_memory` |
| `crates/anno-rag/tests/memory_recall_index.rs` | Create | Plain-`recall_memory` regression test + staleness test (empirically resolves spec §4.1) |
| `crates/anno-rag/tests/rerank_integration.rs` | Modify | Un-`#[ignore]` `reranked_memory_recall_returns_topk`; drop the stale "blocked by FTS gap" note |
| `crates/anno-rag/benches/bench_memory_recall.rs` | Create | Measure recall-path `optimize` cost + steady-state no-op (spec §4.3); `[[bench]]` entry in `Cargo.toml` |

---

## Task 1: `Store::memory_row_count()` for the watermark gate

**Files:**
- Modify: `crates/anno-rag/src/store.rs` (add a public method on `impl Store`, near `build_memories_fts_index` at line 639)
- Test: `crates/anno-rag/src/store.rs` (`#[cfg(test)] mod tests` — add the module if absent, following crate convention)

- [ ] **Step 1: Write the failing test**

Add to (or create) `#[cfg(test)] mod tests` at the bottom of `crates/anno-rag/src/store.rs`:

```rust
#[tokio::test]
#[ignore = "opens LanceDB (~30s); run with --ignored"]
async fn memory_row_count_reflects_inserts() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = crate::config::AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let store = Store::open(&cfg).await.expect("open store");
    assert_eq!(store.memory_row_count().await.expect("count"), 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag --lib store::tests::memory_row_count_reflects_inserts --no-run`
Expected: FAIL — `no method named 'memory_row_count'`.

- [ ] **Step 3: Implement the method**

In `crates/anno-rag/src/store.rs`, on `impl Store`, immediately above `pub async fn build_memories_fts_index` (line 639):

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p anno-rag --lib store::tests::memory_row_count_reflects_inserts -- --ignored --exact`
Expected: PASS (`0` rows on a fresh store).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/store.rs
git commit -m "feat(memory): Store::memory_row_count for the recall optimize gate"
```

---

## Task 2: `Pipeline` watermark field + `ensure_memory_searchable` + wire into `recall_memory`

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs` (struct field at 16-22; `Pipeline::new` literal at 32-43; new private helper; call site at the top of `recall_memory` body, right before `let mut raw = self.store.memories_hybrid_search(...)`)

- [ ] **Step 1: Add the watermark field**

In `crates/anno-rag/src/pipeline.rs`, in `pub struct Pipeline { ... }`, after the `cfg: AnnoRagConfig,` field (keep any `#[cfg(feature="rerank")] reranker` field below it unchanged):

```rust
    /// Memories-table row count as of the last `optimize_memories` fold-in
    /// on the recall path. When the live count exceeds this, recall runs
    /// `optimize()` to index the new rows, then advances the watermark.
    /// `Relaxed` is sufficient: a missed/duplicated optimize is
    /// self-correcting on the next recall (idempotent), never incorrect.
    memory_fts_watermark: std::sync::atomic::AtomicU64,
```

- [ ] **Step 2: Initialize it in `Pipeline::new`**

In the `Ok(Self { ... })` literal inside `Pipeline::new`, after `cfg,` (and before any `#[cfg(feature="rerank")] reranker: OnceCell::new(),`):

```rust
            memory_fts_watermark: std::sync::atomic::AtomicU64::new(0),
```

- [ ] **Step 3: Add the `ensure_memory_searchable` helper**

In `impl Pipeline`, place this method immediately above `pub async fn recall_memory`:

```rust
    /// Guarantee the memories table is FTS-queryable before a hybrid
    /// recall:
    /// 1. Create the FTS index if absent (idempotent, cheap when built).
    /// 2. If memories were added since the last fold-in, `optimize()` so
    ///    the new rows are covered, then advance the watermark.
    ///
    /// This is the *only* path that keeps memory FTS current —
    /// `spawn_compaction_task` is not wired into any entrypoint.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if index build, count, or optimize fails.
    async fn ensure_memory_searchable(&self) -> Result<()> {
        use std::sync::atomic::Ordering;

        // (1) Idempotent: builds once when the table first has rows,
        // no-ops (count_rows + list_indices) thereafter.
        self.store.build_memories_fts_index().await?;

        // (2) Gate optimize on "rows added since last fold-in" so
        // steady-state recall (no new memories) pays only a count_rows.
        let live = self.store.memory_row_count().await?;
        let mark = self.memory_fts_watermark.load(Ordering::Relaxed);
        if live > mark {
            let min_age =
                std::time::Duration::from_secs(self.cfg.compaction_min_age_secs);
            self.store.optimize_memories(min_age).await?;
            self.memory_fts_watermark.store(live, Ordering::Relaxed);
        }
        Ok(())
    }
```

- [ ] **Step 4: Call it in `recall_memory`**

In `Pipeline::recall_memory`, insert the call immediately before the existing
`let mut raw = self.store.memories_hybrid_search(&query_vec, &tokenized_query, top_k.saturating_mul(2)).await?;`
line so the new content reads:

```rust
        let query_vec = self.embedder().await?.embed_query(&tokenized_query)?;

        self.ensure_memory_searchable().await?;

        let mut raw = self
            .store
            .memories_hybrid_search(&query_vec, &tokenized_query, top_k.saturating_mul(2))
            .await?;
```

(Do not change anything else in `recall_memory`. `recall_memory_reranked` calls `recall_memory` and inherits the fix with no edit.)

- [ ] **Step 5: Verify it compiles (default + rerank)**

Run: `cargo check -p anno-rag` then `cargo check -p anno-rag --features rerank`
Expected: both SUCCESS. (No behavior asserted yet — Task 3 proves behavior.)

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag/src/pipeline.rs
git commit -m "fix(memory): recall_memory lazily builds + folds in the FTS index"
```

---

## Task 3: Plain-`recall_memory` regression test + staleness test (resolves spec §4.1)

**Files:**
- Create: `crates/anno-rag/tests/memory_recall_index.rs`

- [ ] **Step 1: Write the tests**

Create `crates/anno-rag/tests/memory_recall_index.rs`:

```rust
//! Regression: `recall_memory` must work through the public API with no
//! manual index setup, including for memories saved *after* the FTS
//! index is first built (the staleness case). Heavy (LanceDB + model);
//! ignored by default.

use anno_rag::{AnnoRagConfig, Pipeline};

fn cfg(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "opens LanceDB + loads embedder"]
async fn recall_memory_works_without_manual_index_setup() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");

    for body in [
        "La prescription quinquennale court à compter de la connaissance du dommage.",
        "Le café de la machine est trop amer ce matin.",
        "La prescription de l'action en responsabilité est de cinq ans.",
    ] {
        p.save_memory(body, None, None).await.expect("save");
    }

    let hits = p
        .recall_memory(
            "délai de prescription en responsabilité",
            3,
            None,
            None,
            None,
            false,
        )
        .await
        .expect("recall_memory must not error on a fresh store");
    assert!(!hits.is_empty(), "expected at least one recalled memory");
    assert!(
        hits.iter().any(|h| h.text.contains("prescription")),
        "expected a prescription memory in hits, got: {:?}",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
}

#[tokio::test]
#[ignore = "opens LanceDB + loads embedder"]
async fn recall_finds_memories_saved_after_first_index_build() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");

    // First save + recall: forces the initial FTS index build.
    p.save_memory(
        "La résiliation du bail commercial obéit à un préavis de six mois.",
        None,
        None,
    )
    .await
    .expect("save 1");
    let _ = p
        .recall_memory("bail commercial", 5, None, None, None, false)
        .await
        .expect("recall 1 (builds index)");

    // Save MORE memories AFTER the index already exists, then recall.
    // This is the staleness case: pre-fix these were never indexed.
    for body in [
        "Le congé doit être délivré par acte extrajudiciaire.",
        "Le preneur dispose d'un droit au renouvellement du bail.",
    ] {
        p.save_memory(body, None, None).await.expect("save more");
    }
    let hits = p
        .recall_memory(
            "droit au renouvellement du bail",
            5,
            None,
            None,
            None,
            false,
        )
        .await
        .expect("recall 2");
    assert!(
        hits.iter().any(|h| h.text.contains("renouvellement")),
        "memory saved AFTER first index build must be recallable; got: {:?}",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
}
```

- [ ] **Step 2: Compile the test target**

Run: `cargo test -p anno-rag --test memory_recall_index --no-run`
Expected: SUCCESS. (Confirm `anno_rag::{AnnoRagConfig, Pipeline}` re-exports — they are used identically by `crates/anno-rag/tests/rerank_integration.rs`. `tempfile` is an existing dev-dep.)

- [ ] **Step 3: Run both tests**

Run: `cargo test -p anno-rag --test memory_recall_index -- --ignored --test-threads=1`
Expected: PASS for both `recall_memory_works_without_manual_index_setup` and `recall_finds_memories_saved_after_first_index_build`.

If `recall_finds_memories_saved_after_first_index_build` FAILS, spec §4.1 resolved to "LanceDB FTS does not cover un-optimized rows AND the watermark gate isn't folding them in" — debug: confirm `ensure_memory_searchable` runs `optimize_memories` on the second recall (the watermark `live > mark` branch). Do **not** weaken the assertion; the staleness guarantee is the point of this work. If it passes, §4.1 is resolved by construction (the gated optimize covers the new rows).

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/tests/memory_recall_index.rs
git commit -m "test(memory): recall_memory regression + post-index-build staleness"
```

---

## Task 4: Un-ignore the rerank memory-recall regression test

**Files:**
- Modify: `crates/anno-rag/tests/rerank_integration.rs` (the `reranked_memory_recall_returns_topk` test + its preceding NOTE comment)

- [ ] **Step 1: Remove the gap note and re-enable the test**

In `crates/anno-rag/tests/rerank_integration.rs`, delete the multi-line `// NOTE: blocked by a pre-existing recall_memory gap ...` comment block above `async fn reranked_memory_recall_returns_topk`, and change its attribute line from:

```rust
#[ignore = "blocked by pre-existing recall_memory FTS-index gap (see note)"]
```

to:

```rust
#[ignore = "downloads model + opens LanceDB"]
```

(Keep the test body unchanged. It stays `#[ignore]` only because it is heavy — consistent with the other rerank integration tests — not because it is broken.)

- [ ] **Step 2: Compile**

Run: `cargo test -p anno-rag --features rerank --test rerank_integration --no-run`
Expected: SUCCESS.

- [ ] **Step 3: Run it (now expected to pass with the fix)**

Run: `cargo test -p anno-rag --features rerank --test rerank_integration reranked_memory_recall_returns_topk -- --ignored`
Expected: PASS — `recall_memory_reranked` now works because `recall_memory` self-builds the index.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/tests/rerank_integration.rs
git commit -m "test(rerank): un-ignore memory-recall test — FTS gap fixed"
```

---

## Task 5: Recall-path optimize cost benchmark (spec §4.3)

**Files:**
- Create: `crates/anno-rag/benches/bench_memory_recall.rs`
- Modify: `crates/anno-rag/Cargo.toml` (add `[[bench]]` entry)

- [ ] **Step 1: Add the bench target**

In `crates/anno-rag/Cargo.toml`, alongside the existing `[[bench]]` entries:

```toml
[[bench]]
name = "bench_memory_recall"
harness = false
```

- [ ] **Step 2: Write the benchmark**

Create `crates/anno-rag/benches/bench_memory_recall.rs`:

```rust
//! Recall-path cost: (a) cold recall (first call builds FTS index +
//! optimizes), (b) steady-state recall (no new memories — the watermark
//! gate must make this pay only a count_rows, no optimize). Establishes
//! the spec §4.3 number. Run:
//! `cargo bench -p anno-rag --bench bench_memory_recall`.
#![allow(clippy::unwrap_used, missing_docs)]

use anno_rag::{AnnoRagConfig, Pipeline};
use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

fn bench_memory_recall(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let p = rt.block_on(async {
        let p = Pipeline::new(
            AnnoRagConfig {
                data_dir: tmp.path().to_path_buf(),
                ..Default::default()
            },
            [0u8; 32],
        )
        .await
        .unwrap();
        for i in 0..50 {
            p.save_memory(&format!("Mémoire de test numéro {i} sur la responsabilité."), None, None)
                .await
                .unwrap();
        }
        p
    });

    // Cold: first recall builds the index + optimizes.
    c.bench_function("memory_recall_cold_then_steady", |b| {
        b.to_async(&rt).iter(|| async {
            let hits = p
                .recall_memory("responsabilité", 5, None, None, None, false)
                .await
                .unwrap();
            criterion::black_box(hits);
        });
    });
}

criterion_group!(benches, bench_memory_recall);
criterion_main!(benches);
```

- [ ] **Step 3: Run the benchmark**

Run: `cargo bench -p anno-rag --bench bench_memory_recall -- --warm-up-time 1 --measurement-time 5`
Expected: completes; prints `memory_recall_cold_then_steady` timing. After the first iteration (cold: index build + optimize) every subsequent iteration adds no new memories, so the watermark gate skips `optimize` — steady-state time should be dominated by hybrid search, not `optimize`. Record the number in the commit message. If steady-state is implausibly slow (optimize not being gated), the watermark logic in Task 2 is wrong — revisit `ensure_memory_searchable`.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/benches/bench_memory_recall.rs crates/anno-rag/Cargo.toml
git commit -m "bench(memory): recall-path index/optimize cost + steady-state gate"
```

---

## Final verification

- [ ] `cargo check -p anno-rag` and `cargo check -p anno-rag --features rerank` — clean
- [ ] `cargo clippy -p anno-rag --all-targets -- -D warnings` and `... --features rerank ...` — clean
- [ ] `cargo fmt --all -- --check` — clean (run `cargo fmt --all` + commit `style:` if not)
- [ ] `cargo test -p anno-rag --test memory_recall_index -- --ignored --test-threads=1` — both pass
- [ ] `cargo test -p anno-rag --features rerank --test rerank_integration reranked_memory_recall_returns_topk -- --ignored` — passes
- [ ] Spec §6 out-of-scope (scalar/vector memory indexes) NOT touched — confirm no `setup_memory_indexes` call was added
