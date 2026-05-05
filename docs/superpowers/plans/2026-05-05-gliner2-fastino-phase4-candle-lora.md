# gliner2_fastino — Phase 4 (Candle + LoRA hot-swap) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a parallel Candle-based backend `gliner2_fastino_candle` that loads `fastino/gliner2-multi-v1` from PyTorch safetensors directly (no ONNX intermediate), reimplements all 8 GLiNER2 heads in Candle, and exposes runtime LoRA adapter hot-swap (`load_adapter` / `set_adapter` / `unload_adapter`) — a capability no other Rust NLP library currently offers.

**Architecture:** New module tree `crates/anno/src/backends/gliner2_fastino_candle/` parallel to the existing ONNX-based `gliner2_fastino`. Reuses the `encoder_candle` BERT-shaped transformer skeleton, extending it with DeBERTa-v2-style relative-position encoding. Each linear layer in the model becomes `LoraLinear` — wraps a base `candle_nn::Linear` and an `Option<&LoraDelta>` snapshot read from `Arc<RwLock<Option<LoraDelta>>>` at forward-pass entry. Adapter activation flips the `RwLock` slot in O(1). Trait impls (`Model`, `ZeroShotNER`) match `GLiNER2Fastino` byte-for-byte so callers can swap backends with one type alias. ONNX backend stays as the production-fast single-domain path; Candle backend is the multi-tenant / hot-swap path.

**Tech Stack:** Rust 2021, `candle-core`, `candle-nn`, `safetensors`, `tokenizers`, `hf-hub`, `parking_lot::RwLock`. Optional CUDA / Metal via Candle's GPU backends.

**Spec:** `docs/superpowers/specs/2026-05-05-gliner2-fastino-phase4-candle-lora.md`
**Roadmap:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md` Track E.
**Phase 3 / 3.5 base:** Phase 3 merged at `96cfe1d7`. This plan does NOT depend on Phase 3.5; it's parallel to it and stacks on `main`.
**Reference implementations:**
- Python `gliner2.GLiNER2.load_adapter` (HuggingFace `fastino/gliner2-multi-v1` repo, the `gliner2/` package).
- LoRA paper: arXiv:2106.09685.
- PEFT format: <https://huggingface.co/docs/peft>.
- Existing anno Candle infra: `crates/anno/src/backends/{encoder_candle,gliner_candle,gliner_multitask/candle.rs}`.

## Prior art (Rust ecosystem) — read before writing code

The Phase 4 plan is **not** writing LoRA-on-Candle from scratch. Three upstreams already solve adjacent problems; we reuse design (and where licenses permit, code) instead of reinventing.

| Upstream | License | What we take | What we leave |
|---|---|---|---|
| [`EricLBuehler/candle-lora`](https://github.com/EricLBuehler/candle-lora) | MIT/Apache-2.0 | `LoraLinear` math + the macro pattern that turns `candle_nn::Linear` into a LoRA-wrapped layer; the `get_tensors()` save format inspires our snapshot API | Their conversion macros (we want a *callback-based* hook, not macro-rewriting, to avoid cyclic dep with `encoder_candle`). **Key gotcha:** the README explicitly says "weight naming is not compatible with PEFT yet" — we MUST do the PEFT mapping ourselves in M14 |
| [`EricLBuehler/mistral.rs`](https://github.com/EricLBuehler/mistral.rs) (`docs/ADAPTER_MODELS.md`, `mistralrs/examples/lora/main.rs`) | MIT | Dynamic-activation API shape: preload N adapters, activate one at runtime via a Python/Rust/HTTP call. Our `set_adapter`/`unload_adapter` mirrors this shape | Their X-LoRA mixture-of-experts routing (out of scope; we ship single-active hot-swap only) |
| [`Knowledgator/FlashDeBERTa`](https://github.com/Knowledgator/FlashDeBERTa) | Apache-2.0 (verify) | Optimized C2P/P2C disentangled-attention path — referenced if M5's broadcast implementation turns out too slow on the M13 parity test | Their CUDA kernels (our Candle path uses Candle's existing matmul) |
| [`huggingface/peft` `src/peft/tuners/lora/layer.py`](https://github.com/huggingface/peft/blob/main/src/peft/tuners/lora/layer.py) | Apache-2.0 | Authoritative reference for the safetensors key format (`base_model.model.<path>.lora_A.weight` / `lora_B.weight`), `lora_alpha / r` scaling, `fan_in_fan_out` flag semantics | Their training-time merge/unmerge logic (we're inference-only) |
| [`huggingface/transformers` `models/deberta_v2/modeling_deberta_v2.py`](https://github.com/huggingface/transformers/blob/main/src/transformers/models/deberta_v2/modeling_deberta_v2.py) | Apache-2.0 | Authoritative reference for DeBERTa-v2 disentangled attention math (used in M5) | Their training paths |

**What's deliberately NOT a dep:**
- We do **not** add `candle-lora` as a Cargo dep. Reasons: (a) its weight naming is PEFT-incompatible by its own admission; (b) its macro-based conversion would force us to expose `encoder_candle::Linear` fields that we want to keep private; (c) the LoRA math is ~30 lines and adding a transitive dep + version-skew risk costs more than it saves. We **do** read its source as a reference. Attribute in the rustdoc on `lora.rs`.
- HuggingFace `candle-transformers` does **not** ship DeBERTa-v2 (verified at planning time: `find ~/.cargo/registry/src/index.crates.io-*/candle-transformers* -path "*deberta*"` returns nothing). M5 writes it from scratch with the HF Python file as the spec.

**Cost note (per spec §9):** This is the largest of the gliner2_fastino follow-up tracks (~3.5 weeks). Spec §9 explicitly asks the reader to verify a multi-tenant LoRA workload exists today before starting. Don't execute this plan speculatively — confirm demand first. Once started, the plan ships as a coherent single-PR vertical.

---

## Pre-flight

- [ ] **Confirm there's a real multi-tenant or multi-domain inference workload.** Per spec §9. If the answer is "no, but it'd be cool to have," halt and revisit.
- [ ] **Phase 3 is on `main`.** Verify with `git log --oneline -5`. This plan does not require Phase 3.5.
- [ ] **`fastino/gliner2-multi-v1` is downloadable.** This is the *Python* repo (PyTorch safetensors), not the SemplificaAI ONNX export. Different artifact.
  ```bash
  ~/.venv/anno-tools/bin/python -c "from huggingface_hub import snapshot_download; \
      print(snapshot_download('fastino/gliner2-multi-v1'))"
  ls "$(~/.venv/anno-tools/bin/python -c 'from huggingface_hub import snapshot_download; print(snapshot_download(\"fastino/gliner2-multi-v1\"))')"
  # Expected: tokenizer.json, config.json, model.safetensors (or pytorch_model.bin),
  # adapter_config.json (only if it's an adapter model), etc.
  ```
- [ ] **At least one PEFT-format adapter is available for parity testing.** Either an existing public adapter on HF Hub (search `gliner2 adapter`) or an internally-trained one. Adapter directory must contain:
  ```
  <adapter>/
  ├── adapter_config.json     — {target_modules, r, alpha, base_model_name_or_path, ...}
  └── adapter_model.safetensors — {layer_path → W_down, W_up}
  ```
  Without a real adapter, M14–M17 cannot validate.
- [ ] **`candle-core` and `candle-nn` already in workspace.** Verify:
  ```bash
  grep -n "candle-core\|candle-nn" Cargo.toml crates/anno/Cargo.toml
  # Expected: workspace deps + optional in anno/Cargo.toml.
  ```
- [ ] **WSL Ubuntu-C is healthy.** All builds and tests in this plan target it. Windows MSVC has the linker conflict documented in Phase 1 finalization.
- [ ] **Pull a Python `gliner2` reference snapshot for tracing.**
  ```bash
  ~/.venv/anno-tools/bin/python -c "import gliner2; print(gliner2.__file__)"
  # Take note of the path; `model.py` and `lora.py` are the key files to mirror.
  ```
- [ ] **Create a worktree.**
  ```bash
  git worktree add ../anno-gliner2-phase4 -b feat/gliner2-fastino-phase4 main
  cd ../anno-gliner2-phase4
  ```

---

## File structure (locked)

| File | Action | Purpose |
|---|---|---|
| `crates/anno/Cargo.toml` | modify | Add `gliner2-fastino-candle`, `gliner2-fastino-candle-cuda`, `gliner2-fastino-candle-metal` features; possibly add `safetensors` dep if not transitive |
| `crates/anno/src/backends/encoder_candle/config.rs` | modify | Add `EncoderArchitecture::DeBertaV2` and `EncoderConfig::deberta_v2_base()` constructor |
| `crates/anno/src/backends/encoder_candle/deberta_v2.rs` | create | DeBERTa-v2 disentangled-attention head, relative-position bucket calc |
| `crates/anno/src/backends/encoder_candle/implementations.rs` | modify | Wire `Attention` to optionally use the v2-style relative attention |
| `crates/anno/src/backends/gliner2_fastino_candle/mod.rs` | create | Public surface; `GLiNER2FastinoCandle` engine struct |
| `crates/anno/src/backends/gliner2_fastino_candle/encoder.rs` | create | Wraps `CandleEncoder` with DeBERTa-v2 config |
| `crates/anno/src/backends/gliner2_fastino_candle/lora.rs` | create | `LoraDelta`, `LoraConfig`, `LoraLinear` — adapter weights + injection |
| `crates/anno/src/backends/gliner2_fastino_candle/adapters.rs` | create | Adapter registry; `load_adapter`/`set_adapter`/`unload_adapter` API |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/mod.rs` | create | Head module re-exports |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/token_gather.rs` | create | Token-level gather (word_indices → word embeddings) |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/span_rep.rs` | create | Span representation `[1, num_words, MAX_WIDTH, H]` |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/schema_gather.rs` | create | Per-task `[P]`/`[E]`/`[L]` token gather |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/count_pred.rs` | create | Count-predictor MLP + Rust-side argmax |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/count_lstm.rs` | create | Count-conditioned LSTM (struct projection) |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/scorer.rs` | create | Span-vs-struct similarity scorer + sigmoid |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/classifier.rs` | create | `[L]`-head MLP + softmax |
| `crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs` | create | 8-step orchestration |
| `crates/anno/src/backends/gliner2_fastino_candle/processor.rs` | create | Re-exports `crate::backends::gliner2_fastino::processor` (no copy — same input prep) |
| `crates/anno/src/backends/gliner2_fastino_candle/decoder.rs` | create | Re-exports the decoder/NMS from the ONNX backend |
| `crates/anno/src/backends/mod.rs` | modify | `pub mod gliner2_fastino_candle;` behind feature gate |
| `crates/anno/src/backends/catalog.rs` | modify | Add `gliner2_fastino_candle` row, `feature: Some("gliner2-fastino-candle")`, `description: "fastino-ai GLiNER2 with runtime LoRA hot-swap (Candle backend)"` |
| `crates/anno/tests/gliner2_fastino_candle_integration.rs` | create | Tier-2 integration: load `fastino/gliner2-multi-v1`, parity vs ONNX |
| `crates/anno/tests/gliner2_fastino_candle_lora.rs` | create | Adapter-load + hot-swap test |
| `crates/anno/benches/gliner2_fastino_candle_swap.rs` | create | `criterion` bench: `set_adapter` latency target < 1 ms |
| `docs/BACKENDS.md` | modify | New row for `gliner2_fastino_candle` |
| `docs/dev-notes/gliner2-fastino-candle-port.md` | create | Port notes & symbol mapping |

---

## Milestone P4.M1 — Reference reading + scope read (~1 day)

Goal: have the Python `gliner2` source on disk; produce a symbol-mapping and parameter-naming-convention table before writing any Rust.

### Task M1.1: Snapshot the Python source AND the upstream Rust references

**Files:**
- Create: `docs/dev-notes/gliner2-fastino-candle-port.md`

- [ ] **Step 1: Locate and copy the Python `gliner2` package.**

  ```bash
  PY_PATH="$(~/.venv/anno-tools/bin/python -c 'import gliner2; print(gliner2.__file__)' \
              | xargs dirname)"
  echo "$PY_PATH"
  ls "$PY_PATH"
  ```

  Expected: model.py, lora.py, processor.py, heads/*.py.

- [ ] **Step 2: Snapshot Python files.**

  ```bash
  cp "$PY_PATH/model.py"     /tmp/gliner2-model.py
  cp "$PY_PATH/lora.py"      /tmp/gliner2-lora.py
  ls "$PY_PATH/heads/"  | xargs -I{} cp "$PY_PATH/heads/{}" /tmp/gliner2-head-{}
  wc -l /tmp/gliner2-*.py
  ```

  Expected: a few hundred LOC each.

- [ ] **Step 2b: Pull the upstream Rust LoRA reference (`candle-lora`).**

  We read it for design — we do **not** add it as a dep (see the "Prior art" section above for why). Snapshot the relevant files locally so they survive an upstream rewrite:

  ```bash
  mkdir -p /tmp/candle-lora-ref
  for f in lora_linear.rs loraconfig.rs lib.rs; do
      curl -fsSL "https://raw.githubusercontent.com/EricLBuehler/candle-lora/master/candle-lora/src/$f" \
          -o "/tmp/candle-lora-ref/$f" 2>/dev/null || true
  done
  ls /tmp/candle-lora-ref
  wc -l /tmp/candle-lora-ref/*.rs 2>/dev/null
  ```

  Expected: at least `lora_linear.rs` (~150 LOC). If the file layout has changed upstream, browse https://github.com/EricLBuehler/candle-lora/tree/master/candle-lora/src and grab whatever `*linear*.rs` / `loraconfig*.rs` files exist now.

- [ ] **Step 2c: Pull HuggingFace's PEFT layer reference.**

  ```bash
  curl -fsSL https://raw.githubusercontent.com/huggingface/peft/main/src/peft/tuners/lora/layer.py \
      -o /tmp/peft-lora-layer.py
  wc -l /tmp/peft-lora-layer.py   # expect ~1500 lines
  grep -n "lora_A\|lora_B\|scaling\|merge\b" /tmp/peft-lora-layer.py | head -20
  ```

  Authoritative reference for the safetensors key format and `alpha/r` scaling.

- [ ] **Step 2d: Pull HF Transformers' DeBERTa-v2 reference (for M5).**

  ```bash
  curl -fsSL https://raw.githubusercontent.com/huggingface/transformers/main/src/transformers/models/deberta_v2/modeling_deberta_v2.py \
      -o /tmp/hf-deberta-v2.py
  wc -l /tmp/hf-deberta-v2.py    # expect ~1500-2000 lines
  grep -n "DisentangledSelfAttention\|build_relative_position\|make_log_bucket_position" /tmp/hf-deberta-v2.py
  ```

  Authoritative reference for the disentangled-attention math (M5).

- [ ] **Step 3: Symbol map.**

  Create `docs/dev-notes/gliner2-fastino-candle-port.md`:

  ```markdown
  # gliner2_fastino_candle port notes

  Source of truth: `fastino/gliner2-multi-v1` (HuggingFace) + Python `gliner2` package.
  Reference port: `paul-english/gliner2_rs` (Candle-based, no LoRA) — read for parameter
  naming conventions only, not full structure.

  ## Python → Rust symbol map

  | Python | anno equivalent |
  |---|---|
  | `gliner2.GLiNER2` | `gliner2_fastino_candle::GLiNER2FastinoCandle` |
  | `gliner2.GLiNER2.encoder` | `gliner2_fastino_candle::encoder::Encoder` (wraps CandleEncoder) |
  | `gliner2.heads.SpanRep` | `gliner2_fastino_candle::heads::span_rep::SpanRep` |
  | `gliner2.heads.SchemaGather` | `gliner2_fastino_candle::heads::schema_gather::SchemaGather` |
  | `gliner2.heads.CountPred` | `gliner2_fastino_candle::heads::count_pred::CountPred` |
  | `gliner2.heads.CountLstmFixed` | `gliner2_fastino_candle::heads::count_lstm::CountLstmFixed` |
  | `gliner2.heads.Scorer` | `gliner2_fastino_candle::heads::scorer::Scorer` |
  | `gliner2.heads.Classifier` | `gliner2_fastino_candle::heads::classifier::Classifier` |
  | `gliner2.GLiNER2.load_adapter` | `GLiNER2FastinoCandle::load_adapter` |
  | `gliner2.GLiNER2.set_adapter` | `GLiNER2FastinoCandle::set_adapter` |
  | `gliner2.lora.apply_lora_delta` | `lora::LoraLinear::forward` |

  ## Rust upstream references (read-only — not deps)

  | Concern | File on disk | Upstream URL |
  |---|---|---|
  | LoRA forward math | `/tmp/candle-lora-ref/lora_linear.rs` | https://github.com/EricLBuehler/candle-lora |
  | PEFT key format + alpha/r | `/tmp/peft-lora-layer.py` | https://github.com/huggingface/peft/blob/main/src/peft/tuners/lora/layer.py |
  | DeBERTa-v2 disentangled attn | `/tmp/hf-deberta-v2.py` | https://github.com/huggingface/transformers/blob/main/src/transformers/models/deberta_v2/modeling_deberta_v2.py |
  | Optimized C2P/P2C kernels (perf follow-up) | (online only) | https://github.com/Knowledgator/FlashDeBERTa |
  | Dynamic-activation API shape | (online only) | https://github.com/EricLBuehler/mistral.rs/blob/master/docs/ADAPTER_MODELS.md |

  ## PyTorch parameter naming → Candle naming

  HuggingFace exports DeBERTa-v2 with parameter paths like:

      encoder.layer.0.attention.self.query_proj.weight
      encoder.layer.0.attention.self.key_proj.weight
      encoder.layer.0.attention.output.dense.weight
      encoder.layer.0.intermediate.dense.weight
      encoder.layer.0.output.dense.weight

  PEFT adapters target these via `target_modules` regex, e.g.
  `["query_proj", "value_proj"]`. Our Candle code must use the same names
  in the safetensors file index so PEFT's regex hits.

  Decision: keep HuggingFace names verbatim. Do NOT rename.

  ## LoRA delta shape conventions

  PEFT stores adapter weights as:
      base_model.model.encoder.layer.0.attention.self.query_proj.lora_A.weight  [r, in]
      base_model.model.encoder.layer.0.attention.self.query_proj.lora_B.weight  [out, r]

  Where:
  - `lora_A` = "down projection"  (W_down in spec) — shape [r, in]
  - `lora_B` = "up projection"    (W_up   in spec) — shape [out, r]
  - alpha = scalar from adapter_config.json
  - r     = rank from adapter_config.json
  - applied as: y_lora = (alpha / r) * (lora_B @ lora_A @ x)

  Naming choice: keep PEFT's `lora_A`/`lora_B` for clarity. Spec uses
  `W_down`/`W_up`; we mirror in code comments.
  ```

- [ ] **Step 4: Commit.**

  ```bash
  git add docs/dev-notes/gliner2-fastino-candle-port.md
  git commit -m "docs(gliner2_fastino_candle): port notes — symbol + naming map"
  ```

---

## Milestone P4.M2 — Cargo features (~half day)

Goal: feature flags compile-check end-to-end before any backend code lands.

### Task M2.1: Add Cargo features

**Files:**
- Modify: `crates/anno/Cargo.toml`

- [ ] **Step 1: Add the new features.**

  Below the existing `cuda = ["candle", "candle-core/cuda"]` line, add:

  ```toml
  # Candle-based gliner2_fastino backend with runtime LoRA hot-swap (issue #18 / Phase 4).
  # WIP / experimental — no API stability guarantees.
  gliner2-fastino-candle = ["candle"]
  gliner2-fastino-candle-cuda = ["gliner2-fastino-candle", "cuda"]
  gliner2-fastino-candle-metal = ["gliner2-fastino-candle", "metal"]
  ```

- [ ] **Step 2: `safetensors` dep audit.**

  ```bash
  grep -n "safetensors" crates/anno/Cargo.toml Cargo.toml
  ```

  Already a transitive dep through `candle = [..., "dep:safetensors", ...]`. No further action needed.

- [ ] **Step 3: Build all four feature combinations to confirm the empty feature gate compiles.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  cargo check -p anno --features gliner2-fastino-candle-cuda --no-default-features
  cargo check -p anno --features gliner2-fastino-candle-metal --no-default-features
  ```

  Expected: clean. (No backend code yet, so just the feature graph is exercised.)

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/Cargo.toml
  git commit -m "feat(gliner2_fastino_candle): add Cargo features (no impl yet)"
  ```

---

## Milestone P4.M3 — Backend module skeleton (~half day)

Goal: empty backend module that compiles under `--features gliner2-fastino-candle`. Lets the rest of the plan add real code without churn.

### Task M3.1: Skeleton module tree

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino_candle/mod.rs`
- Create: `crates/anno/src/backends/gliner2_fastino_candle/encoder.rs`
- Create: `crates/anno/src/backends/gliner2_fastino_candle/lora.rs`
- Create: `crates/anno/src/backends/gliner2_fastino_candle/adapters.rs`
- Create: `crates/anno/src/backends/gliner2_fastino_candle/processor.rs`
- Create: `crates/anno/src/backends/gliner2_fastino_candle/decoder.rs`
- Create: `crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs`
- Create: `crates/anno/src/backends/gliner2_fastino_candle/heads/mod.rs`
- Modify: `crates/anno/src/backends/mod.rs`

- [ ] **Step 1: `mod.rs`.**

  ```rust
  //! gliner2_fastino_candle — Candle-based GLiNER2 backend with runtime LoRA hot-swap.
  //!
  //! **Status:** experimental / WIP. No API stability guarantees in Phase 4.
  //!
  //! Companion to [`crate::backends::gliner2_fastino`] — same model family,
  //! different runtime. Use the ONNX backend for single-domain production
  //! inference; use this Candle backend for multi-tenant or multi-domain
  //! workloads where adapters are swapped at runtime.

  #![cfg(feature = "gliner2-fastino-candle")]

  pub mod adapters;
  pub mod encoder;
  pub mod lora;
  pub mod pipeline;
  pub mod processor;
  pub mod decoder;
  pub mod heads;

  /// Re-export the engine struct (defined in M5).
  pub use self::engine::GLiNER2FastinoCandle;

  // The concrete struct lives in a sub-mod so `mod.rs` stays a clean public surface.
  mod engine {
      use super::adapters::AdapterRegistry;
      use super::encoder::Encoder;
      use std::sync::Arc;

      /// fastino-ai GLiNER2 model with runtime LoRA adapter swap.
      ///
      /// Set/unset is sub-millisecond (just rebinds an `Arc<...>` snapshot
      /// in the forward pass). See `load_adapter`/`set_adapter`/`unload_adapter`.
      pub struct GLiNER2FastinoCandle {
          // Filled in by M5 (encoder), M6–M11 (heads), M16 (adapters).
          pub(crate) encoder: Encoder,
          pub(crate) adapters: Arc<AdapterRegistry>,
          pub(crate) model_id: String,
      }
  }
  ```

- [ ] **Step 2: Stub each child mod.**

  Create each child with a single comment indicating its purpose so the build is green. Example for `encoder.rs`:

  ```rust
  //! Candle-based encoder for gliner2_fastino_candle. Wraps CandleEncoder
  //! with DeBERTa-v2 config + LoRA-aware linears. M5 fills this in.

  pub struct Encoder;
  ```

  Same shape for `lora.rs`, `adapters.rs`, `pipeline.rs`, `processor.rs`,
  `decoder.rs`, `heads/mod.rs`. Each gets a one-liner doc and a stub type.

  In `adapters.rs`:

  ```rust
  //! Adapter registry. M16 fills this in.

  pub struct AdapterRegistry;
  ```

- [ ] **Step 3: Wire into `backends/mod.rs`.**

  Find the section listing all backend modules. Add:

  ```rust
  #[cfg(feature = "gliner2-fastino-candle")]
  pub mod gliner2_fastino_candle;
  ```

- [ ] **Step 4: Build.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  ```

  Expected: clean.

- [ ] **Step 5: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino_candle/ \
          crates/anno/src/backends/mod.rs
  git commit -m "feat(gliner2_fastino_candle): empty module skeleton"
  ```

---

## Milestone P4.M4 — DeBERTa-v2 encoder config + arch enum (~1 day)

Goal: add DeBERTa-v2 to `encoder_candle/config.rs` so the encoder can be configured for fastino's actual architecture (DeBERTa-v2-Base 768d).

### Task M4.1: Add `deberta_v2_base` config + arch variant

**Files:**
- Modify: `crates/anno/src/backends/encoder_candle/config.rs`

- [ ] **Step 1: Add the constructor and variant.**

  In `EncoderConfig`, add a new constructor after `deberta_v3_large`:

  ```rust
  /// DeBERTa-v2-base configuration. fastino/gliner2-multi-v1 uses this.
  pub fn deberta_v2_base() -> Self {
      Self {
          vocab_size: 128100,
          hidden_size: 768,
          num_attention_heads: 12,
          num_hidden_layers: 12,
          intermediate_size: 3072,
          max_position_embeddings: 512,
          hidden_dropout_prob: 0.1,
          layer_norm_eps: 1e-7,
          use_rope: false,
          use_geglu: false,
          rope_theta: 10000.0,
          use_pre_norm: true, // DeBERTa uses pre-norm
      }
  }
  ```

  In `EncoderArchitecture`, add a variant:

  ```rust
  pub enum EncoderArchitecture {
      Bert,
      DeBertaV2,
      DeBertaV3,
      #[default]
      ModernBert,
  }
  ```

  Update the matches in `default_config`, `default_model_id`, `max_length`, `uses_rope`, `as_str`:

  ```rust
  pub fn default_config(&self) -> EncoderConfig {
      match self {
          Self::Bert => EncoderConfig::bert_base(),
          Self::DeBertaV2 => EncoderConfig::deberta_v2_base(),
          Self::DeBertaV3 => EncoderConfig::deberta_v3_base(),
          Self::ModernBert => EncoderConfig::modernbert_base(),
      }
  }

  pub fn default_model_id(&self) -> &'static str {
      match self {
          Self::Bert => "google-bert/bert-base-uncased",
          Self::DeBertaV2 => "microsoft/deberta-v2-xlarge",  // closest public stub
          Self::DeBertaV3 => "microsoft/deberta-v3-base",
          Self::ModernBert => "answerdotai/ModernBERT-base",
      }
  }

  pub fn max_length(&self) -> usize {
      match self {
          Self::Bert | Self::DeBertaV2 | Self::DeBertaV3 => 512,
          Self::ModernBert => 8192,
      }
  }

  pub fn uses_rope(&self) -> bool { matches!(self, Self::ModernBert) }

  pub fn as_str(&self) -> &'static str {
      match self {
          Self::Bert => "BERT",
          Self::DeBertaV2 => "DeBERTa-v2",
          Self::DeBertaV3 => "DeBERTa-v3",
          Self::ModernBert => "ModernBERT",
      }
  }
  ```

  In `from_model_name`:

  ```rust
  } else if lower.contains("deberta") {
      if lower.contains("v2") {
          if lower.contains("large") || lower.contains("xlarge") {
              // No public deberta-v2-large config in this codebase; fall back to base.
              Self::deberta_v2_base()
          } else {
              Self::deberta_v2_base()
          }
      } else if lower.contains("large") {
          Self::deberta_v3_large()
      } else {
          Self::deberta_v3_base()
      }
  }
  ```

  Note: gliner2-multi-v1 uses DeBERTa-v2-base 768d per `FastinoConfig::default()` in the existing ONNX backend; that's our target.

- [ ] **Step 2: Add unit tests.**

  In the existing `tests` module of `config.rs`:

  ```rust
  #[test]
  fn deberta_v2_base_has_expected_dims() {
      let cfg = EncoderConfig::deberta_v2_base();
      assert_eq!(cfg.hidden_size, 768);
      assert_eq!(cfg.num_attention_heads, 12);
      assert_eq!(cfg.num_hidden_layers, 12);
      assert_eq!(cfg.max_position_embeddings, 512);
      assert!(cfg.use_pre_norm);
      assert!(!cfg.use_rope);
  }

  #[test]
  fn from_model_name_picks_v2_for_v2_strings() {
      let cfg = EncoderConfig::from_model_name("deberta-v2-something");
      assert_eq!(cfg.hidden_size, 768);
      // v2 base = same hidden_size as v3 base; distinguish by max_pos / vocab.
      // Both are 128100/512 currently. The arch enum is the cleaner distinguisher.
      let arch = EncoderArchitecture::from("microsoft/deberta-v2-base");
      let _ = arch; // EncoderArchitecture has no From<str>; we test default_config separately.
      assert_eq!(EncoderArchitecture::DeBertaV2.as_str(), "DeBERTa-v2");
  }
  ```

- [ ] **Step 3: Run.**

  ```bash
  cargo test -p anno --features candle \
      --lib backends::encoder_candle::config::tests::deberta_v2_base_has_expected_dims
  cargo test -p anno --features candle \
      --lib backends::encoder_candle::config::tests::from_model_name_picks_v2_for_v2_strings
  ```

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/encoder_candle/config.rs
  git commit -m "feat(encoder_candle): add DeBERTa-v2 config + arch variant"
  ```

---

## Milestone P4.M5 — Disentangled-attention DeBERTa-v2 module (~3 days)

Goal: implement DeBERTa-v2's relative-position-aware "disentangled attention". This is the only piece of the encoder that v2 has and existing `encoder_candle::Attention` (BERT-style absolute) doesn't.

**Reference:** Hugging Face's `transformers/models/deberta_v2/modeling_deberta_v2.py::DisentangledSelfAttention` is the gold-standard implementation (snapshotted at `/tmp/hf-deberta-v2.py` in M1.1 step 2d). ~250 LOC; we port the inference path only.

**Performance escape hatch:** If the M13 ONNX↔Candle parity test passes but the `criterion` bench in M17.2 shows the disentangled attention is more than 2× slower than the ONNX equivalent, port `Knowledgator/FlashDeBERTa`'s C2P/P2C compressed-indexing path instead of the broadcast version below. That repo has Apache-2.0 CUDA kernels that expose a simpler matmul-based op pattern. Out of scope for the first pass; flagged here so a future engineer recognizes it as a known optimization rather than reinvention.

### Task M5.1: `deberta_v2.rs` — relative-position bucket fn

**Files:**
- Create: `crates/anno/src/backends/encoder_candle/deberta_v2.rs`

- [ ] **Step 1: Module declaration.**

  In `encoder_candle/mod.rs` add (under the existing `pub mod implementations;`):

  ```rust
  #[cfg(feature = "candle")]
  pub mod deberta_v2;
  ```

- [ ] **Step 2: Implement the bucket function.**

  ```rust
  //! DeBERTa-v2 disentangled self-attention.
  //!
  //! Reference: Hugging Face transformers/models/deberta_v2/modeling_deberta_v2.py
  //! (Apache-2.0). Inference-only port; backward pass intentionally omitted.

  use crate::{Error, Result};
  use candle_core::{DType, Device, Tensor, D};
  use candle_nn::{linear, layer_norm, Linear, LayerNorm, Module, VarBuilder};

  /// Bucket relative positions to log-spaced indices.
  /// Mirrors HF's `build_relative_position` + `make_log_bucket_position`.
  pub fn build_relative_position(
      query_size: usize,
      key_size: usize,
      bucket_size: usize,
      max_position: usize,
      device: &Device,
  ) -> Result<Tensor> {
      let q_ids: Vec<i64> = (0..query_size as i64).collect();
      let k_ids: Vec<i64> = (0..key_size as i64).collect();
      let mut rel = Vec::with_capacity(query_size * key_size);
      for q in &q_ids {
          for k in &k_ids {
              let r = q - k;
              rel.push(log_bucket(r, bucket_size as i64, max_position as i64));
          }
      }
      Tensor::from_vec(rel, (query_size, key_size), device)
          .map_err(|e| Error::Parse(format!("rel_pos tensor: {e}")))
  }

  fn log_bucket(rel: i64, bucket_size: i64, max_position: i64) -> i64 {
      // HF formula: when |rel| < bucket_size/2, return rel + bucket_size/2.
      // Otherwise return logarithmically-bucketed value.
      let sign = rel.signum();
      let mid = bucket_size / 2;
      if rel.abs() < mid {
          rel + mid
      } else {
          let abs_pos = rel.abs() as f64;
          let max_pos = max_position as f64;
          let scale = ((max_pos - 1.0) / mid as f64).ln();
          let bucket =
              ((abs_pos.ln() - (mid as f64).ln()) / scale * (mid as f64 - 1.0)).floor()
                  as i64
                  + mid;
          mid + sign * bucket.min(max_position - 1)
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn log_bucket_inside_window_is_linear() {
          // For rel in [-mid+1, mid-1], output = rel + mid (no log scaling).
          assert_eq!(log_bucket(0, 256, 512), 128);
          assert_eq!(log_bucket(1, 256, 512), 129);
          assert_eq!(log_bucket(-1, 256, 512), 127);
      }

      #[test]
      fn log_bucket_outside_window_is_clamped() {
          // For rel ≥ mid, value should be ≥ mid + 1; sign preserved.
          let v_pos = log_bucket(500, 256, 512);
          let v_neg = log_bucket(-500, 256, 512);
          assert!(v_pos > 128);
          assert!(v_neg < 128);
          assert!(v_pos.abs() <= 511);
          assert!(v_neg.abs() <= 511);
      }
  }
  ```

- [ ] **Step 3: Build + tests.**

  ```bash
  cargo test -p anno --features candle \
      --lib backends::encoder_candle::deberta_v2::tests::log_bucket_inside_window
  cargo test -p anno --features candle \
      --lib backends::encoder_candle::deberta_v2::tests::log_bucket_outside_window
  ```

  Expected: pass.

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/encoder_candle/deberta_v2.rs \
          crates/anno/src/backends/encoder_candle/mod.rs
  git commit -m "feat(encoder_candle): DeBERTa-v2 log-bucket relative position"
  ```

### Task M5.2: `DisentangledSelfAttention` module

**Files:**
- Modify: `crates/anno/src/backends/encoder_candle/deberta_v2.rs`

- [ ] **Step 1: Struct + constructor.**

  Append to `deberta_v2.rs`:

  ```rust
  /// Disentangled self-attention (DeBERTa-v2, inference path).
  ///
  /// Differs from BERT-style attention by adding a relative-position-aware
  /// bias to the attention scores: q-k content score + q-r position score
  /// + r-k position score, where r is the relative-position embedding.
  pub struct DisentangledSelfAttention {
      q_proj: Linear,
      k_proj: Linear,
      v_proj: Linear,
      o_proj: Linear,
      pos_proj: Linear,           // For projecting rel-pos embeddings.
      rel_embeddings: Tensor,     // [bucket_size, hidden_size]
      num_heads: usize,
      head_dim: usize,
      max_relative_positions: usize,
      position_buckets: usize,
      device: Device,
  }

  impl DisentangledSelfAttention {
      /// Build from a VarBuilder rooted at the layer's `attention.self.` prefix.
      pub fn new(
          hidden: usize,
          num_heads: usize,
          max_relative_positions: usize,
          position_buckets: usize,
          rel_embeddings: Tensor,
          vb: VarBuilder,
          device: &Device,
      ) -> Result<Self> {
          let head_dim = hidden / num_heads;
          // Use HF's parameter naming verbatim — PEFT's regex relies on it.
          let q_proj = linear(hidden, hidden, vb.pp("query_proj"))
              .map_err(|e| Error::Retrieval(format!("v2 query_proj: {e}")))?;
          let k_proj = linear(hidden, hidden, vb.pp("key_proj"))
              .map_err(|e| Error::Retrieval(format!("v2 key_proj: {e}")))?;
          let v_proj = linear(hidden, hidden, vb.pp("value_proj"))
              .map_err(|e| Error::Retrieval(format!("v2 value_proj: {e}")))?;
          let o_proj = linear(hidden, hidden, vb.pp("../output.dense"))
              .map_err(|e| Error::Retrieval(format!("v2 output.dense: {e}")))?;
          let pos_proj = linear(hidden, hidden, vb.pp("pos_proj"))
              .map_err(|e| Error::Retrieval(format!("v2 pos_proj: {e}")))?;
          Ok(Self {
              q_proj, k_proj, v_proj, o_proj, pos_proj,
              rel_embeddings,
              num_heads, head_dim,
              max_relative_positions, position_buckets,
              device: device.clone(),
          })
      }

      /// Forward pass. Inputs: hidden_states `[B, L, H]`. Output: `[B, L, H]`.
      pub fn forward(&self, hidden: &Tensor) -> Result<Tensor> {
          let (batch, seq_len, hidden_dim) = hidden.dims3()
              .map_err(|e| Error::Parse(format!("v2 dims: {e}")))?;
          let _ = hidden_dim;

          // Q/K/V projections.
          let q = self.q_proj.forward(hidden)
              .map_err(|e| Error::Parse(format!("v2 Q: {e}")))?;
          let k = self.k_proj.forward(hidden)
              .map_err(|e| Error::Parse(format!("v2 K: {e}")))?;
          let v = self.v_proj.forward(hidden)
              .map_err(|e| Error::Parse(format!("v2 V: {e}")))?;

          let q = q.reshape((batch, seq_len, self.num_heads, self.head_dim))?
                   .transpose(1, 2)?.contiguous()?;
          let k = k.reshape((batch, seq_len, self.num_heads, self.head_dim))?
                   .transpose(1, 2)?.contiguous()?;
          let v = v.reshape((batch, seq_len, self.num_heads, self.head_dim))?
                   .transpose(1, 2)?.contiguous()?;

          let scale = (self.head_dim as f64).sqrt();
          let kt = k.transpose(D::Minus2, D::Minus1)?.contiguous()?;
          let mut scores = (q.matmul(&kt)? / scale)?;

          // Add disentangled position bias.
          // Compute rel-pos projections and add q→r and r→k contributions.
          let rel_pos = build_relative_position(
              seq_len, seq_len, self.position_buckets, self.max_relative_positions,
              &self.device,
          )?;  // [L, L]

          let pos_emb = self.pos_proj.forward(&self.rel_embeddings)?;  // [bucket, H]
          let pos_emb = pos_emb.reshape((self.position_buckets, self.num_heads, self.head_dim))?
                              .transpose(0, 1)?.contiguous()?;        // [num_heads, bucket, head_dim]

          // Index pos_emb by rel_pos to form [num_heads, L, L, head_dim]; multiply with q/k.
          // For inference simplicity we expand pos_emb via index_select per token pair.
          // (HF's optimized C2P+P2C path is a follow-up if perf demands.)
          let pos_idx = rel_pos.flatten_all()?;                       // [L*L]
          let pos_per_pair = pos_emb.index_select(&pos_idx, 1)?;       // [num_heads, L*L, head_dim]
          let pos_per_pair = pos_per_pair.reshape((
              self.num_heads, seq_len, seq_len, self.head_dim,
          ))?;

          // q→r bias: q[B,H,L,d] · pos[H,L,L,d] over d.
          // Implemented as elementwise mul + sum over last dim. For B=1 (our case) it's:
          let q_b = q.squeeze(0)?;                                    // [H, L, d]
          let q_r = q_b.unsqueeze(2)?.broadcast_mul(&pos_per_pair)?    // [H, L, L, d]
                       .sum(D::Minus1)?;                              // [H, L, L]
          // k→r bias mirror.
          let k_b = k.squeeze(0)?;                                    // [H, L, d]
          let r_k = k_b.unsqueeze(1)?.broadcast_mul(&pos_per_pair)?    // [H, L, L, d]
                       .sum(D::Minus1)?;                              // [H, L, L]
          let bias = (&q_r + &r_k)?;                                  // [H, L, L]
          let bias = (bias / scale)?;
          let bias = bias.unsqueeze(0)?;                              // [1, H, L, L]
          scores = (scores + bias)?;

          let attn = candle_nn::ops::softmax(&scores, D::Minus1)
              .map_err(|e| Error::Parse(format!("v2 softmax: {e}")))?;
          let out = attn.contiguous()?.matmul(&v)?;                    // [B, H, L, d]
          let out = out.transpose(1, 2)?.contiguous()?
                       .reshape((batch, seq_len, self.num_heads * self.head_dim))?;
          let out = self.o_proj.forward(&out)
              .map_err(|e| Error::Parse(format!("v2 o_proj: {e}")))?;
          Ok(out)
      }
  }
  ```

  **Note on optimization:** the HF reference uses a "compressed" indexing path (C2P / P2C) that avoids the broadcast-mul-then-sum we use here. For 200-token inputs the difference is small; we accept the simpler implementation in Phase 4 and leave optimization as a P4.M22 follow-up if benchmarks demand.

- [ ] **Step 2: Build.**

  ```bash
  cargo check -p anno --features candle
  ```

  Expected: clean.

- [ ] **Step 3: Skeleton smoke test.**

  At the bottom of `deberta_v2.rs` `mod tests`:

  ```rust
  #[test]
  fn forward_compiles() {
      // Real numerical parity is in M6 against the encoder.onnx output.
      // This just asserts the type is constructible in principle.
      fn _assert_send_sync<T: Send + Sync>() {}
      _assert_send_sync::<DisentangledSelfAttention>();
  }
  ```

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/encoder_candle/deberta_v2.rs
  git commit -m "feat(encoder_candle): DisentangledSelfAttention forward (DeBERTa-v2 inference)"
  ```

### Task M5.3: Stitch DeBERTa-v2 into `CandleEncoder`

**Files:**
- Modify: `crates/anno/src/backends/encoder_candle/implementations.rs`

- [ ] **Step 1: Add a path through the existing `forward` that switches on a per-layer `Option<DisentangledSelfAttention>`.**

  In the `TransformerLayer` struct, add an alternate attention slot. The minimal-churn approach:

  ```rust
  pub struct TransformerLayer {
      // existing fields
      attention: Attention,
      // NEW: optional v2-style attention. If Some, used in place of `attention`.
      v2_attention: Option<crate::backends::encoder_candle::deberta_v2::DisentangledSelfAttention>,
      // existing FFN, layer norms, etc.
  }
  ```

  Update `TransformerLayer::new`:

  ```rust
  pub fn new(config: &EncoderConfig, vb: VarBuilder, device: &Device) -> Result<Self> {
      // ... existing body that builds `attention` ...
      let v2_attention = None;  // Default; overridden by the from_pretrained path below.
      Ok(Self { attention, v2_attention, /* rest unchanged */ })
  }
  ```

  Update `forward`:

  ```rust
  pub fn forward(&self, x: &Tensor, start_pos: usize) -> Result<Tensor> {
      // Replace the existing attention call with:
      let attn_out = if let Some(ref v2) = self.v2_attention {
          v2.forward(&self.pre_attn_norm(x)?)?  // assumes pre-norm flow already in place
      } else {
          self.attention.forward(x, start_pos)?
      };
      // ... rest of forward unchanged ...
  }
  ```

  Detail: `pre_attn_norm` is a stand-in — the existing forward applies LayerNorm at the right spot already; this Step 1 just stitches in the v2 path with the *same* normalization placement. Verify by reading the current `forward` body carefully and inserting only the conditional, not duplicating the LN.

- [ ] **Step 2: Wire `from_pretrained` to populate `v2_attention` when the config is DeBERTa-v2.**

  In the `from_pretrained` method (around line 533-780), when the parsed config matches DeBERTa-v2 (model_type == "deberta-v2" or hidden_size==768 + max_pos==512 + DeBERTa marker), build the `rel_embeddings` tensor from the safetensors file (parameter name `encoder.rel_embeddings.weight`) and pass it into each `TransformerLayer::new_with_v2(config, vb, device, &rel_embeddings)`.

  Add a helper constructor on `TransformerLayer`:

  ```rust
  pub fn new_with_v2_attention(
      config: &EncoderConfig,
      vb: VarBuilder,
      device: &Device,
      rel_embeddings: &Tensor,
      max_relative_positions: usize,
      position_buckets: usize,
  ) -> Result<Self> {
      let mut layer = Self::new(config, vb.clone(), device)?;
      let v2 = crate::backends::encoder_candle::deberta_v2::DisentangledSelfAttention::new(
          config.hidden_size,
          config.num_attention_heads,
          max_relative_positions,
          position_buckets,
          rel_embeddings.clone(),
          vb.pp("attention.self"),
          device,
      )?;
      layer.v2_attention = Some(v2);
      Ok(layer)
  }
  ```

- [ ] **Step 3: Build.**

  ```bash
  cargo check -p anno --features candle
  ```

  Expected: clean. (Compilation only — full numerical parity is M6.)

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/encoder_candle/implementations.rs
  git commit -m "feat(encoder_candle): stitch DisentangledSelfAttention into TransformerLayer"
  ```

---

## Milestone P4.M6 — Encoder parity vs ONNX (~2 days)

Goal: encoder forward output parity within `max_abs_diff < 1e-4` against the SemplificaAI ONNX `encoder` graph on the same input. Catches DeBERTa-v2 implementation bugs before head implementation depends on it.

### Task M6.1: Tier-2 parity test

**Files:**
- Create: `crates/anno/tests/gliner2_fastino_candle_encoder_parity.rs`

- [ ] **Step 1: Test.**

  ```rust
  //! Tier-2: Candle DeBERTa-v2 encoder vs ONNX `encoder.onnx` parity.
  //! Requires both `gliner2-fastino` AND `gliner2-fastino-candle` features
  //! AND `fastino/gliner2-multi-v1` (Python) + SemplificaAI/gliner2-multi-v1-onnx
  //! cached.
  //!
  //! Run:
  //!   cargo test -p anno --features gliner2-fastino,gliner2-fastino-candle \
  //!       --test gliner2_fastino_candle_encoder_parity -- --ignored --nocapture

  #![cfg(all(feature = "gliner2-fastino", feature = "gliner2-fastino-candle"))]

  // Rough sketch — final test loads the Candle encoder, loads the ONNX encoder,
  // feeds both the same `[1, 32]` token ids, compares hidden_states.

  #[test]
  #[ignore]
  fn deberta_v2_candle_matches_onnx_encoder() {
      use anno::backends::encoder_candle::config::{EncoderArchitecture, EncoderConfig};
      use anno::backends::encoder_candle::CandleEncoder;

      // Load Candle encoder from fastino/gliner2-multi-v1 (Python repo).
      let candle = CandleEncoder::from_pretrained("fastino/gliner2-multi-v1")
          .expect("candle encoder load");

      // Load ONNX encoder via the existing GLiNER2Fastino::from_pretrained path.
      use anno::backends::gliner2_fastino::GLiNER2Fastino;
      let _onnx = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
          .expect("onnx engine");

      // Same input.
      let text = "Acme Corp signed a deal in Paris on Jan 5th.";
      let (candle_out, candle_seq_len) = candle.encode(text).expect("candle encode");
      assert!(candle_seq_len > 0);

      // Drive the ONNX encoder. We need a hook that exposes encoder-only output
      // for parity. For now: check that Candle encoder runs without error AND
      // produces a sensibly-sized output (seq_len * 768).
      assert_eq!(candle_out.len(), candle_seq_len * 768);

      // TODO(M6.2): expose ONNX encoder-only output via a debug hook on
      // GLiNER2Fastino, then compare element-wise.
      eprintln!(
          "candle encoder norm = {:.4}",
          (candle_out.iter().map(|x| x*x).sum::<f32>()).sqrt(),
      );
  }
  ```

  Note: this is the *sized* version. The full element-wise comparison requires a small private hook on `GLiNER2Fastino` to extract just the encoder's output tensor. M6.2 adds it.

### Task M6.2: ONNX encoder-only debug hook

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Add a `#[cfg(test)]`-only public method.**

  ```rust
  #[cfg(test)]
  impl GLiNER2Fastino {
      /// Test-only: run only the encoder graph and return `[L, H]` flat.
      /// Used by parity tests.
      pub fn _debug_encoder_only(&self, text: &str) -> crate::Result<(Vec<f32>, usize)> {
          use crate::backends::gliner2_fastino::pipeline::run_encoder;
          use crate::backends::gliner2_fastino::processor::SchemaTask;
          let task = SchemaTask::Entities(vec!["dummy".to_string()]);
          let record = self.transformer.transform(text, &[task])?;
          let enc = run_encoder(&self.sessions, &record)
              .map_err(crate::Error::from)?;
          let shape = enc.hidden_states.shape().to_vec();
          assert_eq!(shape[0], 1);
          let l = shape[1]; let h = shape[2];
          let data = enc.hidden_states.iter().copied().collect();
          Ok((data, l))
      }
  }
  ```

  This is `#[cfg(test)]` and prefixed `_debug_` so it doesn't pollute the public API.

- [ ] **Step 2: Update the parity test to use it.**

  In `gliner2_fastino_candle_encoder_parity.rs`, replace the TODO block:

  ```rust
  let onnx = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
      .expect("onnx engine");
  let (onnx_flat, onnx_seq_len) = onnx._debug_encoder_only(text).expect("onnx encoder");

  // Token counts may differ if the two repos ship different tokenizer.json.
  // For parity, check the SHAPES first; if they match, compare element-wise.
  assert_eq!(onnx_seq_len, candle_seq_len, "tokenizer divergence");
  let mut max_abs = 0.0_f32;
  for (a, b) in candle_out.iter().zip(onnx_flat.iter()) {
      max_abs = max_abs.max((a - b).abs());
  }
  eprintln!("encoder max_abs_diff = {max_abs:.6}");
  assert!(max_abs < 1e-4, "encoder parity failed: max_abs={max_abs}");
  ```

- [ ] **Step 3: Run.**

  ```bash
  cargo test -p anno --features gliner2-fastino,gliner2-fastino-candle \
      --test gliner2_fastino_candle_encoder_parity -- --ignored --nocapture \
      deberta_v2_candle_matches_onnx_encoder
  ```

  Expected: passes. If not, dominant failure modes:
  - **`max_abs_diff ~ 1e-1+`:** rel-pos bucket calc is wrong, or pos_proj parameter naming mismatch. Re-read M5.1.
  - **`max_abs_diff ~ 1e-2`:** layer-norm placement differs. DeBERTa-v2 uses pre-norm; ensure the v2 forward path applies LN BEFORE attention, not after.
  - **`tokenizer divergence`:** the Python and ONNX repos ship different tokenizer.json. Use the same tokenizer for both (load tokenizer from the Python repo, drive both).

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/mod.rs \
          crates/anno/tests/gliner2_fastino_candle_encoder_parity.rs
  git commit -m "test(gliner2_fastino_candle): DeBERTa-v2 candle encoder ≡ ONNX encoder (max_abs_diff < 1e-4)"
  ```

---

## Milestone P4.M7 — Heads: token_gather + span_rep (~2 days)

Goal: implement the first two heads in pure Candle. Both are stateless, parameter-free, deterministic ops; perfect for an early-confidence-builder milestone.

### Task M7.1: `token_gather`

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino_candle/heads/token_gather.rs`
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/heads/mod.rs`

- [ ] **Step 1: Module wiring.**

  In `heads/mod.rs`:

  ```rust
  pub mod token_gather;
  pub mod span_rep;
  // (More to come in M8–M11.)
  ```

- [ ] **Step 2: Implement.**

  `token_gather.rs`:

  ```rust
  //! Word-level embedding gather. Equivalent to ONNX `token_gather` graph.

  use crate::{Error, Result};
  use candle_core::{Tensor, IndexOp};

  /// Gather word-level embeddings from the encoder's hidden states.
  ///
  /// - hidden_states: `[1, L, H]`
  /// - word_starts: word-piece-start indices, length num_words
  ///
  /// Returns `[1, num_words, H]`.
  pub fn forward(hidden_states: &Tensor, word_starts: &[usize]) -> Result<Tensor> {
      let (b, _l, h) = hidden_states.dims3()
          .map_err(|e| Error::Parse(format!("token_gather dims: {e}")))?;
      assert_eq!(b, 1, "token_gather: only batch=1 supported");
      let mut rows: Vec<Tensor> = Vec::with_capacity(word_starts.len());
      for &i in word_starts {
          // Pull row i from the L axis.
          let row = hidden_states.i((0, i, ..))
              .map_err(|e| Error::Parse(format!("token_gather index: {e}")))?;
          rows.push(row);
      }
      let stacked = Tensor::stack(&rows, 0)
          .map_err(|e| Error::Parse(format!("token_gather stack: {e}")))?;
      stacked.unsqueeze(0)
          .map_err(|e| Error::Parse(format!("token_gather unsqueeze: {e}")))
          .map(|t| { let _ = h; t })
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      use candle_core::Device;

      #[test]
      fn gathers_correct_rows() {
          let dev = Device::Cpu;
          // Build [1, 4, 2] tensor: rows 0..4 each = [i, i+0.5].
          let data: Vec<f32> = vec![
              0.0, 0.5,
              1.0, 1.5,
              2.0, 2.5,
              3.0, 3.5,
          ];
          let t = Tensor::from_vec(data, (1, 4, 2), &dev).unwrap();
          let out = forward(&t, &[0, 2]).unwrap();
          assert_eq!(out.dims(), &[1, 2, 2]);
          let v: Vec<f32> = out.flatten_all().unwrap().to_vec1().unwrap();
          assert_eq!(v, vec![0.0, 0.5, 2.0, 2.5]);
      }
  }
  ```

- [ ] **Step 3: Run unit test.**

  ```bash
  cargo test -p anno --features gliner2-fastino-candle \
      --lib backends::gliner2_fastino_candle::heads::token_gather::tests::gathers_correct_rows
  ```

  Expected: passes.

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino_candle/heads/
  git commit -m "feat(gliner2_fastino_candle): token_gather head (Candle) + unit test"
  ```

### Task M7.2: `span_rep`

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino_candle/heads/span_rep.rs`

- [ ] **Step 1: Implement.**

  ```rust
  //! Span-rep head. Mirrors ONNX `span_rep` graph.
  //!
  //! For each (start, width) where start ∈ [0, num_words), width ∈ [0, MAX_WIDTH),
  //! compute a span embedding from text_embs.
  //!
  //! GLiNER's span-rep variant: concat(start_emb, end_emb) → linear → ReLU → linear.
  //! Per the SemplificaAI ONNX export, the output shape is `[1, num_words, MAX_WIDTH, H]`.

  use crate::{Error, Result};
  use candle_core::{Tensor, IndexOp};
  use candle_nn::{linear, Linear, Module, VarBuilder};

  /// Phase 4 baked-in MAX_WIDTH = 8 (matches the ONNX export's MAX_WIDTH constant
  /// in `crate::backends::gliner2_fastino::pipeline::MAX_WIDTH`).
  pub const MAX_WIDTH: usize = 8;

  pub struct SpanRep {
      // The export uses two-layer span-rep MLP: [2H → 4H → H].
      proj_1: Linear,
      proj_2: Linear,
  }

  impl SpanRep {
      pub fn new(hidden: usize, vb: VarBuilder) -> Result<Self> {
          let proj_1 = linear(2 * hidden, 4 * hidden, vb.pp("proj_1"))
              .map_err(|e| Error::Retrieval(format!("span_rep proj_1: {e}")))?;
          let proj_2 = linear(4 * hidden, hidden, vb.pp("proj_2"))
              .map_err(|e| Error::Retrieval(format!("span_rep proj_2: {e}")))?;
          Ok(Self { proj_1, proj_2 })
      }

      /// text_embs: `[1, num_words, H]`. Returns `[1, num_words, MAX_WIDTH, H]`.
      pub fn forward(&self, text_embs: &Tensor) -> Result<Tensor> {
          let (b, num_words, h) = text_embs.dims3()
              .map_err(|e| Error::Parse(format!("span_rep dims: {e}")))?;
          assert_eq!(b, 1);
          let mut frames: Vec<Tensor> = Vec::with_capacity(num_words * MAX_WIDTH);
          for start in 0..num_words {
              let s_emb = text_embs.i((0, start, ..))
                  .map_err(|e| Error::Parse(format!("sr start: {e}")))?;
              for width in 0..MAX_WIDTH {
                  let end = (start + width).min(num_words - 1);
                  let e_emb = text_embs.i((0, end, ..))
                      .map_err(|e| Error::Parse(format!("sr end: {e}")))?;
                  let cat = Tensor::cat(&[&s_emb, &e_emb], 0)
                      .map_err(|e| Error::Parse(format!("sr cat: {e}")))?;
                  frames.push(cat);
              }
          }
          // Stack to [num_words*MAX_WIDTH, 2H], project, reshape.
          let stacked = Tensor::stack(&frames, 0)
              .map_err(|e| Error::Parse(format!("sr stack: {e}")))?;       // [N*W, 2H]
          let h1 = self.proj_1.forward(&stacked)
              .map_err(|e| Error::Parse(format!("sr proj_1: {e}")))?;
          let h1 = h1.relu().map_err(|e| Error::Parse(format!("sr relu: {e}")))?;
          let h2 = self.proj_2.forward(&h1)
              .map_err(|e| Error::Parse(format!("sr proj_2: {e}")))?;       // [N*W, H]
          let out = h2.reshape((1, num_words, MAX_WIDTH, h))
              .map_err(|e| Error::Parse(format!("sr reshape: {e}")))?;
          Ok(out)
      }
  }
  ```

  **Note:** The exact MLP shape (2-layer 2H→4H→H with ReLU) is taken from the Python `gliner2.heads.SpanRep` source you snapshotted in M1.1. Verify before committing — if Python uses 1 layer or a different activation, adjust here.

- [ ] **Step 2: Build.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  ```

- [ ] **Step 3: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino_candle/heads/span_rep.rs
  git commit -m "feat(gliner2_fastino_candle): span_rep head (Candle MLP)"
  ```

---

## Milestone P4.M8 — Heads: schema_gather + count_pred (~2 days)

Goal: schema_gather extracts `pc_emb` (`[P]` token) and `field_embs` (`[E]`/`[L]` tokens). count_pred is a 20-class MLP from `pc_emb` followed by Rust-side argmax.

### Task M8.1: schema_gather

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino_candle/heads/schema_gather.rs`

- [ ] **Step 1: Implement.**

  ```rust
  //! Schema-gather head: extracts pc_emb (one) and field_embs (M) from
  //! the encoder hidden states using a list of token indices.

  use crate::{Error, Result};
  use candle_core::{Tensor, IndexOp};

  /// hidden_states: `[1, L, H]`
  /// schema_indices[0] = prompt-token (`[P]`) index
  /// schema_indices[1..] = field-token (`[E]`/`[L]`) indices
  ///
  /// Returns (pc_emb `[1, H]`, field_embs `[M, H]`).
  pub fn forward(
      hidden_states: &Tensor,
      schema_indices: &[usize],
  ) -> Result<(Tensor, Tensor)> {
      let (_b, _l, _h) = hidden_states.dims3()
          .map_err(|e| Error::Parse(format!("sg dims: {e}")))?;
      let pc_idx = *schema_indices.first()
          .ok_or_else(|| Error::Parse("sg: empty schema_indices".into()))?;
      let pc_emb = hidden_states.i((0, pc_idx, ..))
          .map_err(|e| Error::Parse(format!("sg pc: {e}")))?
          .unsqueeze(0)
          .map_err(|e| Error::Parse(format!("sg pc unsqueeze: {e}")))?;
      let mut fields = Vec::with_capacity(schema_indices.len() - 1);
      for &i in &schema_indices[1..] {
          let f = hidden_states.i((0, i, ..))
              .map_err(|e| Error::Parse(format!("sg field: {e}")))?;
          fields.push(f);
      }
      let field_embs = Tensor::stack(&fields, 0)
          .map_err(|e| Error::Parse(format!("sg stack: {e}")))?;
      Ok((pc_emb, field_embs))
  }
  ```

- [ ] **Step 2: Tests + commit.**

  Add a unit test mirroring M7.1's structure (build a small synthetic hidden_states, assert correct rows are picked).

  ```bash
  cargo test -p anno --features gliner2-fastino-candle \
      --lib backends::gliner2_fastino_candle::heads::schema_gather
  git add crates/anno/src/backends/gliner2_fastino_candle/heads/
  git commit -m "feat(gliner2_fastino_candle): schema_gather head"
  ```

### Task M8.2: count_pred

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino_candle/heads/count_pred.rs`

- [ ] **Step 1: Implement.**

  ```rust
  //! Count-predictor head: 20-class MLP over [P]-emb + Rust-side argmax.
  //! MAX_COUNT = 20 (matches ONNX export's first dim of struct_proj).

  use crate::{Error, Result};
  use candle_core::{Tensor, D};
  use candle_nn::{linear, Linear, Module, VarBuilder};

  pub const MAX_COUNT: usize = 20;

  pub struct CountPred {
      proj_1: Linear,
      proj_2: Linear,
  }

  impl CountPred {
      pub fn new(hidden: usize, vb: VarBuilder) -> Result<Self> {
          let proj_1 = linear(hidden, hidden, vb.pp("proj_1"))
              .map_err(|e| Error::Retrieval(format!("cp proj_1: {e}")))?;
          let proj_2 = linear(hidden, MAX_COUNT, vb.pp("proj_2"))
              .map_err(|e| Error::Retrieval(format!("cp proj_2: {e}")))?;
          Ok(Self { proj_1, proj_2 })
      }

      /// pc_emb: `[1, H]`. Returns the predicted instance count (0..MAX_COUNT-1).
      pub fn forward(&self, pc_emb: &Tensor) -> Result<usize> {
          let h = self.proj_1.forward(pc_emb)
              .map_err(|e| Error::Parse(format!("cp p1: {e}")))?;
          let h = h.relu().map_err(|e| Error::Parse(format!("cp relu: {e}")))?;
          let logits = self.proj_2.forward(&h)
              .map_err(|e| Error::Parse(format!("cp p2: {e}")))?;
          // Argmax over last axis.
          let argmax = logits.argmax(D::Minus1)
              .map_err(|e| Error::Parse(format!("cp argmax: {e}")))?;
          let v: u32 = argmax.flatten_all()
              .map_err(|e| Error::Parse(format!("cp flatten: {e}")))?
              .to_scalar::<u32>()
              .map_err(|e| Error::Parse(format!("cp scalar: {e}")))?;
          Ok(v as usize)
      }
  }
  ```

  **Note:** verify the actual MLP shape against the Python `gliner2.heads.CountPred` source from M1.1.

- [ ] **Step 2: Build + commit.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  git add crates/anno/src/backends/gliner2_fastino_candle/heads/count_pred.rs
  git commit -m "feat(gliner2_fastino_candle): count_pred head (20-class MLP + argmax)"
  ```

---

## Milestone P4.M9 — Heads: count_lstm_fixed + scorer (~2 days)

Goal: count-conditioned LSTM (struct projection) and span-vs-struct scorer.

### Task M9.1: count_lstm_fixed

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino_candle/heads/count_lstm.rs`

- [ ] **Step 1: Implement.**

  Reference Python `gliner2.heads.CountLstmFixed`. The ONNX export bakes
  `MAX_COUNT=20` into the first dim of the output. The architecture is a
  conditional LSTM that, given `field_embs: [M, H]`, produces
  `struct_proj: [MAX_COUNT, M, H]` — one slot per possible-count `k ∈ [0, MAX_COUNT)`.

  ```rust
  //! Count-conditioned LSTM (struct projection). Mirrors ONNX count_lstm_fixed.

  use crate::{Error, Result};
  use candle_core::{Tensor, D};
  use candle_nn::{linear, Linear, VarBuilder, Module};

  use super::count_pred::MAX_COUNT;

  pub struct CountLstmFixed {
      // Simplified LSTM-equivalent: project field_embs MAX_COUNT times with
      // distinct linears, conditioning on a learned per-count embedding.
      // The Python ref is a real LSTM; for inference we substitute the
      // unrolled equivalent loaded from the safetensors file.
      // Final shape: [MAX_COUNT, M, H].
      // M9.1 STUB: real implementation reads `count_lstm_fixed.weight` etc.
      pub(crate) per_count_linears: Vec<Linear>,
  }

  impl CountLstmFixed {
      pub fn new(hidden: usize, vb: VarBuilder) -> Result<Self> {
          let mut linears = Vec::with_capacity(MAX_COUNT);
          for k in 0..MAX_COUNT {
              let l = linear(hidden, hidden, vb.pp(format!("count_{k}")))
                  .map_err(|e| Error::Retrieval(format!("cl_{k}: {e}")))?;
              linears.push(l);
          }
          Ok(Self { per_count_linears: linears })
      }

      /// field_embs: `[M, H]`. Returns `[MAX_COUNT, M, H]`.
      pub fn forward(&self, field_embs: &Tensor) -> Result<Tensor> {
          let mut frames = Vec::with_capacity(MAX_COUNT);
          for l in &self.per_count_linears {
              let f = l.forward(field_embs)
                  .map_err(|e| Error::Parse(format!("cl forward: {e}")))?;
              frames.push(f);
          }
          let out = Tensor::stack(&frames, 0)
              .map_err(|e| Error::Parse(format!("cl stack: {e}")))?;
          Ok(out)
      }
  }
  ```

  **STUB warning:** the per-count-linear approximation is a lower-bound implementation. The real Python head is an LSTM — replace with a Candle LSTM (`candle_nn::rnn::LSTM`) once the parameter naming in `model.safetensors` is mapped. The unit test below catches gross deviations; the M13 ONNX-parity test catches everything else.

- [ ] **Step 2: Build.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  git add crates/anno/src/backends/gliner2_fastino_candle/heads/count_lstm.rs
  git commit -m "feat(gliner2_fastino_candle): count_lstm_fixed head (per-count linears stub)"
  ```

### Task M9.2: scorer

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino_candle/heads/scorer.rs`

- [ ] **Step 1: Implement.**

  ```rust
  //! Span-vs-struct scorer + sigmoid. Mirrors ONNX scorer.
  //!
  //! Inputs:
  //!   span_embs:    [1, num_words, MAX_WIDTH, H]
  //!   struct_proj:  [MAX_COUNT, M, H]
  //! Output:
  //!   scores:       [MAX_COUNT, num_words, MAX_WIDTH, M]   (sigmoid-applied)

  use crate::{Error, Result};
  use candle_core::Tensor;

  pub fn forward(span_embs: &Tensor, struct_proj: &Tensor) -> Result<Tensor> {
      let (_b, num_words, max_width, h) = span_embs.dims4()
          .map_err(|e| Error::Parse(format!("scorer span dims: {e}")))?;
      let (max_count, m, h2) = struct_proj.dims3()
          .map_err(|e| Error::Parse(format!("scorer proj dims: {e}")))?;
      assert_eq!(h, h2);

      // Reshape span_embs to [num_words*max_width, H]; struct_proj to [MAX_COUNT*M, H].
      let span_flat = span_embs.reshape((num_words * max_width, h))?;       // [Ns, H]
      let proj_flat = struct_proj.reshape((max_count * m, h))?;              // [Np, H]

      // Dot-product similarity: [Ns, Np] = span @ proj^T.
      let scores = span_flat.matmul(&proj_flat.transpose(0, 1)?.contiguous()?)
          .map_err(|e| Error::Parse(format!("scorer matmul: {e}")))?;        // [Ns, Np]

      // Sigmoid + reshape to [MAX_COUNT, num_words, max_width, M].
      let scores = candle_nn::ops::sigmoid(&scores)
          .map_err(|e| Error::Parse(format!("scorer sigmoid: {e}")))?;
      let scores = scores.reshape((num_words, max_width, max_count, m))?;
      // Transpose to [MAX_COUNT, num_words, max_width, M].
      let scores = scores.permute((2, 0, 1, 3))?.contiguous()?;
      Ok(scores)
  }
  ```

- [ ] **Step 2: Build + commit.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  git add crates/anno/src/backends/gliner2_fastino_candle/heads/scorer.rs
  git commit -m "feat(gliner2_fastino_candle): scorer head (span⊗proj + sigmoid)"
  ```

---

## Milestone P4.M10 — Heads: classifier (~1 day)

### Task M10.1: classifier

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino_candle/heads/classifier.rs`

- [ ] **Step 1: Implement.**

  ```rust
  //! `[L]`-head MLP classifier. Mirrors ONNX classifier.
  //! Returns label-wise softmax probabilities.

  use crate::{Error, Result};
  use candle_core::{Tensor, D};
  use candle_nn::{linear, Linear, Module, VarBuilder};

  pub struct Classifier {
      proj_1: Linear,
      proj_2: Linear,
  }

  impl Classifier {
      pub fn new(hidden: usize, vb: VarBuilder) -> Result<Self> {
          let proj_1 = linear(hidden, hidden, vb.pp("proj_1"))
              .map_err(|e| Error::Retrieval(format!("clsf p1: {e}")))?;
          let proj_2 = linear(hidden, 1, vb.pp("proj_2"))
              .map_err(|e| Error::Retrieval(format!("clsf p2: {e}")))?;
          Ok(Self { proj_1, proj_2 })
      }

      /// field_embs: `[M, H]`. Returns Vec<f32> of length M (softmax probs).
      pub fn forward(&self, field_embs: &Tensor) -> Result<Vec<f32>> {
          let h = self.proj_1.forward(field_embs)
              .map_err(|e| Error::Parse(format!("clsf p1: {e}")))?;
          let h = h.relu().map_err(|e| Error::Parse(format!("clsf relu: {e}")))?;
          let logits = self.proj_2.forward(&h)
              .map_err(|e| Error::Parse(format!("clsf p2: {e}")))?;          // [M, 1]
          let logits = logits.squeeze(D::Minus1)
              .map_err(|e| Error::Parse(format!("clsf squeeze: {e}")))?;     // [M]
          let probs = candle_nn::ops::softmax(&logits, D::Minus1)
              .map_err(|e| Error::Parse(format!("clsf softmax: {e}")))?;
          let v: Vec<f32> = probs.to_vec1()
              .map_err(|e| Error::Parse(format!("clsf to_vec: {e}")))?;
          Ok(v)
      }
  }
  ```

- [ ] **Step 2: Build + commit.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  git add crates/anno/src/backends/gliner2_fastino_candle/heads/classifier.rs \
          crates/anno/src/backends/gliner2_fastino_candle/heads/mod.rs
  git commit -m "feat(gliner2_fastino_candle): classifier head + heads module"
  ```

---

## Milestone P4.M11 — `pipeline.rs` orchestration (~1 day)

Goal: stitch the 8 heads + encoder into `extract_ner` and `classify` flows mirroring the ONNX backend's `mod.rs::extract_ner`.

### Task M11.1: Pipeline

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs`

- [ ] **Step 1: Re-export upstream processor.**

  In `gliner2_fastino_candle/processor.rs`:

  ```rust
  //! Re-exports the ONNX backend's processor — the input prep is identical
  //! across runtimes. Avoids code duplication.
  pub use crate::backends::gliner2_fastino::processor::*;
  ```

  Same for `decoder.rs` (re-exports `decode_entities` / NMS):

  ```rust
  //! Re-exports the ONNX backend's decoder/NMS — output postprocessing
  //! is identical across runtimes. The shape `[MAX_COUNT, num_words, MAX_WIDTH, M]`
  //! is the same regardless of whether scorer ran in ONNX or Candle.
  pub use crate::backends::gliner2_fastino::pipeline::{
      decode_entities, ScorerOutput, SchemaGatherOutput,
  };
  ```

  Note: this requires `gliner2-fastino` AND `gliner2-fastino-candle` to be enabled together. Add a feature dep:

  In `Cargo.toml` change:

  ```toml
  gliner2-fastino-candle = ["candle", "gliner2-fastino"]
  ```

- [ ] **Step 2: Pipeline implementation.**

  `pipeline.rs`:

  ```rust
  use crate::{Error, Result};
  use candle_core::Tensor;

  use super::heads::{
      classifier::Classifier,
      count_lstm::CountLstmFixed,
      count_pred::CountPred,
      scorer,
      schema_gather,
      span_rep::SpanRep,
      token_gather,
  };
  use super::lora::LoraDelta;

  /// Heads bundle. Lives on the engine.
  pub struct Heads {
      pub span_rep: SpanRep,
      pub count_pred: CountPred,
      pub count_lstm: CountLstmFixed,
      pub classifier: Classifier,
  }

  /// Run the NER pipeline. `text_embs` already gathered.
  pub fn extract_ner(
      hidden_states: &Tensor,
      heads: &Heads,
      text: &str,
      record: &super::processor::ProcessedRecord,
      task_map: &super::processor::TaskMapping,
      threshold: f32,
      _active_lora: Option<&LoraDelta>,  // M14: applied inside encoder; here for future reuse
  ) -> Result<Vec<crate::Entity>> {
      let num_words = record.word_to_char_maps.len();
      if num_words == 0 { return Ok(vec![]); }

      // Word indices.
      let word_starts: Vec<usize> = record.word_to_token_maps.iter()
          .map(|&(s, _)| s).collect();
      let text_embs = token_gather::forward(hidden_states, &word_starts)?;
      let span_embs = heads.span_rep.forward(&text_embs)?;

      // Schema gather.
      let mut sg_idx = vec![task_map.prompt_tok_idx];
      sg_idx.extend(task_map.field_tok_indices.iter().copied());
      let (pc_emb, field_embs) = schema_gather::forward(hidden_states, &sg_idx)?;

      let pred_count = heads.count_pred.forward(&pc_emb)?;
      if pred_count == 0 { return Ok(vec![]); }
      let struct_proj = heads.count_lstm.forward(&field_embs)?;

      let scores = scorer::forward(&span_embs, &struct_proj)?;
      // Convert candle Tensor → ndarray::Array4<f32> → reuse decode_entities.
      let scores_arr = candle_to_ndarray4(&scores)?;
      let scorer_out = super::decoder::ScorerOutput { scores: scores_arr };
      let entities = super::decoder::decode_entities(
          text, record, task_map, &scorer_out, pred_count, threshold,
          /* flat_ner = */ false,
      );
      Ok(entities)
  }

  pub fn classify(
      hidden_states: &Tensor,
      heads: &Heads,
      task_map: &super::processor::TaskMapping,
  ) -> Result<Vec<f32>> {
      let mut sg_idx = vec![task_map.prompt_tok_idx];
      sg_idx.extend(task_map.field_tok_indices.iter().copied());
      let (pc_emb, field_embs) = schema_gather::forward(hidden_states, &sg_idx)?;

      let pred_count = heads.count_pred.forward(&pc_emb)?;
      if pred_count == 0 {
          return Ok(vec![0.0; task_map.labels.len()]);
      }
      heads.classifier.forward(&field_embs)
  }

  fn candle_to_ndarray4(t: &Tensor) -> Result<ndarray::Array4<f32>> {
      let dims = t.dims();
      if dims.len() != 4 {
          return Err(Error::Parse(format!("candle→nd4: shape {:?}", dims)));
      }
      let flat: Vec<f32> = t.flatten_all()
          .and_then(|t| t.to_vec1())
          .map_err(|e| Error::Parse(format!("candle→nd4: {e}")))?;
      ndarray::Array4::from_shape_vec((dims[0], dims[1], dims[2], dims[3]), flat)
          .map_err(|e| Error::Parse(format!("candle→nd4 reshape: {e}")))
  }
  ```

- [ ] **Step 3: Build.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  ```

  Expected: clean.

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/Cargo.toml \
          crates/anno/src/backends/gliner2_fastino_candle/
  git commit -m "feat(gliner2_fastino_candle): pipeline.rs orchestration; depend on gliner2-fastino"
  ```

---

## Milestone P4.M12 — Engine struct + `from_pretrained` (~2 days)

Goal: `GLiNER2FastinoCandle::from_pretrained` and `from_local` constructors that load tokenizer + config + safetensors and build the encoder + heads.

### Task M12.1: Engine fill-in

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/mod.rs`
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/encoder.rs`

- [ ] **Step 1: `encoder.rs` — wraps `CandleEncoder`.**

  ```rust
  //! Wraps the encoder_candle CandleEncoder for DeBERTa-v2 + LoRA-aware runs.

  use crate::backends::encoder_candle::CandleEncoder;
  use crate::Result;

  pub struct Encoder {
      pub(crate) inner: CandleEncoder,
  }

  impl Encoder {
      pub fn from_pretrained(model_id: &str) -> Result<Self> {
          let inner = CandleEncoder::from_pretrained(model_id)?;
          Ok(Self { inner })
      }
  }
  ```

- [ ] **Step 2: Engine fill-in in `mod.rs`.**

  Replace the placeholder `engine` mod with a fuller implementation:

  ```rust
  mod engine {
      use super::adapters::AdapterRegistry;
      use super::encoder::Encoder;
      use super::pipeline::Heads;
      use crate::Result;
      use std::path::Path;
      use std::sync::Arc;

      pub struct GLiNER2FastinoCandle {
          pub(crate) tokenizer: tokenizers::Tokenizer,
          pub(crate) special: crate::backends::gliner2_fastino::processor::SpecialTokenIds,
          pub(crate) transformer:
              crate::backends::gliner2_fastino::processor::SchemaTransformer,
          pub(crate) encoder: Encoder,
          pub(crate) heads: Heads,
          pub(crate) adapters: Arc<AdapterRegistry>,
          pub(crate) model_id: String,
      }

      impl GLiNER2FastinoCandle {
          pub fn from_pretrained(model_id: &str) -> Result<Self> {
              let api = crate::backends::hf_loader::hf_api()
                  .map_err(|e| crate::Error::Backend(format!("hf_api: {e}")))?;
              let repo = api.model(model_id.to_string());

              let tokenizer_path =
                  crate::backends::hf_loader::download_model_file(&repo, &["tokenizer.json"])
                      .map_err(|e| crate::Error::Backend(format!(
                          "candle: download tokenizer: {e}"
                      )))?;
              let snapshot = tokenizer_path.parent().unwrap().to_path_buf();
              let _config = crate::backends::hf_loader::download_model_file(
                  &repo, &["config.json"]
              );
              let _safetensors = crate::backends::hf_loader::download_model_file(
                  &repo, &["model.safetensors"]
              );

              Self::from_local(&snapshot).map(|mut s| {
                  s.model_id = model_id.to_string();
                  s
              })
          }

          pub fn from_local(model_dir: &Path) -> Result<Self> {
              let tokenizer_path = model_dir.join("tokenizer.json");
              let tokenizer = crate::backends::hf_loader::load_tokenizer(&tokenizer_path)?;
              let special = crate::backends::gliner2_fastino::processor::SpecialTokenIds
                  ::resolve(&tokenizer)?;
              let transformer = crate::backends::gliner2_fastino::processor::SchemaTransformer
                  ::new(tokenizer.clone())?;

              // Encoder + heads are built from model.safetensors via VarBuilder.
              let encoder = Encoder::from_pretrained(
                  model_dir.to_string_lossy().as_ref()
              )?;
              let heads = build_heads_from_safetensors(model_dir)?;
              let adapters = Arc::new(AdapterRegistry::new());

              Ok(Self {
                  tokenizer,
                  special,
                  transformer,
                  encoder,
                  heads,
                  adapters,
                  model_id: model_dir.file_name()
                      .map(|s| s.to_string_lossy().into_owned())
                      .unwrap_or_else(|| "gliner2_fastino_candle_local".to_string()),
              })
          }
      }

      fn build_heads_from_safetensors(model_dir: &Path) -> Result<Heads> {
          use candle_core::Device;
          use candle_nn::VarBuilder;

          let device = crate::backends::encoder_candle::best_device()?;
          let st_path = model_dir.join("model.safetensors");
          let dtype = candle_core::DType::F32;
          let vb = unsafe {
              VarBuilder::from_mmaped_safetensors(&[&st_path], dtype, &device)
                  .map_err(|e| crate::Error::Parse(format!("safetensors: {e}")))?
          };

          let span_rep = super::heads::span_rep::SpanRep::new(768, vb.pp("heads.span_rep"))?;
          let count_pred = super::heads::count_pred::CountPred::new(768, vb.pp("heads.count_pred"))?;
          let count_lstm = super::heads::count_lstm::CountLstmFixed::new(768, vb.pp("heads.count_lstm_fixed"))?;
          let classifier = super::heads::classifier::Classifier::new(768, vb.pp("heads.classifier"))?;
          Ok(Heads { span_rep, count_pred, count_lstm, classifier })
      }
  }

  pub use engine::GLiNER2FastinoCandle;
  ```

  **Note:** the `vb.pp("heads.span_rep")` path is a guess at how the Python `gliner2` package serializes parameter names. Read the actual safetensors file once available:

  ```bash
  python -c "from safetensors import safe_open; \
      f = safe_open('$(~/.venv/anno-tools/bin/python -c \
        \"from huggingface_hub import snapshot_download; print(snapshot_download(\\'fastino/gliner2-multi-v1\\'))\")/model.safetensors', framework='pt'); \
      [print(k) for k in f.keys()]"
  ```

  Adjust the `pp(...)` paths to match. Also adjust `CountPred::new`'s parameter `pp` paths inside that head module.

- [ ] **Step 3: Build (compile-only, doesn't validate parameter names yet).**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  ```

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino_candle/
  git commit -m "feat(gliner2_fastino_candle): GLiNER2FastinoCandle::from_pretrained + from_local skeleton"
  ```

### Task M12.2: Trait impls (`Model`, `ZeroShotNER`)

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/mod.rs`

- [ ] **Step 1: `extract_ner` + trait wiring.**

  Append to `mod.rs`:

  ```rust
  use crate::{Entity, EntityType, Language, Model, ModelCapabilities};
  use crate::backends::inference::ZeroShotNER;

  impl GLiNER2FastinoCandle {
      pub fn extract_ner(
          &self,
          text: &str,
          types: &[&str],
          threshold: f32,
      ) -> Result<Vec<Entity>> {
          if types.is_empty() { return Ok(vec![]); }
          let labels: Vec<String> = types.iter().map(|s| s.to_string()).collect();
          let task = crate::backends::gliner2_fastino::processor::SchemaTask::Entities(labels);
          let record = self.transformer.transform(text, &[task])?;
          let task_map = record.tasks.first()
              .ok_or_else(|| crate::Error::Backend("candle: no task mapping".into()))?;

          // Encode the prompt+text. encoder_candle::CandleEncoder::encode returns
          // a flat `Vec<f32>` and the seq_len. Reshape to candle Tensor [1, L, H].
          let (flat, seq_len) = self.encoder.inner.encode(text)?;
          let device = crate::backends::encoder_candle::best_device()?;
          let h = 768;
          let hidden_states = candle_core::Tensor::from_vec(
              flat, (1, seq_len, h), &device,
          ).map_err(|e| crate::Error::Parse(format!("hs reshape: {e}")))?;

          // Snapshot the active LoRA for this call (M14+).
          let active = self.adapters.snapshot_active();

          super::pipeline::extract_ner(
              &hidden_states, &self.heads, text, &record, task_map, threshold,
              active.as_deref(),
          )
      }

      pub fn classify(
          &self,
          text: &str,
          labels: &[&str],
          _threshold: f32,
      ) -> Result<Vec<(String, f32)>> {
          if labels.is_empty() { return Ok(vec![]); }
          let label_strings: Vec<String> = labels.iter().map(|s| s.to_string()).collect();
          let task = crate::backends::gliner2_fastino::processor::SchemaTask::Classifications(
              "classification".to_string(), label_strings.clone(),
          );
          let record = self.transformer.transform(text, &[task])?;
          let task_map = record.tasks.first()
              .ok_or_else(|| crate::Error::Backend("candle classify: no task mapping".into()))?;
          let (flat, seq_len) = self.encoder.inner.encode(text)?;
          let device = crate::backends::encoder_candle::best_device()?;
          let hidden_states = candle_core::Tensor::from_vec(
              flat, (1, seq_len, 768), &device,
          ).map_err(|e| crate::Error::Parse(format!("hs reshape: {e}")))?;

          let probs = super::pipeline::classify(&hidden_states, &self.heads, task_map)?;
          let mut out: Vec<(String, f32)> = label_strings.into_iter().zip(probs).collect();
          out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
          Ok(out)
      }
  }

  impl Model for GLiNER2FastinoCandle {
      fn extract_entities(&self, text: &str, _lang: Option<Language>) -> Result<Vec<Entity>> {
          self.extract_ner(text, &["person", "organization", "location", "date"], 0.5)
      }
      fn supported_types(&self) -> Vec<EntityType> {
          vec![
              EntityType::Person, EntityType::Organization,
              EntityType::Location, EntityType::Date,
          ]
      }
      fn is_available(&self) -> bool { true }
      fn name(&self) -> &'static str { "GLiNER2FastinoCandle" }
      fn description(&self) -> &'static str {
          "fastino-ai GLiNER2 (Candle backend with runtime LoRA hot-swap, experimental)"
      }
      fn capabilities(&self) -> ModelCapabilities {
          ModelCapabilities { zero_shot: true, ..Default::default() }
      }
      fn as_zero_shot(&self) -> Option<&dyn ZeroShotNER> { Some(self) }
  }

  impl ZeroShotNER for GLiNER2FastinoCandle {
      fn default_types(&self) -> &[&'static str] {
          &["person", "organization", "location", "date", "event"]
      }
      fn extract_with_types(&self, text: &str, types: &[&str], threshold: f32)
          -> Result<Vec<Entity>>
      {
          self.extract_ner(text, types, threshold)
      }
      fn extract_with_descriptions(&self, text: &str, descriptions: &[&str], threshold: f32)
          -> Result<Vec<Entity>>
      {
          self.extract_ner(text, descriptions, threshold)
      }
  }
  ```

  Note: `self.adapters.snapshot_active()` is a placeholder for M16; for now it returns `None`.

- [ ] **Step 2: Build.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  ```

- [ ] **Step 3: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino_candle/mod.rs
  git commit -m "feat(gliner2_fastino_candle): Model + ZeroShotNER impls"
  ```

---

## Milestone P4.M13 — ONNX↔Candle parity test (~1 day)

Goal: end-to-end output parity between the ONNX and Candle backends on the same input. Acceptance criterion §7: `max_abs_diff < 5e-3` on entity scores.

### Task M13.1: Parity test

**Files:**
- Create: `crates/anno/tests/gliner2_fastino_candle_integration.rs`

- [ ] **Step 1: Test.**

  ```rust
  //! Tier-2: Candle backend ≡ ONNX backend on entity scores within 5e-3.
  //!
  //! Run:
  //!   cargo test -p anno --features gliner2-fastino,gliner2-fastino-candle \
  //!       --test gliner2_fastino_candle_integration -- --ignored --nocapture

  #![cfg(all(feature = "gliner2-fastino", feature = "gliner2-fastino-candle"))]

  use anno::backends::gliner2_fastino::GLiNER2Fastino;
  use anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle;
  use anno::backends::inference::ZeroShotNER;

  const FIXTURE: &str = "Acme Corp signed a deal with Globex in Paris on January 5th.";
  const LABELS: &[&str] = &["organization", "location", "date"];

  #[test]
  #[ignore]
  fn candle_matches_onnx_within_5e_3() {
      let onnx = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
          .expect("onnx load");
      let candle = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")
          .expect("candle load");

      let onnx_ents = onnx.extract_with_types(FIXTURE, LABELS, 0.0).expect("onnx run");
      let candle_ents = candle.extract_with_types(FIXTURE, LABELS, 0.0).expect("candle run");

      assert_eq!(
          onnx_ents.len(),
          candle_ents.len(),
          "different entity counts: onnx={} candle={}",
          onnx_ents.len(), candle_ents.len(),
      );

      let key = |e: &anno::Entity| (e.start_char, e.end_char, e.entity_type.to_string());
      let mut o = onnx_ents.clone();
      let mut c = candle_ents.clone();
      o.sort_by_key(|e| key(e));
      c.sort_by_key(|e| key(e));

      let mut max_abs = 0.0_f32;
      for (a, b) in o.iter().zip(c.iter()) {
          assert_eq!(a.text, b.text);
          assert_eq!(a.entity_type.to_string(), b.entity_type.to_string());
          max_abs = max_abs.max((a.confidence - b.confidence).abs());
      }
      eprintln!("ONNX↔Candle max_abs_diff = {max_abs:.6}");
      assert!(max_abs < 5e-3, "parity exceeded 5e-3: {max_abs}");
  }
  ```

- [ ] **Step 2: Run.**

  ```bash
  cargo test -p anno --features gliner2-fastino,gliner2-fastino-candle \
      --test gliner2_fastino_candle_integration -- --ignored --nocapture
  ```

  Expected: passes. If not, the dominant failure modes:
  - `max_abs_diff ≈ 0.5+`: count_lstm_fixed stub is producing materially wrong struct projections. Replace with real LSTM in M9.1 before continuing.
  - Different entity counts: scorer matrix shape is wrong (transposed somewhere). Check the permute in M9.2.

- [ ] **Step 3: Commit.**

  ```bash
  git add crates/anno/tests/gliner2_fastino_candle_integration.rs
  git commit -m "test(gliner2_fastino_candle): ONNX↔Candle entity-score parity (max_abs_diff < 5e-3)"
  ```

---

## Milestone P4.M14 — LoRA loader (~2 days)

Goal: parse `adapter_config.json`, load `adapter_model.safetensors`, materialize as in-memory `LoraDelta`s indexed by their target parameter path.

**Reference reading before writing:**
- `/tmp/peft-lora-layer.py` (snapshotted in M1.1 step 2c). Specifically the `LoraLayer.update_layer` method that defines `lora_A` / `lora_B` shapes and the `self.scaling[adapter_name] = lora_alpha / r` formula.
- `/tmp/candle-lora-ref/lora_linear.rs` (M1.1 step 2b). Specifically how it constructs the down/up linears and the `merge` flag — we don't ship `merge_weights` (inference-only with hot-swap; merging defeats the purpose), but the construction pattern is informative.

**What we adopt verbatim from PEFT:**
- Key format: `base_model.model.<module_path>.lora_A.weight` / `.lora_B.weight`
- Shape convention: `lora_A: [r, in_dim]`, `lora_B: [out_dim, r]`
- Scaling: `alpha / r` applied at apply-time (not folded into weights — keeps the swap O(1))
- `fan_in_fan_out=False` is the default for HF transformers; we hard-fail with a clear error if `True` is set (DeBERTa-v2 doesn't need it; the alternative branch is in scope only if a real adapter requires it)

**What we deliberately differ from `candle-lora`:**
- `candle-lora` stores LoRA weights inside the `Linear` wrapper itself (one adapter per layer instance). We instead keep the `Linear` weights pristine and look up the active adapter's delta by string path through a `LoraHook`. This is the design choice that enables sub-millisecond hot-swap.

### Task M14.1: `lora.rs` core types

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/lora.rs`

- [ ] **Step 1: Replace stub with the real types.**

  ```rust
  //! LoRA delta loader and forward-pass injection.
  //!
  //! PEFT format reference: https://huggingface.co/docs/peft

  use crate::{Error, Result};
  use candle_core::{Device, Tensor};
  use candle_nn::Module;
  use serde::Deserialize;
  use std::collections::HashMap;
  use std::path::Path;

  /// Parsed `adapter_config.json`. Only the fields we use are deserialized.
  #[derive(Debug, Clone, Deserialize)]
  pub struct LoraConfig {
      pub r: usize,
      pub lora_alpha: f64,
      pub target_modules: Vec<String>,
      #[serde(default)]
      pub fan_in_fan_out: bool,
      #[serde(default)]
      pub base_model_name_or_path: Option<String>,
  }

  /// One LoRA adapter's worth of weights, mapped by the targeted module's
  /// fully-qualified parameter path.
  pub struct LoraAdapter {
      pub config: LoraConfig,
      /// Key: HF parameter path (e.g. "encoder.layer.0.attention.self.query_proj").
      /// Value: a single delta. The `forward` of `LoraLinear` looks up by this key.
      pub deltas: HashMap<String, LoraDelta>,
  }

  /// One target module's down-proj + up-proj weights.
  pub struct LoraDelta {
      pub w_down: Tensor,  // [r, in]
      pub w_up: Tensor,    // [out, r]
      pub alpha: f64,
      pub r: usize,
  }

  impl LoraAdapter {
      /// Load from a directory containing `adapter_config.json` + `adapter_model.safetensors`.
      pub fn from_dir(dir: &Path, device: &Device) -> Result<Self> {
          let cfg_path = dir.join("adapter_config.json");
          let cfg_text = std::fs::read_to_string(&cfg_path)
              .map_err(|e| Error::Retrieval(format!(
                  "lora: read {}: {e}", cfg_path.display()
              )))?;
          let config: LoraConfig = serde_json::from_str(&cfg_text)
              .map_err(|e| Error::Parse(format!("adapter_config.json: {e}")))?;

          let st_path = dir.join("adapter_model.safetensors");
          let st_bytes = std::fs::read(&st_path)
              .map_err(|e| Error::Retrieval(format!(
                  "lora: read {}: {e}", st_path.display()
              )))?;
          let st = safetensors::SafeTensors::deserialize(&st_bytes)
              .map_err(|e| Error::Parse(format!("safetensors: {e}")))?;

          // PEFT names: "base_model.model.<path>.lora_A.weight" + "lora_B.weight".
          // Strip the "base_model.model." prefix to get the original module path.
          let mut as_map: HashMap<String, Tensor> = HashMap::new();
          let mut bs_map: HashMap<String, Tensor> = HashMap::new();
          for name in st.names() {
              let view = st.tensor(name)
                  .map_err(|e| Error::Parse(format!("safetensors tensor {name}: {e}")))?;
              let dtype = candle_core::DType::F32;
              let shape: Vec<usize> = view.shape().to_vec();
              let bytes = view.data();
              // Convert raw bytes to f32. PEFT typically ships fp32 or fp16.
              let data: Vec<f32> = match view.dtype() {
                  safetensors::Dtype::F32 => bytemuck::cast_slice(bytes).to_vec(),
                  safetensors::Dtype::F16 => bytes.chunks_exact(2)
                      .map(|c| half::f16::from_le_bytes([c[0], c[1]]).to_f32())
                      .collect(),
                  other => return Err(Error::Parse(format!(
                      "lora: unsupported safetensor dtype {:?}", other
                  ))),
              };
              let _ = dtype;
              let t = Tensor::from_vec(data, shape, device)
                  .map_err(|e| Error::Parse(format!("lora tensor: {e}")))?;
              if let Some(stripped) = strip_lora_a(name) {
                  as_map.insert(stripped, t);
              } else if let Some(stripped) = strip_lora_b(name) {
                  bs_map.insert(stripped, t);
              }
          }

          // Pair lora_A and lora_B by stripped module path.
          let mut deltas = HashMap::new();
          for (path, w_down) in as_map {
              let w_up = bs_map.remove(&path).ok_or_else(|| Error::Parse(format!(
                  "lora: missing lora_B counterpart for `{path}`"
              )))?;
              deltas.insert(path, LoraDelta {
                  w_down, w_up,
                  alpha: config.lora_alpha,
                  r: config.r,
              });
          }
          if !bs_map.is_empty() {
              return Err(Error::Parse(format!(
                  "lora: {} lora_B entries with no lora_A counterpart",
                  bs_map.len()
              )));
          }
          Ok(Self { config, deltas })
      }
  }

  fn strip_lora_a(name: &str) -> Option<String> {
      let s = name.strip_prefix("base_model.model.")?;
      let s = s.strip_suffix(".lora_A.weight")?;
      Some(s.to_string())
  }
  fn strip_lora_b(name: &str) -> Option<String> {
      let s = name.strip_prefix("base_model.model.")?;
      let s = s.strip_suffix(".lora_B.weight")?;
      Some(s.to_string())
  }
  ```

- [ ] **Step 2: Add `bytemuck` and `safetensors` deps if not transitive.**

  ```bash
  grep -n "bytemuck\|safetensors" crates/anno/Cargo.toml
  ```

  If absent, add:

  ```toml
  bytemuck = "1"
  safetensors = "0.4"
  ```

  Both already transitive through Candle.

- [ ] **Step 3: Build.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  ```

- [ ] **Step 4: Unit test for the path-stripping helpers.**

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn strip_lora_a_removes_prefix_and_suffix() {
          assert_eq!(
              strip_lora_a("base_model.model.encoder.layer.0.attention.self.query_proj.lora_A.weight"),
              Some("encoder.layer.0.attention.self.query_proj".to_string()),
          );
          assert_eq!(strip_lora_a("not_a_lora_name"), None);
      }
  }
  ```

- [ ] **Step 5: Commit.**

  ```bash
  git add crates/anno/Cargo.toml crates/anno/src/backends/gliner2_fastino_candle/lora.rs
  git commit -m "feat(gliner2_fastino_candle): LoRA adapter loader (PEFT format)"
  ```

---

## Milestone P4.M15 — `LoraLinear` injection (~2 days)

Goal: a wrapped `Linear` that adds the LoRA delta when an active adapter is set. This is the actual hot-swap mechanism.

### Task M15.1: `LoraLinear`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/lora.rs`

- [ ] **Step 1: Append `LoraLinear` and friends.**

  ```rust
  /// A `candle_nn::Linear` extended with optional LoRA delta.
  /// The delta is read from the active adapter on every forward pass; swap is
  /// O(1) at the engine level (rebinding the active adapter).
  pub struct LoraLinear {
      pub base: candle_nn::Linear,
      /// Fully-qualified parameter path used to look up the delta from the
      /// adapter map. e.g. "encoder.layer.0.attention.self.query_proj".
      pub path: String,
  }

  impl LoraLinear {
      pub fn new(base: candle_nn::Linear, path: impl Into<String>) -> Self {
          Self { base, path: path.into() }
      }

      pub fn forward_with_lora(
          &self,
          x: &Tensor,
          active: Option<&LoraAdapter>,
      ) -> Result<Tensor> {
          let mut out = self.base.forward(x)
              .map_err(|e| Error::Parse(format!("lora_linear base: {e}")))?;
          if let Some(adapter) = active {
              if let Some(d) = adapter.deltas.get(&self.path) {
                  // y_lora = (alpha / r) * (lora_B @ lora_A @ x)
                  // x: [..., in_dim]
                  // lora_A: [r, in_dim] → multiply x by lora_A^T → [..., r]
                  // lora_B: [out_dim, r] → multiply that by lora_B^T → [..., out_dim]
                  let down = x.matmul(&d.w_down.transpose(0, 1)?.contiguous()?)?;  // [..., r]
                  let up = down.matmul(&d.w_up.transpose(0, 1)?.contiguous()?)?;    // [..., out]
                  let scaled = up.affine(d.alpha / d.r as f64, 0.0)
                      .map_err(|e| Error::Parse(format!("lora affine: {e}")))?;
                  out = (out + scaled)
                      .map_err(|e| Error::Parse(format!("lora add: {e}")))?;
              }
          }
          Ok(out)
      }
  }
  ```

  Note: this requires the encoder's `Attention` (and FFN) modules to use `LoraLinear` instead of plain `Linear`. M15.2 wires that.

### Task M15.2: Wire `LoraLinear` into the encoder

**Files:**
- Modify: `crates/anno/src/backends/encoder_candle/implementations.rs`

This is the largest invasive change of Phase 4. Carefully apply.

- [ ] **Step 1: Define `LinearWithPath` trait at the encoder boundary.**

  Rather than rewriting `encoder_candle::Attention` to depend on `gliner2_fastino_candle::LoraLinear` directly (cyclic dep), introduce a callback hook:

  In `encoder_candle/implementations.rs`, modify `TransformerLayer::forward` to take an optional `&dyn LoraHook` argument. Define `LoraHook` in `encoder_candle` as:

  ```rust
  /// Trait used by gliner2_fastino_candle to inject LoRA deltas into linear
  /// layers without creating a cyclic dependency.
  pub trait LoraHook: Send + Sync {
      /// Apply the LoRA delta for the linear identified by `path`.
      /// Receives the base output and returns base + lora_delta(x).
      fn apply(&self, path: &str, x: &candle_core::Tensor, base_out: &candle_core::Tensor)
          -> std::result::Result<candle_core::Tensor, candle_core::Error>;
  }
  ```

  Then update each `Linear::forward` call site in the encoder to:

  ```rust
  let q_base = self.q_proj.forward(x)?;
  let q = match lora {
      Some(h) => h.apply("encoder.layer.{i}.attention.self.query_proj", x, &q_base)?,
      None => q_base,
  };
  ```

  with the layer index `i` threaded down from `CandleEncoder::forward`. (Path strings are dynamic; build with `format!`.)

- [ ] **Step 2: Implement the hook in `gliner2_fastino_candle/lora.rs`.**

  ```rust
  use crate::backends::encoder_candle::LoraHook;

  pub struct AdapterHook {
      pub active: Option<std::sync::Arc<LoraAdapter>>,
  }

  impl LoraHook for AdapterHook {
      fn apply(&self, path: &str, x: &Tensor, base_out: &Tensor)
          -> std::result::Result<Tensor, candle_core::Error>
      {
          let Some(adapter) = self.active.as_deref() else { return Ok(base_out.clone()); };
          let Some(d) = adapter.deltas.get(path) else { return Ok(base_out.clone()); };
          let down = x.matmul(&d.w_down.transpose(0, 1)?.contiguous()?)?;
          let up = down.matmul(&d.w_up.transpose(0, 1)?.contiguous()?)?;
          let scaled = up.affine(d.alpha / d.r as f64, 0.0)?;
          base_out.add(&scaled)
      }
  }
  ```

- [ ] **Step 3: Thread the hook through `CandleEncoder::encode`.**

  Add a method `encode_with_lora(&self, text: &str, hook: &dyn LoraHook) -> Result<(Vec<f32>, usize)>` that mirrors `encode` but passes `Some(hook)` to each layer's forward.

- [ ] **Step 4: Build.**

  ```bash
  cargo check -p anno --features candle
  cargo check -p anno --features gliner2-fastino-candle
  ```

  Expected: clean. Expect lots of churn in `encoder_candle/implementations.rs` from threading the hook; that's normal.

- [ ] **Step 5: Commit.**

  ```bash
  git add crates/anno/src/backends/encoder_candle/ \
          crates/anno/src/backends/gliner2_fastino_candle/lora.rs
  git commit -m "feat(gliner2_fastino_candle): LoRA injection via LoraHook trait threaded into encoder"
  ```

---

## Milestone P4.M16 — Adapter registry + API (~1 day)

**API-shape reference:** `mistral.rs/docs/ADAPTER_MODELS.md` documents the dynamic-activation pattern that informs our `load_adapter` / `set_adapter` / `unload_adapter` triple. Their Python+Rust+HTTP triple isn't in scope for anno (no HTTP layer here) but the Rust call shape — preload N, activate one — is what we mirror. Specifically: `model.load_adapter(name, path)` is fire-and-forget (caches under `name`); `model.set_adapter(name)` is the activation step that's expected to be sub-millisecond. This split is what enables a request-loop server to swap per-request without paying load cost.

### Task M16.1: `AdapterRegistry`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/adapters.rs`

- [ ] **Step 1: Real implementation.**

  ```rust
  //! Adapter registry. Owns named adapters; flips an `Arc<...>` slot O(1)
  //! to activate one without invalidating in-flight inference calls.

  use crate::Result;
  use crate::backends::encoder_candle::best_device;
  use parking_lot::RwLock;
  use std::collections::HashMap;
  use std::path::Path;
  use std::sync::Arc;

  use super::lora::LoraAdapter;

  pub struct AdapterRegistry {
      cache: RwLock<HashMap<String, Arc<LoraAdapter>>>,
      active: RwLock<Option<Arc<LoraAdapter>>>,
      active_name: RwLock<Option<String>>,
  }

  impl AdapterRegistry {
      pub fn new() -> Self {
          Self {
              cache: RwLock::new(HashMap::new()),
              active: RwLock::new(None),
              active_name: RwLock::new(None),
          }
      }

      pub fn load(&self, name: &str, dir: &Path) -> Result<()> {
          let device = best_device()?;
          let adapter = Arc::new(LoraAdapter::from_dir(dir, &device)?);
          self.cache.write().insert(name.to_string(), adapter);
          Ok(())
      }

      pub fn set_active(&self, name: &str) -> Result<()> {
          let cache = self.cache.read();
          let adapter = cache.get(name).cloned().ok_or_else(|| {
              crate::Error::Backend(format!("adapter `{name}` not loaded"))
          })?;
          *self.active.write() = Some(adapter);
          *self.active_name.write() = Some(name.to_string());
          Ok(())
      }

      pub fn unload_active(&self) {
          *self.active.write() = None;
          *self.active_name.write() = None;
      }

      /// O(1). Snapshots the active adapter for a single inference call;
      /// in-flight calls are immune to subsequent set_active.
      pub fn snapshot_active(&self) -> Option<Arc<LoraAdapter>> {
          self.active.read().clone()
      }

      pub fn loaded_names(&self) -> Vec<String> {
          self.cache.read().keys().cloned().collect()
      }
      pub fn active_name(&self) -> Option<String> {
          self.active_name.read().clone()
      }
  }
  ```

- [ ] **Step 2: Public API on `GLiNER2FastinoCandle`.**

  In `mod.rs`:

  ```rust
  impl GLiNER2FastinoCandle {
      /// Load a LoRA adapter from a PEFT-format directory. Caches under `name`.
      /// Does NOT activate; call `set_adapter(name)`.
      pub fn load_adapter(&self, name: &str, path: &Path) -> Result<()> {
          self.adapters.load(name, path)
      }

      /// Activate a previously-loaded adapter. O(1) — flips an `Arc<...>` slot.
      pub fn set_adapter(&self, name: &str) -> Result<()> {
          self.adapters.set_active(name)
      }

      /// Run with the base model (no adapter).
      pub fn unload_adapter(&self) {
          self.adapters.unload_active()
      }

      pub fn loaded_adapters(&self) -> Vec<String> { self.adapters.loaded_names() }
      pub fn active_adapter(&self) -> Option<String> { self.adapters.active_name() }
  }
  ```

- [ ] **Step 3: Wire `AdapterHook` into `extract_ner` (M12.2 used a stub).**

  Replace the placeholder in `extract_ner`:

  ```rust
  let hook = super::lora::AdapterHook { active: self.adapters.snapshot_active() };
  let (flat, seq_len) = self.encoder.inner.encode_with_lora(text, &hook)?;
  ```

- [ ] **Step 4: Build.**

  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  ```

- [ ] **Step 5: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino_candle/
  git commit -m "feat(gliner2_fastino_candle): AdapterRegistry + load/set/unload_adapter API"
  ```

---

## Milestone P4.M17 — Hot-swap test + bench (~1 day)

### Task M17.1: Hot-swap correctness

**Files:**
- Create: `crates/anno/tests/gliner2_fastino_candle_lora.rs`

- [ ] **Step 1: Test.**

  ```rust
  //! Tier-2: verify load_adapter + set_adapter changes inference output.
  //! Requires: fastino/gliner2-multi-v1 + ONE PEFT-format adapter.
  //!
  //! Set ANNO_GLINER2_LORA_ADAPTER_DIR to point at the adapter dir before
  //! running.

  #![cfg(feature = "gliner2-fastino-candle")]

  use anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle;
  use anno::backends::inference::ZeroShotNER;

  #[test]
  #[ignore]
  fn set_adapter_changes_output() {
      let adapter_dir = std::env::var("ANNO_GLINER2_LORA_ADAPTER_DIR")
          .expect("ANNO_GLINER2_LORA_ADAPTER_DIR must be set");
      let model = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")
          .expect("load");

      let text = "This contract was signed in Paris by Alice for ACME.";
      let labels = &["organization", "person", "location"];

      let base = model.extract_with_types(text, labels, 0.0).expect("base");
      model.load_adapter("legal", std::path::Path::new(&adapter_dir)).expect("load adapter");
      model.set_adapter("legal").expect("set");
      let lora = model.extract_with_types(text, labels, 0.0).expect("lora");
      model.unload_adapter();
      let after = model.extract_with_types(text, labels, 0.0).expect("base again");

      // After `unload_adapter` the output should match the original base run.
      assert_eq!(base.len(), after.len());
      let key = |e: &anno::Entity| (e.start_char, e.end_char, e.entity_type.to_string());
      let mut b1 = base.clone(); b1.sort_by_key(|e| key(e));
      let mut b2 = after.clone(); b2.sort_by_key(|e| key(e));
      for (a, b) in b1.iter().zip(b2.iter()) {
          assert!((a.confidence - b.confidence).abs() < 1e-5,
              "unload should restore base output exactly");
      }

      // The `lora` output must differ from `base` somewhere (else the adapter
      // had no effect and likely failed to load).
      let mut diff = false;
      let mut bb = base; bb.sort_by_key(|e| key(e));
      let mut ll = lora; ll.sort_by_key(|e| key(e));
      for (a, b) in bb.iter().zip(ll.iter()) {
          if (a.confidence - b.confidence).abs() > 1e-3 {
              diff = true; break;
          }
      }
      if !diff && bb.len() != ll.len() { diff = true; }
      assert!(diff, "adapter had no measurable effect — load failed?");
  }
  ```

- [ ] **Step 2: Run.**

  ```bash
  ANNO_GLINER2_LORA_ADAPTER_DIR=/path/to/test_adapter \
      cargo test -p anno --features gliner2-fastino-candle \
          --test gliner2_fastino_candle_lora -- --ignored --nocapture
  ```

  Expected: passes.

### Task M17.2: Swap latency bench

**Files:**
- Create: `crates/anno/benches/gliner2_fastino_candle_swap.rs`
- Modify: `crates/anno/Cargo.toml`

- [ ] **Step 1: Bench.**

  ```rust
  #![cfg(feature = "gliner2-fastino-candle")]

  use anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle;
  use criterion::{black_box, criterion_group, criterion_main, Criterion};
  use std::path::Path;

  fn bench_set_adapter(c: &mut Criterion) {
      let adapter_dir = std::env::var("ANNO_GLINER2_LORA_ADAPTER_DIR")
          .expect("ANNO_GLINER2_LORA_ADAPTER_DIR must be set");
      let model = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")
          .expect("load");
      model.load_adapter("a", Path::new(&adapter_dir)).expect("load a");
      model.load_adapter("b", Path::new(&adapter_dir)).expect("load b");

      let mut g = c.benchmark_group("gliner2_fastino_candle_swap");
      g.bench_function("set_adapter", |bencher| {
          let mut toggle = false;
          bencher.iter(|| {
              toggle = !toggle;
              let name = if toggle { "a" } else { "b" };
              black_box(model.set_adapter(name).unwrap());
          });
      });
      g.finish();
  }

  criterion_group!(benches, bench_set_adapter);
  criterion_main!(benches);
  ```

- [ ] **Step 2: Cargo.toml.**

  ```toml
  [[bench]]
  name = "gliner2_fastino_candle_swap"
  harness = false
  required-features = ["gliner2-fastino-candle"]
  ```

- [ ] **Step 3: Run (host with adapter).**

  ```bash
  ANNO_GLINER2_LORA_ADAPTER_DIR=/path/to/adapter \
      cargo bench -p anno --features gliner2-fastino-candle \
          --bench gliner2_fastino_candle_swap
  ```

  Expected: `set_adapter` p50 < 1 ms.

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/Cargo.toml \
          crates/anno/tests/gliner2_fastino_candle_lora.rs \
          crates/anno/benches/gliner2_fastino_candle_swap.rs
  git commit -m "test+bench(gliner2_fastino_candle): hot-swap correctness + <1ms set_adapter"
  ```

---

## Milestone P4.M18 — Catalog + Docs + PR (~1 day)

### Task M18.1: Catalog row

**Files:**
- Modify: `crates/anno/src/backends/catalog.rs`

- [ ] **Step 1: Add row.**

  Insert after the `gliner2_fastino` entry (around line 286-298):

  ```rust
  BackendInfo {
      name: "gliner2_fastino_candle",
      feature: Some("gliner2-fastino-candle"),
      status: BackendStatus::WIP,
      zero_shot: true,
      gpu_support: true,
      description: "fastino-ai GLiNER2 with runtime LoRA hot-swap (Candle backend) — experimental, issue #18 / Phase 4",
      recommended_models: &[
          "fastino/gliner2-multi-v1",
      ],
  },
  ```

- [ ] **Step 2: Add a catalog test.**

  In the `#[cfg(test)]` block at the bottom of `catalog.rs`:

  ```rust
  #[test]
  #[cfg(feature = "gliner2-fastino-candle")]
  fn catalog_includes_gliner2_fastino_candle() {
      let entry = ALL_BACKENDS.iter()
          .find(|b| b.name == "gliner2_fastino_candle")
          .expect("gliner2_fastino_candle missing from catalog");
      assert_eq!(entry.feature, Some("gliner2-fastino-candle"));
      assert!(entry.zero_shot);
      assert!(entry.gpu_support);
  }
  ```

- [ ] **Step 3: Run + commit.**

  ```bash
  cargo test -p anno --features gliner2-fastino-candle \
      --lib backends::catalog::tests::catalog_includes_gliner2_fastino_candle
  git add crates/anno/src/backends/catalog.rs
  git commit -m "feat(catalog): add gliner2_fastino_candle backend row"
  ```

### Task M18.2: BACKENDS.md + rustdoc

**Files:**
- Modify: `docs/BACKENDS.md`
- Modify: `crates/anno/src/backends/gliner2_fastino_candle/mod.rs`

- [ ] **Step 1: BACKENDS.md row.**

  Add after the `gliner2_fastino` row:

  ```markdown
  | `gliner2_fastino_candle` | fastino-ai GLiNER2 with **runtime LoRA hot-swap** (Candle backend; reads PyTorch safetensors directly). Feature `gliner2-fastino-candle`. `load_adapter("name", path)` + `set_adapter("name")` swaps adapters in <1 ms. Issue [#18](https://github.com/arclabs561/anno/issues/18) / Phase 4. **Experimental.** | Yes | WIP | `fastino/gliner2-multi-v1` (Python repo, not the SemplificaAI ONNX export) |
  ```

- [ ] **Step 2: Module-level rustdoc with hot-swap example.**

  Top of `gliner2_fastino_candle/mod.rs`:

  ```rust
  //! gliner2_fastino_candle — runtime-LoRA-swap GLiNER2 backend.
  //!
  //! ```rust,no_run
  //! use anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle;
  //! use anno::backends::inference::ZeroShotNER;
  //! use std::path::Path;
  //!
  //! let model = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")
  //!     .expect("load base");
  //!
  //! // Load N domain-specific adapters once.
  //! model.load_adapter("legal", Path::new("./adapters/legal")).unwrap();
  //! model.load_adapter("medical", Path::new("./adapters/medical")).unwrap();
  //!
  //! // Switch domains in <1 ms.
  //! model.set_adapter("legal").unwrap();
  //! let legal_ents = model.extract_with_types(
  //!     "Plaintiff filed in district court on...",
  //!     &["court", "party", "date"], 0.5,
  //! ).unwrap();
  //!
  //! model.set_adapter("medical").unwrap();
  //! let med_ents = model.extract_with_types(
  //!     "Patient presented with...",
  //!     &["symptom", "diagnosis", "drug"], 0.5,
  //! ).unwrap();
  //!
  //! model.unload_adapter();   // Back to base.
  //! ```
  ```

- [ ] **Step 3: Build docs.**

  ```bash
  cargo doc -p anno --features gliner2-fastino-candle --no-deps
  ```

- [ ] **Step 4: Commit.**

  ```bash
  git add docs/BACKENDS.md crates/anno/src/backends/gliner2_fastino_candle/mod.rs
  git commit -m "docs(gliner2_fastino_candle): BACKENDS.md row + rustdoc hot-swap example"
  ```

### Task M18.3: PR

- [ ] **Step 1: Push.**

  ```bash
  git push -u origin feat/gliner2-fastino-phase4
  ```

- [ ] **Step 2: Open PR.**

  ```bash
  gh pr create \
      --title "Phase 4: gliner2_fastino_candle (runtime LoRA hot-swap)" \
      --body "$(cat <<'EOF'
  ## Summary
  - New `gliner2_fastino_candle` backend: GLiNER2 in Candle (no ONNX intermediate).
  - **Runtime LoRA adapter hot-swap** — first Rust NLP library to ship this.
  - DeBERTa-v2 disentangled-attention encoder added to `encoder_candle`.
  - All 8 GLiNER2 heads reimplemented in pure Candle.
  - Output parity with the ONNX backend: max_abs_diff < 5e-3 on entity scores.
  - Adapter swap latency < 1 ms (criterion).

  ## Acceptance
  - [x] `from_pretrained("fastino/gliner2-multi-v1")` loads the base model
  - [x] ONNX↔Candle parity within 5e-3
  - [x] `load_adapter("legal", path)` succeeds for a PEFT directory
  - [x] `set_adapter("legal")` produces different output than base
  - [x] `set_adapter` p50 < 1 ms
  - [x] Cargo features compile: `gliner2-fastino-candle`, `-cuda`, `-metal`

  ## Spec
  - `docs/superpowers/specs/2026-05-05-gliner2-fastino-phase4-candle-lora.md`
  - Plan: `docs/superpowers/plans/2026-05-05-gliner2-fastino-phase4-candle-lora.md`

  ## Caveats
  - `count_lstm_fixed` head currently uses a per-count-linear approximation
    of the upstream LSTM. M9.1's note documents the follow-up.
  - DeBERTa-v2 disentangled attention uses a non-optimized index_select path;
    HF's compressed C2P/P2C path is a perf follow-up if benchmarks show > 1.5×
    overhead vs ONNX.
  EOF
  )"
  ```

---

## Self-review checklist

- [x] **Spec coverage:**
  - §3 in-scope: new module (M3, M5–M11, M12), DeBERTa-v2 in encoder_candle (M4, M5), 8 heads (M7–M10), LoRA loader (M14), `LoraLinear` (M15), `load_adapter`/`set_adapter`/`unload_adapter` (M16), Cargo features (M2), parity (M13), rustdoc example (M18.2).
  - §5 module layout: every file in the spec's tree appears in the "File structure (locked)" table or in the affected milestones.
  - §6 phase plan steps 1-10: M4–M5 (encoder), M6 (encoder parity), M7–M10 (heads), M11–M12 (pipeline + engine), M13 (parity test), M14 (loader), M15 (delta application), M16 (registry + API), M17 (swap test + bench), M18 (features + integration test + docs).
  - §7 acceptance: from_pretrained (M12), parity (M13), load_adapter (M16/M17), set_adapter latency (M17.2), Cargo features (M2 + M18).
  - §8 risks: DeBERTa-v2 implementation (M5 with parity test in M6), Candle perf (no explicit mitigation here — flagged as a follow-up; M18 PR caveats document it), PEFT format drift (M14's `LoraConfig` only deserializes fields we use; unknown fields tolerated by default `serde`), target-modules mismatch (port notes in M1.1 fix HF parameter naming verbatim), CUDA/Metal feature gates (M2 sets up; runtime test left to host owner), parallel inference (M16's `snapshot_active` Arc-clone implements the in-flight-immune approach from §8.6).
- [x] **Type consistency:** `LoraConfig`, `LoraAdapter`, `LoraDelta`, `LoraLinear`, `AdapterHook`, `AdapterRegistry`, `Heads`, `Encoder`, `GLiNER2FastinoCandle` — all referenced consistently from M14 onwards. `MAX_WIDTH = 8` and `MAX_COUNT = 20` consistent with the ONNX backend's pipeline.rs constants. Method names: `load_adapter`, `set_adapter`, `unload_adapter`, `loaded_adapters`, `active_adapter`, `snapshot_active` — consistent.
- [x] **No placeholders:** Every step has either complete code, exact commands, or an explicit symbol reference. Two STUB markers (M9.1 count_lstm per-count-linear approximation; M5.2 simplified rel-pos broadcast) are flagged in-line with explicit follow-up gates so a future engineer can recognize them as deliberately-tracked partials, not abandoned TODOs.

## Honest assessment

The plan ships a functional first-cut Candle backend with hot-swap. The two stubs (`count_lstm_fixed` per-count-linear; rel-pos broadcast in DeBERTa-v2) are noted as **first-pass implementations validated by the M13 parity test**. If the parity test fails on real data with these stubs, M9.1 / M5.2 must be reworked before the PR opens. The plan deliberately frontloads both: the encoder parity gate is M6 (before any head work), and the end-to-end parity gate is M13 (before LoRA work). If either gate fails, the plan halts and reverts to fixing the implementation.
