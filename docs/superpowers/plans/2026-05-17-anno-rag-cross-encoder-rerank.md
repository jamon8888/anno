# Cross-Encoder Rerank Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in, feature-gated cross-encoder reranker (BGE-reranker-v2-m3, pre-quantized INT8 ONNX, 571 MB) that re-scores the top-N RRF candidates by semantic relevance, for both the chunk-search and memory-recall paths.

**Architecture:** A standalone `rerank.rs` module owns an `ort` ONNX session + tokenizer and exposes `score_pairs(query, &[passage]) -> Vec<f32>` with zero dependency on `store`/`pipeline`/`vault`. `Pipeline` gains a lazy `OnceCell<Arc<Reranker>>` and two additive methods (`search_reranked`, `recall_memory_reranked`) that over-fetch via the existing RRF search, rehydrate hits through the vault (the cross-encoder must see real entities, not `<PERSON_42>`), score, re-sort, truncate. Everything is behind a default-off `rerank` Cargo feature.

**Tech Stack:** Rust, `ort` 2.0.0-rc.12 (ONNX Runtime, already a workspace dep via `gliner2_fastino`), `ndarray`, `tokenizers`, `hf-hub`, `tokio::sync::OnceCell`, `proptest`, `criterion`.

**Spec:** `docs/superpowers/specs/2026-05-16-anno-rag-cross-encoder-rerank-design.md` (amended 2026-05-17).

**Prerequisites:**
- This plan modifies `crates/anno-rag` **and** `crates/anno-rag-mcp`. The `anno-rag-mcp` crate exists only after PR #9 (`feat/anno-rag-mcp-split`) merges to `main`. Before starting Task 13 (MCP param), confirm `crates/anno-rag-mcp/src/lib.rs` exists on the working branch; if not, rebase onto `main` after PR #9 merges. Tasks 1–12 + 14–16 touch only `crates/anno-rag` and are unblocked.
- Work happens on a branch off the latest `main` (post PR #8/#9). The spec lives on `claude/strange-nash-978baf`; cherry-pick or rebase the spec commit onto the implementation branch so the plan + spec travel together.

**Conventions in this codebase (read before starting):**
- Errors: every fallible fn returns `crate::error::Result<T>`; map foreign errors with `.map_err(|e| Error::Variant(format!("ctx: {e}")))`. See `crates/anno-rag/src/error.rs`.
- Lazy model load: `OnceCell::get_or_try_init` returning `Arc<T>`, mirrors `Pipeline::embedder()` at `crates/anno-rag/src/pipeline.rs:46`.
- ort session build: `Session::builder()?.with_optimization_level(GraphOptimizationLevel::Level3)?.with_intra_threads(n)?.commit_from_file(path)?` — pattern from `crates/anno/src/backends/hf_loader.rs:239`.
- ort run + extract: `session.run(ort::inputs!["input_ids" => t.into_dyn(), "attention_mask" => t.into_dyn()])?` then `out.get(name).try_extract_tensor::<f32>()` → `(shape, cow)`. Pattern from `crates/anno/src/backends/gliner2_fastino/pipeline.rs:54`.
- ort Tensor from ndarray: `ort::value::Tensor::from_array((shape_vec, data_vec))` — inlined here (no dependency on the `anno` crate's `ort_compat` helper, per spec §7 boundary).
- Tests live in `#[cfg(test)] mod tests` at the bottom of the source file; integration tests in `crates/anno-rag/tests/`.
- Commit message style: `feat(rerank): ...` / `test(rerank): ...` / `chore(rerank): ...`.
- Run a single test: `cargo test -p anno-rag --features rerank --lib rerank::tests::NAME -- --exact`.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/anno-rag/Cargo.toml` | Modify | Optional `ort`/`ndarray` deps; `rerank` feature; `is-terminal` dev/dep |
| `crates/anno-rag/src/error.rs` | Modify | Add `Error::Rerank(String)` variant |
| `crates/anno-rag/src/lib.rs` | Modify | `#[cfg(feature="rerank")] pub mod rerank;` |
| `crates/anno-rag/src/config.rs` | Modify | 4 config fields + serde defaults + `Default` impl |
| `crates/anno-rag/src/rerank.rs` | Create | `Reranker` (ort session + tokenizer); `Reranker::load`, `score_pairs` |
| `crates/anno-rag/src/pipeline.rs` | Modify | `reranker` `OnceCell` field; `reranker()`, `search_reranked`, `recall_memory_reranked` |
| `crates/anno-rag/tests/rerank_smoke.rs` | Create | Ignored-by-default ONNX I/O introspection (spec §11 commit 1) |
| `crates/anno-rag/tests/rerank_integration.rs` | Create | Relevance + privacy-boundary integration tests |
| `crates/anno-rag/benches/bench_rerank.rs` | Create | Pool-of-30 latency/RSS benchmark |
| `crates/anno-rag/tests/fixtures/eval_baseline_reranked.toml` | Create | Reranked-path eval gate baseline |
| `crates/anno-rag-mcp/src/lib.rs` | Modify | `rerank: bool` param on `search` + `memory_recall`; `score_source` audit field |

---

## Task 1: Add `Error::Rerank` variant

**Files:**
- Modify: `crates/anno-rag/src/error.rs:53` (after the `Memory` variant)
- Test: `crates/anno-rag/src/error.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/anno-rag/src/error.rs`:

```rust
#[test]
fn rerank_display_includes_context() {
    let e = Error::Rerank("onnx session build".into());
    assert_eq!(format!("{e}"), "rerank: onnx session build");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag --lib error::tests::rerank_display_includes_context -- --exact`
Expected: FAIL — `no variant named 'Rerank'`.

- [ ] **Step 3: Add the variant**

In `crates/anno-rag/src/error.rs`, after the `Memory` variant (line 53):

```rust
    /// Cross-encoder reranker load or inference failed. Recoverable:
    /// callers fall back to the non-reranked ordering.
    #[error("rerank: {0}")]
    Rerank(String),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p anno-rag --lib error::tests -- --exact`
Expected: PASS (both `error_is_send_sync` and the new test).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/error.rs
git commit -m "feat(rerank): add Error::Rerank variant"
```

---

## Task 2: Cargo feature + optional deps

**Files:**
- Modify: `crates/anno-rag/Cargo.toml:13-14` (features), dependencies section, dev-dependencies

- [ ] **Step 1: Add optional deps and the feature**

In `crates/anno-rag/Cargo.toml`, in `[dependencies]` (alongside the existing `candle-*` lines ~33-38):

```toml
ort      = { workspace = true, features = ["ndarray"], optional = true }
ndarray  = { workspace = true, optional = true }
```

Replace the `[features]` block (lines 13-14):

```toml
[features]
default = []
# Opt-in cross-encoder reranker (BGE-reranker-v2-m3 INT8 ONNX, ~571 MB
# download on first use). Pulls ort + ndarray. Default off so plain
# `cargo install anno-rag` users don't pay the model download.
rerank = ["dep:ort", "dep:ndarray"]
```

In `[dev-dependencies]` (near `proptest = "1"` at line 67):

```toml
is-terminal = "0.4"
```

- [ ] **Step 2: Verify the workspace pins `ndarray`**

Run: `grep -n '^ndarray' Cargo.toml`
Expected: a workspace `ndarray = ...` line. If absent, add `ndarray = "0.16"` under `[workspace.dependencies]` in the root `Cargo.toml` (match the version `ort`'s `ndarray` feature expects — check `cargo tree -p ort -i ndarray` after).

- [ ] **Step 3: Verify default build is unchanged**

Run: `cargo check -p anno-rag`
Expected: SUCCESS, no `ort`/`ndarray` compiled (they are optional and `rerank` is off).

- [ ] **Step 4: Verify the feature compiles**

Run: `cargo check -p anno-rag --features rerank`
Expected: SUCCESS (ort + ndarray compile; nothing uses them yet — that's fine).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/Cargo.toml Cargo.toml
git commit -m "chore(rerank): optional ort/ndarray deps + rerank feature"
```

---

## Task 3: Config fields for the reranker

**Files:**
- Modify: `crates/anno-rag/src/config.rs` (struct fields ~line 15, default fns ~line 108, `Default` impl line 150)
- Test: `crates/anno-rag/src/config.rs` (`#[cfg(test)] mod tests` — add if absent)

- [ ] **Step 1: Write the failing test**

Add to (or create) `#[cfg(test)] mod tests` at the bottom of `crates/anno-rag/src/config.rs`:

```rust
#[test]
fn rerank_defaults_are_sane() {
    let c = AnnoRagConfig::default();
    assert_eq!(c.rerank_model, "onnx-community/bge-reranker-v2-m3-ONNX");
    assert_eq!(c.rerank_onnx_file, "onnx/model_int8.onnx");
    assert_eq!(c.rerank_pool_size, 30);
    assert_eq!(c.rerank_batch_size, 8);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag --lib config::tests::rerank_defaults_are_sane -- --exact`
Expected: FAIL — `no field 'rerank_model'`.

- [ ] **Step 3: Add the fields, default fns, and Default values**

In `crates/anno-rag/src/config.rs`, add to the `AnnoRagConfig` struct (after `embedder_dtype`, ~line 54):

```rust
    /// Reranker repo id on HuggingFace Hub (cross-encoder, opt-in).
    #[serde(default = "default_rerank_model")]
    pub rerank_model: String,

    /// ONNX file within `rerank_model`. INT8 by default; point at
    /// "onnx/model_q4f16.onnx" (702 MB) if INT8 regresses on your corpus.
    #[serde(default = "default_rerank_onnx_file")]
    pub rerank_onnx_file: String,

    /// RRF candidates to over-fetch before reranking. Default 30.
    #[serde(default = "default_rerank_pool_size")]
    pub rerank_pool_size: usize,

    /// Max (query,passage) pairs per ONNX forward batch. Default 8.
    #[serde(default = "default_rerank_batch_size")]
    pub rerank_batch_size: usize,
```

Add the default fns near the other `default_*` fns (~line 136):

```rust
fn default_rerank_model() -> String {
    "onnx-community/bge-reranker-v2-m3-ONNX".to_string()
}
fn default_rerank_onnx_file() -> String {
    "onnx/model_int8.onnx".to_string()
}
fn default_rerank_pool_size() -> usize {
    30
}
fn default_rerank_batch_size() -> usize {
    8
}
```

Add to the `Default for AnnoRagConfig` impl (inside the `Self { ... }` literal, after `graph_per_hop_limit`, ~line 170):

```rust
            rerank_model: default_rerank_model(),
            rerank_onnx_file: default_rerank_onnx_file(),
            rerank_pool_size: default_rerank_pool_size(),
            rerank_batch_size: default_rerank_batch_size(),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p anno-rag --lib config::tests::rerank_defaults_are_sane -- --exact`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/config.rs
git commit -m "feat(rerank): AnnoRagConfig rerank_* fields with defaults"
```

---

## Task 4: Smoke-load test — confirm the ONNX I/O contract (spec §11 commit 1)

**Files:**
- Create: `crates/anno-rag/tests/rerank_smoke.rs`
- Modify: `crates/anno-rag/src/lib.rs` (add the gated module declaration so the test target can reference nothing yet — module added in Task 5; this task only adds the test harness)

> This test is `#[ignore]` by default (network + 571 MB download). It runs in CI behind an explicit `--ignored` invocation and locally before wiring the rest. It introspects the real ONNX graph so later tasks hard-code nothing.

- [ ] **Step 1: Write the smoke test**

Create `crates/anno-rag/tests/rerank_smoke.rs`:

```rust
//! Smoke-load: fetch the INT8 ONNX, build the ort session, assert the
//! input/output contract, run one forward pass. Ignored by default —
//! downloads ~571 MB. Run: `cargo test -p anno-rag --features rerank
//! --test rerank_smoke -- --ignored --nocapture`.
#![cfg(feature = "rerank")]

#[tokio::test]
#[ignore = "downloads ~571 MB BGE-reranker-v2-m3 INT8 ONNX"]
async fn onnx_io_contract_holds() {
    use hf_hub::api::tokio::Api;
    use ort::session::{builder::GraphOptimizationLevel, Session};

    let api = Api::new().expect("hf api");
    let repo = api.model("onnx-community/bge-reranker-v2-m3-ONNX".to_string());
    let onnx = repo
        .get("onnx/model_int8.onnx")
        .await
        .expect("fetch model_int8.onnx");
    let _tok = repo.get("tokenizer.json").await.expect("fetch tokenizer");

    let session = Session::builder()
        .expect("builder")
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .expect("opt level")
        .commit_from_file(&onnx)
        .expect("commit onnx");

    let in_names: Vec<&str> = session.inputs.iter().map(|i| i.name.as_str()).collect();
    let out_names: Vec<&str> = session.outputs.iter().map(|o| o.name.as_str()).collect();
    eprintln!("ONNX inputs={in_names:?} outputs={out_names:?}");

    assert!(
        in_names.contains(&"input_ids"),
        "expected an 'input_ids' input, got {in_names:?}"
    );
    assert!(
        in_names.contains(&"attention_mask"),
        "expected an 'attention_mask' input, got {in_names:?}"
    );
    assert_eq!(
        out_names.len(),
        1,
        "expected a single logits output, got {out_names:?}"
    );

    // One forward pass on a trivial 1×4 batch of zeros — shape sanity only.
    let ids = ndarray::Array2::<i64>::zeros((1, 4));
    let mask = ndarray::Array2::<i64>::from_elem((1, 4), 1i64);
    let ids_t = ort::value::Tensor::from_array((
        vec![1_i64, 4],
        ids.into_raw_vec_and_offset().0,
    ))
    .expect("ids tensor");
    let mask_t = ort::value::Tensor::from_array((
        vec![1_i64, 4],
        mask.into_raw_vec_and_offset().0,
    ))
    .expect("mask tensor");

    let mut session = session;
    let out = session
        .run(ort::inputs![
            "input_ids" => ids_t.into_dyn(),
            "attention_mask" => mask_t.into_dyn(),
        ])
        .expect("forward run");
    let (shape, _data) = out
        .iter()
        .next()
        .expect("one output")
        .1
        .try_extract_tensor::<f32>()
        .expect("extract f32 logits");
    eprintln!("logits shape={shape:?}");
    assert_eq!(shape[0], 1, "batch dim must be 1");
}
```

- [ ] **Step 2: Compile the test target**

Run: `cargo test -p anno-rag --features rerank --test rerank_smoke --no-run`
Expected: SUCCESS (compiles; not executed).

- [ ] **Step 3: Run the smoke test explicitly (network)**

Run: `cargo test -p anno-rag --features rerank --test rerank_smoke -- --ignored --nocapture`
Expected: PASS. Note the printed `ONNX inputs=[...] outputs=[...]` and `logits shape=[1, 1]`. **If input names differ** (e.g. `token_type_ids` also required), record the actual names — Task 5 reads them from the session rather than hard-coding.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/tests/rerank_smoke.rs
git commit -m "test(rerank): ONNX I/O contract smoke-load (ignored by default)"
```

---

## Task 5: `Reranker::load` — fetch + build the ort session

**Files:**
- Create: `crates/anno-rag/src/rerank.rs`
- Modify: `crates/anno-rag/src/lib.rs` (add gated module decl)
- Test: `crates/anno-rag/src/rerank.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Declare the module**

In `crates/anno-rag/src/lib.rs`, alongside the other `pub mod` lines:

```rust
#[cfg(feature = "rerank")]
pub mod rerank;
```

- [ ] **Step 2: Write the failing test**

Create `crates/anno-rag/src/rerank.rs` with the struct skeleton + a load test:

```rust
//! Cross-encoder reranker: BGE-reranker-v2-m3, pre-quantized INT8 ONNX,
//! run via `ort`. Scores (query, passage) pairs; higher = more relevant.
//!
//! Owns the ONNX session + tokenizer. Loaded once per process via
//! `Pipeline::reranker()` (lazy `OnceCell`). Depends only on ort,
//! ndarray, tokenizers, hf-hub — NOT on store/pipeline/vault (spec §7).

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use std::sync::Mutex;
use tokenizers::Tokenizer;

/// Loaded cross-encoder reranker.
pub struct Reranker {
    /// `ort::session::Session::run` takes `&mut self`; the session is
    /// behind a `Mutex` so `score_pairs` can take `&self` (mirrors
    /// gliner2_fastino's `SessionSlot`).
    session: Mutex<ort::session::Session>,
    tokenizer: Tokenizer,
    /// Hard cap on the combined (query+passage) token length. 512 for
    /// BGE-reranker-v2-m3.
    max_seq_len: usize,
}

impl Reranker {
    /// Fetch the INT8 ONNX + tokenizer from the Hub (cached under the
    /// hf-hub cache, same as `Embedder::load`) and build the ort session.
    ///
    /// # Errors
    /// [`Error::Rerank`] on hub fetch, tokenizer parse, or session build.
    pub async fn load(cfg: &AnnoRagConfig) -> Result<Self> {
        use hf_hub::api::tokio::Api;
        use ort::session::{builder::GraphOptimizationLevel, Session};

        let api = Api::new().map_err(|e| Error::Rerank(format!("hf-hub init: {e}")))?;
        let repo = api.model(cfg.rerank_model.clone());

        let onnx_path = repo
            .get(&cfg.rerank_onnx_file)
            .await
            .map_err(|e| Error::Rerank(format!("onnx fetch {}: {e}", cfg.rerank_onnx_file)))?;
        let tok_path = repo
            .get("tokenizer.json")
            .await
            .map_err(|e| Error::Rerank(format!("tokenizer.json fetch: {e}")))?;

        let tokenizer = Tokenizer::from_file(&tok_path)
            .map_err(|e| Error::Rerank(format!("tokenizer load: {e}")))?;

        let session = Session::builder()
            .map_err(|e| Error::Rerank(format!("session builder: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| Error::Rerank(format!("opt level: {e}")))?
            .commit_from_file(&onnx_path)
            .map_err(|e| Error::Rerank(format!("commit onnx: {e}")))?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            max_seq_len: 512,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "downloads ~571 MB"]
    async fn load_succeeds() {
        let cfg = AnnoRagConfig::default();
        let r = Reranker::load(&cfg).await.expect("reranker loads");
        assert_eq!(r.max_seq_len, 512);
    }
}
```

- [ ] **Step 3: Run test to verify it compiles and is gated**

Run: `cargo test -p anno-rag --features rerank --lib rerank::tests::load_succeeds --no-run`
Expected: SUCCESS (compiles). The test itself is `#[ignore]`.

- [ ] **Step 4: Run the load test explicitly**

Run: `cargo test -p anno-rag --features rerank --lib rerank::tests::load_succeeds -- --ignored`
Expected: PASS (uses the cache from Task 4's download).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/lib.rs crates/anno-rag/src/rerank.rs
git commit -m "feat(rerank): Reranker::load — hub fetch + ort session"
```

---

## Task 6: `score_pairs` — tokenize → ndarray → run → sigmoid

**Files:**
- Modify: `crates/anno-rag/src/rerank.rs` (add `score_pairs` + a private `score_batch`)
- Test: `crates/anno-rag/src/rerank.rs` (`mod tests`)

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/anno-rag/src/rerank.rs`:

```rust
    #[tokio::test]
    #[ignore = "uses cached ~571 MB model"]
    async fn relevant_outranks_irrelevant() {
        let r = Reranker::load(&AnnoRagConfig::default())
            .await
            .expect("load");
        let scores = r
            .score_pairs(
                "responsabilité contractuelle du débiteur",
                &[
                    "Le débiteur engage sa responsabilité contractuelle en cas d'inexécution.",
                    "La recette des crêpes nécessite de la farine et des œufs.",
                ],
            )
            .expect("score");
        assert_eq!(scores.len(), 2);
        assert!(
            scores[0] > scores[1],
            "legal passage ({}) must outrank pancake recipe ({})",
            scores[0],
            scores[1]
        );
    }

    #[tokio::test]
    #[ignore = "uses cached ~571 MB model"]
    async fn empty_passages_is_empty_no_panic() {
        let r = Reranker::load(&AnnoRagConfig::default())
            .await
            .expect("load");
        assert!(r.score_pairs("q", &[]).expect("score").is_empty());
    }

    #[tokio::test]
    #[ignore = "uses cached ~571 MB model"]
    async fn batching_matches_single_and_is_deterministic() {
        let r = Reranker::load(&AnnoRagConfig::default())
            .await
            .expect("load");
        let passages: Vec<String> = (0..17).map(|i| format!("clause numéro {i}")).collect();
        let refs: Vec<&str> = passages.iter().map(String::as_str).collect();
        let a = r.score_pairs("clause", &refs).expect("a");
        let b = r.score_pairs("clause", &refs).expect("b");
        assert_eq!(a.len(), 17);
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < f32::EPSILON, "determinism: {x} vs {y}");
        }
    }

    #[tokio::test]
    #[ignore = "uses cached ~571 MB model"]
    async fn overlong_passage_truncates_no_panic() {
        let r = Reranker::load(&AnnoRagConfig::default())
            .await
            .expect("load");
        let long = "lorem ipsum ".repeat(5000);
        let s = r.score_pairs("q", &[long.as_str()]).expect("score");
        assert_eq!(s.len(), 1);
        assert!(s[0].is_finite());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p anno-rag --features rerank --lib rerank::tests::empty_passages_is_empty_no_panic --no-run`
Expected: FAIL — `no method named 'score_pairs'`.

- [ ] **Step 3: Implement `score_pairs` + `score_batch`**

Add to `impl Reranker` in `crates/anno-rag/src/rerank.rs`:

```rust
    /// Score each `(query, passage)` pair. Returns relevance scores in
    /// [0, 1] (sigmoid of the classifier logit), in input order. Higher
    /// = more relevant. Batched internally up to `cfg.rerank_batch_size`
    /// (caller passes the batch size via `score_pairs_batched`; this
    /// convenience wrapper uses a fixed 8).
    ///
    /// `query` is shared across pairs. Passages are right-truncated so
    /// `query + passage + specials <= max_seq_len`.
    ///
    /// # Errors
    /// [`Error::Rerank`] on tokenization, tensor build, or ONNX run.
    pub fn score_pairs(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>> {
        self.score_pairs_batched(query, passages, 8)
    }

    /// Same as [`Reranker::score_pairs`] with an explicit batch size
    /// (wired to `cfg.rerank_batch_size` by the pipeline).
    ///
    /// # Errors
    /// [`Error::Rerank`] on tokenization, tensor build, or ONNX run.
    pub fn score_pairs_batched(
        &self,
        query: &str,
        passages: &[&str],
        batch_size: usize,
    ) -> Result<Vec<f32>> {
        if passages.is_empty() {
            return Ok(Vec::new());
        }
        let bs = batch_size.max(1);
        let mut out = Vec::with_capacity(passages.len());
        for chunk in passages.chunks(bs) {
            out.extend(self.score_batch(query, chunk)?);
        }
        Ok(out)
    }

    /// One forward pass over a batch of pairs. `<= max_seq_len` enforced
    /// by `encode_pair` truncation config plus a hard right-truncation.
    fn score_batch(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>> {
        // 1. Tokenize each (query, passage) pair → padded i64 matrices.
        let mut encs = Vec::with_capacity(passages.len());
        for p in passages {
            let enc = self
                .tokenizer
                .encode((query, *p), true)
                .map_err(|e| Error::Rerank(format!("encode_pair: {e}")))?;
            encs.push(enc);
        }
        let max_len = encs
            .iter()
            .map(|e| e.get_ids().len().min(self.max_seq_len))
            .max()
            .unwrap_or(0);
        let n = passages.len();

        let mut ids: Vec<i64> = Vec::with_capacity(n * max_len);
        let mut mask: Vec<i64> = Vec::with_capacity(n * max_len);
        for e in &encs {
            let take = e.get_ids().len().min(max_len);
            let pad = max_len - take;
            ids.extend(e.get_ids()[..take].iter().map(|&x| i64::from(x)));
            ids.extend(std::iter::repeat_n(0i64, pad));
            mask.extend(e.get_attention_mask()[..take].iter().map(|&x| i64::from(x)));
            mask.extend(std::iter::repeat_n(0i64, pad));
        }

        // 2. Build ort tensors (shape, data) — inlined, no anno dep.
        let shape = vec![n as i64, max_len as i64];
        let ids_t = ort::value::Tensor::from_array((shape.clone(), ids))
            .map_err(|e| Error::Rerank(format!("ids tensor: {e}")))?;
        let mask_t = ort::value::Tensor::from_array((shape, mask))
            .map_err(|e| Error::Rerank(format!("mask tensor: {e}")))?;

        // 3. Run. Session::run is &mut self → lock the Mutex.
        let mut guard = self
            .session
            .lock()
            .map_err(|e| Error::Rerank(format!("session lock poisoned: {e}")))?;
        let outputs = guard
            .run(ort::inputs![
                "input_ids" => ids_t.into_dyn(),
                "attention_mask" => mask_t.into_dyn(),
            ])
            .map_err(|e| Error::Rerank(format!("onnx run: {e}")))?;

        // 4. Extract logits [n, 1] → sigmoid → Vec<f32> length n.
        let (oshape, cow) = outputs
            .iter()
            .next()
            .ok_or_else(|| Error::Rerank("onnx: no outputs".into()))?
            .1
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Rerank(format!("extract logits: {e}")))?;
        let logits = cow.as_ref();
        if logits.len() < n {
            return Err(Error::Rerank(format!(
                "expected >= {n} logits, got {} (shape {oshape:?})",
                logits.len()
            )));
        }
        Ok(logits[..n]
            .iter()
            .map(|&z| 1.0_f32 / (1.0 + (-z).exp()))
            .collect())
    }
```

> Note on `encode((query, passage), true)`: the `tokenizers` crate's
> `Tokenizer::encode` accepts `(&str, &str)` for pair encoding and emits
> the XLM-RoBERTa special-token layout. Truncation to `max_seq_len` is
> enforced defensively by the `take` slice above even if the tokenizer's
> own truncation config is absent.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p anno-rag --features rerank --lib rerank::tests -- --ignored --test-threads=1`
Expected: PASS — all five `rerank::tests` (`load_succeeds`, `relevant_outranks_irrelevant`, `empty_passages_is_empty_no_panic`, `batching_matches_single_and_is_deterministic`, `overlong_passage_truncates_no_panic`).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/rerank.rs
git commit -m "feat(rerank): score_pairs — tokenize, ort run, sigmoid"
```

---

## Task 7: Property test — score_pairs is total and finite

**Files:**
- Modify: `crates/anno-rag/src/rerank.rs` (`mod tests`, add a proptest)

- [ ] **Step 1: Write the failing property test**

Add to `mod tests` in `crates/anno-rag/src/rerank.rs`:

```rust
    // Heavy: loads the model once, reuses it across cases via a process
    // OnceLock so proptest shrinking doesn't reload 571 MB per case.
    #[test]
    #[ignore = "uses cached ~571 MB model"]
    fn prop_score_pairs_total_and_finite() {
        use proptest::prelude::*;
        use std::sync::OnceLock;
        static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        static R: OnceLock<Reranker> = OnceLock::new();
        let rt = RT.get_or_init(|| tokio::runtime::Runtime::new().unwrap());
        let reranker = R.get_or_init(|| {
            rt.block_on(Reranker::load(&AnnoRagConfig::default()))
                .expect("load")
        });

        proptest!(|(q in ".{0,40}", ps in proptest::collection::vec(".{0,80}", 0..20))| {
            let refs: Vec<&str> = ps.iter().map(String::as_str).collect();
            let scores = reranker.score_pairs(&q, &refs).expect("score");
            prop_assert_eq!(scores.len(), ps.len());
            for s in scores {
                prop_assert!(s.is_finite(), "score must be finite, got {}", s);
                prop_assert!((0.0..=1.0).contains(&s), "score in [0,1], got {}", s);
            }
        });
    }
```

- [ ] **Step 2: Run to verify it compiles then passes**

Run: `cargo test -p anno-rag --features rerank --lib rerank::tests::prop_score_pairs_total_and_finite -- --ignored`
Expected: PASS (proptest runs default 256 cases reusing one loaded model).

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/src/rerank.rs
git commit -m "test(rerank): proptest score_pairs totality + finiteness"
```

---

## Task 8: `Pipeline::reranker` lazy-init

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs:16-22` (struct field), `:32-43` (`new`), add `reranker()` near `embedder()` (`:46`)
- Test: `crates/anno-rag/src/pipeline.rs` (`#[cfg(test)] mod tests` if present, else a gated integration assert)

- [ ] **Step 1: Add the field + lazy initializer**

In `crates/anno-rag/src/pipeline.rs`, add to the `Pipeline` struct (after `embedder` line 19):

```rust
    #[cfg(feature = "rerank")]
    reranker: tokio::sync::OnceCell<std::sync::Arc<crate::rerank::Reranker>>,
```

In `Pipeline::new`'s `Ok(Self { ... })` literal (after `embedder: OnceCell::new(),` line 39):

```rust
            #[cfg(feature = "rerank")]
            reranker: tokio::sync::OnceCell::new(),
```

After the `embedder()` method (line 50), add:

```rust
    /// Lazy-init the cross-encoder reranker. Downloads ~571 MB (INT8
    /// ONNX) on first call; cached thereafter. Only compiled when the
    /// `rerank` feature is on.
    ///
    /// # Errors
    /// [`Error::Rerank`] if the model fetch or session build fails.
    #[cfg(feature = "rerank")]
    async fn reranker(&self) -> Result<&std::sync::Arc<crate::rerank::Reranker>> {
        self.reranker
            .get_or_try_init(|| async {
                crate::rerank::Reranker::load(&self.cfg)
                    .await
                    .map(std::sync::Arc::new)
            })
            .await
    }

    /// Returns `true` if the reranker has been initialized.
    #[cfg(feature = "rerank")]
    #[must_use]
    pub fn reranker_loaded(&self) -> bool {
        self.reranker.initialized()
    }
```

- [ ] **Step 2: Write the failing test**

Create `crates/anno-rag/tests/rerank_integration.rs`:

```rust
//! Integration tests for the reranked search path. Heavy (model
//! download + LanceDB); ignored by default.
#![cfg(feature = "rerank")]

use anno_rag::{AnnoRagConfig, Pipeline};

fn cfg(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "downloads model + opens LanceDB"]
async fn reranker_lazy_inits_only_on_demand() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");
    assert!(!p.reranker_loaded(), "reranker must not load at construction");
}
```

- [ ] **Step 3: Run to verify compile + pass**

Run: `cargo test -p anno-rag --features rerank --test rerank_integration reranker_lazy_inits_only_on_demand -- --ignored`
Expected: PASS (no model download — only asserts non-initialization).

- [ ] **Step 4: Verify default build still excludes all of it**

Run: `cargo check -p anno-rag`
Expected: SUCCESS — the `#[cfg(feature = "rerank")]` field/methods are absent without the feature.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/tests/rerank_integration.rs
git commit -m "feat(rerank): Pipeline::reranker lazy OnceCell"
```

---

## Task 9: `Pipeline::search_reranked`

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs` (add after `search` at line 208)
- Test: `crates/anno-rag/tests/rerank_integration.rs`

- [ ] **Step 1: Write the failing relevance test**

Add to `crates/anno-rag/tests/rerank_integration.rs`:

```rust
#[tokio::test]
#[ignore = "downloads model + opens LanceDB; ingests a small corpus"]
async fn reranked_search_reorders_vs_rrf() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");

    // Minimal hand-tuned FR-legal corpus written to a folder, ingested.
    let corpus = tmp.path().join("corpus");
    std::fs::create_dir_all(&corpus).unwrap();
    let docs = [
        ("a.txt", "La responsabilité contractuelle suppose une obligation de moyen et un dommage."),
        ("b.txt", "Le bail commercial fixe la durée et le loyer du local."),
        ("c.txt", "L'obligation de moyen engage la responsabilité contractuelle du débiteur négligent."),
        ("d.txt", "Les congés payés sont calculés sur la base de cinq semaines annuelles."),
    ];
    for (name, body) in docs {
        std::fs::write(corpus.join(name), body).unwrap();
    }
    let out = tmp.path().join("out");
    p.ingest_folder(&corpus, false, &out).await.expect("ingest");

    let q = "responsabilité contractuelle obligation de moyen";
    let rrf = p.search(q, 4).await.expect("rrf search");
    let reranked = p.search_reranked(q, 3, 4).await.expect("reranked");

    assert_eq!(reranked.len(), 3);
    // a.txt and c.txt are the doctrinal pair; both must be top-2 reranked.
    let top2: Vec<&str> = reranked
        .iter()
        .take(2)
        .map(|h| h.source_path.as_str())
        .collect();
    assert!(
        top2.iter().any(|s| s.ends_with("a.txt"))
            && top2.iter().any(|s| s.ends_with("c.txt")),
        "expected a.txt + c.txt in reranked top-2, got {top2:?}"
    );
    // Prove the reranker did work: ordering differs from raw RRF top-3.
    let rrf_order: Vec<&str> = rrf.iter().take(3).map(|h| h.source_path.as_str()).collect();
    let rr_order: Vec<&str> = reranked.iter().map(|h| h.source_path.as_str()).collect();
    assert_ne!(rrf_order, rr_order, "rerank must change the ordering");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p anno-rag --features rerank --test rerank_integration reranked_search_reorders_vs_rrf -- --ignored --no-run`
Expected: FAIL — `no method named 'search_reranked'`.

- [ ] **Step 3: Implement `search_reranked`**

In `crates/anno-rag/src/pipeline.rs`, after `search` (line 208):

```rust
    /// Search + cross-encoder rerank.
    ///
    /// 1. `store::search` with `pool_size` (over-fetch).
    /// 2. Rehydrate each hit's `text_pseudo` to plaintext via the vault
    ///    — the cross-encoder must see real entities, not `<PERSON_42>`.
    /// 3. Score `(plaintext_query, rehydrated_text)` pairs.
    /// 4. Reorder by score desc; replace `SearchHit::score` with the
    ///    cross-encoder score.
    /// 5. Truncate to `top_k`.
    ///
    /// The plaintext query is used **only** for the rerank stage; the
    /// upstream embed + FTS lookup still runs on the pseudonymized query,
    /// preserving the privacy invariant.
    ///
    /// # Errors
    /// [`Error::Detect`] / [`Error::Vault`] / [`Error::Embed`] /
    /// [`Error::Store`] / [`Error::Rerank`] per failing layer.
    #[cfg(feature = "rerank")]
    pub async fn search_reranked(
        &self,
        query: &str,
        top_k: usize,
        pool_size: usize,
    ) -> Result<Vec<SearchHit>> {
        let pool = pool_size.max(top_k).max(1);
        let mut hits = self.search(query, pool).await?;
        if hits.is_empty() {
            return Ok(hits);
        }

        // Rehydrate each hit's pseudonymized text to plaintext.
        let mut passages: Vec<String> = Vec::with_capacity(hits.len());
        for h in &hits {
            let r = self.rehydrate(&h.text_pseudo).await?;
            passages.push(r.text);
        }
        let refs: Vec<&str> = passages.iter().map(String::as_str).collect();

        let reranker = self.reranker().await?;
        let scores =
            reranker.score_pairs_batched(query, &refs, self.cfg.rerank_batch_size)?;

        for (h, s) in hits.iter_mut().zip(&scores) {
            h.score = *s;
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(top_k);
        Ok(hits)
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p anno-rag --features rerank --test rerank_integration reranked_search_reorders_vs_rrf -- --ignored`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/tests/rerank_integration.rs
git commit -m "feat(rerank): Pipeline::search_reranked (over-fetch, rehydrate, rerank)"
```

---

## Task 10: Privacy-boundary integration test

**Files:**
- Modify: `crates/anno-rag/tests/rerank_integration.rs`

- [ ] **Step 1: Write the privacy test**

Add to `crates/anno-rag/tests/rerank_integration.rs`:

```rust
#[tokio::test]
#[ignore = "downloads model + opens LanceDB"]
async fn reranked_search_preserves_privacy_boundary() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");

    let corpus = tmp.path().join("corpus");
    std::fs::create_dir_all(&corpus).unwrap();
    // Contains a PII email that the vault will pseudonymize at ingest.
    std::fs::write(
        corpus.join("contract.txt"),
        "Le contrat engage Jean Dupont (jean.dupont@example.fr) au titre de \
         la responsabilité contractuelle.",
    )
    .unwrap();
    let out = tmp.path().join("out");
    p.ingest_folder(&corpus, false, &out).await.expect("ingest");

    let hits = p
        .search_reranked("responsabilité contractuelle", 1, 4)
        .await
        .expect("reranked");
    assert_eq!(hits.len(), 1);

    // The returned hit's stored text is still pseudonymized: the raw
    // email must NOT be present in SearchHit::text_pseudo (rehydration
    // happens only transiently inside search_reranked for scoring).
    assert!(
        !hits[0].text_pseudo.contains("jean.dupont@example.fr"),
        "stored text must remain pseudonymized: {}",
        hits[0].text_pseudo
    );
    assert!(
        hits[0].text_pseudo.contains('<') || hits[0].text_pseudo.contains("EMAIL"),
        "expected a pseudo-token in stored text: {}",
        hits[0].text_pseudo
    );
}
```

- [ ] **Step 2: Run to verify it passes**

Run: `cargo test -p anno-rag --features rerank --test rerank_integration reranked_search_preserves_privacy_boundary -- --ignored`
Expected: PASS — `search_reranked` returns the original (pseudonymized) `SearchHit`; rehydration is transient and never written back into the hit.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/tests/rerank_integration.rs
git commit -m "test(rerank): lock the rehydration privacy boundary"
```

---

## Task 11: `Pipeline::recall_memory_reranked` (spec §10.2 — memories in v1)

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs` (add after `recall_memory`, which ends ~line 540; place the new method directly after its closing brace)
- Test: `crates/anno-rag/tests/rerank_integration.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/anno-rag/tests/rerank_integration.rs`:

```rust
#[tokio::test]
#[ignore = "downloads model + opens LanceDB"]
async fn reranked_memory_recall_returns_topk() {
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
        .recall_memory_reranked(
            "délai de prescription en responsabilité",
            2,
            None,
            None,
            None,
            false,
            10,
        )
        .await
        .expect("reranked recall");
    assert_eq!(hits.len(), 2);
    assert!(
        hits[0].text.contains("prescription"),
        "top hit must be about prescription, got: {}",
        hits[0].text
    );
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p anno-rag --features rerank --test rerank_integration reranked_memory_recall_returns_topk -- --ignored --no-run`
Expected: FAIL — `no method named 'recall_memory_reranked'`.

- [ ] **Step 3: Implement `recall_memory_reranked`**

In `crates/anno-rag/src/pipeline.rs`, immediately after the closing brace of `recall_memory`:

```rust
    /// Memory recall + cross-encoder rerank. Same contract as
    /// [`Pipeline::recall_memory`] plus a `pool_size` over-fetch and a
    /// rerank stage. Memory text is already plaintext post-rehydration
    /// inside `recall_memory`, so no extra vault round-trip is needed —
    /// the cross-encoder scores `(query, hit.text)` directly.
    ///
    /// # Errors
    /// [`Error::Detect`] / [`Error::Vault`] / [`Error::Embed`] /
    /// [`Error::Store`] / [`Error::Rerank`] per failing layer.
    #[cfg(feature = "rerank")]
    #[allow(clippy::too_many_arguments)]
    pub async fn recall_memory_reranked(
        &self,
        query: &str,
        top_k: usize,
        session_id: Option<String>,
        kinds: Option<Vec<crate::memory::MemoryKind>>,
        as_of: Option<chrono::DateTime<chrono::Utc>>,
        graph_expand: bool,
        pool_size: usize,
    ) -> Result<Vec<crate::memory::MemoryHit>> {
        let pool = pool_size.max(top_k).max(1);
        let mut hits = self
            .recall_memory(query, pool, session_id, kinds, as_of, graph_expand)
            .await?;
        if hits.is_empty() {
            return Ok(hits);
        }

        let passages: Vec<&str> = hits.iter().map(|h| h.text.as_str()).collect();
        let reranker = self.reranker().await?;
        let scores =
            reranker.score_pairs_batched(query, &passages, self.cfg.rerank_batch_size)?;

        let mut scored: Vec<(crate::memory::MemoryHit, f32)> =
            hits.drain(..).zip(scores).collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(scored
            .into_iter()
            .take(top_k)
            .map(|(mut h, s)| {
                h.score = s;
                h
            })
            .collect())
    }
```

> Verify against `crate::memory::MemoryHit`: the test sets `h.score`.
> Confirm `MemoryHit` has a public `score: f32` field (it does — used
> by `recall_memory`'s ranking). If the field name differs, adjust the
> `h.score = s` line and the test's assertion accordingly.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p anno-rag --features rerank --test rerank_integration reranked_memory_recall_returns_topk -- --ignored`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/tests/rerank_integration.rs
git commit -m "feat(rerank): Pipeline::recall_memory_reranked (memories path)"
```

---

## Task 12: README + CHANGELOG — non-goal → opt-in

**Files:**
- Modify: `crates/anno-rag/README.md` (the "v0.2 deliberate non-goals" / reranker line, ~line 84)
- Modify: `crates/anno-rag/CHANGELOG.md` (top entry; create the file if absent)

- [ ] **Step 1: Update the README**

In `crates/anno-rag/README.md`, find the line listing the reranker as a non-goal (~line 84) and replace it with:

```markdown
- **Cross-encoder reranking**: available as an opt-in `--features rerank`
  build. Uses BGE-reranker-v2-m3 (pre-quantized INT8 ONNX, ~571 MB
  downloaded on first use, cached). Memory envelope with rerank on is
  ~3 GB peak (vs the <1.5 GB default-build cap, which is unchanged —
  rerank is off by default). Enable per the user guide; expect a
  0.6–2.5 s rerank stage per query on CPU.
```

- [ ] **Step 2: Update the CHANGELOG**

Prepend to `crates/anno-rag/CHANGELOG.md` (create with a `# Changelog` header if it does not exist):

```markdown
## Unreleased

### Added
- Opt-in cross-encoder reranker (`rerank` feature): `Pipeline::search_reranked`
  and `Pipeline::recall_memory_reranked` using BGE-reranker-v2-m3 INT8 ONNX
  via `ort`. Default off; ~571 MB model fetched on first use.
```

- [ ] **Step 3: Verify docs build**

Run: `cargo doc -p anno-rag --features rerank --no-deps`
Expected: SUCCESS, no broken intra-doc links.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/README.md crates/anno-rag/CHANGELOG.md
git commit -m "docs(rerank): README opt-in entry + CHANGELOG"
```

---

## Task 13: MCP `rerank: bool` param + `score_source` audit field

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (the `SearchParams` and `MemoryRecallParams` structs + the `search` / `memory_recall` tool bodies)

> **Blocked until PR #9 merges.** Confirm `crates/anno-rag-mcp/src/lib.rs` exists on the branch before starting. The struct/handler names below match PR #9's `lib.rs` (`SearchParams`, `MemoryRecallParams`, `#[tool] async fn search`, `#[tool] async fn memory_recall`).

- [ ] **Step 1: Add the `rerank` field to `SearchParams`**

In `crates/anno-rag-mcp/src/lib.rs`, in `pub struct SearchParams`:

```rust
    /// When true, re-score the top candidates with the cross-encoder
    /// reranker. Requires the server built with `--features rerank`;
    /// otherwise this call returns a clear error.
    #[serde(default)]
    pub rerank: bool,
```

Add the same field to `pub struct MemoryRecallParams`.

- [ ] **Step 2: Branch the `search` handler**

In `async fn search`, replace the `self.pipeline.search(&params.query, params.top_k).await` call with:

```rust
        let result = if params.rerank {
            #[cfg(feature = "rerank")]
            {
                tracing::info!(
                    target: "anno_rag::audit",
                    tool = "search",
                    score_source = "cross_encoder",
                    ""
                );
                self.pipeline
                    .search_reranked(&params.query, params.top_k, self.cfg.rerank_pool_size)
                    .await
            }
            #[cfg(not(feature = "rerank"))]
            {
                return "Error: rerank requested but server built without \
                        the `rerank` feature"
                    .to_string();
            }
        } else {
            tracing::info!(
                target: "anno_rag::audit",
                tool = "search",
                score_source = "rrf",
                ""
            );
            self.pipeline.search(&params.query, params.top_k).await
        };
        match result {
```

(The existing `match self.pipeline.search(...).await { Ok(hits) => ... }`
becomes `match result { Ok(hits) => ... }` — only the binding changes;
the `Ok`/`Err` arms are unchanged.)

- [ ] **Step 3: Branch the `memory_recall` handler the same way**

In `async fn memory_recall`, replace the existing
`self.pipeline.recall_memory(&p.query, p.top_k, p.session_id, kinds, p.as_of, p.graph_expand).await`
call (keep the `kinds` parsing line that precedes it unchanged) with:

```rust
        let result = if p.rerank {
            #[cfg(feature = "rerank")]
            {
                tracing::info!(
                    target: "anno_rag::audit",
                    tool = "memory_recall",
                    score_source = "cross_encoder",
                    ""
                );
                self.pipeline
                    .recall_memory_reranked(
                        &p.query,
                        p.top_k,
                        p.session_id,
                        kinds,
                        p.as_of,
                        p.graph_expand,
                        self.cfg.rerank_pool_size,
                    )
                    .await
            }
            #[cfg(not(feature = "rerank"))]
            {
                return "Error: rerank requested but server built without \
                        the `rerank` feature"
                    .to_string();
            }
        } else {
            tracing::info!(
                target: "anno_rag::audit",
                tool = "memory_recall",
                score_source = "rrf",
                ""
            );
            self.pipeline
                .recall_memory(&p.query, p.top_k, p.session_id, kinds, p.as_of, p.graph_expand)
                .await
        };
        match result {
```

(As in Step 2, the existing `match ... { Ok(hits) => ... Err(e) => ... }`
arms are unchanged — only the bound expression becomes `result`. If
`kinds` is consumed by both branches, clone it: `kinds.clone()` in the
rerank branch, `kinds` in the else — or hoist the parse above the `if`.)

- [ ] **Step 4: Verify both feature states compile**

Run: `cargo check -p anno-rag-mcp` then `cargo check -p anno-rag-mcp --features rerank`
Expected: both SUCCESS. (If `anno-rag-mcp` has no `rerank` feature yet, add `rerank = ["anno-rag/rerank"]` to its `Cargo.toml` `[features]` and a passthrough; the check command will tell you.)

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/Cargo.toml
git commit -m "feat(rerank): MCP rerank:bool param + score_source audit"
```

---

## Task 14: Pre-flight download UX (spec §10.4)

**Files:**
- Modify: `crates/anno-rag/src/rerank.rs` (`Reranker::load` — warn before the fetch)

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/anno-rag/src/rerank.rs`:

```rust
    #[test]
    fn download_notice_is_silent_when_not_a_tty() {
        // is-terminal returns false under `cargo test` (piped stderr),
        // so the interactive prompt path must not be taken. We assert
        // the helper returns Ok without blocking.
        let decided = super::download_notice("onnx-community/bge-reranker-v2-m3-ONNX", 571);
        assert!(decided.is_ok());
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p anno-rag --features rerank --lib rerank::tests::download_notice_is_silent_when_not_a_tty -- --exact`
Expected: FAIL — `cannot find function 'download_notice'`.

- [ ] **Step 3: Implement `download_notice` and call it in `load`**

Add to `crates/anno-rag/src/rerank.rs` (module level, above `impl Reranker`):

```rust
/// Emit a one-line notice before the ~571 MB model fetch. In an
/// interactive terminal it warns on stderr; in MCP / daemon / CI
/// contexts (no TTY) it logs at `info` and proceeds silently. Never
/// blocks — the fetch is opt-in via the `rerank` feature already, so a
/// hard prompt would deadlock non-interactive hosts (spec §10.4).
fn download_notice(repo: &str, approx_mb: u32) -> Result<()> {
    use is_terminal::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprintln!(
            "anno-rag: fetching reranker model '{repo}' (~{approx_mb} MB, \
             one-time, cached). Set the `rerank` feature off to skip."
        );
    } else {
        tracing::info!(
            target: "anno_rag::rerank",
            repo,
            approx_mb,
            "fetching reranker model (non-interactive: silent)"
        );
    }
    Ok(())
}
```

Make `is-terminal` available to non-test builds: in `crates/anno-rag/Cargo.toml`, move `is-terminal = "0.4"` from `[dev-dependencies]` to `[dependencies]` as `is-terminal = { version = "0.4", optional = true }` and add it to the feature: `rerank = ["dep:ort", "dep:ndarray", "dep:is-terminal"]`.

In `Reranker::load`, call it immediately before the first `repo.get(...)`:

```rust
        download_notice(&cfg.rerank_model, 571)?;
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p anno-rag --features rerank --lib rerank::tests::download_notice_is_silent_when_not_a_tty -- --exact`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/rerank.rs crates/anno-rag/Cargo.toml
git commit -m "feat(rerank): pre-flight download notice (TTY warn / silent otherwise)"
```

---

## Task 15: Benchmark — pool-of-30 latency floor (spec §8.9)

**Files:**
- Create: `crates/anno-rag/benches/bench_rerank.rs`
- Modify: `crates/anno-rag/Cargo.toml` (`[[bench]]` entry)

- [ ] **Step 1: Add the bench target**

In `crates/anno-rag/Cargo.toml`, alongside the existing `[[bench]]` entries:

```toml
[[bench]]
name = "bench_rerank"
harness = false
required-features = ["rerank"]
```

- [ ] **Step 2: Write the benchmark**

Create `crates/anno-rag/benches/bench_rerank.rs`:

```rust
//! Rerank a pool of 30 candidates against a canonical FR-legal query.
//! Establishes the §5.6 perf floor. Run:
//! `cargo bench -p anno-rag --features rerank --bench bench_rerank`.
#![cfg(feature = "rerank")]

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_rerank_pool_30(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let reranker = rt
        .block_on(anno_rag::rerank::Reranker::load(
            &anno_rag::AnnoRagConfig::default(),
        ))
        .expect("load reranker");

    let query = "responsabilité contractuelle et obligation de moyen";
    let passages: Vec<String> = (0..30)
        .map(|i| format!("Clause {i} relative à la responsabilité contractuelle du débiteur."))
        .collect();
    let refs: Vec<&str> = passages.iter().map(String::as_str).collect();

    c.bench_function("rerank_pool_30", |b| {
        b.iter(|| {
            let s = reranker
                .score_pairs_batched(query, &refs, 8)
                .expect("score");
            criterion::black_box(s);
        });
    });
}

criterion_group!(benches, bench_rerank_pool_30);
criterion_main!(benches);
```

- [ ] **Step 3: Verify it compiles and runs**

Run: `cargo bench -p anno-rag --features rerank --bench bench_rerank -- --warm-up-time 1 --measurement-time 5`
Expected: completes; prints a `rerank_pool_30` time. Sanity: total per-iter should be under ~4 s (the §5.6 documented floor). If it exceeds ~4 s, record it — that is the regression signal, not expected behaviour.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/benches/bench_rerank.rs crates/anno-rag/Cargo.toml
git commit -m "bench(rerank): pool-of-30 latency floor"
```

---

## Task 16: Eval baseline non-regression gate (spec §10.5)

**Files:**
- Create: `crates/anno-rag/tests/fixtures/eval_baseline_reranked.toml`
- Modify: `crates/anno-rag/tests/` (the existing eval-gate test — locate it via the reference to `eval_baseline.toml`)

- [ ] **Step 1: Locate the existing RRF eval gate**

Run: `grep -rn "eval_baseline.toml" crates/anno-rag/tests/`
Expected: a test (e.g. `tests/eval_gate.rs` or similar) that loads `eval_baseline.toml` and asserts nDCG@10 / recall non-regression for the RRF path. Read it to learn the metric struct + assertion shape; the reranked gate mirrors it.

- [ ] **Step 2: Write the reranked baseline fixture**

Create `crates/anno-rag/tests/fixtures/eval_baseline_reranked.toml` with the same key structure as `eval_baseline.toml` (read that file first to copy the exact schema — keys like `ndcg_at_10`, `recall_at_10`). Seed it with the **RRF baseline values** as a floor (the reranked path must do at least as well):

```toml
# Reranked-path eval baseline. The reranked gate asserts:
#   ndcg_at_10(reranked) >= ndcg_at_10(rrf baseline)   [non-regression vs RRF]
# Seeded from eval_baseline.toml; tighten upward once the reranker's
# real numbers are measured on the eval corpus.
ndcg_at_10 = 0.0   # replace with eval_baseline.toml's ndcg_at_10
recall_at_10 = 0.0 # replace with eval_baseline.toml's recall_at_10
```

> Read `crates/anno-rag/tests/fixtures/eval_baseline.toml` and copy its
> exact keys + the RRF numeric values into the two fields above. Do not
> invent keys — match the existing schema 1:1.

- [ ] **Step 3: Write the reranked eval gate test**

Add an `#[ignore]` test to the eval-gate test file (mirroring the RRF gate found in Step 1), running `search_reranked` over the eval corpus and asserting nDCG@10 ≥ the value in `eval_baseline_reranked.toml`, and additionally `ndcg_reranked >= ndcg_rrf` (non-regression vs RRF — if the reranker doesn't beat or match RRF on FR legal, the gate fails and we do not ship). Use the same corpus-load + metric helpers the RRF gate uses (do not duplicate metric code — call the existing helper).

- [ ] **Step 4: Run the gate**

Run: `cargo test -p anno-rag --features rerank --test <eval_gate_file> -- --ignored --nocapture`
Expected: PASS, and prints `ndcg_reranked=… ndcg_rrf=…`. Update `eval_baseline_reranked.toml` with the measured reranked nDCG (as the new floor) and re-run; it must still pass.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/tests/fixtures/eval_baseline_reranked.toml crates/anno-rag/tests/
git commit -m "test(rerank): eval-baseline non-regression gate vs RRF"
```

---

## Final verification

- [ ] **Default build untouched:** `cargo check -p anno-rag && cargo clippy -p anno-rag --all-targets -- -D warnings`
- [ ] **Feature build clean:** `cargo clippy -p anno-rag --features rerank --all-targets -- -D warnings`
- [ ] **Format:** `cargo fmt --all -- --check`
- [ ] **Fast tests green (no model):** `cargo test -p anno-rag --features rerank` (the `#[ignore]` model tests stay skipped)
- [ ] **Heavy tests green (one machine, cached model):** `cargo test -p anno-rag --features rerank -- --ignored --test-threads=1`
- [ ] **MCP both states:** `cargo check -p anno-rag-mcp && cargo check -p anno-rag-mcp --features rerank`
