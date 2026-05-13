# gliner2_fastino + fastino/* — v0.3 research

**Date:** 2026-05-13
**Purpose:** Lock in what anno's `gliner2_fastino` / `gliner2_fastino_candle` backends actually expose today, what the `fastino/*` HuggingFace model family ships, and exactly which v0.3 features unlock the biggest quality wins for anno-rag.

This is a dev-note, not a spec. It informs the v0.3 brainstorming and freezes the API contracts so we don't re-read the anno source every time we touch detector/embed code.

---

## Part 1 — Anno's `gliner2_fastino` + `gliner2_fastino_candle`

### 1.1 The 8-graph multi-task pipeline

Both backends implement GLiNER2 (Zaratiana et al., arXiv:2507.18546) as a chain of 8 small graphs:

| Graph | Role |
|---|---|
| `encoder` | DeBERTa-v2/v3 token embeddings (the only "big" graph; ~150–200M params) |
| `token_gather` | First-subword-of-word pooling |
| `span_rep` | Builds span representations for every (start, end) candidate |
| `schema_gather` | Pulls per-label embeddings out of the encoder at `[E]`/`[L]`/`[P]` special-token positions |
| `count_pred_argmax` | Predicts how many entities to emit (argmax) |
| `count_lstm_fixed` | LSTM-based count refinement; backend picks among `count_lstm`, `count_lstm_moe`, `count_lstm_v2` |
| `scorer` | Dot-product span ⊗ label → confidence (Eq. 1 of paper) |
| `classifier` | Dedicated softmax head for `[L]` classification tasks |

Schema prompt format: `( [P] task_name ( [E] label1 [E] label2 ) ) [SEP_TEXT] tokens…`. Special-token IDs are read from `tokenizer.json` at load.

### 1.2 `ExecutionMode::IoBinding` (Phase 3.5)

From `crates/anno/src/backends/gliner2_fastino/mod.rs:114-167`:

```rust
pub enum ExecutionMode { Standard, IoBinding }
pub struct GLiNER2FastinoConfig {
    pub onnx: hf_loader::OnnxSessionConfig,
    pub execution_mode: ExecutionMode,
}
```

```rust
GLiNER2Fastino::from_pretrained_with_config(
    "SemplificaAI/gliner2-multi-v1-onnx",
    GLiNER2FastinoConfig::default().with_execution_mode(ExecutionMode::IoBinding),
)
```

In IoBinding, tensors stay device-resident in a single ort allocator across the 8-session chain. Prefers `*_iobinding{suffix}` ONNX exports when present, falls back gracefully. Claims 1.5–3× CPU speedup; required for efficient GPU. Parity tested to 1e-4 vs Standard.

### 1.3 Phase 4 Candle + LoRA hot-swap

From `crates/anno/src/backends/gliner2_fastino_candle/mod.rs`:

```rust
pub fn load_adapter(&mut self, name: &str, adapter_dir: &Path) -> crate::Result<()>
pub fn unload_adapter(&mut self) -> crate::Result<()>
pub fn active_adapter(&self) -> Option<&str>
pub fn from_local(model_dir: &Path) -> crate::Result<Self>
pub fn from_pretrained(model_id: &str) -> crate::Result<Self>  // PyTorch repo, not ONNX
```

Math: `W_merged = W_base + (alpha/r) · (lora_B @ lora_A)` applied **once at `load_adapter` time**, zero per-forward overhead. ~100 ms per swap. Standard PEFT format (`adapter_config.json` + `adapter_model.safetensors`).

Defensively rejects adapters whose `base_model_name_or_path` mismatches the engine's `model_id` — can't merge a `gliner2-base-v1`-trained adapter into a `gliner2-multi-v1` base.

**ONNX backend rejects LoRA** (returns `LoraAdapterNotSupported`). Use Candle backend, or pre-merge to ONNX via `scripts/gliner2_export_onnx.py`.

### 1.4 Public API exposed (both backends, same surface)

- `extract_ner(text, types, threshold) -> Vec<Entity>` (via `Model` and `ZeroShotNER` traits)
- `extract_with_label_descriptions(text, &[(&str, &str)], threshold)` — `[E] label [DESCRIPTION] desc` prompt format, measurable accuracy boost per paper
- `extract_with_label_thresholds(text, &[(&str, f32)])` — per-label thresholds
- `extract_structure(text, &TaskSchema, threshold) -> Vec<ExtractedStructure>` — schema-driven JSON-like extraction
- `classify(text, labels, _threshold) -> Vec<(String, f32)>` — single-label, softmax-sorted (the `threshold` arg is reserved for future multi-label)
- `batch_extract_with_schema_mode(...)` and `batch_extract_streaming(...)` (ONNX only, Phase 1.5)

Gated by features `gliner2-fastino` / `gliner2-fastino-candle`. Reachable as `anno::backends::gliner2_fastino::{GLiNER2Fastino, ExecutionMode, GLiNER2FastinoConfig, BatchSchemaMode, schema::*}`. **No top-level re-export** — callers must use the full path.

### 1.5 `TaskSchema` shape

From `crates/anno/src/backends/gliner_multitask/schema.rs:96-255` (re-exported by `gliner2_fastino::schema`):

```rust
pub struct TaskSchema {
    pub entities: Option<EntityTask>,
    pub classifications: Vec<ClassificationTask>,
    pub structures: Vec<StructureTask>,
}
pub struct StructureTask { pub name: String, pub fields: Vec<StructureField> }
pub struct StructureField {
    pub name: String,
    pub field_type: FieldType,
    pub description: Option<String>,
    pub choices: Option<Vec<String>>,
}
pub enum FieldType { String, List, Choice }
```

**Caveat:** `List` and `Choice` currently decode as single-best-span (same as `String`). Multi-instance extraction happens at the `MAX_COUNT` axis of the scorer — each `StructureTask` runs one inference pass and emits one `ExtractedStructure` per predicted instance.

### 1.6 M&A NDA structure-extraction sketch

```rust
let schema = TaskSchema::new()
    .with_structure(
        StructureTask::new("nda_party")
            .with_field("name", FieldType::String)
            .with_field("role", FieldType::String)
            .with_field("jurisdiction", FieldType::String),
    )
    .with_structure(
        StructureTask::new("nda_term")
            .with_field("duration_months", FieldType::String)
            .with_field("governing_law", FieldType::String)
            .with_field("survival_clause", FieldType::String),
    );
let results = model.extract_structure(&nda_fr_text, &schema, 0.5)?;
```

Cost scales linearly with the number of `StructureTask`s — each runs a full 8-session pass.

---

## Part 2 — HuggingFace `fastino/` family

As of 2026-05-13:

| Model | Params | Downloads | Counting layer |
|---|---|---|---|
| `fastino/gliner2-base-v1` | 205M | 379k | `count_lstm` |
| `fastino/gliner2-large-v1` | ~400M+ | 150k | `count_lstm_moe` (MoE) |
| `fastino/gliner2-multi-v1` | ~280M (hidden_size 768) | 90k | `count_lstm_v2` |
| `fastino/gliguard-LLMGuardrails-300M` | 300M | 254 | (classifier-only) |

### 2.1 `gliner2-base-v1` ("small" variant — but English-leaning)

- BERT-based bidirectional encoder, GLiNER2 multi-task head stack
- 205M params, Apache-2.0, CPU-first
- Tasks: NER, single-label + multi-label classification, structured (JSON) extraction, relation extraction, intent, sentiment, topic — all in one forward pass via the schema interface
- **Languages:** model card does not enumerate; appears English-leaning
- Max seq: 512 tokens
- PII categories: zero-shot — provide labels at inference
- LoRA: standard PEFT format

### 2.2 `gliner2-multi-v1` — the right variant for French legal

- The **multilingual** variant; the SemplificaAI ONNX export confirms "5 languages supported"
- **Use this for `anno-rag`** — `base-v1` would be a quality regression on French text

### 2.3 `SemplificaAI/gliner2-multi-v1-onnx`

The ONNX export anno's `from_pretrained` pulls by default (`gliner2_fastino/mod.rs:84`). Variants shipped:

- `fp32_v2/` — recommended for CPU (anno loader's first choice)
- `fp16_v2/` — zero-copy VRAM + IoBinding-friendly
- Legacy `fp32/`, `fp16/`

Loader prefers `fp32_v2/` then falls back to `fp16_v2/` (`mod.rs:388-398`).

### 2.4 LoRA-friendly base

`fastino/gliner2-multi-v1` (PyTorch repo) is the right Candle target — `gliner2_fastino_candle::from_pretrained("fastino/gliner2-multi-v1")` downloads the layout anno expects.

---

## Part 3 — Candidate v0.3 features

Mapped to slots in the v1 Northstar spec:

| # | Feature | Complexity | Prereqs | Slot |
|---|---|---|---|---|
| 1 | **Replace `StackedNER::default()` PII scrub with `GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")` + `extract_with_types(text, &["person","organization","email","phone","address","nir","iban"], 0.5)`** | **S** | multi-v1 ONNX (already shipped); `gliner2-fastino` feature | Ingest > PII scrub (kills the warmup workaround) |
| 2 | **Chunk-level clause classifier** — `classify(chunk, &["confidentiality","termination","ip_assignment","non_compete","governing_law","indemnity","payment_terms"], 0.0)` and store top label as chunk metadata | S | same | Index metadata > filtered search facets |
| 3 | **Structured-field extraction for tabular review (v1.1)** — `TaskSchema` with one `StructureTask` per review row, replacing per-field LLM prompts | M | multi-v1; `extract_structure` already wired | v1.1 Tabular Review |
| 4 | **IoBinding mode for batch ingest** — flip `ExecutionMode::IoBinding`; keep `Standard` for hot single-query path until parity confirmed | S | Phase 3.5 sessions; benchmark first | Pipeline > ingest throughput |
| 5 | **`legal-fr` LoRA adapter pack** — `gliner2_fastino_candle` + `load_adapter("legal-fr", path)` per ingest profile | **L** | Candle backend + training corpus (LexGLUE-fr, French CASS rulings, Légifrance) + GPU for training | New: `LegalProfile` enum |
| 6 | **Per-label thresholds for French NER** — `extract_with_label_thresholds(&[("person",0.4),("address",0.7),("organization",0.55)])` to tune over-prediction | S | already exposed | Ingest > PII scrub tuning |
| 7 | **Per-label descriptions for legal jargon** — `extract_with_label_descriptions(&[("partie","personne physique ou morale signataire"),…])` for accuracy boost without fine-tune | S | already exposed | Ingest > legal-aware NER |
| 8 | **GLiGuard for prompt-injection filtering** on user queries before retrieval — `fastino/gliguard-LLMGuardrails-300M` | M | new backend integration; 300M model | Query > safety filter |

### Recommendation for v0.3 — prioritize these 2-3

1. **Feature #1 (S):** Wire `GLiNER2Fastino` as the default PII scrubber. Highest-leverage, lowest-risk. Kills the warmup workaround and gives proper French PII via the multilingual `multi-v1`.
2. **Feature #3 (M):** `extract_structure` for tabular review. Direct replacement for per-field LLM prompts in the v1.1 plan — same model call, zero extra infrastructure.
3. **Feature #2 (S):** Clause classifier as chunk metadata. Cheapest meaningful retrieval upgrade — unlocks `WHERE clause_type = 'termination'` filtered search and provides labeled data for feature #5 later.

Defer the Candle LoRA adapter (#5) to v0.4 — needs a training corpus and isn't worth shipping without one.

---

## Key file paths for implementation reference

- `crates/anno/src/backends/gliner2_fastino/mod.rs` — public API surface (ONNX)
- `crates/anno/src/backends/gliner2_fastino/config.rs` — `FastinoConfig`, `CountingLayer` enum tying model variants
- `crates/anno/src/backends/gliner2_fastino_candle/mod.rs` — LoRA `load_adapter` / `unload_adapter` / `active_adapter` API
- `crates/anno/src/backends/gliner_multitask/schema.rs` — canonical `TaskSchema`, `StructureTask`, `FieldType` (re-exported by `gliner2_fastino::schema`)

---

*Compiled from agent research run 2026-05-13. Source: direct reading of anno source at HEAD `84c323d1` + HuggingFace `fastino/` model cards.*
