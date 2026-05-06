# gliner2_fastino_candle port notes (revised)

> Phase 4 implementation notes. Source of truth: `fastino/gliner2-multi-v1`
> (HuggingFace, PyTorch safetensors) + the Python `gliner2` package.
> See `docs/superpowers/plans/2026-05-06-gliner2-fastino-phase4-candle-lora-revised.md`
> for the milestone plan.

## Architectural decisions (with evidence)

| Decision | Evidence / rationale |
|---|---|
| Reuse `candle-transformers::models::debertav2::DebertaV2Model` | [PR #2743](https://github.com/huggingface/candle/pull/2743) merged 2025-01-29. Production-ready, "insignificant precision difference" vs Python per the PR's own benchmarking against `Clinical-AI-Apollo/Medical-NER` |
| **Merge-at-load** (not hot-swap) for adapters | [`MacPaw/Gliner2Swift`](https://github.com/MacPaw/Gliner2Swift) ships this pattern with "zero runtime overhead, identical results to Python." Hot-swap moved to optional Phase 4.5 |
| Re-export processor + decoder from ONNX backend | Same input prep, same NMS — no Candle-specific changes needed. Phase 3.5's `ScorerOutput` Array4<f32> is the contract |
| `Option<String>` active_adapter on engine struct | Merge-at-load needs only a name, not a runtime delta. `RwLock<Option<LoraDelta>>` is a Phase 4.5 concern |

## Snapshotted upstream references (read-only)

| Concern | File on disk | Upstream URL |
|---|---|---|
| Candle DeBERTa-v2 example (381 LOC) | `/tmp/phase4-refs/candle-debertav2-example.rs` | https://github.com/huggingface/candle/blob/main/candle-examples/examples/debertav2/main.rs |
| PEFT layer.py — safetensors key format + alpha/r scaling (2510 LOC) | `/tmp/phase4-refs/peft-lora-layer.py` | https://github.com/huggingface/peft/blob/main/src/peft/tuners/lora/layer.py |
| candle-lora `loralinear.rs` — runtime LoRA forward (167 LOC) | `/tmp/phase4-refs/candle-lora-linear.rs` | https://github.com/EricLBuehler/candle-lora/blob/master/candle-lora/src/loralinear.rs |
| pii.engineer pipeline.rs — production Rust GLiNER2 post-processing (793 LOC) | `/tmp/phase4-refs/pii-engineer-pipeline.rs` | https://github.com/gantz-ai/pii.engineer/blob/main/crates/pii-engineer-core/src/pipeline.rs (Apache-2.0) |
| pii.engineer gliner/mod.rs — ONNX inference engine shape (327 LOC) | `/tmp/phase4-refs/pii-engineer-gliner.rs` | https://github.com/gantz-ai/pii.engineer/blob/main/crates/pii-engineer-core/src/gliner/mod.rs (Apache-2.0) |

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
| `gliner2.GLiNER2.load_adapter(path)` | `GLiNER2FastinoCandle::load_adapter(name, path)` (merges into base) |
| `gliner2.GLiNER2.unload_adapter()` | `GLiNER2FastinoCandle::unload_adapter()` (reloads base from disk) |
| `gliner2.lora.apply_lora_delta` | `lora::merge_into_base` |

## PEFT adapter format (verified against /tmp/phase4-refs/peft-lora-layer.py)

```
<adapter>/
├── adapter_config.json
│   {
│     "base_model_name_or_path": "fastino/gliner2-multi-v1",
│     "task_type": "TOKEN_CLS",
│     "r": 8,
│     "lora_alpha": 16,
│     "target_modules": ["query", "key", "value", ...],
│     "lora_dropout": 0.0,
│     "bias": "none",
│     "fan_in_fan_out": false
│   }
└── adapter_model.safetensors  // or adapter_weights.safetensors
    // keys:
    //   base_model.model.<path>.lora_A.weight    [r, in]
    //   base_model.model.<path>.lora_B.weight    [out, r]
```

## Merge formula

```
W_merged = W_base + (alpha / r) * (lora_B @ lora_A)
```

Where:
- `W_base`: `[out, in]`
- `lora_A`: `[r, in]`     (down-projection)
- `lora_B`: `[out, r]`    (up-projection)
- `(lora_B @ lora_A)`: `[out, in]` — same shape as W_base
- `alpha / r`: scaling factor (alpha=16, r=8 → 2.0)

Per-module application: walk safetensors keys, group by module path,
multiply, scale, add. Done at `load_adapter` time; nothing per-forward.

If `fan_in_fan_out: true` (HF Conv1D-style layers), transpose the
delta before adding. Most modern adapters use the default `false`.

## What we deliberately don't do (vs original 2026-05-05 plan)

| Original plan | This revision |
|---|---|
| Implement DeBERTa-v2 disentangled attention from scratch (M5, 3 days) | Use `candle_transformers::models::debertav2::DebertaV2Model` |
| `Arc<RwLock<Option<LoraDelta>>>` hot-swap with per-forward delta computation | Merge-at-load: apply once, zero per-forward cost |
| Adapter registry with multiple cached adapters | One active adapter at a time. Re-load base on `unload_adapter` |
| Custom `LoraLinear` wrapper around `candle_nn::Linear` | No wrapper needed — base weights are mutated directly via VarBuilder rebuild |
| `set_adapter` / `loaded_adapters` API | `load_adapter(name, path)` / `unload_adapter` / `active_adapter()` only. Hot-swap is Phase 4.5 |
| Bench `set_adapter` latency < 1 ms | Bench is moot — merge-at-load is ~100 ms (safetensors re-read), not sub-ms. Add bench only if a workload demands it |

## Pipeline data flow (Candle vs Phase 3.5 ONNX)

The 8-step orchestration is identical to the ONNX path's `pipeline_iobinding::run_pipeline_for_decoding`, but with Candle ops instead of ort sessions:

```
input_ids, attention_mask
    │
    ▼  encoder.forward (DebertaV2Model)
hidden_states [1, L, H]
    │
    ├──▶  token_gather  ──▶  text_embs [1, num_words, H]
    │                            │
    │                            ▼  span_rep
    │                       span_embs [1, num_words, MAX_WIDTH, H]
    │
    ├──▶  schema_gather  ──▶  pc_emb [1, H], field_embs [M, H]
    │                            │
    │                            ▼  count_pred (with Rust-side argmax)
    │                       pred_count: usize
    │                            │
    │                            ▼  count_lstm_fixed
    │                       struct_proj [MAX_COUNT, M, H]
    │                            │
    │   span_embs  ─┬─────────────┘
    │              ▼  scorer
    │         scores [MAX_COUNT, num_words, MAX_WIDTH, M]
    │              │
    │              ▼  ScorerOutput → decoder::decode_entities (re-used from ONNX backend)
    │
    └──▶  schema_gather only: classifier path
                        │
                        ▼  classifier (4-session subset)
                   probs [num_labels]
```

Reuse `Phase 3.5 ScorerOutput { scores: Array4<f32> }` as the bridge between the Candle pipeline and the existing decoder family — that means after the scorer, all the decode logic (NMS, span deduplication, structure assembly) is shared verbatim.

## Open verification questions (resolved during execution)

- [ ] **Q1**: `fastino/gliner2-multi-v1`'s `config.json` schema — does it match `candle_transformers::models::debertav2::Config` exactly? (Verified at M3.2 smoke test.)
- [ ] **Q2**: Does the gliner2 base model use DeBERTa-v2 or v3 weights? Per `MacPaw/Gliner2Swift` and `gantz-ai/pii.engineer`, it's mDeBERTa-v3. The candle-transformers DeBERTa-v2 module also handles v3 per its docs. Verify at M3.2.
- [ ] **Q3**: LSTM gate weight ordering for `count_lstm_fixed` — PyTorch uses `[i,f,g,o]`; Candle's `LSTMCell` uses what? (Verify at M5.3, fail-fast at M6 parity test.)

## License attribution

- `candle-transformers::debertav2`: MIT/Apache-2.0 (Candle's license).
- Reference reading from PEFT, candle-lora, gliner2-swift, pii.engineer: all Apache-2.0 or MIT. We **read for design**; we don't copy code verbatim. Where helper logic is materially derivative (e.g., LoRA merge formula in `lora.rs`), we attribute in the rustdoc.
