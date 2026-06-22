# GLiNER2 Candle Pipeline — Phase 4 Completion Design

**Date:** 2026-06-22
**Status:** Approved — pending implementation plan
**Scope:** `crates/anno/src/backends/gliner2_fastino_candle/`
**Feature flag:** `gliner2-fastino-candle` (CPU), `gpu-metal` (Metal)

---

## 1. Purpose

Complete the Candle backend for GLiNER2 so that `anno-rag`'s `Detector` can
select it at runtime via feature flags `gliner2-fastino-candle` (CPU) or
`gpu-metal` (Metal/CUDA). The backend adds LoRA adapter merge-at-load on top
of the standard ONNX path, enabling per-client domain specialization without
re-exporting ~6 GB ONNX bundles per adapter.

**What already exists:**
- `lora.rs` — PEFT adapter load + `merge_into_base` — complete and tested
- `mod.rs` — `GLiNER2FastinoCandle` struct + `load_adapter` / `unload_adapter` — complete
- `encoder.rs` — DeBERTa-v2 wrapper via `candle_transformers` — written
- `heads/` — all 7 heads (`span_rep`, `token_gather`, `schema_gather`,
  `count_pred`, `count_lstm`, `scorer`, `classifier`) — written
- `processor.rs` / `decoder.rs` — re-exports from ONNX backend — complete
- `detect.rs` — `NerBackend::Candle` variant, feature routing, config wiring — complete

**What is missing (this spec):**
- `pipeline.rs` — 8-step orchestration
- `from_local_with_device` / `from_pretrained_with_device` on `mod.rs`
- `ZeroShotNER` trait impl on `GLiNER2FastinoCandle`
- Parity test + unit shape test

---

## 2. Data Flow

```
text + labels
    │
    ▼  Processor::process()   [re-export of ONNX backend's processor]
ProcessedRecord {
    input_ids, attention_mask,
    word_to_token_maps,   // [(token_start, token_end)] per word
    schema_positions,     // token positions of [P]/[E]/[L] markers
    num_words,
    num_labels,
}
    │
    ▼  Step 1: encoder.model.forward(input_ids, attn_mask)
Tensor [1, L, H]   hidden states
    │
    ▼  Step 2: token_gather — index hs by word_to_token_maps start positions
Tensor [1, T, H]   word-level embeddings
    │
    ▼  Step 3: span_rep.forward(text_embs, span_idx)
               span_idx built via `build_span_idx(num_words)` — already
               defined in ONNX `pipeline.rs` as `pub(crate)`; the Candle
               pipeline calls it via the full path
               `crate::backends::gliner2_fastino::pipeline::build_span_idx`
Tensor [1, T, MAX_WIDTH, H]
    │
    ▼  Step 4: schema_gather — index hs by schema_positions
Tensor [1, M, H]   label embeddings
    │
    ▼  Step 5: count_pred.forward(schema_embs)  → argmax → N: usize
    │
    ▼  Step 6: count_lstm.forward(N, schema_embs)
Tensor [N, M, H]   count-conditioned struct projections
    │
    ▼  Step 7: scorer.forward(span_rep[0], struct_proj)
Tensor [N, M, T, MAX_WIDTH]   sigmoid scores
    │
    ▼  Step 8: classifier.forward(struct_proj) + decode_entities()
               [re-exported from ONNX decoder]
Vec<Entity>
```

`MAX_WIDTH = 8`, `MAX_COUNT = 20` — constants already defined in the ONNX
`pipeline.rs` and re-exported by `decoder.rs`.

---

## 3. Components

### 3.1 `pipeline.rs` (new file)

Single public function:

```rust
pub fn run(
    encoder: &Encoder,
    heads: &AllHeads,
    record: &ProcessedRecord,
    device: &Device,
) -> crate::Result<ScorerOutput>
```

`ScorerOutput` is `ndarray::Array4<f32>` shape `[MAX_COUNT, num_labels, T, MAX_WIDTH]`,
re-exported from the ONNX `decoder.rs`. The ONNX `decode_entities` function
consumes it unchanged.

Each step wraps Candle errors as:
```rust
crate::Error::Backend(format!("gliner2_fastino_candle: step N <name>: {e}"))
```

**Step 2 (token_gather) implemented in Candle:**
The ONNX backend runs token_gather through a session. In Candle it is a pure
index op:
```rust
let word_starts = Tensor::from_vec(word_start_indices, (T,), device)?;
let text_embs = hidden_states.squeeze(0)?.index_select(&word_starts, 0)?.unsqueeze(0)?;
// result: [1, T, H]
```

**Step 4 (schema_gather) implemented in Candle:**
Same pattern — index hidden states by schema token positions.

**Steps 3, 5, 6, 7, 8** delegate to the existing head structs' `forward`
methods and the re-exported `decode_entities`.

### 3.2 `mod.rs` additions

Two constructors (called by `detect.rs`):

```rust
pub fn from_local_with_device(model_dir: &Path, device: &Device) -> crate::Result<Self>
pub fn from_pretrained_with_device(model_id: &str, device: &Device) -> crate::Result<Self>
```

`from_local_with_device` loads:
- `tokenizer.json` → `tokenizers::Tokenizer`
- `config.json` → `DebertaV2Config` (passed to `Encoder::from_safetensors`)
- `model.safetensors` → `Encoder` + `AllHeads` (same file, different key prefixes)

`from_pretrained_with_device` downloads via `hf_hub` into the local cache,
then delegates to `from_local_with_device`. Uses the same HF hub pattern as
the existing `GLiNER2Fastino::from_pretrained_with_config`.

**ZeroShotNER trait impl:**

```rust
impl ZeroShotNER for GLiNER2FastinoCandle {
    fn extract_with_types(&self, text, labels, threshold) -> Result<Vec<Entity>> {
        let record = Processor::process(text, labels, &self.tokenizer)?;
        let scorer_out = pipeline::run(&self.encoder, &self.heads, &record, &self.device)?;
        decode_entities(&scorer_out, &record, threshold)
    }

    fn extract_with_label_thresholds(&self, text, label_thresholds) -> Result<Vec<Entity>> {
        // wrapper: call extract_with_types with min threshold, post-filter
        let min_t = label_thresholds.iter().map(|(_, t)| *t).fold(f32::MAX, f32::min);
        let all = self.extract_with_types(text, &labels_from(label_thresholds), min_t)?;
        Ok(filter_by_label_thresholds(all, label_thresholds))
    }

    fn extract_with_label_descriptions(&self, text, labeled, threshold) -> Result<Vec<Entity>> {
        let labels: Vec<&str> = labeled.iter().map(|(l, _)| *l).collect();
        self.extract_with_types(text, &labels, threshold)
    }
}
```

The two secondary methods are thin wrappers — no new pipeline paths.

---

## 4. Error Handling

- All Candle ops return `candle_core::Error`, wrapped as `crate::Error::Backend`.
- Shape assertions before each step: if dims don't match expectations, return
  `Err(crate::Error::Backend(...))` with step name + actual shape.
- No panics. No `unwrap()` in the pipeline.

---

## 5. Testing

### 5.1 `tests/candle_pipeline_unit.rs`

Gate: `#[cfg(feature = "gliner2-fastino-candle")]`

Constructs a `ProcessedRecord` with 3 words and 2 labels (hard-coded fixture,
no model file). Initializes `Encoder` and `AllHeads` with `VarBuilder::zeros`
(all-zero weights, CPU device). Calls `pipeline::run`. Asserts output shape
is `[MAX_COUNT, 2, 3, MAX_WIDTH]`.

Validates the full tensor wiring without any model download.

### 5.2 `tests/candle_parity.rs`

Gate: `#[cfg(feature = "gliner2-fastino-candle")]` + `#[ignore]`

Reads `ANNO_MODELS_DIR` env var; skips with `eprintln!` if absent.

Loads:
- ONNX backend from `$ANNO_MODELS_DIR/<ner_onnx_dir>`
- Candle backend from `$ANNO_MODELS_DIR/<ner_candle_dir>`

Runs identical input (`"Marie Dupont est avocate à Paris"`,
labels `["personne", "organisation", "lieu"]`) through both.

Asserts entity sets match and all per-entity scores satisfy
`(candle_score - onnx_score).abs() < 5e-3`.

Run locally with:
```
ANNO_MODELS_DIR=~/.cache/huggingface/hub \
cargo test --features gliner2-fastino-candle candle_parity -- --ignored
```

---

## 6. Files Changed

| File | Change |
|---|---|
| `crates/anno/src/backends/gliner2_fastino_candle/pipeline.rs` | **New** — 8-step orchestration |
| `crates/anno/src/backends/gliner2_fastino_candle/mod.rs` | Add constructors + ZeroShotNER impl |
| `crates/anno/tests/candle_pipeline_unit.rs` | **New** — shape unit test |
| `crates/anno/tests/candle_parity.rs` | **New** — parity test (ignored by default) |

No changes to `detect.rs`, `config.rs`, or any other crate.

---

## 7. Acceptance Criteria

- [ ] `cargo check --features gliner2-fastino-candle` passes with zero warnings
- [ ] `cargo test --features gliner2-fastino-candle candle_pipeline_unit` passes
- [ ] `scripts/lint-check.ps1` exits 0 (fmt + clippy clean)
- [ ] `candle_parity` test passes on a host with both models cached
      (`max_abs_diff < 5e-3` on entity scores)
- [ ] `anno-rag`'s `Detector::new()` successfully constructs with
      `ANNO_RAG_ACCELERATOR=cpu` and feature `gliner2-fastino-candle`
