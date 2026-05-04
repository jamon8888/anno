# gliner2_fastino backend — refined design

Refines and extends `docs/dev-notes/fastino-backend-plan.md` (committed in
d302b04). Tracks issue arclabs561/anno#18.

## Status

Spec for the `gliner2-fastino` backend. ONNX-first, feature-gated,
experimental. WIP from day one. No SLA. Issue #18 stays open as a
contributor magnet, not a promise.

## 1. Scope and posture

### Delta from the baseline plan

The baseline plan (`docs/dev-notes/fastino-backend-plan.md`) is taken as
direction. This spec resolves four open items the baseline left
unspecified:

1. **LoRA story made explicit.** Phase 1 supports merged ONNX exports
   only. Adapter hot-swap is **not** implemented at runtime. LoRA-tuned
   models reach anno via a documented merge-and-export workflow, not
   through runtime adapter dispatch.
2. **ONNX export route.** Ship `scripts/gliner2_export_onnx.py`.
   Documentation additionally points at `SemplificaAI/gliner2-multi-v1-onnx`
   as a pre-baked fast path for the most common case. Two entry points,
   one in user docs.
3. **Classification API.** Internal-only on the struct in Phase 1 (a `pub`
   method, not a public trait). Promotion to a trait is deferred until a
   second backend implements the same shape.
4. **Structure extraction API.** Same posture as classification — Phase 2
   adds `extract_structure` as a method on the struct, not a new public
   trait.

### What the issue posture requires

- Cargo feature `gliner2-fastino = ["onnx"]`. Default off. Zero cost to
  users who don't opt in.
- WIP status in `BACKENDS.md` and the catalog row.
- `experimental` notice in module rustdoc.
- No SLA on bug fixes or model variant churn.

### Non-goals

- Runtime LoRA adapter hot-swap (deferred to Phase 4 if/when demand
  appears).
- Full-Candle path for Phase 1 (flagged as Phase 4).
- DirectML / non-CUDA non-CoreML execution providers.
- API stability across phases.

## 2. Architecture

### Module layout

```
crates/anno/src/backends/gliner2_fastino/
├── mod.rs        — public surface: GLiNER2Fastino struct, from_pretrained,
│                   Model + ZeroShotNER impls, internal classify(),
│                   source-attribution comment header
├── processor.rs  — port of SemplificaAI/gliner2-rs processor.rs;
│                   prompt assembly, special-token registration from
│                   tokenizer.json, span scoring (Eq. 1 of arXiv:2507.18546),
│                   label vocab handling
├── config.rs     — fastino config.json shape, counting_layer enum
│                   (count_lstm | count_lstm_moe | count_lstm_v2),
│                   hf_loader integration
├── session.rs    — ort::Session wrapper, I/O tensor names, execution
│                   provider wiring (CPU only Phase 1)
└── errors.rs     — backend-local error enum, mapped into anno::Error at
                    the boundary
```

No new external dependencies. Uses anno's existing `ort` (rc.12) and
`tokenizers` pins.

### Cargo features

```toml
[features]
gliner2-fastino = ["onnx"]
```

Mirrors the existing `gliner-multitask` pattern.

### Discovery and dispatch

Two existing touch points are modified:

- **`gliner_multitask::check_model_id_is_supported`** currently rejects
  `fastino/*` ids with a hard error. The check is **redirected**, not
  removed:
  - With the `gliner2-fastino` feature disabled: error message updated
    to "use the `gliner2-fastino` feature to load this model."
  - With the feature enabled: dispatch transparently to the new backend.
  - The existing rejection regression test stays, with assertions
    updated to match the new message.
- **`backends::catalog`** gets new rows for `fastino/gliner2-multi-v1`,
  `fastino/gliner2-large-v1`, `fastino/gliner2-base-v1`, all with
  `status: WIP` and an export-source pointer field.

### Tooling artifacts (outside the crate)

- **`scripts/gliner2_export_onnx.py`** — covers all three fastino
  variants. Accepts `--lora-adapter PATH` to invoke
  `peft.merge_and_unload()` before export. Mirrors `lmoe/gliner2-onnx`
  in approach. Estimated 150–250 LOC.
- **`docs/dev-notes/gliner2-fastino-export.md`** — user-facing doc
  covering both the SemplificaAI fast path and the script.

## 3. Public API surface (Phase 1)

### Construction

```rust
impl GLiNER2Fastino {
    pub fn from_pretrained(model_id: &str) -> Result<Self>;
    pub fn from_pretrained_with_options(
        model_id: &str,
        options: LoadOptions,
    ) -> Result<Self>;
    pub fn from_local(model_dir: &Path) -> Result<Self>;
}
```

`LoadOptions` reuses anno's existing struct (HF revision, cache dir,
execution provider). No new fields for Phase 1.

### Trait impls (existing public traits, no new ones)

- `Model` — name, model id, etc., per the existing trait contract.
- `ZeroShotNER`:
  - `extract_with_types(&self, text: &str, types: &[&str], threshold: f32) -> Result<Vec<Entity>>`
  - **Character offsets**, not token offsets. This is the contract the
    SemplificaAI port doesn't satisfy and is the largest single porting
    hazard (see §6 risk #1).

### Internal-only methods (no trait, on the struct directly)

```rust
impl GLiNER2Fastino {
    pub fn classify(
        &self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> Result<Vec<(String, f32)>>;

    // Phase 2 will add:
    // pub fn extract_structure(
    //     &self,
    //     text: &str,
    //     schema: &Schema,
    // ) -> Result<serde_json::Value>;
}
```

Both methods are `pub` (so feature-gated downstream users can call them)
but **not behind a trait**. Promotion is deferred until a second
implementor exists. Module rustdoc documents this as deliberate.

### Behavioral contract

These notes go in module rustdoc:

- **Prompt format**: `( [P] task_name ( [E] label1 [E] label2 ) ) [SEP_TEXT] tokens...`
  — exact, since position determines decoding.
- **Special-token IDs** are read from `tokenizer.json` at load time,
  never hardcoded. (The hardcoded `<<ENT>>=128002` / `<<SEP>>=128003`
  in `gliner_multitask` is the failure mode being explicitly avoided.)
- **Threshold semantics** match `gliner_multitask` — global float,
  applied per-span post-sigmoid.
- **Empty `types` slice** returns `Ok(vec![])` without invoking the model.
- **LoRA adapter directories** (containing `adapter_config.json`) are
  detected at load time and produce a typed error pointing the user at
  `scripts/gliner2_export_onnx.py --lora-adapter`. Runtime adapter
  loading is not supported in Phase 1.

## 4. Testing and verification

### Tier 1 — pure-Rust unit tests (every CI job)

No model download. Runs on every PR.

- **Prompt assembly**: given `(task, labels, text)`, assert exact token-id
  sequence matches a hand-computed fixture using a stub `tokenizer.json`
  in `testdata/gliner2_fastino/`.
- **Special-token registration**: load a stub `tokenizer.json` with the
  seven fastino tokens, assert each ID is correctly resolved.
- **Span-score → entity decoding**: feed a synthetic `(scores, spans)`
  tensor, assert character offsets are correct in the original string.
  This is the regression test for the porting hazard from §3.
- **Edge inputs**: empty types, empty text, single-token text.
- **Catalog row** presence and `WIP` status.
- **LoRA-directory rejection**: pointing `from_local` at a directory
  containing `adapter_config.json` returns the typed error with the
  script suggestion.

### Tier 2 — integration tests (`#[ignore]`, nightly CI)

Requires `fastino/gliner2-multi-v1` cached in the HF cache.

- Load `fastino/gliner2-multi-v1`. Implements `Model + ZeroShotNER`.
- `extract_with_types("Acme Corp signed a deal in Paris.", &["organization", "location"], 0.5)`
  returns at least `Acme Corp` (organization) and `Paris` (location)
  with character offsets.
- `classify("This is a positive review.", &["positive", "negative"], 0.5)`
  ranks `positive` above `negative`.
- Same expectations against the SemplificaAI external pre-export pin
  — sanity-checks the docs' fast path actually loads.

### Tier 3 — Python parity test (`#[ignore]`, nightly CI)

Mirrors the existing plan's improvement idea #5.

- Stored fixture of expected scores from Python `gliner2` reference
  (generated once, checked into `testdata/gliner2_fastino/parity/`).
- Bound: `max_abs_diff < 5e-3` on the score vector for the fixture
  inputs.

### Export-script CI (gated, opt-in label)

A CI job runs `scripts/gliner2_export_onnx.py` against a tiny test
checkpoint to keep the script from rotting. Doesn't run on every PR.
The `--lora-adapter` path is exercised by exporting a
randomly-initialized PEFT adapter on top of a tiny base — verifies the
merge pipeline doesn't error. Correctness is not asserted at this tier.

### Documentation verification

A doctest in `docs/dev-notes/gliner2-fastino-export.md` runs
`scripts/gliner2_export_onnx.py --help`. Catches silent script
moves/renames.

### Explicitly NOT tested in Phase 1

- Phase 2 structure extraction.
- Phase 3 IOBinding / GPU paths.
- LoRA hot-swap (out of scope).
- Cross-platform ONNX EP behavior beyond CPU.

## 5. Phase plan

| Phase | Scope | Acceptance | Estimate |
|---|---|---|---|
| **1** | NER + classification (internal). Module + Cargo feature. `processor.rs` port. ONNX session loader. `from_pretrained`/`Model`/`ZeroShotNER`. Export script with `--lora-adapter`. Catalog row (WIP). Redirect of `gliner_multitask` rejection. | Issue #18 Phase-1 acceptance + script `--help` doctest + parity fixture (max_abs_diff < 5e-3). | ~2.5 wk |
| **2** | Structure extraction. Count-predictor head, occurrence ID embeddings, `extract_structure(text, schema) -> serde_json::Value`. | Issue #18 Phase-2 acceptance + at least one multi-instance schema test. | ~2 wk |
| **3** | IOBinding pipeline (port from `gliner2-rs/lib_v2.rs`), OS-aware artifact selection, GPU EP wiring per `f891a31`. | CPU+GPU parity within 5e-3 on a fixture. | ~1 wk |
| **4 (optional)** | Candle parity. DeBERTa-v2 in `encoder_candle`. Native LoRA adapter loader. `load_adapter`/`set_adapter` API for runtime hot-swap. | Score parity to ONNX path within 5e-3; hot-swap docs. | ~3 wk |

## 6. Risks and mitigations

1. **Token-offset → char-offset conversion.** The SemplificaAI port
   returns token offsets; anno's `Entity` uses character offsets in the
   original input. *Mitigation:* dedicated unit test in Tier 1 with the
   synthetic span fixture.
2. **`counting_layer` config drift across fastino variants.**
   `count_lstm` (base) / `count_lstm_moe` (large) / `count_lstm_v2`
   (multi) aren't interchangeable. *Mitigation:* explicit enum in
   `config.rs` + per-variant integration test (Phase 2 only — Phase 1
   doesn't touch the count head).
3. **External pre-export drift (`SemplificaAI/gliner2-multi-v1-onnx`).**
   Third-party repo can vanish or re-export with different I/O tensor
   names. *Mitigation:* docs explicitly say "if pin breaks, use our
   script"; the integration test treats it as opt-in fast-path
   verification, not a blocker.
4. **ONNX export script bit-rot.** Python tooling can break silently
   under `transformers` / `torch` upgrades. *Mitigation:* gated CI job
   runs export on a tiny test model.
5. **LoRA expectations gap.** Users coming from Python may expect
   `load_adapter()`. We don't provide it in Phase 1. *Mitigation:*
   loading a directory with `adapter_config.json` returns a typed error
   pointing at `scripts/gliner2_export_onnx.py --lora-adapter`. Phase 4
   closes the gap natively.
6. **Source attribution.** Apache-2.0 port from
   `SemplificaAI/gliner2-rs` requires source comments. *Mitigation:*
   every ported file's header carries an attribution block per the
   baseline plan §"License attribution".

## 7. Phase 1 ship-blocker checklist

- [ ] `gliner2_fastino::GLiNER2Fastino::from_pretrained("fastino/gliner2-multi-v1")`
      returns a value implementing `Model + ZeroShotNER`.
- [ ] Tier-2 integration test against fixture text passes
      (`#[ignore]`-gated, nightly).
- [ ] Tier-3 Python-parity fixture committed; comparison test passes.
- [ ] `gliner_multitask::check_model_id_is_supported` redirect
      implemented; existing rejection test updated.
- [ ] Catalog rows added for all three variants with `WIP` status.
- [ ] `scripts/gliner2_export_onnx.py` exports `gliner2-multi-v1` and
      handles `--lora-adapter` smoke case.
- [ ] `BACKENDS.md` entry with WIP banner.
- [ ] Module rustdoc says `experimental`.
- [ ] Source attribution comments on every file ported from
      `SemplificaAI/gliner2-rs`.

## 8. Out-of-scope improvement ideas (tracked in baseline plan, not gated on this work)

The baseline plan (§"Improvement ideas surfaced by the research")
lists eight follow-ups that this spec does not block on:

1. Per-label thresholds.
2. Label descriptions in the prompt.
3. Streaming batch with callback.
4. `PerSample` batch schema mode.
5. Parity test between `gliner_onnx` and `gliner_candle`.
6. Macro-based backend method sharing.
7. Backend env var override.
8. README benchmark tables.

These remain independent follow-ups.

## References

- Issue: arclabs561/anno#18
- Baseline plan: `docs/dev-notes/fastino-backend-plan.md` (d302b04)
- Issue #17 (closed) and PR #16 — prior context

Papers:

- Zaratiana et al. 2024, "GLiNER: Generalist Model for NER",
  arXiv:2311.08526
- Stepanov & Shtopko 2024, "GLiNER multi-task: Generalist Lightweight
  Model for Various Information Extraction Tasks", arXiv:2406.12925
- Zaratiana et al. 2025, "GLiNER2: An Efficient Multi-Task Information
  Extraction System with Schema-Driven Interface", arXiv:2507.18546

External implementations:

- github.com/fastino-ai/GLiNER2 (Python reference)
- github.com/SemplificaAI/gliner2-rs (Rust port, not on crates.io;
  Apache-2.0 — porting source)
- github.com/paul-english/gliner2_rs (Rust crate; API contract mismatch
  prevents direct dep)
- github.com/fbilhaut/gline-rs (Rust GLiNER v1 only; ort version
  mismatch)
- github.com/lmoe/gliner2-onnx (community ONNX export tooling; covers
  large-v1 / multi-v1, no base-v1, no structure)
