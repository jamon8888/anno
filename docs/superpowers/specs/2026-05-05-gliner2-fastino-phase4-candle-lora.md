# gliner2_fastino — Phase 4: Candle + LoRA hot-swap

**Status:** spec — speculative until a real multi-tenant LoRA workload surfaces. This is the most differentiating but also the most expensive of the follow-up tracks.

## 1. The problem

Phase 1-3 ships the ONNX path: 8 sessions loaded from a fixed model directory at `from_local`/`from_pretrained`. To use a LoRA-fine-tuned variant, the user must:

1. Merge the LoRA adapter into the base model in Python (`peft.merge_and_unload()`).
2. Export the merged model to ONNX (8 graphs).
3. Load the merged ONNX into anno (~6 GB on disk per domain).

For single-domain inference this is fine. For **multi-domain or multi-tenant inference** (e.g., one process serving legal + medical + financial models), the disk + memory cost is prohibitive: 6 GB per domain, fully duplicated.

The Python `gliner2` package solves this via `model.load_adapter(path)` — load the base once, swap LoRA adapters at runtime in milliseconds. Adapter weights are ~5-10 MB (vs 450 MB for a full model, ~6 GB for a full ONNX export with all 8 graphs).

**Phase 4 brings that capability to anno** via the Candle backend (Candle reads PyTorch safetensors directly; no ONNX intermediate; LoRA adapters apply natively).

## 2. Why Candle and not ort

- **ort doesn't load PyTorch safetensors** — it loads ONNX. Adapters are typically distributed as `adapter_model.safetensors` + `adapter_config.json` (PEFT format). Going through ort means re-exporting to ONNX, which defeats the "swap in milliseconds" goal.
- **Candle reads safetensors natively** and applies low-rank deltas to specific linear layers at inference time:
  ```
  attention_layer_output = base_layer(x) + lora_alpha/r * (W_down @ W_up @ x)
  ```
  with `W_down` and `W_up` being the small adapter matrices. Loading + applying = O(adapter_size) which is megabytes, not gigabytes.
- **anno already has Candle infrastructure**: `gliner_candle`, `encoder_candle` (BERT/DeBERTa-v3), `gliner_multitask::candle`. Phase 4 reuses this plumbing.

## 3. Scope

### In scope

- New module `gliner2_fastino_candle` parallel to the existing ONNX-based `gliner2_fastino`.
- DeBERTa-v2 encoder support in `encoder_candle` (currently only v3 is wired).
- All 8 GLiNER2 heads reimplemented in Candle: encoder forward, token_gather, span_rep, schema_gather, count_pred (with argmax in Rust), count_lstm_fixed, scorer, classifier.
- LoRA adapter loading: read `adapter_config.json` + `adapter_model.safetensors`, apply low-rank deltas to target modules at load time.
- `load_adapter(path)` / `set_adapter(name)` / `unload_adapter()` API. Adapters cached in memory; swap is sub-millisecond (just rebinds the active delta in the forward pass).
- Cargo features: `gliner2-fastino-candle` (depends on existing `candle`), `gliner2-fastino-candle-cuda`, `gliner2-fastino-candle-metal`.
- Score parity to ONNX path: max_abs_diff < 5e-3 on a fixed fixture.
- Documented hot-swap example in rustdoc.

### Out of scope

- Re-implementing the ONNX backend in Candle just for unification (keep ONNX as the production-fast path for single-domain workloads).
- Training. anno is inference-only; LoRA training stays in Python.
- IOBinding (Candle has its own memory model; not applicable).
- Quantization (fp16, int8). Phase 4 ships fp32 weights only.

## 4. Why this is genuinely unique

As of 2026-05, no Rust NLP library ships **runtime LoRA adapter swap**:

| Library | LoRA support |
|---|---|
| `paul-english/gliner2_rs` | Candle-based but no LoRA |
| `gline-rs` | GLiNER v1 only, no LoRA |
| `SemplificaAI/gliner2-rs` | ort-only, no LoRA |
| anno (after Phase 4) | Candle + native LoRA hot-swap |

Python's `gliner2` package has it but pulls Python's startup cost + PyTorch's memory footprint. Phase 4 = same capability in pure Rust with anno's startup time (sub-second).

**This is the feature that puts anno in unique territory.** Phases 1-3 give parity with what's possible elsewhere; Phase 4 gives a capability nothing else offers.

## 5. Design

### Module layout

```
crates/anno/src/backends/
├── encoder_candle/
│   ├── deberta_v2.rs           — NEW: DeBERTa-v2 forward (v3 already exists)
│   └── ...
├── gliner2_fastino/             — Phase 1-3: ONNX path. UNCHANGED.
├── gliner2_fastino_candle/     — NEW: full Candle implementation
│   ├── mod.rs                   — public surface
│   ├── encoder.rs               — Candle wrapper around DeBERTa-v2
│   ├── heads/
│   │   ├── span_rep.rs          — Token-aware span representation head
│   │   ├── schema_gather.rs     — [P]/[E]/[L]-token gather
│   │   ├── count_pred.rs        — Count predictor MLP + argmax
│   │   ├── count_lstm.rs        — Count-conditioned LSTM
│   │   ├── scorer.rs            — Span-vs-struct scorer + sigmoid
│   │   └── classifier.rs        — [L]-head MLP
│   ├── lora.rs                  — LoRA adapter loader + delta application
│   ├── adapters.rs              — Adapter registry (load/swap/unload)
│   ├── pipeline.rs              — 8-step orchestration
│   └── tests.rs
```

### LoRA application

PEFT-format adapters live as:
```
my_adapter/
├── adapter_config.json         — {target_modules, r, alpha, ...}
└── adapter_model.safetensors   — {layer_path → W_down, W_up}
```

The forward pass for a target linear layer becomes:

```rust
fn linear_with_lora(
    x: &Tensor,
    base_w: &Tensor,
    base_b: Option<&Tensor>,
    active_lora: Option<&LoraDelta>,  // <- swappable
) -> Result<Tensor> {
    let mut out = x.matmul(&base_w.t())?;
    if let Some(b) = base_b { out = out.add(b)?; }
    if let Some(d) = active_lora {
        // out += alpha/r * (x @ W_down) @ W_up
        let down = x.matmul(&d.w_down.t())?;
        let up = down.matmul(&d.w_up.t())?;
        out = out.add(&up.affine(d.alpha / d.r as f64, 0.0)?)?;
    }
    Ok(out)
}
```

`active_lora` is an `Option<&LoraDelta>` read from `Arc<RwLock<Option<LoraDelta>>>` on the engine. `set_adapter("legal")` flips this in O(1); `unload_adapter()` sets it to None.

### Adapter API

```rust
impl GLiNER2FastinoCandle {
    pub fn from_pretrained(model_id: &str) -> Result<Self>;
    pub fn from_local(model_dir: &Path) -> Result<Self>;

    /// Load a LoRA adapter from disk. Caches under `name`.
    /// Does NOT activate. `set_adapter(name)` to use.
    pub fn load_adapter(&mut self, name: &str, path: &Path) -> Result<()>;

    /// Activate a previously-loaded adapter. O(1) — flips a pointer.
    pub fn set_adapter(&mut self, name: &str) -> Result<()>;

    /// Run with the base model (no adapter).
    pub fn unload_adapter(&mut self);

    pub fn loaded_adapters(&self) -> Vec<&str>;
    pub fn active_adapter(&self) -> Option<&str>;
}
```

Internally each `LoraDelta` is `~5-10 MB` (fp32) or `~2.5-5 MB` (fp16). Loading 10 adapters = ~50-100 MB extra RAM beyond the base model. Set/unset is sub-millisecond.

### Trait impls

`GLiNER2FastinoCandle` implements `Model` + `ZeroShotNER` exactly like `GLiNER2Fastino`. Same public method shapes: users can swap backends by changing one type alias. `classify` and (if Phase 2 lands) `extract_structure` ditto.

## 6. Phase plan

| Step | Estimate |
|---|---|
| 1. Add DeBERTa-v2 to `encoder_candle` | 2 days |
| 2. Implement encoder forward + verify against fastino's encoder.onnx output | 2 days |
| 3. Implement remaining 7 heads (token_gather, span_rep, schema_gather, count_pred, count_lstm_fixed, scorer, classifier) | 5 days |
| 4. Wire 8-step pipeline; parity test against ONNX path (max_abs_diff < 5e-3) | 2 days |
| 5. LoRA loader: parse adapter_config.json, load adapter_model.safetensors | 1.5 days |
| 6. Apply LoRA deltas in forward pass; load_adapter / set_adapter / unload_adapter API | 2 days |
| 7. Hot-swap test: load 2 adapters, swap, verify outputs differ; benchmark swap latency | 1 day |
| 8. Cargo features (`gliner2-fastino-candle`, `-cuda`, `-metal`); catalog row | 0.5 day |
| 9. Integration test against `fastino/gliner2-multi-v1` (Python repo, not the SemplificaAI ONNX export) | 1 day |
| 10. Docs: rustdoc with hot-swap example, BACKENDS.md entry | 1 day |

**Total: ~18 days (~3.5 weeks).** Phase 4 is the largest of the follow-up tracks because it's a complete second backend, not an extension of the existing one.

## 7. Acceptance

- [ ] `GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-multi-v1")` loads the base model.
- [ ] Output parity with the ONNX backend on a fixed fixture: `max_abs_diff < 5e-3` on entity scores.
- [ ] `load_adapter("legal", path)` succeeds for a `peft`-format adapter directory.
- [ ] `set_adapter("legal")` then `extract_with_types(...)` produces different output than the base model on legal-domain text.
- [ ] `set_adapter` latency < 1 ms (benchmarked with `criterion`).
- [ ] Multi-adapter test: load 5 adapters, swap among them in a loop, verify each produces consistent output for its domain.
- [ ] Cargo features all compile-check: `gliner2-fastino-candle`, `gliner2-fastino-candle-cuda`, `gliner2-fastino-candle-metal`.

## 8. Risks

1. **DeBERTa-v2 implementation.** anno's `encoder_candle` has v3 (different attention pattern, different relative positional encoding). v2 is similar enough to share most code but differs in:
   - Tokenizer (SentencePiece BPE vs ELECTRA-style).
   - Relative-position bucket calculation.
   - Pre-LN vs post-LN ordering (varies across HuggingFace exports).
   Mitigation: validate encoder output against the SemplificaAI encoder.onnx output on a fixed input; max_abs_diff < 1e-4.

2. **Forward pass perf.** Pure Candle CPU inference is typically 1.5-3× slower than ort. If Phase 4 is "slow but flexible," users will stay on ONNX for production. Mitigation: benchmark with criterion early; if Candle is more than 4× slower than ort on the fixture, escalate (consider mixed-mode: ort encoder + Candle adapters via runtime conversion).

3. **PEFT adapter format drift.** The `peft` library has changed adapter file formats over versions. Adapter packs from 2024 may not match 2026 conventions. Mitigation: support a documented set of `adapter_config.json` versions; reject unknown ones with a clear error.

4. **Target-modules mismatch.** PEFT's `target_modules` regex is matched against the model's named parameter list. Our Candle model's parameter naming might differ from the PyTorch original (e.g., `attention.query.weight` vs `attn.q_proj.weight`). Mitigation: include a mapping table in `lora.rs` that translates HF naming → anno naming.

5. **CUDA/Metal correctness.** Candle's CUDA backend is well-tested, Metal less so. LoRA delta application uses matmul which is the most-tested op; should be fine. Mitigation: test all three feature configurations (`-cuda`, `-metal`, default CPU) on whatever hosts are available.

6. **GIL-equivalent**: parallel inference. The active LoRA adapter is shared state. If two threads `extract_with_types` concurrently and one of them swaps the adapter mid-call, results are undefined. Mitigation: hold an `Arc<...>` snapshot of the active adapter at `extract_with_types` entry; subsequent `set_adapter` doesn't affect the in-flight call.

## 9. Costs to weigh before starting

This is the largest follow-up phase. Real questions before committing:

- **Do you have a multi-tenant or multi-domain inference workload TODAY?** If no, Phase 4 is theoretical capability without users.
- **Is the alternative (ship 6 GB per domain) actually unworkable?** For a server with 64 GB RAM and 1 TB SSD, 5 domains × 6 GB = 30 GB; not great but not blocker.
- **Could you achieve the same outcome by running Python `gliner2` in a sidecar process?** Yes, with worse startup time and Python deployment cost. But it's free.

If the answer to "do you have the workload" is no, defer Phase 4 indefinitely. The ONNX backend is sufficient for ~95% of users.

## 10. References

- PEFT format: <https://huggingface.co/docs/peft>
- LoRA paper: arXiv:2106.09685
- Candle: <https://github.com/huggingface/candle>
- Phase 1 spec: `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md`
- Phase 3 spec/plan: `docs/superpowers/specs/...`, `docs/superpowers/plans/2026-05-05-gliner2-fastino-phase3.md`
- Roadmap Track E: `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md`
- anno's existing Candle infrastructure: `crates/anno/src/backends/{gliner_candle,encoder_candle,candle.rs}`
