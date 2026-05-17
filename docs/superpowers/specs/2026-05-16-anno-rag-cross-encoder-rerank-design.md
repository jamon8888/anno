# Design — Cross-Encoder Rerank for `anno-rag`

**Date**: 2026-05-16 (amended 2026-05-17 — runtime + model-format decision)
**Status**: Draft for review
**Scope**: `crates/anno-rag` only. New module + a thin opt-in wrapper layered between RRF and the existing pipeline filters. Feature-gated; default off.

> **Amendment 2026-05-17.** The original draft chose candle
> `XLMRobertaForSequenceClassification` with fp32 safetensors (~2.3 GB).
> After review against the master design's carried RSS constraint
> (< 1.5 GB peak, [docs/superpowers/specs/2026-05-12-anno-rag-design.md]),
> the runtime + format are changed to **pre-quantized INT8 ONNX run via
> `ort`**: `onnx-community/bge-reranker-v2-m3-ONNX :: model_int8.onnx`
> (**571 MB** vs 2.27 GB fp32). Rationale: same model (no quality-floor
> regression — INT8 on a cross-encoder costs <1 nDCG point), 4× smaller,
> zero DIY-quantization risk (pre-quantized by the Transformers.js team),
> and `ort` is already a proven production dependency in the workspace
> (`gliner2_fastino`, `ort = "=2.0.0-rc.12"`, Windows CRT workaround
> already solved in `.cargo/config.toml`). Sections 4, 5.1, 5.4–5.6,
> 6.1–6.2, 7, 9 and the open questions in §10 are amended accordingly.

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
- ~~Quantized weights. Plain safetensors in v1.~~ **Amended:** v1 ships a **pre-quantized INT8 ONNX** model (§4). We do not *perform* quantization (no DIY GGML/ONNX quant pass) — we *consume* an already-quantized artifact published by a reputable source. Producing our own quantization (e.g. legal-domain calibration data) remains a follow-up.
- DIY quantization or candle GGUF loading. Candle's quantized path is mature for llama-family `QMatMul` but unproven for XLM-RoBERTa / BERT-family. We sidestep it entirely by using the ONNX runtime with a pre-quantized graph.
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

### 4.1 Format + runtime (amended 2026-05-17)

**Artifact: `onnx-community/bge-reranker-v2-m3-ONNX :: model_int8.onnx` (571 MB).**

The repo `onnx-community/bge-reranker-v2-m3-ONNX` (maintained by the
Transformers.js / Xenova team) publishes pre-quantized variants:

| Variant | Size | Decision |
|---|---|---|
| `model.onnx` + `model.onnx_data` (fp32) | 2.27 GB | Rejected — blows the RSS budget |
| `model_fp16.onnx` | 1.14 GB | Rejected — still too heavy alongside the bge-m3 embedder |
| `model_q4f16.onnx` | 702 MB | Backup if INT8 accuracy regresses in eval |
| **`model_int8.onnx`** (≡ `model_quantized.onnx` ≡ `model_uint8.onnx`) | **571 MB** | **Picked** |

INT8 dynamic quantization on a cross-encoder classification head is
well-behaved: the published evaluations and the general literature put
the nDCG/MRR delta vs fp32 at well under one point — negligible for the
quality goal, decisive for the 4× memory saving. `model_q4f16.onnx`
(702 MB) is the documented fallback if the §8 eval shows an INT8
regression on French legal.

**Runtime: `ort` (ONNX Runtime), not candle.** The original draft
proposed candle `XLMRobertaForSequenceClassification`. That path is
sound for fp32 safetensors but cannot consume a pre-quantized INT8
graph (candle's quantized kernels target llama-family `QMatMul`, not
BERT-family). `ort` runs the INT8 ONNX graph natively. It is already a
production workspace dependency — `ort = "=2.0.0-rc.12"` ([Cargo.toml:68])
used by `gliner2_fastino` ([crates/anno/src/backends/gliner2_fastino/sessions.rs]) —
and the Windows MSVC CRT-mismatch workaround it requires is already
solved in [.cargo/config.toml]. No new toolchain risk.

**Tokenization** stays in the `tokenizers` crate ([Cargo.toml:37]),
unchanged: `Tokenizer::encode_pair(query, passage, true)` produces the
XLM-RoBERTa special-token layout `<s> query </s></s> passage </s>`. The
ONNX graph's inputs are `input_ids` + `attention_mask` (RoBERTa-family;
no `token_type_ids` input in this exported graph — confirmed by the
smoke-load test, §11 commit 1). Output is logits `[batch, 1]`, sigmoid'd
to a relevance score in [0, 1].

No fallback / hand-rolled head needed.

## 5. Architecture

### 5.1 New module

`crates/anno-rag/src/rerank.rs` (single file in v1; split into a submodule if it grows):

```rust
use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use std::sync::Mutex;
use tokenizers::Tokenizer;

/// Loaded cross-encoder reranker.
///
/// Owns the ONNX session + tokenizer. Loaded once per process via
/// `Pipeline::reranker()` (lazy, `OnceCell`). Mirrors the `Embedder`
/// lifecycle at crates/anno-rag/src/embed.rs:20 and the ort session
/// pattern at crates/anno/src/backends/gliner2_fastino/sessions.rs:34.
pub struct Reranker {
    /// INT8 ONNX graph. `ort::session::Session::run` takes `&mut self`,
    /// so the session is behind a `Mutex` exactly like gliner2_fastino's
    /// `SessionSlot` — `score_pairs` takes `&self`, locks per batch.
    session: Mutex<ort::session::Session>,
    tokenizer: Tokenizer,
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
    /// Lazy-init the reranker. Downloads ~571 MB (INT8 ONNX) on first
    /// call; cached under `cfg.models_cache()` thereafter.
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

### 5.4 Model implementation (amended 2026-05-17 — ort/ONNX)

INT8 XLM-RoBERTa cross-encoder run through `ort::session::Session`. The
graph returns logits `[batch, 1]`; sigmoid'd to a relevance score in
[0, 1]. Input format is the standard cross-encoder layout:

```
<s> query </s></s> passage </s>
```

Tokenized via the `tokenizers` crate ([Cargo.toml:37]):
`Tokenizer::encode_pair(query, passage, true)` yields the XLM-RoBERTa
special-token wiring without hand-rolling ids.

The loader follows the **ort session pattern** at
[crates/anno/src/backends/gliner2_fastino/sessions.rs:34-48] combined
with the embedder's hub-fetch pattern at
[crates/anno-rag/src/embed.rs:33-60]:

- `hf_hub::api::tokio::Api` fetches `tokenizer.json` and the ONNX file
  named by `cfg.rerank_onnx_file` (default `onnx/model_int8.onnx`) from
  `cfg.rerank_model` (default `onnx-community/bge-reranker-v2-m3-ONNX`).
  Cached under `cfg.models_cache()` via hf-hub's own resolution, exactly
  like `Embedder::load`.
- Build the session: `ort::session::Session::builder()?` →
  `.with_optimization_level(GraphOptimizationLevel::Level3)?` →
  `.with_intra_threads(n)?` → `.commit_from_file(onnx_path)?`. (anno's
  `crates/anno` exposes a `hf_loader::create_onnx_session` helper but it
  lives in the `anno` crate, not `anno-rag`; v1 inlines the equivalent
  ~6-line builder in `rerank.rs` rather than taking a new `anno`
  dependency. Keeping `rerank.rs` dependency-light is a §7 boundary.)
- `Tokenizer::from_file(tokenizer_path)`.
- CPU execution provider (default; no provider feature flags).

Failures map to `Error::Rerank(String)` (new variant, §6 / §10.3).

### 5.5 Batching

`score_pairs` batches internally up to `max_batch_pairs` (config; default 8). For each batch:

1. Tokenize all pairs in the batch (sequential in v1 — `tokenizers` is already fast on Rust).
2. Pad to the batch's max length (right-pad; right-truncate the *passage* so total ≤ 512 after the query + special tokens).
3. Build `input_ids` and `attention_mask` as `ndarray` arrays shape `[batch, seq]` (i64), fed to `ort` via the `ndarray` feature already used by `gliner2_fastino` ([crates/anno/Cargo.toml:38]).
4. `session.lock().run(inputs)?` → logits `[batch, 1]` → sigmoid → `f32` score per row.
5. Append to results in input order.

Memory at peak: INT8 weights (~571 MB) + ort arena + `batch × 512 × hidden × 4 B ≈ 8 × 512 × 1024 × 4 = 16 MB` activations. Total reranker resident ≈ **~0.7 GB**, vs ~2.3 GB for the rejected fp32 path.

### 5.6 Performance budget (CPU)

Approximate, measured per the implementation plan but with these expectations going in:
- INT8 BGE-reranker-v2-m3 via `ort` on a modern x86 laptop (Zen 3+, AVX2/AVX-512 VNNI): **~20–80 ms per pair**, batched. INT8 is faster than the rejected fp32 path (VNNI dot-product), not just smaller.
- Pool of 30 candidates → **~0.6–2.5 s** end-to-end for the rerank stage.
- This is per **recall**, not per chunk read. MCP `search_reranked` calls are user-driven (one per Claude tool call), not high-QPS.

Documented latency floor: if the rerank stage adds more than ~4 s, that is a regression worth investigating, not the expected behaviour.

### 5.8 RSS budget reconciliation (amended 2026-05-17)

The master design carries a **< 1.5 GB peak-RSS** constraint
([docs/superpowers/specs/2026-05-12-anno-rag-design.md]). This spec
reconciles with it explicitly:

- **The 1.5 GB cap governs the default build** (`rerank` feature off).
  Nothing in the default path changes; the cap is untouched and remains
  CI-enforced as today.
- **`--features rerank` is an opt-in power feature** with a documented,
  higher memory envelope. With INT8 (~0.7 GB reranker resident) added
  to the bge-m3 embedder (~2 GB) + LanceDB + vault, rerank-on peak is
  **~3 GB** — down from the ~5 GB the rejected fp32 path implied. This
  is acceptable for a consciously-enabled feature on a 16 GB-class dev
  laptop and is documented in the README + user guide.
- A follow-up may shrink this further (embedder fp16 opt-in already
  exists at [embed.rs:64]; reranker `model_q4f16` is the documented
  fallback) but v1 does not depend on it.

This is a *scope clarification*, not a cap violation: the original
constraint was always about the shipped default, and the reranker was
always designed feature-gated and off by default (§6.1).

### 5.7 Score semantics

Cross-encoder score replaces the RRF score in `SearchHit::score` ([store.rs:176-179](crates/anno-rag/src/store.rs:176)) for the reranked path. Rationale:

- A consumer of `search_reranked` cares about the cross-encoder ordering; preserving the RRF value alongside would double the surface and create a "which one to sort by" footgun.
- The doc on `SearchHit::score` already says "higher = more relevant" without committing to the producer; this stays accurate.
- Audit log records `score_source: "cross_encoder"` vs `"rrf"` so historical comparisons remain possible (additive field on the existing audit record; see §6).

For the unreranked `search` path, scores remain RRF as today. The two paths produce comparable orderings but not comparable scalar values — callers must not mix them in the same downstream computation.

## 6. Configuration

### 6.1 Cargo feature

```toml
[dependencies]
ort = { workspace = true, features = ["ndarray"], optional = true }
ndarray = { workspace = true, optional = true }

[features]
default = []
rerank = ["dep:ort", "dep:ndarray"]   # public on/off switch
```

Unlike the original draft (which assumed candle and gated no
dependency), the amended runtime adds `ort` + `ndarray` as **optional**
deps activated only by the `rerank` feature, mirroring exactly how
`crates/anno` declares `ort = { workspace = true, features = ["ndarray"], optional = true }`
([crates/anno/Cargo.toml:38]). The workspace already pins
`ort = "=2.0.0-rc.12"` ([Cargo.toml:68]) and compiles it for
`gliner2_fastino`, so this introduces **no new workspace dependency and
no new build risk** — only a new opt-in edge for the `anno-rag` crate.

The flag `#[cfg(feature = "rerank")]`-gates the new module, the
`Pipeline::reranker` field, and `search_reranked` / `recall_memory_reranked`.
Default off so existing builds and `cargo install anno-rag` users do not
pay the ~571 MB model download on first run. Anno's heavy ML backends
already follow this pattern (`gliner2-fastino` features in
`crates/anno/Cargo.toml`).

### 6.2 Runtime config additions

In `AnnoRagConfig` ([crates/anno-rag/src/config.rs](crates/anno-rag/src/config.rs)):

```rust
pub struct AnnoRagConfig {
    // ... existing fields ...

    /// Reranker repo id on HuggingFace Hub.
    /// Default: "onnx-community/bge-reranker-v2-m3-ONNX".
    pub rerank_model: String,

    /// ONNX file within the repo. Default: "onnx/model_int8.onnx".
    /// Power users can point at "onnx/model_q4f16.onnx" (702 MB,
    /// documented INT8-accuracy fallback) or "onnx/model_fp16.onnx".
    pub rerank_onnx_file: String,

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

- `rerank.rs` depends on: `ort` (+ `ndarray`), `tokenizers`, `hf-hub`. `ort`/`ndarray` are the new optional deps (`rerank` feature); `tokenizers`/`hf-hub` are already in `Cargo.toml`.
- `rerank.rs` does **not** depend on the `anno` crate (no `hf_loader` reuse — the ~6-line ort session builder is inlined to keep the boundary clean), nor on `store.rs`, `pipeline.rs`, or `vault.rs`. It takes plain `&str` query + `&[&str]` passages and returns `Vec<f32>`. Composable in isolation.
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
| ONNX graph input/output names differ from assumed (`input_ids`/`attention_mask` → logits) | Low (Transformers.js export follows the standard convention) | §11 commit 1 is a smoke-load test that introspects `session.inputs` / `session.outputs` and asserts names + shapes before any wiring. If they differ, the names are read from the session, not hard-coded. |
| INT8 accuracy regression on French legal vs fp32 | Medium | §8 eval-baseline non-regression gate. Documented fallback: switch `rerank_onnx_file` to `model_q4f16.onnx` (702 MB) — config-only, no code change. |
| ~571 MB model download surprises users on first run | Medium | Feature-gated; opt-in. Document download size in README. Pre-flight warn + prompt in interactive terminals, silent + `tracing::info` in MCP/daemon (resolves §10.4). |
| CPU latency too slow for interactive Cowork use | Low–Medium | INT8+VNNI is faster than fp32; document expected 0.6–2.5 s; config knob shrinks pool_size. |
| Rehydration leaks entities into a long-lived cross-encoder cache | Low (no cache in v1) | Explicitly: no scoring cache. `score_pairs` is stateless. |
| Cross-encoder score (sigmoid'd logit) is incomparable to RRF score | Inherent | §5.7 documents this; audit field `score_source` records which is which. |
| Reranker disagrees with retrieval ranking and hides relevant chunks | Medium | `pool_size = 30` default gives the reranker a fat candidate pool; raise it if recall regresses in eval. |
| GPU users frustrated by CPU-only v1 | Low (anno's audience is CPU-first) | Document v0.2 GPU path matches embedder's; not in this spec's scope. |
| Recall regression on French legal vs RRF baseline | Medium | Eval gate: `bench_eval` (already exists at Cargo.toml:87) must show non-regression on the v0.2 baseline at [crates/anno-rag/tests/fixtures/eval_baseline.toml:6](crates/anno-rag/tests/fixtures/eval_baseline.toml:6). If it regresses, do not ship; investigate. |

## 10. Open questions — resolved (amended 2026-05-17)

All deferred decisions are now closed so the plan is unambiguous.

1. ~~**candle xlm_roberta?**~~ **Obsolete.** Runtime changed to `ort` +
   pre-quantized INT8 ONNX (§4.1). The equivalent confirmation step is
   the §11 commit-1 smoke-load test that introspects ONNX input/output
   names + shapes.
2. **`recall_memory_reranked` in v1?** **Yes — in scope.** The chunk
   path (`search_reranked`) and memory path (`recall_memory_reranked`)
   both ship in v1. The memory wrapper is a near-identical
   over-fetch → rehydrate → `score_pairs` → re-sort mirror; bundling
   keeps the MCP surface coherent (`rerank: bool` on both tools).
3. **`score_source` in the audit log?** **Additive field.** Add a
   `score_source: &'static str` (`"rrf"` | `"cross_encoder"`) to the
   existing search/recall audit record — no new row type. The plan
   reads `audit.rs` and threads the value from the path that produced
   the final ordering.
4. **Pre-flight download UX?** **Warn + prompt in interactive TTYs;
   silent `tracing::info` in MCP/daemon.** Detect via `IsTerminal` on
   stderr. Non-interactive contexts (MCP server, CI) never block on a
   prompt.
5. **Eval baseline?** **Add a separate `eval_baseline_reranked.toml`.**
   The reranked path produces a different (better, expected) ranking;
   reusing the RRF baseline would force either a perpetual "expected
   diff" or a baseline rewrite that loses the RRF regression guard.
   Two baselines, two gates: RRF path vs `eval_baseline.toml`, reranked
   path vs `eval_baseline_reranked.toml`. Reranked gate asserts
   **non-regression vs RRF** (nDCG@10 reranked ≥ nDCG@10 RRF) — if the
   reranker doesn't beat RRF on French legal, do not ship.
6. **Spec A composition.** Confirmed: rerank score is `SearchHit::score`;
   the budgeter is order-preserving and score-blind, so it needs no
   knowledge of `score_source`. No further action.

## 11. Implementation outline (not the plan)

Anticipated commits:

1. Smoke-load test (`rerank` feature, ignored-by-default): fetch
   `onnx/model_int8.onnx` + `tokenizer.json`, build the `ort` session,
   introspect and assert `session.inputs` / `session.outputs` names +
   shapes, run one forward pass on a dummy `encode_pair`. Confirms the
   ONNX I/O contract (§9 risk). No production code yet.
2. `rerank.rs` module: `Reranker::load` (hub-fetch + ort session
   builder, inlined) + `score_pairs` (tokenize → ndarray → run →
   sigmoid), with unit tests #1–#5. Add `Error::Rerank` variant.
3. `Pipeline::reranker` lazy-init (`OnceCell`) + `Pipeline::search_reranked`
   (over-fetch → rehydrate → score → re-sort → truncate), feature-gated.
4. Config additions (`rerank_model`, `rerank_onnx_file`,
   `rerank_pool_size`, `rerank_batch_size`); `Cargo.toml` optional
   `ort`/`ndarray` + `rerank` feature; CHANGELOG; README non-goal →
   opt-in + documented ~571 MB / ~3 GB-peak envelope.
5. Integration tests #6 (relevance) + #7 (privacy/rehydration boundary).
6. `recall_memory_reranked` mirror (§10.2 = yes).
7. MCP `rerank: bool` param on both search + recall tools; reject with a
   clear error when the feature is compiled out.
8. `score_source` audit field (§10.3); pre-flight download UX (§10.4);
   `bench_rerank` + `eval_baseline_reranked.toml` non-regression gate
   (§10.5).

Each commit compiles and tests pass green. Default Cargo build is unchanged because `rerank` is opt-in.
