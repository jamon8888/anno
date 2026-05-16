# Design — Cross-Encoder Rerank for `anno-rag`

**Date**: 2026-05-16
**Status**: Draft for review
**Scope**: `crates/anno-rag` only. New module + a thin opt-in wrapper layered between RRF and the existing pipeline filters. Feature-gated; default off.

## 1. Goal

Add a learned cross-encoder reranker that scores each (query, chunk) pair through a transformer and reorders the top-N RRF candidates by semantic relevance. Today the only reranker in the pipeline is `RRFReranker` ([crates/anno-rag/src/store.rs:698](crates/anno-rag/src/store.rs:698), [crates/anno-rag/src/store.rs:765](crates/anno-rag/src/store.rs:765)), which is rank-fusion only — it never looks at the text of the pair. A cross-encoder does, and on French legal it typically improves top-k recall 1.5–3× over RRF alone.

The reranker is explicitly listed as a v0.2 non-goal in [crates/anno-rag/README.md:84](crates/anno-rag/README.md:84). This spec pulls it forward as an opt-in v0.3 feature.

## 2. Why

- RRF fuses ranks (`1/(k+rank_dense) + 1/(k+rank_fts)`) without seeing the query/chunk text. Two chunks tied in RRF can have very different actual relevance; a cross-encoder breaks those ties using semantics.
- Pairs naturally with Spec A (context-token budgeter, [docs/superpowers/specs/2026-05-16-anno-rag-context-budgeter-design.md](docs/superpowers/specs/2026-05-16-anno-rag-context-budgeter-design.md)): rerank improves *what* fits in a fixed budget; the budgeter shrinks *how much* reaches the LLM. Independent, complementary.
- French legal is exact-match heavy (article numbers, ECLI), which is exactly where rank-fusion is weakest and a semantic reranker shines.

## 3. Non-goals

- Multi-stage reranking (cascading multiple models). One cross-encoder, that's it.
- Online learning or fine-tuning. Off-the-shelf weights only.
- Replacing RRF. RRF stays; the cross-encoder runs **on top of** RRF's output.
- GPU inference. CPU-only in v1, matching the embedder pattern at [crates/anno-rag/src/embed.rs:34](crates/anno-rag/src/embed.rs:34). GPU opt-in mirrors the embedder's documented v0.2 path; not in scope for this spec.
- Quantized weights. Plain safetensors in v1. Quantization is a follow-up.
- Rerank for the chunk-store `pipeline::search` path AND the memories `recall_memory` path simultaneously in v1. Spec covers chunks first; memories path is a parallel, near-identical wrapper that the implementation plan may bundle or split.

## 4. Model choice

**BGE-reranker-v2-m3** (BAAI, MIT licence, multilingual, XLM-RoBERTa-large base, 568M params).

Why this and not alternatives:

| Model | Multilingual | Licence | Size | Verdict |
|---|---|---|---|---|
| **BGE-reranker-v2-m3** | yes (100+ langs, French strong) | MIT | 568M | **Picked** |
| BGE-reranker-base / large | English only | MIT | 278M / 568M | Out — French is anno's core |
| jina-reranker-v2-base-multilingual | yes | CC-BY-NC-4.0 | 278M | **Out** — non-commercial breaks anno's dual MIT/Apache posture |
| mxbai-rerank-base-v1 | English only | Apache-2.0 | 184M | Out — French |
| BGE-reranker-v2-gemma | yes | Gemma terms | 2.5B | Out — too big for CPU baseline |

**Candle support — verified.** `candle-transformers` v0.10.x ships `candle_transformers::models::xlm_roberta::XLMRobertaForSequenceClassification` (huggingface/candle, [`candle-transformers/src/models/xlm_roberta.rs`](https://github.com/huggingface/candle/blob/0.10.1/candle-transformers/src/models/xlm_roberta.rs) — verified against the v0.10.1 tag; v0.10.2 is anno's resolved version per `Cargo.lock`). The relevant API:

```rust
// candle_transformers::models::xlm_roberta
pub struct XLMRobertaForSequenceClassification { /* roberta + classifier head */ }

impl XLMRobertaForSequenceClassification {
    pub fn new(num_labels: usize, cfg: &Config, vb: VarBuilder) -> Result<Self>;
    pub fn forward(&self, input_ids: &Tensor, attention_mask: &Tensor,
                   token_type_ids: &Tensor) -> Result<Tensor>;
}
```

The variable-builder scoping (`vb.pp("roberta")` for the encoder, `vb.pp("classifier")` for the head) matches HuggingFace's safetensors layout for `XLMRobertaForSequenceClassification`, which is exactly what BGE-reranker-v2-m3 ships with. `num_labels = 1` gives the single-logit relevance head BGE uses. RoBERTa ignores `token_type_ids` semantically but the forward signature requires a tensor of zeros — trivial.

No fallback / hand-rolled head needed.

## 5. Architecture

### 5.1 New module

`crates/anno-rag/src/rerank.rs` (single file in v1; split into a submodule if it grows):

```rust
use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use candle_core::{Device, Tensor};
use tokenizers::Tokenizer;

/// Loaded cross-encoder reranker.
///
/// Owns the model + tokenizer + device. Loaded once per process via
/// `Pipeline::reranker()` (lazy, `OnceCell`). Same pattern as `Embedder`
/// at crates/anno-rag/src/embed.rs:20.
pub struct Reranker {
    model: RerankerModel,   // XLM-RoBERTa classifier; details in §5.4
    tokenizer: Tokenizer,
    device: Device,
    max_seq_len: usize,     // 512 for BGE-reranker-v2-m3
}

impl Reranker {
    /// Load from HuggingFace Hub. Weights cached under `cfg.models_cache()`.
    ///
    /// # Errors
    /// `Error::Rerank` on fetch/parse/graph-construction failures.
    pub async fn load(cfg: &AnnoRagConfig) -> Result<Self>;

    /// Score `(query, passage)` pairs. Returns relevance scores in input order.
    /// Higher = more relevant. Batched internally up to `max_batch_pairs`.
    ///
    /// `query` is shared across all pairs (saves one tokenization per pair).
    /// Truncation: passages are right-truncated to fit `max_seq_len` after
    /// the query + special tokens are accounted for.
    pub fn score_pairs(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>>;
}
```

### 5.2 Pipeline wiring

`Pipeline` gains an optional reranker field (`OnceCell<Arc<Reranker>>`) and a new method:

```rust
impl Pipeline {
    /// Lazy-init the reranker. Loads ~2.3 GB of weights on first call.
    /// Only callable when the `rerank` Cargo feature is on.
    #[cfg(feature = "rerank")]
    async fn reranker(&self) -> Result<&Arc<Reranker>>;

    /// Search + cross-encoder rerank.
    ///
    /// 1. Calls `store::search` with `pool_size` (over-fetch, default 30).
    /// 2. Rehydrates each hit's text_pseudo to original via the vault.
    ///    Why: cross-encoder must see real entities, not `<PERSON_42>` tokens,
    ///    because BGE was trained on natural text. Pseudonyms would tank
    ///    relevance scores.
    /// 3. Scores all (query, rehydrated_text) pairs.
    /// 4. Reorders by score descending, replacing `SearchHit::score` with
    ///    the cross-encoder score.
    /// 5. Truncates to `top_k`.
    ///
    /// The query passed to the cross-encoder is the **plaintext** query
    /// (pre-pseudonymization) so the model sees real entities on both sides.
    /// The pseudonymized query is still used for the upstream embed + FTS
    /// lookup in step 1; only the rerank stage works on plaintext.
    #[cfg(feature = "rerank")]
    pub async fn search_reranked(
        &self,
        query: &str,
        top_k: usize,
        pool_size: usize,
    ) -> Result<Vec<SearchHit>>;
}
```

`pipeline::search` and `pipeline::recall_memory` stay untouched. The reranked variants are additive; callers opt in.

### 5.3 Insertion point in the existing flow

```
query (plaintext)
   ├─→ detect + pseudonymize  ─→ pseudo_query
   │                              │
   │                              ↓
   │                       embed(pseudo_query)
   │                              │
   │                              ↓
   │                  store.search(pseudo_query, qv, pool_size)
   │                       (RRF over vector + FTS)
   │                              │
   │                              ↓
   │                  rehydrate each hit's text_pseudo
   │                              │
   │                              ↓
   └────────→ reranker.score_pairs(plaintext_query, rehydrated_texts)
                                  │
                                  ↓
                          sort desc by score,
                          truncate to top_k
                                  │
                                  ↓
                          Vec<SearchHit> (with cross-encoder scores)
```

The rehydration step uses the existing `Pipeline::rehydrate` ([pipeline.rs:216](crates/anno-rag/src/pipeline.rs:216)). The plaintext query never enters the embed / FTS / store path, preserving the privacy invariant that nothing un-pseudonymized leaves the vault for upstream retrieval. The cross-encoder runs inside the same process as the vault; rehydrated content stays local.

### 5.4 Model implementation

XLM-RoBERTa-large classifier via `candle_transformers::models::xlm_roberta::XLMRobertaForSequenceClassification::new(1, &cfg, vb)` (single-logit head). The forward returns logits shape `[batch, 1]`; sigmoid'd to a relevance score in [0, 1]. The input format is the standard cross-encoder layout:

```
<s> query </s></s> passage </s>
```

Tokenized via the same `tokenizers` crate already in `Cargo.toml:37`; specifically `Tokenizer::encode_pair(query, passage, true)` to get the right special-token wiring without hand-rolling token ids.

The model loader follows the embedder pattern at [crates/anno-rag/src/embed.rs:33-60](crates/anno-rag/src/embed.rs:33):
- `hf_hub::api::tokio::Api` to fetch `config.json`, `tokenizer.json`, `model.safetensors`.
- Read `config.json` into the appropriate Candle config struct.
- `VarBuilder` + safetensors mmap.
- Construct the model + classifier head.
- Device: `Device::Cpu`.

### 5.5 Batching

`score_pairs` batches internally up to `max_batch_pairs` (config; default 8). For each batch:

1. Tokenize all pairs in the batch (parallelizable but v1 keeps it sequential — `tokenizers` is already fast on Rust).
2. Pad to the batch's max length (right-pad; left-truncate the *passage* if total exceeds 512).
3. Build `input_ids` and `attention_mask` tensors.
4. Forward pass → take the `[CLS]` (or `<s>` for RoBERTa-family) hidden state → linear head → sigmoid.
5. Append to results.

Memory at peak: `batch_size × max_seq_len × hidden_dim × 4 bytes ≈ 8 × 512 × 1024 × 4 = 16 MB` activations, plus the model itself (~2.3 GB fp32). Fits on any laptop running anno today.

### 5.6 Performance budget (CPU)

Approximate, measured per the implementation plan but with these expectations going in:
- BGE-reranker-v2-m3 on a modern x86 laptop (Zen 3+, AVX2): **~50–150 ms per pair**, batched.
- Pool of 30 candidates → ~1.5–4.5 seconds end-to-end for the rerank stage.
- This is per **recall**, not per chunk read. MCP `search_reranked` calls are user-driven (one per Claude tool call), not high-QPS.

Documented latency floor: if the rerank stage adds more than ~5 s, that is a regression worth investigating, not the expected behaviour.

Quantization (Q4_K via candle's GGML loader) is a follow-up; trades ~1.5–2× speedup and 4× disk shrink for a small recall loss. Not in v1.

### 5.7 Score semantics

Cross-encoder score replaces the RRF score in `SearchHit::score` ([store.rs:176-179](crates/anno-rag/src/store.rs:176)) for the reranked path. Rationale:

- A consumer of `search_reranked` cares about the cross-encoder ordering; preserving the RRF value alongside would double the surface and create a "which one to sort by" footgun.
- The doc on `SearchHit::score` already says "higher = more relevant" without committing to the producer; this stays accurate.
- Audit log records `score_source: "cross_encoder"` vs `"rrf"` so historical comparisons remain possible (additive field on the existing audit record; see §6).

For the unreranked `search` path, scores remain RRF as today. The two paths produce comparable orderings but not comparable scalar values — callers must not mix them in the same downstream computation.

## 6. Configuration

### 6.1 Cargo feature

```toml
[features]
default = []
rerank = []   # public on/off switch for the rerank module
```

`candle-transformers` is already a required dep ([Cargo.toml:35](crates/anno-rag/Cargo.toml:35)), so the feature does not need to gate any dependency. The flag exists purely to `#[cfg(feature = "rerank")]`-gate the new module, the `Pipeline::reranker` field, and the `search_reranked` method. Default off so existing builds and `cargo install anno-rag` users do not pay the ~2.3 GB model download on first run. Anno's heavy ML backends already follow this pattern (`gliner2-fastino`, `gliner2-fastino-candle` features in `crates/anno/Cargo.toml`).

### 6.2 Runtime config additions

In `AnnoRagConfig` ([crates/anno-rag/src/config.rs](crates/anno-rag/src/config.rs)):

```rust
pub struct AnnoRagConfig {
    // ... existing fields ...

    /// Reranker model id on HuggingFace Hub.
    /// Default: "BAAI/bge-reranker-v2-m3".
    pub rerank_model: String,

    /// Default pool size for `search_reranked` callers that don't specify.
    /// Default: 30.
    pub rerank_pool_size: usize,

    /// Maximum pairs per forward pass batch. Default: 8.
    pub rerank_batch_size: usize,
}
```

Reasonable defaults; opinionated values tuned for legal French / Cowork single-user scenario. All three are tunable for users who need different trade-offs (smaller pool, larger batch on a beefier machine).

### 6.3 MCP surface

The MCP tool gains an optional `rerank: bool` parameter (default `false`). When `true`, the server calls `pipeline::search_reranked` instead of `pipeline::search`. When the Cargo feature is off, the param is rejected with a clear error message at deserialization time. Composes naturally with the Spec A budgeter (`rerank` + `budget_tokens` in the same call).

## 7. Module boundaries

- `rerank.rs` depends on: `candle_core`, `candle_nn`, `candle_transformers`, `tokenizers`, `hf-hub`. All already in `Cargo.toml`.
- `rerank.rs` does **not** depend on `store.rs`, `pipeline.rs`, or `vault.rs`. It takes plain `&str` query + `&[&str]` passages and returns `Vec<f32>`. Composable in isolation.
- The pipeline wrapper in `pipeline.rs` is the only place that knows about the "rehydrate before scoring" privacy invariant.
- Vault rehydration is the only operation that crosses the privacy boundary inside this flow; it already exists and is unchanged.

## 8. Testing

Unit tests in `rerank.rs`:

1. `score_pairs` with one obviously-relevant and one obviously-irrelevant passage produces ordered scores (relevant > irrelevant).
2. Empty `passages` → empty `Vec<f32>`, no panic.
3. Batching: 17 passages with `max_batch_pairs = 8` returns 17 scores; correctness identical to single-batch.
4. Truncation: a passage longer than `max_seq_len` is right-truncated; no panic.
5. Determinism: same `(query, passages)` produces identical scores across two calls (within `f32::EPSILON`).

Integration test (`tests/rerank_integration.rs`, behind `#[cfg(feature = "rerank")]`):

6. Small fixture corpus (10 chunks, hand-tuned French legal). Query "responsabilité contractuelle"; assert top-3 reranked hits include chunks containing both that bigram and `obligation de moyen` (the canonical doctrinal pair), and that this is *different from* the top-3 RRF baseline — proving the reranker is doing work.

7. Privacy invariant: capture stdout/tracing during `search_reranked`; assert no `<PERSON_*>` / `<EMAIL_*>` tokens appear in any logged passage, and no plaintext entity (a fixture email address) appears in the upstream embed / FTS call path. Lock the rehydration boundary in a test.

Property test (`proptest`, dev-dep at Cargo.toml:67):

8. ∀ query, ∀ passages with `len <= 20`: `score_pairs` returns `passages.len()` scores, all finite (`f32::is_finite`).

Benchmark (`benches/bench_rerank.rs`, additive next to the existing benches at Cargo.toml:69-87):

9. Rerank a pool of 30 candidates against a canonical query. Time and RSS. Establishes the perf floor referenced in §5.6.

## 9. Risks and mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| BGE-reranker-v2-m3 safetensors key names don't match Candle's `vb.pp("roberta")` / `vb.pp("classifier")` scoping | Low (HF convention is followed) | Verified upstream API in §4; plan still adds a smoke-load test as commit 1 before wiring the rest. If keys diverge, a thin `VarBuilder::rename_keys` mapping resolves it. |
| ~2.3 GB model download surprises users on first run | High consequence if mishandled | Feature-gated; opt-in. Document download size in README. Optional: pre-flight check that warns + asks for confirmation before fetching. |
| CPU latency too slow for interactive Cowork use | Medium | Document expected 1.5–4.5 s; offer config knob to shrink pool_size; quantization as a follow-up. |
| Rehydration leaks entities into a long-lived cross-encoder cache | Low (no cache in v1) | Explicitly: no scoring cache. `score_pairs` is stateless. |
| Cross-encoder score (sigmoid'd logit) is incomparable to RRF score | Inherent | §5.7 documents this; audit field `score_source` records which is which. |
| Reranker disagrees with retrieval ranking and hides relevant chunks | Medium | `pool_size = 30` default gives the reranker a fat candidate pool; raise it if recall regresses in eval. |
| GPU users frustrated by CPU-only v1 | Low (anno's audience is CPU-first) | Document v0.2 GPU path matches embedder's; not in this spec's scope. |
| Recall regression on French legal vs RRF baseline | Medium | Eval gate: `bench_eval` (already exists at Cargo.toml:87) must show non-regression on the v0.2 baseline at [crates/anno-rag/tests/fixtures/eval_baseline.toml:6](crates/anno-rag/tests/fixtures/eval_baseline.toml:6). If it regresses, do not ship; investigate. |

## 10. Open questions (decisions deferred to the plan)

1. ~~**Does `candle_transformers::models::xlm_roberta` exist and accept BGE-reranker-v2-m3 weights?**~~ **Resolved**: `XLMRobertaForSequenceClassification` exists in candle-transformers 0.10.1+ with the right shape (see §4). Plan keeps a smoke-load test as the first commit to confirm safetensors-key alignment before wiring the rest.
2. **Should `recall_memory_reranked` ship in v1, or come right after?** Lean: include it; the cost is mostly copy-paste from `search_reranked`. Plan decides.
3. **Where exactly does score_source go in the audit log?** Plan reads `audit.rs` and decides additive-field vs separate-row.
4. **Pre-flight download warning UX**: CLI prompt, hard error, or silent fetch with `tracing::info` only? Lean: warn + prompt only in interactive terminals; silent in MCP / daemon contexts.
5. **Eval baseline update**: does the v0.2 baseline at [tests/fixtures/eval_baseline.toml:6](crates/anno-rag/tests/fixtures/eval_baseline.toml:6) need an alongside `eval_baseline_reranked.toml` for the new path? Plan call.
6. **Future composition with Spec A**: confirmed the rerank score should be `SearchHit::score`, but does the budgeter benefit from knowing which `score_source` produced it? Probably no — budgeter is order-preserving and doesn't look at scores. Confirmed by §7 of Spec A.

## 11. Implementation outline (not the plan)

Anticipated commits:

1. Smoke-load test: instantiate `XLMRobertaForSequenceClassification::new(1, ...)` from the BGE-reranker-v2-m3 safetensors and run a single forward pass on a dummy pair. Confirms key alignment (§9 risk). No production code yet.
2. `rerank.rs` module: `Reranker::load` + `score_pairs`, with unit tests #1–#5.
3. `Pipeline::reranker` lazy-init + `Pipeline::search_reranked`, feature-gated.
4. Config additions; CHANGELOG; README "v0.2 deliberate non-goals" entry moves from non-goal to opt-in.
5. Integration tests #6 (relevance) + #7 (privacy boundary).
6. `recall_memory_reranked` mirror (if §10.2 says yes).
7. MCP `rerank: bool` param.
8. Benchmark + eval-baseline non-regression check.

Each commit compiles and tests pass green. Default Cargo build is unchanged because `rerank` is opt-in.
