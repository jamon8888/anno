# gliner2_fastino — Phase 1.5: polish

**Status:** spec — not yet implemented. Smaller-scope follow-ups that improve output quality and developer ergonomics without touching the core pipeline architecture. Each item ships independently; pick the ones that match your workload.

## Why Phase 1.5

Phase 3 produces correct output but leaves the model's accuracy on the table:

- Labels are passed as bare strings ("organization", "location"). The GLiNER paper documents that **label descriptions** ("a company or institution"; "a geographic place") improve F1 by 1-3 points on most NER benchmarks.
- The threshold is global. Real workloads need **per-label thresholds** (e.g., 0.5 for "person", 0.85 for "drug" because the medical model over-predicts drugs).
- Large-document workloads (legal contracts, medical records) need **streaming batch processing** with progress callbacks, not "load 50 MB into RAM and pray."

Phase 1.5 is a grab-bag of these. None individually justifies a phase boundary; collectively they significantly improve real-world output quality and ergonomics.

## Scope

### In: each of these is its own milestone

1. **Label descriptions** in the prompt (most impactful — accuracy boost).
2. **Per-label thresholds** (cheap UX win).
3. **Streaming batch with `on_batch` callback** (large-document workloads).
4. **`PerSample` batch schema mode** (different labels per text in a batch).
5. **Macro-based backend method sharing** (DRY between gliner2_fastino + the future gliner2_fastino_candle).
6. **Backend env var override** (`ANNO_BACKEND=candle` for runtime selection).
7. **README benchmark tables** with reproduction commands.
8. **Dead-code cleanup** (Phase-2-reserved fields in ProcessedRecord et al that currently generate clippy warnings).

### Out

- Architectural changes (those are 3.5/4).
- Phase 2 features (structure extraction).
- Anything that needs the multi-graph pipeline to grow new sessions.

## Per-item designs

### 1. Label descriptions

The prompt format upstream uses `[E]` markers per entity label:

```
( [P] entities ( [E] organization [E] location ) ) [SEP_TEXT] words...
```

With descriptions, the GLiNER paper's recommended format is:

```
( [P] entities ( [E] organization [DESC] a company or institution
                 [E] location     [DESC] a geographic place ) ) [SEP_TEXT] words...
```

`[DESC]` is one of fastino's special tokens (the upstream `gliner2-rs` constants include `DESC_TOKEN = "[DESCRIPTION]"`).

**API:**

```rust
pub fn extract_with_descriptions(
    &self,
    text: &str,
    typed: &[(&str, &str)],  // (label, description)
    threshold: f32,
) -> Result<Vec<Entity>>;
```

The existing `ZeroShotNER::extract_with_descriptions` trait method (currently delegates to `extract_with_types` and ignores descriptions) gets a real implementation.

**Implementation:** extend `processor::SchemaTask::Entities` to optionally carry per-label descriptions, or add a new variant `EntitiesDescribed`. The transformer's `[E]`-emitting loop pushes `[DESC] description` after each label.

**Estimate:** 1 day. Validation: fastino model with descriptions on the standard fixture should produce slightly different (theoretically better) scores. Hard to assert "better" in a unit test — relies on integration parity vs the Python reference impl with descriptions.

### 2. Per-label thresholds

Replace the single `threshold: f32` in `extract_with_types` with optional per-label overrides.

**API:**

```rust
pub fn extract_with_label_thresholds(
    &self,
    text: &str,
    typed_thresholds: &[(&str, f32)],  // (label, threshold)
) -> Result<Vec<Entity>>;
```

**Implementation:** in `decode_entities`, replace the single `if prob <= threshold { continue }` check with a per-label lookup keyed on `task.labels[m]`. The existing `extract_with_types(types, threshold)` becomes `extract_with_label_thresholds(types.iter().map(|t| (t, threshold)).collect())`.

Reference pattern: `paul-english/gliner2_rs::ExtractionMetadata` (uses a HashMap-per-label-override approach).

**Estimate:** 0.5 day. Mostly mechanical.

### 3. Streaming batch with `on_batch` callback

For long-document workloads (e.g., a 200-page contract), users want incremental output as chunks complete, not "wait 60 seconds, get everything."

**API:**

```rust
pub fn batch_extract_streaming<F>(
    &self,
    texts: &[&str],
    types: &[&str],
    threshold: f32,
    batch_size: usize,
    on_batch: F,
) -> Result<()>
where
    F: FnMut(usize, &[Entity]),  // (text_index, entities_for_this_text)
```

**Implementation:** chunk `texts` into windows of size `batch_size`, run `extract_with_types` on each, invoke `on_batch(idx, &entities)` after each chunk. No new ONNX path; just orchestration over the existing pipeline. With `parallel` feature, can run chunks on rayon's pool.

Reference pattern: `paul-english/gliner2_rs::batch_extract_streaming`.

**Estimate:** 0.5 day.

### 4. `PerSample` batch schema mode

By default, all texts in a batch share the same label set. For multi-tenant-ish workloads, each text in a batch has its own labels:

```rust
pub enum BatchSchemaMode<'a> {
    Shared(&'a [&'a str]),
    PerSample(Vec<Vec<&'a str>>),
}

pub fn batch_extract_with_schema_mode(
    &self,
    texts: &[&str],
    schema: BatchSchemaMode<'_>,
    threshold: f32,
) -> Result<Vec<Vec<Entity>>>;
```

**Implementation:** in `Shared` mode, the encoder runs once per text. In `PerSample` mode, also once per text (since tasks differ per text, can't share the encoder forward across the batch). The win is the API ergonomics for callers, not perf.

**Estimate:** 0.5 day.

### 5. Macro-based backend method sharing

When Phase 4 (Candle backend) lands, both `GLiNER2Fastino` (ONNX) and `GLiNER2FastinoCandle` will need the same trait impls (`Model`, `ZeroShotNER`), the same `extract_with_types` / `classify` shapes. The Phase 1 plan's improvement idea #6 mentioned this.

**Implementation:** define a macro `impl_gliner2_api!` that emits the trait impls and helper methods given a struct name. Like `paul-english/gliner2_rs::impl_gliner2_api!`. Saves 100-200 LOC of duplication.

**Estimate:** 1 day, but only worthwhile if Phase 4 lands. **Defer until then.**

### 6. Backend env var override

Anno already has type aliases: `GLiNERMultitask = GLiNERMultitaskCandle` (when `candle` feature) or `GLiNERMultitaskOnnx`. Phase 1.5 extends this to runtime override:

```rust
// User code:
let model = anno::backends::resolve_default_gliner2_fastino()?;
// Returns Box<dyn Model> based on:
//   1. ANNO_BACKEND env var (if set: "ort" | "candle")
//   2. Compile-time feature priority (ort > candle).
```

**Implementation:** small lookup function in `mod.rs`. Boxes a trait object. Cost: virtual call overhead, irrelevant vs ONNX inference.

**Estimate:** 0.5 day. Defer until Phase 4.

### 7. README benchmark tables

`crates/anno/README.md` currently mentions backends but doesn't include reproducible benchmark numbers for gliner2_fastino. Phase 1.5 adds:

- Latency: P50/P95 for `extract_with_types` on a 200-token fixture (CPU).
- Memory: peak RSS during inference.
- Comparison row: gliner2_fastino vs gliner_multitask vs nuner.
- Reproduction command: `cargo bench --features gliner2-fastino -- gliner2_fastino`.

**Implementation:** new criterion bench in `crates/anno/benches/gliner2_fastino_bench.rs`. README sentinel block for auto-regeneration via a script.

**Estimate:** 1 day.

### 8. Dead-code cleanup

Phase 1's `ProcessedRecord` has fields (`tasks`, `text_start`, `text_end`, `word_to_token_maps`) that aren't read by the standard NER path. They were laid out for Phase 2 (structure extraction). Phase 3 didn't add `extract_structure`; Phase 1.5 either:

- (a) **Implements Phase 2 structure extraction** (separate spec — not Phase 1.5 scope).
- (b) **Adds `#[allow(dead_code)]` to the unused fields** with a comment explaining why they exist.

Phase 1.5 takes path (b). Path (a) is its own track (`docs/superpowers/plans/2026-05-04-gliner2-fastino-phase2.md`).

**Estimate:** 0.25 day.

## Sequencing

If picking only one, **label descriptions (#1)** has the highest accuracy impact. **Per-label thresholds (#2)** is the cheapest. The streaming/batch work (#3, #4) only matters if you have those workloads.

Items 5, 6 should be deferred until Phase 4 (no Candle backend = nothing to share / runtime-select).

Item 7 is best done after Phase 3.5 (IOBinding) so the benchmarks reflect production-recommended settings.

Item 8 is trivial; bundle with item 1 or 2.

## Acceptance per item

| Item | Acceptance |
|---|---|
| 1 Label descriptions | Real impl behind existing trait method; integration test against fastino model with descriptions produces non-empty output |
| 2 Per-label thresholds | New method exists; unit test asserts spans below per-label threshold are dropped |
| 3 Streaming batch | `on_batch` callback fires N times for N batches; total entities equals non-streaming version |
| 4 PerSample batch | Two texts with different label sets in one call return correct per-text entities |
| 5 Macro | (deferred until Phase 4) |
| 6 Env var | (deferred until Phase 4) |
| 7 Benchmarks | README has reproducible table; `cargo bench` regenerates it |
| 8 Cleanup | Clippy clean for `gliner2-fastino` feature |

## Total cost

Items 1+2+3+4+7+8 (skip the Phase-4-dependent ones): ~3.5 days. Cumulative quality-of-life win for users who actually deploy gliner2_fastino in production.

## References

- GLiNER paper (label descriptions): arXiv:2311.08526
- `paul-english/gliner2_rs` (streaming + per-label thresholds reference): <https://github.com/paul-english/gliner2_rs>
- Phase 1 plan's improvement-ideas section: `docs/dev-notes/fastino-backend-plan.md`
- Roadmap Track B: `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md`
