# gliner2_fastino — multi-track roadmap

Index of all follow-up work after Phase 1 implementation lands. Each
track is independent; sequencing is recommended, not enforced.

**Phase 1 status** (as of 2026-05-04): 21 commits on local branch
`feat/gliner2-fastino`. Code complete, `cargo check` clean, **tests
not yet executed** (Windows MSVC linker blocker — see Track A).

## Track index

| Track | Title | Blocking? | Effort | Status |
|---|---|---|---|---|
| A | Phase 1 finalization | yes — gates merge | 2–3 days | not started |
| B | Phase 1.5 polish | no | 3–5 days | not started |
| C | Phase 2: structure extraction | no | ~2 weeks | not started |
| D | Phase 3: IOBinding + GPU | no | ~1 week | not started |
| E | Phase 4: Candle + LoRA hot-swap | optional | ~3 weeks | not started |

## Recommended sequence

1. **Track A first.** Phase 1 isn't actually shippable until tests are
   verified to pass on a non-blocked host and the `// VERIFY` comments
   in `extract_ner` are resolved against a real fastino model.
2. **Track B selective.** Cherry-pick the `[L]`-head replacement for
   `classify` (closes a documented Phase 1 caveat) and per-label
   thresholds (cheap quality win). Defer the rest.
3. **Track C.** Structure extraction. Biggest user-visible payoff.
   Has its own plan: `docs/superpowers/plans/2026-05-04-gliner2-fastino-phase2.md`.
4. **Track D.** Performance work. Schedule when someone has a real GPU
   workload to validate against.
5. **Track E.** Only if runtime LoRA hot-swap demand surfaces. Big
   compounded test matrix; not worth the cost speculatively.

---

## Track A — Phase 1 finalization

**Plan:** `docs/superpowers/plans/2026-05-04-gliner2-fastino-finalization.md`

**Goal:** Make Phase 1 actually demonstrably-working and merge-ready.

**Scope:**

- Resolve the Windows MSVC linker conflict (`MT_StaticRelease` vs
  `MD_DynamicRelease` in `esaxx-rs` vs `ort_sys`). Currently blocks
  `cargo test --features onnx` for any developer on Windows.
- Verify the four `// VERIFY` comments in
  `crates/anno/src/backends/gliner2_fastino/mod.rs::extract_ner`:
  input tensor names (`input_ids`, `attention_mask`), output tensor
  names (`scores`, `spans`), output shapes, and sigmoid handling.
  Requires loading a real `fastino/gliner2-multi-v1` ONNX export.
- Add a CI job that runs `cargo test --features gliner2-fastino` on
  Linux/macOS to validate all the test-blind code we wrote.
- Fill in T20: Python parity fixture generation + Rust comparison test
  that asserts `max_abs_diff < 5e-3` against a stored reference.
- Open the cross-repo PR.

**Out of scope:** anything beyond what was written in
`docs/superpowers/plans/2026-05-04-gliner2-fastino-phase1.md`.

---

## Track B — Phase 1.5 polish

**Plan:** TBD (not committed; lightweight enough to live as inline
follow-up issues rather than a separate plan doc).

**Items, in priority order:**

1. **Real `[L]`-head classification.** Replace the NER-head
   approximation in `classify`. Requires extracting the `[L]` token's
   hidden state from the encoder output and running a classification
   MLP. The current implementation's rustdoc explicitly flags this as
   "Phase 1.5 follow-up." File: `crates/anno/src/backends/gliner2_fastino/mod.rs::classify`.
   ~2 days.
2. **Per-label thresholds.** Add
   `extract_with_label_thresholds(text, &[(label, threshold)])` to the
   `ZeroShotNER` trait (or as a method on the struct). Reference
   pattern: `paul-english/gliner2_rs::ExtractionMetadata`. ~1 day.
3. **Real `extract_with_descriptions`.** Currently the trait method
   delegates to `extract_with_types` (descriptions ignored). Wire
   descriptions into the prompt assembly per the GLiNER paper. ~1 day.
4. **Streaming batch with `on_batch` callback.** For large-file
   workloads. Reference: `paul-english/gliner2_rs::batch_extract_streaming`.
   ~1 day.
5. **Dead-code cleanup.** Remove or `#[allow(dead_code)]` the
   Phase-2-reserved fields in `ProcessedRecord`, `TaskMapping`, etc.
   that currently generate clippy warnings. ~half day.

**Estimate:** 3–5 days end-to-end if all five items land.

---

## Track C — Phase 2: structure extraction

**Plan:** `docs/superpowers/plans/2026-05-04-gliner2-fastino-phase2.md`

**Goal:** Implement `extract_structure(text, schema) -> serde_json::Value`
with the count-predictor MLP and occurrence ID embeddings, per the
GLiNER2 paper (Zaratiana et al. 2025, arXiv:2507.18546).

**Scope:**

- Restore `SchemaTask::Relations` and `SchemaTask::Classifications` to
  the processor port (Phase 1 deliberately omitted them).
- Port the count-predictor head (20-class MLP from `[P]` embedding for
  0–19 instances).
- Port occurrence ID embeddings + per-attribute span scoring.
- Define the `Schema` / `Field` / `FieldType` types (loosely matching
  `gliner_multitask::schema` but with fastino-specific extensions).
- Wire JSON output assembly.
- Variant-specific test against `fastino/gliner2-multi-v1` (which uses
  `count_lstm_v2`).
- Spec acceptance: at least one multi-instance schema returns
  well-formed JSON.

**Out of scope:** structure extraction performance (Phase 3),
LoRA-aware structure extraction (Phase 4).

---

## Track D — Phase 3: IOBinding + GPU

**Plan:** TBD (not committed; sketch below).

**Goal:** Eliminate the inter-session memory copies and add GPU
execution providers.

**Sketch:**

- Port the 8-session IOBinding pipeline from `gliner2-rs/lib_v2.rs`.
  Each session output stays in device memory for the next session's
  input.
- OS-aware artifact selection: load `_iobinding` ONNX variants on
  Linux/Windows; fall back to `fp16` non-IOBinding artifacts on macOS
  (CoreML EP doesn't support IOBinding cleanly).
- CUDA EP wiring via `OnnxSessionConfig::prefer_cuda = true` (already
  exists in `hf_loader`).
- CoreML EP wiring on macOS via `OnnxSessionConfig::prefer_coreml`.
- CPU↔GPU parity test: same input, both EPs, `max_abs_diff < 5e-3`.

**Reference:** see commit `f891a31` for where existing backends wire
GPU execution providers.

**Estimate:** ~1 week.

---

## Track E — Phase 4 (optional): Candle + LoRA hot-swap

**Plan:** TBD (only write if Track A finishes and demand for runtime
adapter swap surfaces).

**Goal:** Full-Rust loading path (no Python export tooling needed at
runtime) with native LoRA adapter dispatch.

**Sketch:**

- Add DeBERTa-v2 to `encoder_candle/config.rs` (currently only v3).
  Tokenizer differs (v2 SentencePiece vs v3 ELECTRA-style).
- Implement GLiNER2 heads in Candle: encoder forward + count predictor
  + span scorer + classifier.
- Native LoRA adapter loader: read `adapter_config.json` +
  `adapter_model.safetensors`, apply `xW_down W_up * α/r` deltas to
  target linear layers at load time.
- Public API: `load_adapter(path)`, `set_adapter(name)`, optional
  `unload_adapter()`. The hot-swap is the headline feature.
- Score parity to ONNX path: `max_abs_diff < 5e-3` on a fixture text.
- Documented hot-swap example in rustdoc.

**Why optional:** the spec §1 explicitly defers this and lists no SLA.
Only worth building if a real multi-domain workload needs it (e.g.,
multi-tenant inference where different customers' fine-tuned adapters
ride on top of the same base in one process).

**Estimate:** ~3 weeks.

---

## References

- Phase 1 spec: `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md`
- Phase 1 plan: `docs/superpowers/plans/2026-05-04-gliner2-fastino-phase1.md`
- Issue: [arclabs561/anno#18](https://github.com/arclabs561/anno/issues/18)
- Baseline plan: `docs/dev-notes/fastino-backend-plan.md`
