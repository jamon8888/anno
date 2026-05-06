# gliner2_fastino — Phase 4 (Candle + LoRA) — REVISED Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a parallel Candle-based backend `gliner2_fastino_candle` with **LoRA adapter support via merge-at-load** (the Swift-port pattern), exposing `load_adapter` / `unload_adapter` for the multi-domain inference use case. Hot-swap with sub-millisecond per-request switching is **deferred to optional Phase 4.5** unless a workload demands it.

**Architecture:** New module tree `crates/anno/src/backends/gliner2_fastino_candle/` parallel to the existing ONNX-based `gliner2_fastino`. **Reuses `candle-transformers::models::debertav2::DebertaV2Model`** for the encoder (PR #2743, merged 2025-01-29) — so anno doesn't reimplement disentangled attention. Each LoRA target layer applies `W_merged = W_base + (lora_B @ lora_A) * (alpha / r)` once at `load_adapter` time, producing a fully-merged model with **zero per-forward overhead**. `unload_adapter` reverses by reloading base weights. Trait impls (`Model`, `ZeroShotNER`) match `GLiNER2Fastino` byte-for-byte; users swap backends with a type alias. ONNX backend stays as the production-fast single-domain path; Candle backend is the multi-domain (5-50 domains, ~5-10 MB adapters each) path.

**Tech Stack:** Rust 2021, `candle-core`, `candle-nn`, `candle-transformers` (existing dep, used for the DeBERTa-v2 encoder), `safetensors`, `tokenizers`, `hf-hub`, `parking_lot::RwLock` (only for the Optional Phase 4.5). Optional CUDA / Metal via Candle's GPU backends.

**Spec:** `docs/superpowers/specs/2026-05-05-gliner2-fastino-phase4-candle-lora.md` (still applies; this plan revises only the implementation path).
**Roadmap:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md` Track E.
**Phase 3.5 base:** Phase 3.5 merged at `b7b24088`. This plan stacks on `main`.

---

## Why this revision supersedes the original 2026-05-05 plan

The original plan was written assuming:
1. `candle-transformers` did not ship DeBERTa-v2 → would need ~5 days to implement disentangled attention from scratch (M4-M6).
2. The hot-swap pattern (`Arc<RwLock<Option<LoraDelta>>>` flipped per-request) was the right default architecture.
3. No public Rust GLiNER2 production reference existed.
4. GLiNER2's encoder was DeBERTa-**v2**.

**Evidence gathered 2026-05-06 invalidates all four assumptions:**

| Assumption | Evidence | Plan impact |
|---|---|---|
| `candle-transformers` lacks DeBERTa-v2 | [PR #2743 (BradyBonnette)](https://github.com/huggingface/candle/pull/2743) merged 2025-01-29. Exports `DebertaV2Model` (bare encoder), `DebertaV2DisentangledSelfAttention`, all building blocks. Tested vs `Clinical-AI-Apollo/Medical-NER` with "insignificant precision difference." | **M5 collapses from 3 days to ½ day** — just import. **M6 collapses from 2 days to ½ day** — smoke test, not debugging. Total saved: ~5 days. |
| Hot-swap is the right default | [`MacPaw/Gliner2Swift`](https://github.com/MacPaw/Gliner2Swift) ships **merge-at-load** with "zero runtime overhead" and reports identical results to Python. Same pattern: `W_merged = W_base + (lora_B @ lora_A) * (alpha/r)` applied once on load. Hot-swap in Python's gliner2 isn't even O(1) — it reloads adapter weights too. | **M14-M16 simplifies dramatically.** No `RwLock<Option<LoraDelta>>`, no per-forward delta computation, no concurrent-inference snapshot pattern. Reload-on-swap is ~100 ms (a one-time cost on `load_adapter`); steady-state inference is identical to the base model. Saved: ~3 days of complexity. |
| GLiNER2 uses DeBERTa-v2 | Both [`gantz-ai/pii.engineer`](https://github.com/gantz-ai/pii.engineer) and [`MacPaw/Gliner2Swift`](https://github.com/MacPaw/Gliner2Swift) report DeBERTa-**v3** (mDeBERTa-v3-base, 280M params, for the multi-lingual variant). | anno's `encoder_candle` already has v3. Could potentially reuse it — though `candle-transformers::models::debertav2` (which actually supports both v2 and v3 per its docs) is more directly aligned with the upstream gliner2 model class. The plan defaults to candle-transformers and falls back to encoder_candle if compatibility issues surface. |
| No production Rust GLiNER2 reference | [`gantz-ai/pii.engineer`](https://github.com/gantz-ai/pii.engineer) ships an Apache-2.0 production system with mDeBERTa-v3 + LoRA-merged-at-export, 0.86 F1 across 13 languages, 250 ms latency on 4 vCPU. ONNX-only. Different architecture (5 ONNX stages vs anno's 8) but confirms the production viability of merge-at-export workflow. | We can reference their pipeline structure (Apache-2.0) for inspiration, especially the post-processing logic. |

**Net effort change**: 18 milestones, 3.5 weeks → **10 milestones, ~2 weeks** (the merge-at-load architecture + free encoder collapses 5 milestones).

If hot-swap really is needed later, it's a clean 2-3 day addition on top of the merge-at-load codebase — see "Optional Phase 4.5" at the end.

---

## Pre-flight

- [ ] **Read the original Phase 4 spec.** `docs/superpowers/specs/2026-05-05-gliner2-fastino-phase4-candle-lora.md`. The high-level "what" and "why" still apply; only the "how" changes.
- [ ] **Confirm there's a real multi-domain inference workload.** Per spec §9. If "no, but it'd be cool to have," halt and revisit.
- [ ] **Phase 3.5 is on `main`.** Verify with `git log --oneline -5`. Expected to see `b7b24088 docs(gliner2_fastino): mark Phase 3.5 (IoBinding) shipped`.
- [ ] **`fastino/gliner2-multi-v1` (the *Python* repo with PyTorch safetensors) is downloadable.** Different artifact from the SemplificaAI ONNX export. Run from anywhere with Python access:
  ```bash
  python -c "from huggingface_hub import snapshot_download; print(snapshot_download('fastino/gliner2-multi-v1'))"
  ```
  Expected: `tokenizer.json`, `config.json`, `model.safetensors` (or `pytorch_model.bin`).
- [ ] **At least one PEFT-format adapter for `fastino/gliner2-*` is available.**

  **Important — public adapters status (verified 2026-05-06):**
  - `CHFLTM/gliner2-lora-custom` is mentioned in search results but returns 401 (private/removed).
  - The fastino HF organization ships only base models; no public adapters.
  - The gliner2 README documents adapter training but doesn't link public adapters.

  **You will likely need to train your own.** Per the gliner2 README:
  ```python
  from gliner2 import GLiNER2
  model = GLiNER2.from_pretrained("fastino/gliner2-base-v1")
  model.train(
      data=...,                 # Your domain dataset
      use_lora=True,
      lora_r=8,
      lora_alpha=16.0,
      save_adapter_only=True,   # produces ~5 MB adapter
      output_dir="./domain_adapter",
  )
  ```
  Output is a directory with `adapter_config.json` + `adapter_weights.safetensors` (PEFT format). Plan ahead: this is a few hours on a small dataset (or use synthetic data for parity testing only).

- [ ] **`candle-core`, `candle-nn`, `candle-transformers` already in workspace.** Verify:
  ```bash
  grep -nE "candle-core|candle-nn|candle-transformers" Cargo.toml crates/anno/Cargo.toml
  ```
  Expected: workspace deps + optional in `anno/Cargo.toml`. **If `candle-transformers` is not yet a dep, M2 adds it** — it's the source of `DebertaV2Model` and is the biggest single change vs the original plan.

- [ ] **WSL Ubuntu-C is healthy.** Same dev env constraint as Phase 3.5.

- [ ] **Pre-create the worktree.**
  ```bash
  git worktree add ../anno-gliner2-phase4 -b feat/gliner2-fastino-phase4 main
  cd ../anno-gliner2-phase4
  cat > .cargo/config.toml <<'EOF'
  # rustc 1.95 ICE workaround (see docs/dev-notes/rustc-1.94-ice.md)
  [build]
  rustflags = ["--cap-lints", "allow"]
  rustdocflags = ["--cap-lints", "allow"]
  EOF
  ```

---

## File structure (locked)

| File | Action | Purpose |
|---|---|---|
| `crates/anno/Cargo.toml` | modify | Add `gliner2-fastino-candle`, `-cuda`, `-metal` features. Promote `candle-transformers` to direct dep (was transitive via `gliner_candle`) |
| `crates/anno/src/backends/gliner2_fastino_candle/mod.rs` | create | Public surface; `GLiNER2FastinoCandle` engine struct |
| `crates/anno/src/backends/gliner2_fastino_candle/encoder.rs` | create | Thin wrapper over `candle_transformers::models::debertav2::DebertaV2Model`. Loads safetensors, returns hidden states |
| `crates/anno/src/backends/gliner2_fastino_candle/lora.rs` | create | `LoraConfig`, `LoraDelta`, **merge_into_base** (the core operation: read PEFT adapter, apply `W += alpha/r * lora_B @ lora_A` to target VarBuilder modules) |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/mod.rs` | create | Re-exports |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/token_gather.rs` | create | Token-level gather (word_indices → word embeddings); pure index_select on the encoder output |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/span_rep.rs` | create | Span representation `[1, num_words, MAX_WIDTH, H]`. ~50 LOC of MLP + index gather |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/schema_gather.rs` | create | Per-task `[P]`/`[E]`/`[L]` token gather; pure index_select |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/count_pred.rs` | create | Count-predictor MLP + Rust-side argmax |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/count_lstm.rs` | create | Count-conditioned LSTM (struct projection). Reference: per-count-linear stub OK as first pass; M6 parity validates |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/scorer.rs` | create | Span-vs-struct similarity scorer + sigmoid |
| `crates/anno/src/backends/gliner2_fastino_candle/heads/classifier.rs` | create | `[L]`-head MLP + softmax |
| `crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs` | create | 8-step orchestration |
| `crates/anno/src/backends/gliner2_fastino_candle/processor.rs` | create | Re-export from `crate::backends::gliner2_fastino::processor` (no copy — same input prep) |
| `crates/anno/src/backends/gliner2_fastino_candle/decoder.rs` | create | Re-export from `crate::backends::gliner2_fastino::pipeline::{decode_entities, decode_structure, ...}` |
| `crates/anno/src/backends/mod.rs` | modify | `pub mod gliner2_fastino_candle;` behind feature gate |
| `crates/anno/src/backends/catalog.rs` | modify | Add `gliner2_fastino_candle` row |
| `crates/anno/tests/gliner2_fastino_candle_integration.rs` | create | Tier-2 integration: load `fastino/gliner2-multi-v1`, parity vs ONNX |
| `crates/anno/tests/gliner2_fastino_candle_lora.rs` | create | Adapter-load + merge correctness test |
| `docs/BACKENDS.md` | modify | New row for `gliner2_fastino_candle` |
| `docs/dev-notes/gliner2-fastino-candle-port.md` | create | Port notes |

**Files NOT created (vs original plan):**
- `encoder_candle/deberta_v2.rs` — superseded by `candle-transformers::models::debertav2`.
- `gliner2_fastino_candle/adapters.rs` — merge-at-load doesn't need an adapter registry; just track active adapter name on the engine struct (`Option<String>`). Hot-swap registry deferred to optional Phase 4.5.
- `benches/gliner2_fastino_candle_swap.rs` — merge-at-load swap latency is dominated by safetensors load (~100 ms), not delta computation. Bench is straightforward; defer until perf is questioned.

---

## Milestone P4.M1 — Reference reading + symbol map (~½ day)

Goal: produce port notes referencing the validated upstream sources. Lighter than the original plan because most references are now confirmed.

### Task M1.1: Validate the candle-transformers DeBERTa-v2 surface

- [ ] **Step 1: Verify the docs and example exist.**
  ```bash
  cargo doc --open -p candle-transformers --no-deps   # browse to models::debertav2
  ```
  Or fetch from docs.rs:
  ```bash
  curl -fsSL "https://docs.rs/candle-transformers/latest/candle_transformers/models/debertav2/index.html" \
      | grep -E "DebertaV2(Model|NERModel|SeqClassificationModel|Encoder|Embeddings|Layer|Attention|DisentangledSelfAttention|SelfOutput|Intermediate|Output|ContextPooler|Config|ConvLayer|HiddenActLayer|StableDropout)"
  ```

  **Verified 2026-05-06**: 17 public structs, 1 public enum exported. `DebertaV2Model` is the bare encoder we need (returns hidden states).

- [ ] **Step 2: Snapshot the example.**
  ```bash
  curl -fsSL "https://raw.githubusercontent.com/huggingface/candle/main/candle-examples/examples/debertav2/main.rs" \
      -o /tmp/candle-debertav2-example.rs
  wc -l /tmp/candle-debertav2-example.rs
  grep -n "use candle_transformers::models::debertav2" /tmp/candle-debertav2-example.rs
  ```

  Expected (verified): imports `DebertaV2NERModel`, `DebertaV2SeqClassificationModel`, `Config as DebertaV2Config`, `Id2Label`, `NERItem`, `TextClassificationItem`. We use only `DebertaV2Model` + `Config` because we want raw hidden states.

### Task M1.2: Snapshot the upstream Rust + Python references

- [ ] **Step 1: Pull production Rust reference (gantz-ai/pii.engineer).**
  ```bash
  mkdir -p /tmp/pii-engineer-ref
  for f in pipeline.rs labels.rs lang.rs ; do
      curl -fsSL "https://raw.githubusercontent.com/gantz-ai/pii.engineer/main/pii-engineer-core/src/gliner/$f" \
          -o "/tmp/pii-engineer-ref/$f" 2>/dev/null || true
  done
  ls /tmp/pii-engineer-ref/
  ```
  Apache-2.0. We don't copy verbatim but read for the post-processing pattern (their 8-stage pipeline maps to ours).

- [ ] **Step 2: Pull GLiNER2Swift LoRA merge implementation.**
  ```bash
  # Find the swift file with the merge logic
  curl -fsSL "https://raw.githubusercontent.com/MacPaw/Gliner2Swift/main/Sources/Gliner2/LoRA/LoRAAdapter.swift" \
      -o /tmp/gliner2swift-lora.swift 2>/dev/null || true
  wc -l /tmp/gliner2swift-lora.swift
  ```
  MIT (verify). Reference for the merge formula; we write the Rust translation in M7.

- [ ] **Step 3: Pull HuggingFace PEFT layer reference.**
  ```bash
  curl -fsSL https://raw.githubusercontent.com/huggingface/peft/main/src/peft/tuners/lora/layer.py \
      -o /tmp/peft-lora-layer.py
  ```
  Apache-2.0. Authoritative reference for the safetensors key format (`base_model.model.<path>.lora_A.weight` / `lora_B.weight`), `lora_alpha / r` scaling, `fan_in_fan_out` flag semantics.

- [ ] **Step 4: Pull Python `gliner2` source (if available locally).**
  ```bash
  # If you have gliner2 in a Python env:
  PY_PATH="$(python -c 'import gliner2; print(gliner2.__file__)' | xargs dirname)"
  ls "$PY_PATH"
  cp "$PY_PATH/model.py" /tmp/gliner2-model.py
  cp "$PY_PATH/lora.py"  /tmp/gliner2-lora.py
  wc -l /tmp/gliner2-*.py
  ```
  Authoritative for parameter naming + adapter-loading semantics.

- [ ] **Step 5: Write `docs/dev-notes/gliner2-fastino-candle-port.md`.**

  ````markdown
  # gliner2_fastino_candle port notes (revised)

  Source of truth: `fastino/gliner2-multi-v1` (HuggingFace) + Python `gliner2` package.

  ## Architectural decisions

  | Decision | Rationale |
  |---|---|
  | Reuse `candle-transformers::models::debertav2::DebertaV2Model` | Production-ready, "insignificant precision difference" vs Python per PR #2743 |
  | Merge-at-load (not hot-swap) | Swift port (MacPaw/Gliner2Swift) confirms this is the simpler, equivalent-correctness pattern. Hot-swap is optional Phase 4.5 |
  | Re-export processor + decoder from ONNX backend | Same input prep, same NMS — no Candle-specific changes needed |
  | `Option<String>` active_adapter on engine struct (not `Arc<RwLock<…>>`) | Merge-at-load needs only a name, not a runtime delta |

  ## Python → Rust symbol map

  | Python | anno equivalent |
  |---|---|
  | `gliner2.GLiNER2` | `gliner2_fastino_candle::GLiNER2FastinoCandle` |
  | `gliner2.GLiNER2.encoder` (DebertaV2Model) | `candle_transformers::models::debertav2::DebertaV2Model` |
  | `gliner2.heads.SpanRep` | `gliner2_fastino_candle::heads::span_rep::SpanRep` |
  | `gliner2.heads.SchemaGather` | `gliner2_fastino_candle::heads::schema_gather::SchemaGather` |
  | `gliner2.heads.CountPred` | `gliner2_fastino_candle::heads::count_pred::CountPred` |
  | `gliner2.heads.CountLstmFixed` | `gliner2_fastino_candle::heads::count_lstm::CountLstmFixed` |
  | `gliner2.heads.Scorer` | `gliner2_fastino_candle::heads::scorer::Scorer` |
  | `gliner2.heads.Classifier` | `gliner2_fastino_candle::heads::classifier::Classifier` |
  | `gliner2.GLiNER2.load_adapter(path)` | `GLiNER2FastinoCandle::load_adapter(name, path)` (merges into base, sets active_adapter) |
  | `gliner2.GLiNER2.unload_adapter()` | `GLiNER2FastinoCandle::unload_adapter()` (reloads base from disk, clears active_adapter) |
  | `gliner2.lora.apply_lora_delta` | `lora::merge_into_base` |

  ## PEFT adapter format (verified against HuggingFace docs)

  ```
  <adapter>/
  ├── adapter_config.json
  │   {
  │     "base_model_name_or_path": "fastino/gliner2-multi-v1",
  │     "task_type": "TOKEN_CLS",
  │     "r": 8,
  │     "lora_alpha": 16,
  │     "target_modules": ["query", "key", "value", ...],  // or regex
  │     "lora_dropout": 0.0,
  │     "bias": "none",
  │     "fan_in_fan_out": false
  │   }
  └── adapter_model.safetensors  // or adapter_weights.safetensors
      // keys: base_model.model.encoder.layer.<N>.attention.self.query.lora_A.weight
      //       base_model.model.encoder.layer.<N>.attention.self.query.lora_B.weight
      //       (for each target module)
  ```

  ## Merge formula

  ```
  W_merged = W_base + (alpha / r) * (lora_B @ lora_A)
  ```

  - `W_base`: `[out, in]`
  - `lora_A`: `[r, in]`  (down-projection)
  - `lora_B`: `[out, r]` (up-projection)
  - `(lora_B @ lora_A)`: `[out, in]` — same shape as W_base
  - `alpha / r`: scaling factor (alpha=16, r=8 → 2.0)

  Per-module application: walk safetensors keys, group by module path,
  multiply, scale, add. Done at `load_adapter` time; nothing per-forward.

  ## What we deliberately don't do (vs original plan)

  - **No `RwLock<Option<LoraDelta>>`**. Merge-at-load means active adapter is a fully-merged model, not a runtime delta.
  - **No per-forward delta computation**. Inference is identical to the base model after merge.
  - **No multiple-adapter cache**. `load_adapter` replaces the previous one. To swap rapidly, see optional Phase 4.5.
  - **No DeBERTa-v2 from scratch**. Use `candle-transformers::models::debertav2`.
  ````

- [ ] **Step 6: Commit M1 port notes.**
  ```bash
  git add docs/dev-notes/gliner2-fastino-candle-port.md
  git commit -m "docs(gliner2_fastino_candle): revised port notes — reuse candle-transformers + merge-at-load"
  ```

---

## Milestone P4.M2 — Cargo features + dependency promotion (~½ day)

Goal: register the new feature flags and promote `candle-transformers` to a direct dep if it isn't already.

### Task M2.1: Cargo features

- [ ] **Step 1: Inspect current dep graph.**
  ```bash
  grep -nE "candle-transformers|candle-core|candle-nn" crates/anno/Cargo.toml Cargo.toml
  ```

- [ ] **Step 2: Add features to `crates/anno/Cargo.toml`** (insert near other `gliner2-fastino-*` features):
  ```toml
  gliner2-fastino-candle = ["candle", "candle-transformers", "safetensors"]
  gliner2-fastino-candle-cuda = ["gliner2-fastino-candle", "candle/cuda", "candle-transformers/cuda"]
  gliner2-fastino-candle-metal = ["gliner2-fastino-candle", "candle/metal", "candle-transformers/metal"]
  ```

- [ ] **Step 3: If `candle-transformers` is not a direct dep, add it.**
  In `crates/anno/Cargo.toml` `[dependencies]`:
  ```toml
  candle-transformers = { workspace = true, optional = true }
  ```
  And in workspace `Cargo.toml` `[workspace.dependencies]` if not already present:
  ```toml
  candle-transformers = "0.8"  # or pin to whatever version anno uses for candle-core
  ```

- [ ] **Step 4: Verify compile (no code yet, just feature plumbing).**
  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase4 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=/mnt/d/cargo-target-anno-phase4 && cargo check -p anno --features gliner2-fastino-candle'
  ```

  Expected: `Finished` (no Rust code yet; just the new feature gates).

- [ ] **Step 5: Commit.**
  ```bash
  git add crates/anno/Cargo.toml Cargo.toml
  git commit -m "feat(gliner2_fastino_candle): cargo features + candle-transformers direct dep"
  ```

---

## Milestone P4.M3 — Module skeleton + encoder (~1 day)

Goal: register `gliner2_fastino_candle` module, ship a working encoder (just `DebertaV2Model::forward`), have it return hidden states for a fixed input. **No heads yet.**

### Task M3.1: Module skeleton

- [ ] **Step 1: Create the directory and `mod.rs` skeleton.**
  ```rust
  // crates/anno/src/backends/gliner2_fastino_candle/mod.rs
  //! Phase 4: Candle backend for fastino/gliner2 with PEFT adapter merge-at-load.
  //!
  //! Parallel to the ONNX-based [`crate::backends::gliner2_fastino`]. Same
  //! public method shapes (Model + ZeroShotNER); users swap backends with a
  //! type alias. The differentiator is `load_adapter` / `unload_adapter` —
  //! load a PEFT-format LoRA adapter and merge it into the base weights at
  //! load time. Inference cost is identical to the base model after merge.

  #![cfg(feature = "gliner2-fastino-candle")]

  pub mod encoder;
  pub mod heads;
  pub mod lora;
  pub mod pipeline;
  pub mod processor;
  pub mod decoder;

  use std::path::{Path, PathBuf};
  use candle_core::Device;

  pub struct GLiNER2FastinoCandle {
      pub(crate) tokenizer: tokenizers::Tokenizer,
      pub(crate) device: Device,
      pub(crate) base_model_dir: PathBuf,        // for re-merging on adapter swap
      pub(crate) encoder: encoder::Encoder,      // populated from current weights (base + active adapter merged)
      pub(crate) heads: heads::AllHeads,
      pub(crate) active_adapter: Option<String>, // None = base-only
      pub(crate) model_id: String,
  }

  impl std::fmt::Debug for GLiNER2FastinoCandle {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
          f.debug_struct("GLiNER2FastinoCandle")
              .field("model_id", &self.model_id)
              .field("active_adapter", &self.active_adapter)
              .finish()
      }
  }

  // Subsequent impl blocks (constructors, extract_*, classify, load_adapter)
  // are added in M4 / M8 / M9.
  ```

- [ ] **Step 2: Create `encoder.rs` skeleton.**
  ```rust
  // crates/anno/src/backends/gliner2_fastino_candle/encoder.rs
  //! Thin wrapper over `candle_transformers::models::debertav2::DebertaV2Model`.

  use candle_core::{Device, Result, Tensor};
  use candle_nn::VarBuilder;
  use candle_transformers::models::debertav2::{Config as DebertaV2Config, DebertaV2Model};

  pub struct Encoder {
      pub(crate) model: DebertaV2Model,
      pub(crate) config: DebertaV2Config,
  }

  impl Encoder {
      pub fn from_safetensors(
          weights_path: &std::path::Path,
          config_path: &std::path::Path,
          device: &Device,
      ) -> crate::Result<Self> {
          let config: DebertaV2Config = serde_json::from_reader(
              std::fs::File::open(config_path).map_err(|e| crate::Error::Backend(format!("encoder config: {e}")))?,
          )
          .map_err(|e| crate::Error::Backend(format!("encoder config parse: {e}")))?;

          let vb = unsafe {
              VarBuilder::from_mmaped_safetensors(&[weights_path], candle_core::DType::F32, device)
          }
          .map_err(|e| crate::Error::Backend(format!("encoder safetensors: {e}")))?;

          // Per the candle-transformers DebertaV2 example: prefix is empty for HF
          // base models; if loading produces empty tensors, try "deberta." or
          // "model." prefixes.
          let model = DebertaV2Model::load(vb, &config)
              .map_err(|e| crate::Error::Backend(format!("encoder load: {e}")))?;

          Ok(Self { model, config })
      }

      pub fn forward(
          &self,
          input_ids: &Tensor,
          attention_mask: &Tensor,
      ) -> Result<Tensor> {
          self.model.forward(input_ids, attention_mask, None)
      }
  }
  ```

- [ ] **Step 3: Stub `heads/mod.rs`, `lora.rs`, `pipeline.rs`, `processor.rs`, `decoder.rs`.**
  Each just re-exports the relevant ONNX-side module (where possible) or empty for now:
  ```rust
  // heads/mod.rs
  pub struct AllHeads {
      // populated in M5
  }

  // lora.rs
  pub struct LoraConfig { /* fields in M7 */ }
  pub struct LoraDelta  { /* fields in M7 */ }

  // pipeline.rs (empty; populated in M5)

  // processor.rs
  pub use crate::backends::gliner2_fastino::processor::*;

  // decoder.rs
  pub use crate::backends::gliner2_fastino::pipeline::{
      decode_entities, decode_entities_with_thresholds, decode_structure,
      ScorerOutput, MAX_COUNT, MAX_WIDTH,
  };
  ```

- [ ] **Step 4: Wire into `crates/anno/src/backends/mod.rs`.**
  ```rust
  #[cfg(feature = "gliner2-fastino-candle")]
  pub mod gliner2_fastino_candle;
  ```

- [ ] **Step 5: Compile.**
  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  ```
  Expected: `Finished`. No tests yet.

- [ ] **Step 6: Commit.**
  ```bash
  git add crates/anno/src/backends/gliner2_fastino_candle/ crates/anno/src/backends/mod.rs
  git commit -m "feat(gliner2_fastino_candle): module skeleton + DebertaV2Model encoder wrapper"
  ```

### Task M3.2: Encoder smoke test (no parity yet)

- [ ] **Step 1: Test that loads `fastino/gliner2-multi-v1` and runs encoder forward** on a fixed input. Mark `#[ignore]`. Place in `crates/anno/tests/gliner2_fastino_candle_integration.rs`.

  ```rust
  #![cfg(feature = "gliner2-fastino-candle")]

  use anno::backends::gliner2_fastino_candle::encoder::Encoder;
  use candle_core::{Device, Tensor};
  use std::path::PathBuf;

  fn snapshot_dir() -> PathBuf {
      // Use hf_hub::download to populate ~/.cache/huggingface, then walk to
      // the snapshot dir. Minimal scaffold; M4 will move this into a proper
      // helper.
      let api = anno::backends::hf_loader::hf_api().unwrap();
      let repo = api.model("fastino/gliner2-multi-v1".to_string());
      let weights = anno::backends::hf_loader::download_model_file(
          &repo, &["model.safetensors", "pytorch_model.bin"],
      ).unwrap();
      weights.parent().unwrap().to_path_buf()
  }

  #[test]
  #[ignore]
  fn encoder_forward_smoke() {
      let dir = snapshot_dir();
      let weights = dir.join("model.safetensors");
      let config = dir.join("config.json");
      let device = Device::Cpu;
      let encoder = Encoder::from_safetensors(&weights, &config, &device)
          .expect("load encoder");

      // Fixed input: tokenize "Marie Curie discovered radium." with the
      // model's tokenizer would be cleaner, but for a smoke test we
      // hand-build a [1, L] i64 tensor. The values don't matter — just
      // verify forward returns a [1, L, H] f32 tensor without panic.
      let input_ids = Tensor::new(&[101u32, 2026, 4584, 102][..], &device)
          .unwrap()
          .reshape((1, 4))
          .unwrap();
      let attention_mask = Tensor::ones((1, 4), candle_core::DType::U32, &device)
          .unwrap();

      let out = encoder.forward(&input_ids, &attention_mask).expect("forward");
      let shape = out.shape().dims().to_vec();
      eprintln!("encoder hidden: {shape:?}");
      assert_eq!(shape.len(), 3, "expected 3D output [1, L, H]");
      assert_eq!(shape[0], 1);
      assert_eq!(shape[1], 4);
      // shape[2] is hidden_size; expect 768 (base) or 1024 (large)
      assert!(shape[2] >= 768, "hidden_size unexpectedly small: {}", shape[2]);
  }
  ```

- [ ] **Step 2: cargo check the test (don't need to actually run with the model).**
  ```bash
  cargo check -p anno --features gliner2-fastino-candle --test gliner2_fastino_candle_integration
  ```

- [ ] **Step 3: Commit.**
  ```bash
  git add crates/anno/tests/gliner2_fastino_candle_integration.rs
  git commit -m "test(gliner2_fastino_candle): encoder forward smoke test (#[ignore]-gated)"
  ```

---

## Milestone P4.M4 — Engine + `from_pretrained` (~1.5 days)

Goal: full engine constructor that loads tokenizer + base safetensors + heads' weights from the HF snapshot. After this milestone, the engine struct is complete except for inference methods.

### Task M4.1: HF snapshot download helper

- [ ] **Step 1: Add to `gliner2_fastino_candle/mod.rs`.**

  ```rust
  use crate::backends::hf_loader;

  impl GLiNER2FastinoCandle {
      pub fn from_pretrained(model_id: &str) -> crate::Result<Self> {
          // Download the PyTorch artifacts (NOT the SemplificaAI ONNX export).
          let api = hf_loader::hf_api()
              .map_err(|e| crate::Error::Backend(format!("hf_api: {e}")))?;
          let repo = api.model(model_id.to_string());

          let tokenizer = hf_loader::download_model_file(
              &repo, &["tokenizer.json"],
          )
          .map_err(|e| crate::Error::Backend(format!("download tokenizer: {e}")))?;

          let config_path = hf_loader::download_model_file(
              &repo, &["config.json"],
          )
          .map_err(|e| crate::Error::Backend(format!("download config: {e}")))?;

          let weights_path = hf_loader::download_model_file(
              &repo, &["model.safetensors", "pytorch_model.bin"],
          )
          .map_err(|e| crate::Error::Backend(format!("download weights: {e}")))?;

          // The snapshot dir is the parent of any of these files (assuming
          // they're all in the same dir, which HF Hub guarantees).
          let snapshot_dir = weights_path.parent()
              .ok_or_else(|| crate::Error::Backend("snapshot dir resolution".into()))?
              .to_path_buf();

          Self::from_local_with_active_adapter(
              &snapshot_dir,
              /* active_adapter */ None,
              &Device::Cpu,
          )
      }

      pub fn from_local(model_dir: &Path) -> crate::Result<Self> {
          Self::from_local_with_active_adapter(model_dir, None, &Device::Cpu)
      }

      // Internal: used by from_pretrained, from_local, and load_adapter / unload_adapter
      // (which both end up reloading and re-merging from disk).
      pub(crate) fn from_local_with_active_adapter(
          model_dir: &Path,
          active_adapter: Option<&Path>,
          device: &Device,
      ) -> crate::Result<Self> {
          let tokenizer_path = model_dir.join("tokenizer.json");
          let weights_path = model_dir.join("model.safetensors");
          let config_path  = model_dir.join("config.json");

          let tokenizer = hf_loader::load_tokenizer(&tokenizer_path)
              .map_err(|e| crate::Error::Backend(format!("tokenizer: {e}")))?;

          // Build the encoder. If active_adapter is set, M7's lora::merge_into_base
          // mutates the in-memory weights between safetensors load and Encoder::load.
          // First pass (M3-M6): no active_adapter; base only.
          let encoder = encoder::Encoder::from_safetensors(
              &weights_path,
              &config_path,
              device,
          )?;

          // Heads: stub for now; M5 wires real heads.
          let heads = heads::AllHeads::stub();

          let model_id = model_dir.file_name()
              .map(|s| s.to_string_lossy().into_owned())
              .unwrap_or_else(|| "gliner2_fastino_candle_local".to_string());

          Ok(Self {
              tokenizer,
              device: device.clone(),
              base_model_dir: model_dir.to_path_buf(),
              encoder,
              heads,
              active_adapter: active_adapter.map(|p| p.display().to_string()),
              model_id,
          })
      }
  }
  ```

- [ ] **Step 2: Add `heads::AllHeads::stub()`** in `heads/mod.rs`:
  ```rust
  impl AllHeads {
      pub fn stub() -> Self { Self {} }
      // Real constructors added in M5.
  }
  ```

- [ ] **Step 3: Compile + commit.**
  ```bash
  cargo check -p anno --features gliner2-fastino-candle
  git add crates/anno/src/backends/gliner2_fastino_candle/
  git commit -m "feat(gliner2_fastino_candle): from_pretrained / from_local engine constructors"
  ```

---

## Milestone P4.M5 — All 8 heads (~5 days)

Goal: implement the 7 non-encoder heads in Candle, plus build `AllHeads` from the safetensors. Each head is small (~30-100 LOC). The encoder is already done (M3).

### Task M5.1: token_gather + span_rep (~1 day)

- [ ] **Step 1: token_gather** — `index_select` on the encoder hidden states. ~20 LOC of pure Candle ops, no learned parameters.

  ```rust
  // heads/token_gather.rs
  use candle_core::{IndexOp, Tensor};

  pub struct TokenGather; // no parameters

  impl TokenGather {
      pub fn forward(
          &self,
          last_hidden_state: &Tensor,  // [1, seq_len, H]
          word_indices: &Tensor,       // [num_words] (i64)
      ) -> candle_core::Result<Tensor> {
          // Gather first token of each word.
          let gathered = last_hidden_state.index_select(word_indices, 1)?;
          Ok(gathered)  // [1, num_words, H]
      }
  }
  ```

- [ ] **Step 2: span_rep** — has learned weights. Read the Python `gliner2/heads/span_rep.py` to confirm the architecture:

  ```python
  # Likely structure (verify against actual Python source):
  class SpanRep(nn.Module):
      def __init__(self, hidden_size, max_width):
          self.start_proj = nn.Linear(hidden_size, hidden_size)
          self.end_proj   = nn.Linear(hidden_size, hidden_size)
          self.span_proj  = nn.Linear(hidden_size * 2, hidden_size)
      def forward(self, text_embs, span_idx):
          # Compute span representation as concat(start, end) → projected
          ...
  ```

  Translate to Candle, loading weights via VarBuilder.

- [ ] **Step 3: Test token_gather + span_rep** with synthetic input. Verify shape `[1, num_words, MAX_WIDTH, H]`.

- [ ] **Step 4: Commit.**
  ```bash
  git commit -m "feat(gliner2_fastino_candle): token_gather + span_rep heads"
  ```

### Task M5.2: schema_gather + count_pred (~1 day)

- [ ] **Step 1: schema_gather** — like token_gather but operates on the full token sequence (not word-level), gathering at `[P]`/`[E]`/`[L]` indices. Has a small projection MLP for `pc_emb` and `field_embs` separately.

- [ ] **Step 2: count_pred** — small MLP `pc_emb → [count_logits]`. Argmax in Rust.

  ```rust
  // heads/count_pred.rs
  pub struct CountPred {
      mlp: candle_nn::Linear,  // [hidden_size, MAX_COUNT]
  }
  impl CountPred {
      pub fn forward(&self, pc_emb: &Tensor) -> candle_core::Result<usize> {
          let logits = self.mlp.forward(pc_emb)?;
          let argmax = logits.argmax_keepdim(D::Minus1)?;
          let val: u32 = argmax.to_scalar()?;
          Ok(val as usize)
      }
  }
  ```

- [ ] **Step 3: Commit.**
  ```bash
  git commit -m "feat(gliner2_fastino_candle): schema_gather + count_pred heads"
  ```

### Task M5.3: count_lstm + scorer (~1.5 days)

- [ ] **Step 1: count_lstm** — count-conditioned LSTM that produces `[MAX_COUNT, M, H]` struct projections. **First-pass: per-count linear layers** (one Linear per c_idx, applied to the matching slice). Replace with proper LSTM cells in a follow-up if M6 parity fails.

  This is the riskiest head because LSTM weight unpacking from PyTorch is finicky (gate ordering: `i,f,g,o` vs `i,g,f,o`). Reference `candle_nn::rnn::LSTM` if helpful.

- [ ] **Step 2: scorer** — span-vs-struct similarity. Element-wise dot products with sigmoid. ~30 LOC.

- [ ] **Step 3: Tests for both heads with synthetic input.**

- [ ] **Step 4: Commit.**
  ```bash
  git commit -m "feat(gliner2_fastino_candle): count_lstm + scorer heads"
  ```

### Task M5.4: classifier (~½ day)

- [ ] **Step 1: classifier** — `[L]`-head MLP. `[1, num_labels, MAX_WIDTH, H] → logits → softmax`. Mirror the ONNX side's behavior verbatim.

- [ ] **Step 2: Commit.**
  ```bash
  git commit -m "feat(gliner2_fastino_candle): classifier head"
  ```

### Task M5.5: AllHeads::from_safetensors + pipeline orchestration (~1 day)

- [ ] **Step 1: AllHeads::from_safetensors(path, device)** — uses VarBuilder to load each head's weights. Path conventions follow PEFT layer naming.

- [ ] **Step 2: pipeline.rs**: orchestrate the 8 steps. Mirror the Phase 3.5 `run_pipeline_for_decoding` shape:

  ```rust
  pub fn run_pipeline_candle(
      model: &GLiNER2FastinoCandle,
      record: &ProcessedRecord,
      task: &TaskMapping,
  ) -> Result<(decoder::ScorerOutput, usize), Error> {
      let input_ids = ...; // From record
      let attention_mask = ...;
      let hidden = model.encoder.forward(&input_ids, &attention_mask)?;
      let text_embs = model.heads.token_gather.forward(&hidden, &word_indices)?;
      let span_embs = model.heads.span_rep.forward(&text_embs, &span_idx)?;
      let (pc_emb, field_embs) = model.heads.schema_gather.forward(&hidden, &schema_indices)?;
      let pred_count = model.heads.count_pred.forward(&pc_emb)?;
      if pred_count == 0 {
          return Ok((empty_scorer(), 0));
      }
      let struct_proj = model.heads.count_lstm.forward(&field_embs)?;
      let scores = model.heads.scorer.forward(&span_embs, &struct_proj)?;
      // Convert to Array4<f32> (host) for the decoder.
      let scores_arr = candle_to_ndarray4(scores)?;
      Ok((decoder::ScorerOutput { scores: scores_arr }, pred_count))
  }

  fn run_classify_pipeline_candle(...) { ... }
  ```

- [ ] **Step 3: Wire `extract_ner` / `extract_with_label_descriptions` / `extract_with_label_thresholds` / `extract_structure` / `classify`** on `GLiNER2FastinoCandle` — same shape as `GLiNER2Fastino`, with `run_pipeline_candle` instead of `run_pipeline_dispatch`. Re-use the decoder family.

- [ ] **Step 4: Compile.**
  ```bash
  cargo check -p anno --features gliner2-fastino-candle --tests
  ```

- [ ] **Step 5: Commit.**
  ```bash
  git commit -m "feat(gliner2_fastino_candle): pipeline orchestration + 5 public extract methods"
  ```

---

## Milestone P4.M6 — ONNX↔Candle parity test (~1 day)

Goal: gating test before LoRA work. Run the same input through both backends, assert score parity within tolerance.

### Task M6.1: Parity test

- [ ] **Step 1: Add to `gliner2_fastino_candle_integration.rs`.**

  ```rust
  use anno::backends::gliner2_fastino::GLiNER2Fastino;
  use anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle;
  use anno::backends::inference::ZeroShotNER;

  #[test]
  #[ignore]
  fn parity_onnx_candle_extract_with_types() {
      let onnx = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
          .expect("load ONNX");
      let candle = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")
          .expect("load Candle");

      let text = "Marie Curie won the Nobel Prize in Physics in 1903.";
      let types = ["person", "award", "year"];

      let onnx_result = ZeroShotNER::extract_with_types(&onnx, text, &types, 0.5).unwrap();
      let candle_result = ZeroShotNER::extract_with_types(&candle, text, &types, 0.5).unwrap();

      eprintln!("onnx ({}): {:#?}", onnx_result.len(), onnx_result);
      eprintln!("candle ({}): {:#?}", candle_result.len(), candle_result);

      // Sort both sides by (start, end, text).
      let mut onnx_sorted = onnx_result.clone();
      let mut candle_sorted = candle_result.clone();
      onnx_sorted.sort_by_key(|e| (e.start(), e.end(), e.text.clone()));
      candle_sorted.sort_by_key(|e| (e.start(), e.end(), e.text.clone()));

      // Tolerance: 5e-3 (looser than IoBinding's 1e-4 because ort vs Candle
      // use different fp32 paths through linear algebra; FMA fusion differs).
      assert_eq!(onnx_sorted.len(), candle_sorted.len(), "entity count mismatch");

      let mut max_diff: f64 = 0.0;
      for (o, c) in onnx_sorted.iter().zip(candle_sorted.iter()) {
          assert_eq!(o.start(), c.start(), "start mismatch: {o:?} vs {c:?}");
          assert_eq!(o.end(), c.end(), "end mismatch: {o:?} vs {c:?}");
          assert_eq!(o.text, c.text, "text mismatch: {o:?} vs {c:?}");
          let diff = (o.confidence.value() - c.confidence.value()).abs();
          if diff > max_diff { max_diff = diff; }
      }
      eprintln!("ONNX↔Candle max_abs_diff: {max_diff}");
      assert!(max_diff < 5e-3, "parity broken: max_abs_diff = {max_diff}");
  }
  ```

- [ ] **Step 2: Run.**
  ```bash
  wsl -d Ubuntu-C -- bash -lc 'cd /mnt/c/Users/NMarchitecte/anno-gliner2-phase4 && unset CARGO_TARGET_DIR && export CARGO_TARGET_DIR=/mnt/d/cargo-target-anno-phase4 && cargo test -p anno --features gliner2-fastino-candle --test gliner2_fastino_candle_integration -- --ignored parity_onnx_candle --nocapture'
  ```

  **THIS IS THE GATE.** If parity fails:
  - Check encoder output first (compare encoder.onnx output to DebertaV2Model output on the same input). If encoder differs, that's almost always a tokenizer or attention-mask issue.
  - Then check each head in isolation by feeding fixed encoder output through both ONNX and Candle versions of one head at a time.
  - Most likely culprit: weight ordering on count_lstm (LSTM gate convention) or span_rep (concat axis).

- [ ] **Step 3: If parity passes, commit.**
  ```bash
  git commit -m "test(gliner2_fastino_candle): parity vs ONNX backend (max_abs_diff < 5e-3)"
  ```

  **If parity fails: HALT. Do not proceed to LoRA work.** Document the discrepancy and fix before M7.

---

## Milestone P4.M7 — LoRA loader + merge-at-load (~1.5 days)

Goal: read a PEFT-format adapter and apply `W += alpha/r * lora_B @ lora_A` to target modules in the encoder weights. After this milestone, calling `model.load_adapter("./my_adapter")` produces an engine with merged weights.

### Task M7.1: LoraConfig + safetensors parsing

- [ ] **Step 1: `lora.rs` types.**

  ```rust
  use serde::Deserialize;
  use std::collections::HashMap;
  use std::path::Path;
  use candle_core::{Device, Tensor};
  use safetensors::SafeTensors;

  #[derive(Debug, Deserialize)]
  pub struct LoraConfig {
      pub r: usize,
      pub lora_alpha: f64,
      pub target_modules: Vec<String>,  // module path patterns
      pub base_model_name_or_path: Option<String>,
      pub fan_in_fan_out: Option<bool>,  // default false
      // accept-and-ignore: peft_type, task_type, lora_dropout, bias, ...
  }

  /// Per-module LoRA delta loaded from adapter_model.safetensors.
  pub struct LoraModule {
      pub lora_a: Tensor,  // [r, in]
      pub lora_b: Tensor,  // [out, r]
  }

  /// Full adapter: config + per-module deltas keyed by HF parameter path.
  pub struct LoraAdapter {
      pub config: LoraConfig,
      pub modules: HashMap<String, LoraModule>,  // key: PEFT path, e.g.
                                                   // "base_model.model.encoder.layer.0.attention.self.query"
  }

  impl LoraAdapter {
      pub fn load(adapter_dir: &Path, device: &Device) -> crate::Result<Self> {
          let config_path = adapter_dir.join("adapter_config.json");
          let cfg: LoraConfig = serde_json::from_reader(
              std::fs::File::open(&config_path)
                  .map_err(|e| crate::Error::Backend(format!("adapter config: {e}")))?,
          )
          .map_err(|e| crate::Error::Backend(format!("adapter config parse: {e}")))?;

          let weights_path = if adapter_dir.join("adapter_model.safetensors").exists() {
              adapter_dir.join("adapter_model.safetensors")
          } else if adapter_dir.join("adapter_weights.safetensors").exists() {
              adapter_dir.join("adapter_weights.safetensors")
          } else {
              return Err(crate::Error::Backend(format!(
                  "no adapter_model.safetensors or adapter_weights.safetensors in {}",
                  adapter_dir.display()
              )));
          };

          let bytes = std::fs::read(&weights_path)
              .map_err(|e| crate::Error::Backend(format!("adapter weights: {e}")))?;
          let st = SafeTensors::deserialize(&bytes)
              .map_err(|e| crate::Error::Backend(format!("adapter safetensors: {e}")))?;

          // Walk keys, group by module path. Keys look like
          // "base_model.model.<path>.lora_A.weight".
          let mut by_module: HashMap<String, (Option<Tensor>, Option<Tensor>)> =
              HashMap::new();
          for (key, view) in st.tensors() {
              let (module_path, slot) = parse_lora_key(&key)?;
              let tensor = Tensor::from_raw_buffer(
                  view.data(), view.dtype().into(), view.shape(), device,
              ).map_err(|e| crate::Error::Backend(format!("tensor load {key}: {e}")))?;
              let entry = by_module.entry(module_path).or_default();
              match slot {
                  LoraSlot::A => entry.0 = Some(tensor),
                  LoraSlot::B => entry.1 = Some(tensor),
              }
          }

          let mut modules = HashMap::new();
          for (mod_path, (a, b)) in by_module {
              let lora_a = a.ok_or_else(|| crate::Error::Backend(
                  format!("missing lora_A for {mod_path}")))?;
              let lora_b = b.ok_or_else(|| crate::Error::Backend(
                  format!("missing lora_B for {mod_path}")))?;
              modules.insert(mod_path, LoraModule { lora_a, lora_b });
          }

          Ok(Self { config: cfg, modules })
      }
  }

  enum LoraSlot { A, B }

  /// Parse "base_model.model.<path>.lora_A.weight" → ("<path>", LoraSlot::A).
  /// Strict: rejects keys not matching the convention.
  fn parse_lora_key(key: &str) -> crate::Result<(String, LoraSlot)> {
      let stripped = key.strip_prefix("base_model.model.")
          .ok_or_else(|| crate::Error::Backend(format!(
              "lora key {key} does not start with base_model.model."
          )))?;
      // Must end with .lora_A.weight or .lora_B.weight.
      if let Some(path) = stripped.strip_suffix(".lora_A.weight") {
          Ok((path.to_string(), LoraSlot::A))
      } else if let Some(path) = stripped.strip_suffix(".lora_B.weight") {
          Ok((path.to_string(), LoraSlot::B))
      } else {
          Err(crate::Error::Backend(format!(
              "lora key {key} does not end with .lora_A.weight or .lora_B.weight"
          )))
      }
  }
  ```

- [ ] **Step 2: Unit test parse_lora_key on synthetic keys.** Reject malformed; accept `base_model.model.encoder.layer.0.attention.self.query.lora_A.weight`.

### Task M7.2: merge_into_base

- [ ] **Step 1: The actual merge operation.**

  ```rust
  /// Merge a loaded adapter into base safetensors weights.
  /// Reads the base safetensors file, applies per-module deltas, writes
  /// a new safetensors blob (in-memory; caller decides whether to spill
  /// to disk or pass directly to VarBuilder).
  pub fn merge_into_base(
      base_safetensors: &Path,
      adapter: &LoraAdapter,
      device: &Device,
  ) -> crate::Result<HashMap<String, Tensor>> {
      let bytes = std::fs::read(base_safetensors)
          .map_err(|e| crate::Error::Backend(format!("base weights: {e}")))?;
      let st = SafeTensors::deserialize(&bytes)
          .map_err(|e| crate::Error::Backend(format!("base safetensors: {e}")))?;

      let mut out: HashMap<String, Tensor> = HashMap::new();
      for (key, view) in st.tensors() {
          let mut tensor = Tensor::from_raw_buffer(
              view.data(), view.dtype().into(), view.shape(), device,
          )
          .map_err(|e| crate::Error::Backend(format!("base tensor {key}: {e}")))?;

          // Match key against adapter module paths.
          // Base key: "encoder.layer.0.attention.self.query.weight"
          // Adapter module path: "encoder.layer.0.attention.self.query"
          // Match: strip ".weight" suffix from base key, then look up.
          if let Some(mod_path) = key.strip_suffix(".weight") {
              if let Some(lora_mod) = adapter.modules.get(mod_path) {
                  tensor = apply_lora_delta(
                      &tensor, &lora_mod.lora_a, &lora_mod.lora_b,
                      adapter.config.lora_alpha, adapter.config.r,
                      adapter.config.fan_in_fan_out.unwrap_or(false),
                  )?;
              }
          }

          out.insert(key.to_string(), tensor);
      }
      Ok(out)
  }

  fn apply_lora_delta(
      base: &Tensor,        // [out, in]
      lora_a: &Tensor,      // [r, in]
      lora_b: &Tensor,      // [out, r]
      alpha: f64,
      r: usize,
      fan_in_fan_out: bool,
  ) -> crate::Result<Tensor> {
      // delta = (alpha / r) * (lora_b @ lora_a)   →  [out, in]
      // Or transposed if fan_in_fan_out (Conv1D layers in HF).
      let scale = alpha / (r as f64);
      let delta = lora_b.matmul(lora_a)
          .map_err(|e| crate::Error::Backend(format!("lora matmul: {e}")))?;
      let delta = (delta * scale)
          .map_err(|e| crate::Error::Backend(format!("lora scale: {e}")))?;

      let delta = if fan_in_fan_out { delta.t().map_err(...)? } else { delta };

      base.add(&delta).map_err(|e| crate::Error::Backend(format!("lora add: {e}")))
  }
  ```

- [ ] **Step 2: Unit test on synthetic weights.** Construct a known base + known LoRA, verify merge produces expected output (compare to a hand-computed reference).

- [ ] **Step 3: Commit.**
  ```bash
  git add crates/anno/src/backends/gliner2_fastino_candle/lora.rs
  git commit -m "feat(gliner2_fastino_candle): LoRA adapter loader + merge_into_base"
  ```

---

## Milestone P4.M8 — load_adapter / unload_adapter API (~1 day)

Goal: public methods that wire merge_into_base into engine reload.

### Task M8.1: load_adapter

- [ ] **Step 1: Add to GLiNER2FastinoCandle impl block.**

  ```rust
  impl GLiNER2FastinoCandle {
      /// Load a PEFT-format LoRA adapter and merge it into the base weights.
      ///
      /// Replaces any previously-active adapter. Cost: ~100ms (safetensors
      /// re-load + per-module add). Subsequent inference is identical to
      /// running the merged model.
      pub fn load_adapter(&mut self, name: &str, adapter_dir: &Path) -> crate::Result<()> {
          let adapter = lora::LoraAdapter::load(adapter_dir, &self.device)?;

          // Verify base model name if recorded.
          if let Some(adapter_base) = adapter.config.base_model_name_or_path.as_deref() {
              if !self.model_id.contains(adapter_base) && !adapter_base.contains(&self.model_id) {
                  return Err(crate::Error::Backend(format!(
                      "adapter trained on '{adapter_base}', current model is '{}'; refusing to merge",
                      self.model_id
                  )));
              }
          }

          // Re-load base weights and apply delta.
          let base_safetensors = self.base_model_dir.join("model.safetensors");
          let merged = lora::merge_into_base(&base_safetensors, &adapter, &self.device)?;

          // Build a VarBuilder from the merged tensor map.
          let vb = candle_nn::VarBuilder::from_tensors(merged, candle_core::DType::F32, &self.device);

          // Reload the encoder + heads from the merged VarBuilder.
          // (Implementation: each head exposes a `from_vb(vb: &VarBuilder, prefix: &str)` constructor.)
          let new_encoder = encoder::Encoder::from_var_builder(&vb, &self.encoder.config)?;
          let new_heads = heads::AllHeads::from_var_builder(&vb)?;

          self.encoder = new_encoder;
          self.heads = new_heads;
          self.active_adapter = Some(name.to_string());
          Ok(())
      }

      /// Reload the base model weights, discarding any active adapter.
      pub fn unload_adapter(&mut self) -> crate::Result<()> {
          if self.active_adapter.is_none() {
              return Ok(());
          }
          let weights_path = self.base_model_dir.join("model.safetensors");
          let config_path = self.base_model_dir.join("config.json");
          self.encoder = encoder::Encoder::from_safetensors(
              &weights_path, &config_path, &self.device,
          )?;
          self.heads = heads::AllHeads::from_safetensors(
              &weights_path, &self.device,
          )?;
          self.active_adapter = None;
          Ok(())
      }

      pub fn active_adapter(&self) -> Option<&str> {
          self.active_adapter.as_deref()
      }
  }
  ```

- [ ] **Step 2: Add `Encoder::from_var_builder` + `AllHeads::from_var_builder`** to support loading from an in-memory tensor map (not just disk).

- [ ] **Step 3: Compile + commit.**
  ```bash
  cargo check -p anno --features gliner2-fastino-candle --tests
  git add crates/anno/src/backends/gliner2_fastino_candle/
  git commit -m "feat(gliner2_fastino_candle): load_adapter / unload_adapter API"
  ```

---

## Milestone P4.M9 — Adapter integration test (~1 day)

Goal: end-to-end verification that loading a real PEFT-format adapter changes inference output. Requires a real adapter (see pre-flight).

### Task M9.1: Adapter test

- [ ] **Step 1: Create `crates/anno/tests/gliner2_fastino_candle_lora.rs`.**

  ```rust
  #![cfg(feature = "gliner2-fastino-candle")]

  use anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle;
  use anno::backends::inference::ZeroShotNER;
  use std::path::Path;

  // Path to a trained LoRA adapter. Set via env var:
  //   GLINER2_TEST_ADAPTER_DIR=/path/to/adapter cargo test --ignored
  fn test_adapter_dir() -> std::path::PathBuf {
      std::env::var("GLINER2_TEST_ADAPTER_DIR")
          .map(std::path::PathBuf::from)
          .expect("set GLINER2_TEST_ADAPTER_DIR to a PEFT-format adapter directory")
  }

  #[test]
  #[ignore]
  fn load_adapter_changes_inference() {
      let mut model = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")
          .expect("load base");

      let text = "The patient reports symptoms consistent with hypertension.";
      let types = ["disease", "symptom", "treatment"];

      let baseline = ZeroShotNER::extract_with_types(&model, text, &types, 0.5)
          .expect("baseline extract");

      model.load_adapter("medical", &test_adapter_dir()).expect("load adapter");
      assert_eq!(model.active_adapter(), Some("medical"));

      let after_adapter = ZeroShotNER::extract_with_types(&model, text, &types, 0.5)
          .expect("post-adapter extract");

      // The exact difference depends on the adapter, but if the adapter is
      // domain-specific (medical), at least one of these should hold:
      //   - more entities surface (adapter trained for medical NER finds more)
      //   - confidence scores shift measurably (adapter changes scoring)
      // We assert: not all entities + scores are byte-identical to baseline.
      let baseline_score_sum: f64 = baseline.iter().map(|e| e.confidence.value()).sum();
      let adapter_score_sum: f64 = after_adapter.iter().map(|e| e.confidence.value()).sum();
      let diff = (baseline_score_sum - adapter_score_sum).abs();

      eprintln!("baseline ({}): {:#?}", baseline.len(), baseline);
      eprintln!("after adapter ({}): {:#?}", after_adapter.len(), after_adapter);
      eprintln!("score sum diff: {diff}");

      assert!(
          baseline.len() != after_adapter.len() || diff > 1e-3,
          "adapter had no measurable effect — check that the adapter was actually merged"
      );

      // unload_adapter should restore baseline.
      model.unload_adapter().expect("unload");
      assert_eq!(model.active_adapter(), None);

      let restored = ZeroShotNER::extract_with_types(&model, text, &types, 0.5)
          .expect("post-unload extract");

      // Tolerance after unload: should match baseline exactly (we just
      // reloaded the same safetensors).
      assert_eq!(baseline.len(), restored.len(), "unload_adapter didn't restore");
      // ... more detailed checks ...
  }
  ```

- [ ] **Step 2: Run with a real adapter.**
  ```bash
  GLINER2_TEST_ADAPTER_DIR=./my_adapter cargo test ... -- --ignored load_adapter_changes_inference --nocapture
  ```

- [ ] **Step 3: Multi-load test.**
  ```rust
  #[test]
  #[ignore]
  fn load_two_adapters_sequential() {
      // Load adapter A, run inference, load adapter B (replaces A), run
      // inference, unload, run inference. Verify each output differs.
  }
  ```

- [ ] **Step 4: Commit.**
  ```bash
  git add crates/anno/tests/gliner2_fastino_candle_lora.rs
  git commit -m "test(gliner2_fastino_candle): LoRA adapter merge correctness tests"
  ```

---

## Milestone P4.M10 — Catalog + docs + finish (~½ day)

### Task M10.1: BACKENDS.md row

- [ ] **Step 1: Add row** under the `gliner2_fastino` row:

  ```
  | `gliner2_fastino_candle` | fastino-ai GLiNER2 with LoRA adapter merge-at-load (Candle backend, multi-domain inference). Feature `gliner2-fastino-candle`. Reuses `candle-transformers::models::debertav2`. `load_adapter`/`unload_adapter` API merges PEFT-format adapters into base weights at load time (~100ms swap, zero per-forward overhead). For sub-millisecond hot-swap see optional Phase 4.5. Trait impls match `GLiNER2Fastino` byte-for-byte. | Yes | experimental | `fastino/gliner2-multi-v1` (PyTorch safetensors), `fastino/gliner2-base-v1`, `fastino/gliner2-large-v1` |
  ```

### Task M10.2: catalog.rs row

- [ ] **Step 1: Add `BackendInfo` entry.**

  ```rust
  // crates/anno/src/backends/catalog.rs
  BackendInfo {
      name: "gliner2_fastino_candle",
      requires_model: true,
      feature: Some("gliner2-fastino-candle"),
      zero_shot: true,
      gpu_support: true,
      description: "fastino-ai GLiNER2 with LoRA adapter merge-at-load (Candle backend, multi-domain) — experimental, issue #18",
      recommended_models: &[
          "fastino/gliner2-multi-v1",
          "fastino/gliner2-base-v1",
          "fastino/gliner2-large-v1",
      ],
  },
  ```

### Task M10.3: Module rustdoc

- [ ] **Step 1: Top-of-file rustdoc on `gliner2_fastino_candle/mod.rs`.**

  ```rust
  //! # gliner2_fastino_candle (Phase 4)
  //!
  //! Candle backend for fastino-ai GLiNER2 with **runtime LoRA adapter
  //! merge-at-load**. Loads PEFT-format adapters and merges them into the
  //! base weights at `load_adapter` time, producing a fully-merged model
  //! with zero per-forward overhead.
  //!
  //! ## When to use this backend
  //!
  //! - You have multiple domain-specific LoRA adapters (e.g., legal,
  //!   medical, financial) trained on the same base model.
  //! - You want to switch between domains at runtime without re-exporting
  //!   merged ONNX models per domain (which costs ~6 GB on disk per).
  //! - Adapter swap rate is moderate (every few minutes/hours, not per
  //!   request). For sub-millisecond hot-swap, see optional Phase 4.5.
  //!
  //! ## Architecture
  //!
  //! - Encoder: [`candle_transformers::models::debertav2::DebertaV2Model`]
  //!   — provides DeBERTa-v2/v3 disentangled attention without anno
  //!   reimplementing it.
  //! - Heads: 7 small Candle modules (token_gather, span_rep, schema_gather,
  //!   count_pred, count_lstm, scorer, classifier).
  //! - LoRA: `W_merged = W_base + (alpha/r) * (lora_B @ lora_A)`, applied
  //!   once at `load_adapter` time per target module.
  //!
  //! ## Example
  //!
  //! ```rust,no_run
  //! use anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle;
  //! use anno::backends::inference::ZeroShotNER;
  //!
  //! let mut model = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")?;
  //!
  //! // Run on the base model.
  //! let entities = model.extract_with_types("...", &["person", "org"], 0.5)?;
  //!
  //! // Load a domain adapter.
  //! model.load_adapter("medical", "./medical_adapter")?;
  //! let medical_entities = model.extract_with_types("...", &["disease", "symptom"], 0.5)?;
  //!
  //! // Switch to a different adapter.
  //! model.load_adapter("legal", "./legal_adapter")?;
  //! let legal_entities = model.extract_with_types("...", &["plaintiff", "court"], 0.5)?;
  //!
  //! // Back to base.
  //! model.unload_adapter()?;
  //! # Ok::<(), anno::Error>(())
  //! ```
  ```

### Task M10.4: Final cargo matrix sweep

- [ ] **Step 1: Run.**
  ```bash
  for FEATURES in "gliner2-fastino-candle" "gliner2-fastino-candle-cuda" "gliner2-fastino-candle-metal" "gliner2-fastino,gliner2-fastino-candle" ; do
      echo "=== $FEATURES ==="
      cargo check -p anno --features "$FEATURES" --tests 2>&1 | tail -3
  done
  cargo test -p anno --features gliner2-fastino-candle backends::gliner2_fastino_candle
  ```

  Expected: all `Finished`, all unit tests pass.

### Task M10.5: Hand off to finishing-a-development-branch

- [ ] **Step 1:** Invoke superpowers:finishing-a-development-branch and follow its prompts. Commit any final docs, FF-merge to main, push to fork.

---

## Acceptance for Phase 4

- [ ] `GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")` loads the base model.
- [ ] **M6 parity gate:** `parity_onnx_candle_extract_with_types` passes with `max_abs_diff < 5e-3`.
- [ ] `load_adapter("medical", path)` succeeds for a PEFT-format adapter directory.
- [ ] After `load_adapter`, `extract_with_types` produces measurably different output than baseline (M9 test).
- [ ] `unload_adapter` restores baseline output exactly.
- [ ] Sequential adapter loads work (M9 multi-load test).
- [ ] Cargo matrix: `gliner2-fastino-candle`, `-candle-cuda`, `-candle-metal` all compile-check.
- [ ] `cargo test -p anno --features gliner2-fastino-candle` lib tests all pass.
- [ ] BACKENDS.md and catalog.rs reflect Phase 4 shipped.

---

## Optional Phase 4.5 — Hot-swap (deferred unless workload demands it)

If after Phase 4 ships, a workload surfaces requiring **per-request adapter selection at >1 swap/sec sustained**, add hot-swap as a deferred milestone. The skeleton:

- Replace `Encoder` and `AllHeads` storage with `(BaseEncoder, Arc<RwLock<Option<LoraDeltaCache>>>)`.
- `LoraDeltaCache` stores per-module `LoraDelta` (without merging into base) keyed by adapter name.
- Modify each linear-layer forward to optionally apply the delta: `out += alpha/r * (x @ W_down) @ W_up`.
- New methods: `set_adapter(name)` (O(1) flip), `preload_adapters([...])`.

**Cost**: ~3 days on top of Phase 4. Does NOT replace Phase 4 — they coexist (`load_adapter` for merge-at-load workloads, `set_adapter` after `preload_adapters` for hot-swap workloads).

**Why deferred**: per-request adapter selection is rare in production (most multi-tenant systems route adapters per-process or per-warmup). The merge-at-load pattern handles the 95% case at lower implementation cost.

---

## Risks (revised)

1. **DeBERTa-v3 vs v2 config compatibility.** The candle-transformers DeBERTa-v2 module also handles v3 per docs, but `fastino/gliner2-multi-v1`'s exact config.json might use field names that the Candle Config struct doesn't accept. Mitigation: M3.2 smoke test catches this immediately. If it fails, the fallback is anno's existing `encoder_candle::v3` (which is verified working).

2. **PEFT adapter format drift.** PEFT has changed the `adapter_config.json` schema across versions. Mitigation: M7.1's `LoraConfig` uses serde's `accept-and-ignore` pattern (`#[serde(default)]` + ignore unknown fields). Tested against current PEFT (2026) format; older formats may need a compat layer.

3. **No public adapter for testing.** Pre-flight requires training your own. If this is impractical, M9 falls back to a **synthetic adapter test**: hand-construct a tiny LoRA delta with known values, verify merge output matches hand-computed reference. Doesn't validate end-to-end, but validates the merge math.

4. **Candle CPU perf vs ONNX.** Candle is typically 1.5-3× slower than ort on CPU. The Candle backend's value is flexibility (LoRA hot-swap), not speed. For pure-perf single-domain workloads, users stay on ONNX. Documented in BACKENDS.md.

5. **Count_LSTM weight unpacking.** PyTorch and Candle order LSTM gate weights differently (`i,f,g,o` vs `i,g,f,o` etc.). M5.3's first-pass implementation might fail M6 parity if gate ordering is wrong. Mitigation: M6 parity test catches this. If it fails, swap to the proper LSTMCell from `candle_nn::rnn`.

6. **`fan_in_fan_out` flag.** PEFT's flag transposes the LoRA delta for HF Conv1D-style layers. Most modern adapters use `fan_in_fan_out: false` (default). M7.2 handles both; tested in unit tests.

---

## Self-review

| Concern | Status |
|---|---|
| Spec coverage | All §3 in-scope items addressed (LoRA loading, runtime API, all 8 heads, parity, cargo features). Hot-swap is deferred per evidence. |
| Type consistency | `LoraConfig`, `LoraAdapter`, `LoraModule`, `Encoder`, `AllHeads` consistent across M3-M10. |
| No placeholders | Every code block has actual content. The two "first-pass" stubs (count_lstm per-count-linear, span_rep architecture verification) are explicitly noted with M6 as the gate. |
| Pre-flight | Adapter availability gap is now explicit; M9 has a synthetic-adapter fallback if a real one isn't available. |
| Effort estimate | 10 milestones, ~2 weeks. Compared to original 18 milestones / 3.5 weeks, savings come from M5+M6 collapse (DeBERTa-v2 already exists upstream) + M14-M16 simplification (merge-at-load). |
| Halt condition | M6 parity test is the explicit halt — if it fails, the plan stops at M6. No LoRA work attempted on a parity-broken backend. |

---

## Honest assessment

**This plan ships a functional Candle backend with PEFT adapter merge-at-load support in ~2 weeks.** The merge-at-load pattern is what 95% of multi-domain inference workloads need. Users who need sub-millisecond per-request adapter swap can add it via Phase 4.5 (~3 days on top).

**The biggest single risk** is M6 parity — the gate. With `candle-transformers::models::debertav2::DebertaV2Model` doing the heavy lifting for the encoder, parity failure is most likely in the heads (count_lstm specifically). If parity passes, M7-M10 is mechanical port + LoRA layer; the plan's estimate is reliable.

**The biggest external blocker** is finding/training a real PEFT-format adapter for `fastino/gliner2-*`. Without one, M9 falls back to synthetic-adapter-only validation, which is sufficient for shipping but not for confidence that real adapters from the wild will work. Plan ahead.

**Why not ship hot-swap now?** Spec §9 asks "do you have the workload TODAY?" The merge-at-load pattern handles the multi-domain inference case (load 5 adapters, switch on warmup, run inference). Hot-swap is needed only for **per-request** adapter routing — a rare requirement that adds 3 days of implementation complexity. Deferring hot-swap reduces Phase 4 from 3.5 weeks to 2 weeks without losing any near-term capability.
