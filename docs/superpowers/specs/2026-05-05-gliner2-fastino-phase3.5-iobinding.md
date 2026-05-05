# gliner2_fastino — Phase 3.5: IOBinding mode

**Status:** spec — not yet implemented. Builds on Phase 3 (multi-session standard mode shipped 2026-05-05). Companion to `2026-05-04-gliner2-fastino-design.md`.

## 1. The problem

Phase 3 standard mode runs the 8 ONNX sessions sequentially. Each session boundary round-trips tensors through Rust ndarrays:

```
encoder → [hidden_states f32]      ← Vec<f32>, 1*L*768 = ~2 KB/token
       → [span_rep input]
span_rep → [span_embeddings f32]   ← Vec<f32>, 1*L*8*768 = ~24 KB/token
       → [scorer input]
scorer  → [scores f32]             ← Vec<f32>, 20*L*8*M
```

For a 200-token input with 4 labels:
- Tensor data shuffled across session boundaries: ~5 MB per inference.
- Each transition: `try_extract_tensor` → `into_owned()` (alloc + copy) → `Tensor::from_array` (alloc + copy).

**On CPU:** the data movement is RAM-bandwidth-limited. ~2-3× overhead vs. fused inference.

**On GPU:** every transition is a CPU↔GPU memory copy via PCIe. 8 sessions × 2 copies = 16 PCIe round trips per inference. PCIe 4.0 x16 ≈ 32 GB/s → 5 MB ≈ 156 µs per copy → **~2.5 ms per inference just in memcpy** before any compute. For an inference whose actual GPU compute is ~10-30 ms, that's 10-25% pure overhead, scaling worse on smaller batches.

## 2. What IOBinding solves

ort's `IoBinding` API binds session inputs/outputs to a specific allocator (CPU host memory, CUDA device memory, etc.). Once tensors live in that allocator, subsequent sessions can read them directly — no copy, no Rust-side ownership transfer.

For our pipeline:

```
encoder.io_binding:
    bind_input  "input_ids"      → host buffer
    bind_output "hidden_states"  → CUDA device buffer A
encoder.run(io_binding)

token_gather.io_binding:
    bind_input  "last_hidden_state" → CUDA device buffer A   (same buffer!)
    bind_output "text_embs"          → CUDA device buffer B
token_gather.run(io_binding)

... etc through all 8 sessions ...

scorer.io_binding:
    bind_input  "span_embeddings" → device buffer K
    bind_input  "struct_proj"     → device buffer L
    bind_output "entity_scores"   → host buffer or device buffer
scorer.run(io_binding)
```

Tensors stay in device memory the whole pipeline. The only host↔device transfers are: input_ids (in), final scores (out).

## 3. Reference implementation

Upstream `SemplificaAI/gliner2-rs/rust_component/src/lib_v2.rs:285-660` — `Gliner2EngineV2::extract_iobinding`. ~370 LOC of `IoBinding` setup, allocator management, and session chaining. The spec for this phase = port that function.

The corresponding `_iobinding.onnx` ONNX files in the SemplificaAI snapshot's `fp16_v2/` and `fp32_v2/` subdirs (named `<base>_<dtype>_iobinding.onnx`) are functionally identical to the regular `_v2.onnx` graphs — they just have I/O tensor metadata that ort's IoBinding API can hook into more efficiently.

## 4. Scope

### In scope

- Add an `ExecutionMode` enum (`Standard | IoBinding`) on `GLiNER2Fastino`.
- Default to `Standard` (Phase 3 path); `IoBinding` is opt-in via `from_local_with_options(cfg)` taking an extended config.
- `Sessions::from_dir_with_cfg` recognizes the `_iobinding` ONNX variants (currently it only loads non-iobinding files).
- Port `extract_iobinding` from upstream `lib_v2.rs:285-660` as `pipeline_iobinding.rs`.
- The IoBinding pipeline replicates the same 8-session chain as standard mode, but uses `Session::create_binding()` and `binding.bind_*` calls.
- GPU EP wiring: when CUDA or CoreML EP is selected, IoBinding mode binds outputs to device memory automatically.
- Validation parity: `cargo test` runs both modes against the same fixture; outputs must match within `max_abs_diff < 1e-5`.

### Out of scope

- Re-architecting the standard mode pipeline (it's good as-is for CPU-mostly workloads).
- Custom allocators (use ort's defaults).
- Multi-device pipelining (e.g., encoder on GPU0, scorer on GPU1).
- Async / streaming inference.

## 5. Design

### Module layout

```
crates/anno/src/backends/gliner2_fastino/
├── pipeline.rs             — Phase 3 standard mode (no change)
├── pipeline_iobinding.rs   — Phase 3.5, NEW: IoBinding mode
├── sessions.rs             — extended: IoBinding ONNX variant selection
├── mod.rs                  — extract_ner / classify dispatch on ExecutionMode
└── ...
```

### API surface

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionMode {
    /// Phase 3 path: round-trip tensors through ndarrays.
    /// CPU-friendly, simple debugging.
    #[default]
    Standard,
    /// Phase 3.5 path: zero-copy via ort IoBinding. Required for
    /// efficient GPU inference; 2-3× CPU speedup.
    IoBinding,
}

// Public: extend OnnxSessionConfig conceptually, but keep
// gliner2_fastino-specific config to avoid bloating hf_loader's API.
#[non_exhaustive]
pub struct GLiNER2FastinoConfig {
    pub onnx: hf_loader::OnnxSessionConfig,
    pub execution_mode: ExecutionMode,
}

impl GLiNER2Fastino {
    pub fn from_local_with_options(
        model_dir: &Path,
        cfg: GLiNER2FastinoConfig,
    ) -> crate::Result<Self>;
}
```

The struct gains an `execution_mode: ExecutionMode` field. `extract_ner` and `classify` dispatch on it:

```rust
pub(crate) fn extract_ner(...) -> Result<Vec<Entity>> {
    match self.execution_mode {
        ExecutionMode::Standard => pipeline::extract_ner_standard(self, ...),
        ExecutionMode::IoBinding => pipeline_iobinding::extract_ner_iobinding(self, ...),
    }
}
```

Both code paths share `processor::SchemaTransformer` (input prep) and `decoder::greedy_nms` / `decode_entities` (output decoding). Only the 8-session chain differs.

### ONNX variant selection

When `execution_mode == IoBinding`, `Sessions::from_dir_with_cfg` looks for `<base>_<dtype>_iobinding.onnx` first, falling back to `<base>_<dtype>.onnx`:

```rust
let candidate = |name: &str| -> PathBuf {
    let with_iob = try_dir.join(format!("{name}{suffix_no_ext}_iobinding.onnx"));
    if cfg.execution_mode == ExecutionMode::IoBinding && with_iob.exists() {
        with_iob
    } else {
        try_dir.join(format!("{name}{suffix}"))
    }
};
```

The `_iobinding` ONNX files have ONNX-Runtime-specific metadata (input/output shape annotations) that allow IoBinding to pre-allocate device memory. They're functionally identical to the non-iobinding variants — output shapes match.

### Allocator management

Per upstream `lib_v2.rs:285-340`, allocators are created lazily on first `extract_iobinding` call and cached:

```rust
struct IoBindingState {
    cpu_allocator: ort::memory::Allocator,
    device_allocator: Option<ort::memory::Allocator>, // CUDA/CoreML if available
    // Reusable host-side staging buffers (input_ids, attention_mask).
    // Reused across calls to avoid per-call alloc.
    input_ids_host: ort::value::Tensor<i64>,
    attn_mask_host: ort::value::Tensor<i64>,
}
```

Stored in `GLiNER2Fastino` behind a `RwLock` (matches upstream's `RwLock<ExecutionMode>` pattern).

## 6. Phase plan

| Step | Estimate |
|---|---|
| 1. Read upstream `extract_iobinding` (lib_v2.rs:285-660); port allocator setup | 1 day |
| 2. Implement `pipeline_iobinding::run_encoder` + chain through token_gather/span_rep | 2 days |
| 3. Implement schema_gather, count_pred_argmax, count_lstm_fixed, scorer chain | 2 days |
| 4. Implement classifier in IoBinding mode | 1 day |
| 5. ExecutionMode dispatch in mod.rs; from_local_with_options API | 0.5 day |
| 6. ONNX variant selection in sessions.rs | 0.5 day |
| 7. Parity test: standard vs IoBinding produce same output (max_abs_diff < 1e-5) | 1 day |
| 8. CUDA-host integration test (gated, opt-in CI label) | 1 day |
| 9. Docs + benchmarks | 1 day |

**Total: ~10 days.** Larger than Phase 3's ~9 days because the IoBinding API is more verbose and has more edge cases (mid-pipeline shape inference, CUDA EP gotchas).

## 7. Acceptance

- [ ] `ExecutionMode::IoBinding` opt-in via `from_local_with_options`.
- [ ] Both modes produce bit-identical output on CPU (max_abs_diff < 1e-5).
- [ ] CPU benchmark: IoBinding ≥ 2× faster than Standard on a 200-token, 4-label fixture.
- [ ] CUDA benchmark (if a GPU host is available): IoBinding ≥ 5× faster than Standard with `prefer_cuda=true`.
- [ ] Existing Phase 3 integration tests still pass in `Standard` mode (regression check).

## 8. Risks

1. **Shape inference at session boundaries.** IoBinding pre-allocates output buffers; if a session's output shape depends on input shape (it does for span_rep — shape is `[B, num_words, MAX_WIDTH, H]` where num_words varies per call), the bound output buffer may need to be re-bound per call. Upstream handles this; port carefully.
2. **CUDA EP silent CPU fallback.** If the CUDA EP can't load a graph, ort silently runs on CPU. IoBinding mode would still "work" but with no speedup. Mitigation: log an explicit warning at session creation time when CUDA was requested but cudart wasn't found.
3. **`_iobinding.onnx` metadata drift.** Future SemplificaAI re-exports might change tensor names or remove the iobinding variants. Mitigation: fall back to non-iobinding ONNX files (already designed in §5).
4. **`half::f16` ↔ ort Tensor type-class mismatch.** ort rc.12 doesn't directly support `Tensor<f16>`; Phase 3 worked around this for the classifier by converting fp16 → f32 in Rust. IoBinding mode needs to bind fp16 buffers natively. Mitigation: investigate `ort::value::Tensor::from_array_with_dtype` or fall back to f32 for the classifier specifically.
5. **Test parity tolerance.** `max_abs_diff < 1e-5` may be too tight if any session uses fp16 internally and standard-mode reads the f32 cast. Suggest 1e-4 instead, document why.

## 9. Out of scope; tracked separately

- **Phase 4 (Candle path):** standalone. IoBinding only matters for ort.
- **Multi-device:** future, requires async runtime.
- **Fused single-graph export:** would obviate IoBinding entirely. Different track; depends on fastino's export tooling cooperating.

## 10. References

- Upstream port source: `SemplificaAI/gliner2-rs/rust_component/src/lib_v2.rs:285-660` (Apache-2.0).
- ort IoBinding API: <https://ort.pyke.io/perf/io-binding>
- Phase 3 spec: `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md` §5 Phase 3.
- Phase 3 plan: `docs/superpowers/plans/2026-05-05-gliner2-fastino-phase3.md`.
- Roadmap: `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md` Track D.
