# gliner2_fastino IoBinding port notes

Source: SemplificaAI/gliner2-rs/rust_component/src/lib_v2.rs:285-660
      — Gliner2EngineV2::extract_iobinding (Apache-2.0).

Snapshotted 2026-05-06 to `/tmp/extract_iobinding.rs` (376 LOC); upstream
file is 905 LOC total.

## Symbol mapping

| upstream | anno equivalent |
|---|---|
| `Gliner2EngineV2::sessions.encoder` | `crate::backends::gliner2_fastino::sessions::Sessions::encoder` |
| `Gliner2EngineV2::sessions.token_gather` | `Sessions::token_gather` |
| `Gliner2EngineV2::sessions.span_rep` | `Sessions::span_rep` |
| `Gliner2EngineV2::sessions.schema_gather` | `Sessions::schema_gather` |
| `Gliner2EngineV2::sessions.count_pred_argmax` | `Sessions::count_pred_argmax` |
| `Gliner2EngineV2::sessions.count_lstm_fixed` | `Sessions::count_lstm_fixed` |
| `Gliner2EngineV2::sessions.scorer` | `Sessions::scorer` |
| `Gliner2EngineV2::sessions.classifier` | `Sessions::classifier` |
| `processor::process_text` | `processor::SchemaTransformer::transform` |
| upstream `Entity` struct | `crate::Entity` (different field names — see decoder.rs adapter) |
| upstream `oe!` macro | `.map_err(|e| Error::Tokenizer(format!(...)))` (see Phase 3 mechanical translation) |

## Allocator strategy

Upstream creates one `Allocator` per session at construction. We mirror
that but cache the allocator on a single `IoBindingState` struct held
behind `parking_lot::RwLock<Option<IoBindingState>>` on the engine, so
we don't pay `CreateAllocator` cost on every `extract_*` call.

The encoder's allocator is treated as the canonical "device" allocator —
all subsequent sessions reuse the same `MemoryInfo` for chained
`bind_output_to_device` calls. Verified upstream: lib_v2.rs uses one
shared `device_mem` across all 8 sessions (line 22 onwards).

## Session input/output names (verified against upstream)

| Session | Inputs | Output(s) |
|---|---|---|
| encoder | `input_ids`, `attention_mask` | `last_hidden_state` (canonical name; falls back to `hidden_states` / `output` in some exports) |
| token_gather | `last_hidden_state`, `word_indices` | `text_embs` (first output) |
| span_rep | `hidden_states`, `span_idx` | `span_embeddings` (first output) |
| schema_gather | `last_hidden_state`, `schema_indices` | `pc_emb`, `field_embs` (two outputs) |
| count_pred_argmax | `pc_emb` | `count` (scalar i64) |
| count_lstm_fixed | `field_embs` | `struct_proj` (first output) |
| scorer | `span_embeddings`, `struct_proj` | `entity_scores` (first output) |
| classifier | `span_embeddings` | `cls_logits` (first output) |

For Phase 3.5 we use `.outputs.first().unwrap().name.clone()` rather than
hardcoding output names, matching how Phase 3's `pipeline.rs` is robust to
naming variation.

## Output binding strategy

Output shapes that depend on input shape (per-call):
- encoder.hidden_states: `[1, L, H]`            where L varies
- token_gather.text_embs: `[1, num_words, H]`   where num_words varies
- span_rep.span_embs: `[1, num_words, MAX_WIDTH, H]`   where num_words varies
- schema_gather.pc_emb: `[1, H]` — fixed shape but we still bind to device
- schema_gather.field_embs: `[M, H]`            where M varies
- count_lstm_fixed.struct_proj: `[MAX_COUNT, M, H]`    where M varies
- scorer.scores: `[MAX_COUNT, num_words, MAX_WIDTH, M]`  where num_words, M vary

→ use `bind_output_to_device(name, &mem_info)` for all of these. ort
allocates the right-sized buffer at run time.

Output shapes that are fixed:
- count_pred_argmax: scalar i64 — bound to **CPU** memory directly so
  we can read it back without a device→host copy. Upstream uses
  `MemoryType::CPUOutput` for this.

Final scorer output is the only large device→host copy in the IoBinding
path: it must be read back to host as `Array4<f32>` for `decode_entities`
/ `decode_structure` / `decode_entities_with_thresholds`. Cheap relative
to the 7 inter-session copies it eliminates.

## ort 2.0.0-rc.12 API reference (verified against local crate cache)

- `Session::create_binding() -> Result<IoBinding>` (io_binding.rs:162)
- `Session::run_binding(&IoBinding) -> Result<SessionOutputs>` (mod.rs:340)
- `IoBinding::bind_input(name, &Value<T>)` (io_binding.rs:123)
- `IoBinding::bind_output(name, Value<T>)` — pre-allocated buffer (io_binding.rs:144)
- `IoBinding::bind_output_to_device(name, &MemoryInfo)` — EP allocates (io_binding.rs:167)
- `MemoryInfo::new(AllocationDevice, c_int, AllocatorType, MemoryType)` (memory.rs:400)
- `Allocator::new(&Session, MemoryInfo)` (memory.rs:153)
- `SessionOutputs` impl `IntoIterator` (output.rs:173) — used in chained-output extraction

## Differences from upstream

1. **Allocator lifetime**: upstream creates a fresh allocator per call.
   We cache on the engine. Trade-off: caching is correct only if the
   `MemoryInfo` is reusable across calls. Verified by reading
   ort docs: `MemoryInfo` is a pure metadata handle (no buffer
   ownership), so caching is safe.
2. **Per-call `RwLock` snapshot**: extracted_*_iobinding methods take
   a read lock at entry and hold it for the duration of inference.
   This matches Phase 3's `Mutex<Session>` lock-hold-during-run pattern
   (see `SessionSlot::with_session`).
3. **Single shared inner orchestrator (Phase 3.5 amendment)**: upstream
   has separate IoBinding paths per task type. We extract the shared
   8-session chain into `pipeline_iobinding::run_pipeline_io` returning
   `(record, task_map, scorer_out, pred_count)`, then dispatch to
   appropriate decoder. This way `extract_ner_iobinding`,
   `extract_with_label_descriptions_iobinding`,
   `extract_with_label_thresholds_iobinding`, and
   `extract_structure_iobinding` are thin wrappers over the same
   inner function — each only differs in the SchemaTask variant they
   build and the decoder they call.

## Plan amendment vs original (2026-05-06)

The plan as written (2026-05-05) only mentions wiring `extract_ner` and
`classify` to dispatch on `ExecutionMode`. Phase 1.5 and Phase 2 added
`extract_with_label_descriptions`, `extract_with_label_thresholds`, and
`extract_structure`. M11 of this execution wires all 5 methods through
ExecutionMode dispatch via the shared inner-orchestrator pattern noted
above.
