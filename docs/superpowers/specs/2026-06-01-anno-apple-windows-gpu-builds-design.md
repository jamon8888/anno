# Apple and Windows GPU Builds for anno-rag - Design

**Date:** 2026-06-01
**Status:** Draft (awaiting review)
**Scope:** Apple Silicon Metal and Windows NVIDIA CUDA sidecar builds for `anno-rag`

## 1. Goal

Ship optimized `anno-rag` binaries for the two GPU-capable desktop OS targets that matter now:

- **Apple:** `aarch64-apple-darwin` with Metal acceleration.
- **Windows:** `x86_64-pc-windows-msvc` with NVIDIA CUDA acceleration.

The default release remains CPU-first and stable. GPU binaries are explicit sidecar artifacts so normal users keep a predictable install path, while users with compatible hardware can opt into a faster build.

## 2. Non-goals

- No DirectML, AMD, or Intel GPU support on Windows in V1.
- No Linux GPU release artifact in V1.
- No replacement of the current CPU cargo-dist release flow.
- No bundled GPU drivers, CUDA toolkit, or Apple system frameworks beyond normal OS/runtime linkage.
- No mandatory GPU path. `anno-rag` must continue to run on CPU.
- No silent "GPU" marketing artifact that actually runs on CPU without reporting it.

## 3. Current repo context

The lower-level `anno` crate already has most build-time pieces:

- `onnx-cuda = ["onnx", "ort/cuda"]`
- `onnx-coreml = ["onnx", "ort/coreml"]`
- `gliner2-fastino-cuda = ["gliner2-fastino", "onnx-cuda"]`
- `gliner2-fastino-coreml = ["gliner2-fastino", "onnx-coreml"]`
- `gliner2-fastino-candle-metal = ["gliner2-fastino-candle", "candle-core/metal", "candle-transformers/metal"]`
- `gliner2-fastino-candle-cuda = ["gliner2-fastino-candle", "candle-core/cuda", "candle-transformers/cuda"]`
- `metal = ["candle", "candle-core/metal"]`
- `cuda = ["candle", "candle-core/cuda"]`

The runtime gaps are in `anno-rag`:

- `crates/anno-rag/src/embed.rs` currently selects `Device::Cpu`.
- `crates/anno-rag/src/detect.rs` loads `GLiNER2Fastino::from_local` / `from_pretrained` without passing `OnnxSessionConfig`.
- `anno-rag`, `anno-rag-mcp`, and `anno-rag-bin` do not yet expose GPU feature pass-throughs.
- Existing ONNX session setup can register CUDA/CoreML providers, but GPU preference is not surfaced from the `anno-rag` CLI/config.

## 4. Product shape

### Artifacts

Keep the existing CPU release artifacts as the default. Add explicit accelerated sidecars:

| Target | Artifact name | Acceleration |
|--------|---------------|--------------|
| `aarch64-apple-darwin` | `hacienda-${tag}-aarch64-apple-darwin-metal.tar.gz` | Candle Metal |
| `x86_64-pc-windows-msvc` | `hacienda-${tag}-x86_64-pc-windows-msvc-cuda.zip` | ONNX CUDA, plus Candle CUDA where feasible |

Installers and `.mcpb` packaging stay on the CPU release path first. GPU `.mcpb` artifacts can be added after the binary sidecars are proven stable.

### Runtime selection

Add a single accelerator preference surface shared by CLI, MCP, and library code:

```text
ANNO_ACCELERATOR=auto|cpu|metal|cuda
```

Behavior:

- `auto`: try the platform GPU flavor compiled into the binary, then fall back to CPU with a diagnostic warning.
- `cpu`: force CPU even in a GPU-enabled binary.
- `metal`: require Metal. If unavailable or not compiled in, fail with a clear error.
- `cuda`: require CUDA. If unavailable or not compiled in, fail with a clear error.

Explicit accelerator requests must fail loudly. Only `auto` may fall back to CPU.

### Diagnostics

Add an `anno-rag diagnose-gpu` command before shipping the sidecars. It should report:

- binary flavor and target triple,
- compiled GPU features,
- requested accelerator preference,
- selected embedder device,
- selected detector provider,
- CUDA/CoreML/Metal availability where detectable,
- whether CPU fallback occurred,
- actionable remediation when an explicit accelerator cannot run.

This command is required because ONNX Runtime CUDA can silently fall back to CPU if runtime libraries or drivers are missing.

## 5. Implementation architecture

### Accelerator preference type

Introduce a small shared type in `anno-rag`:

```rust
enum AcceleratorPreference {
    Auto,
    Cpu,
    Metal,
    Cuda,
}
```

Parsing order:

1. CLI/config override, if added for a command.
2. `ANNO_ACCELERATOR`.
3. `Auto`.

The resolver returns a structured decision rather than a bare device:

```rust
struct AcceleratorDecision {
    requested: AcceleratorPreference,
    selected: SelectedAccelerator,
    fallback_reason: Option<String>,
}
```

This keeps diagnostics and normal startup consistent.

### Feature propagation

Add pass-through features rather than making GPU paths default:

```toml
# crates/anno-rag/Cargo.toml
gpu-metal = ["anno/gliner2-fastino-candle-metal", "anno/metal"]
gpu-cuda = ["anno/gliner2-fastino-cuda"]

# crates/anno-rag-mcp/Cargo.toml
gpu-metal = ["anno-rag/gpu-metal"]
gpu-cuda = ["anno-rag/gpu-cuda"]

# crates/anno-rag-bin/Cargo.toml
gpu-metal = ["anno-rag/gpu-metal", "anno-rag-mcp/gpu-metal"]
gpu-cuda = ["anno-rag/gpu-cuda", "anno-rag-mcp/gpu-cuda"]
```

If the Windows embedder also moves to Candle CUDA in V1, extend `gpu-cuda` to include `anno/cuda` and the Candle CUDA feature set after validating build and runtime behavior on a CUDA runner.

### Apple V1: Metal

Apple Silicon should use the Candle Metal path first:

- Target: `aarch64-apple-darwin`.
- Build feature: `gpu-metal`.
- Embedder: choose `Device::Metal(0)` when compiled and available.
- Detector: expose a public `GLiNER2Fastino` Candle loading path that accepts a device/config, then use Metal for the Candle backend.

CoreML is not the V1 primary route. It remains a possible follow-up for ONNX models where CoreML provider support is materially better than Candle Metal.

### Windows V1: CUDA

Windows should use NVIDIA CUDA first because it matches the existing `ort/cuda` support:

- Target: `x86_64-pc-windows-msvc`.
- Build feature: `gpu-cuda`.
- Detector: construct `GLiNER2Fastino` with `OnnxSessionConfig { prefer_cuda: true, .. }`.
- Embedder: remain CPU in the first CUDA sidecar unless Candle CUDA is validated on Windows; upgrade to Candle CUDA only after a real GPU smoke confirms correctness and packaging.

DirectML is deferred. It is the right future path for broader Windows GPU coverage, but it should be a separate `gpu-directml` design after the NVIDIA CUDA sidecar is reliable.

## 6. Release and CI design

### Release workflow

Do not fold accelerated sidecars into cargo-dist initially. Add a manual/tag-only accelerated release path, either:

- a new `.github/workflows/release-accelerated.yml`, or
- a clearly separated job group in `.github/workflows/release-binaries.yml`.

Recommended V1: a new workflow. It keeps GPU artifacts visibly separate from CPU cargo-dist and reduces the risk of breaking installers.

Triggers:

- `workflow_dispatch` with an existing `v*` tag.
- Optional tag trigger after the workflow is stable.

Outputs:

- `dist/hacienda-${tag}-aarch64-apple-darwin-metal.tar.gz`
- `dist/hacienda-${tag}-x86_64-pc-windows-msvc-cuda.zip`
- `dist/SHA256SUMS-accelerated.txt`

### CI checks

Use three validation tiers:

1. **Compile checks on normal runners:** `cargo check -p anno-rag-bin --features gpu-metal` on macOS and `cargo check -p anno-rag-bin --features gpu-cuda` on Windows where dependencies allow it.
2. **Apple Metal smoke:** run `anno-rag diagnose-gpu` and one tiny inference path on an Apple Silicon runner when available.
3. **Windows CUDA smoke:** run on a self-hosted or paid GPU runner with NVIDIA drivers/CUDA runtime. A build-only check is not enough because ONNX CUDA can fall back to CPU.

Required negative test:

- On a non-CUDA host, `ANNO_ACCELERATOR=cuda anno-rag diagnose-gpu` must fail clearly rather than reporting success on CPU.

### Budget constraint

As of 2026-06-01, GitHub Actions cannot start jobs for this account because billing/spending-limit checks are failing. GPU release validation must wait until that is fixed. Even after billing is fixed, GPU workflows should stay manual/tag-only until cache behavior and runtime smoke costs are known.

## 7. User-facing docs

Add a short release note / install section once implementation exists:

- CPU build: default, recommended for all users.
- Apple Metal build: Apple Silicon macOS users with local workloads.
- Windows CUDA build: NVIDIA GPU users only.
- How to run `anno-rag diagnose-gpu`.
- How to force CPU: `ANNO_ACCELERATOR=cpu`.
- How to require GPU and fail if unavailable: `ANNO_ACCELERATOR=metal` or `ANNO_ACCELERATOR=cuda`.

## 8. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Silent CPU fallback | `diagnose-gpu`, explicit accelerator failures, smoke tests that assert selected provider/device. |
| CUDA runtime mismatch | Document NVIDIA-only support; require runtime smoke on real GPU before publishing a CUDA sidecar. |
| Candle Metal op coverage gaps | Start with Apple Silicon smoke tests and keep CPU fallback for `auto`. |
| Large binaries and support burden | Keep sidecars separate from default CPU artifacts. |
| Build minutes increase | Manual/tag-only GPU release workflow, no PR matrix explosion. |
| Runtime config drift between CLI and MCP | Put accelerator parsing/decision in `anno-rag`, pass through from `anno-rag-mcp` and `anno-rag-bin`. |

## 9. Phases

1. **Feature plumbing and diagnostics**
   - Add `gpu-metal` / `gpu-cuda` feature pass-throughs.
   - Add accelerator preference parsing.
   - Add `anno-rag diagnose-gpu`.
   - Keep CPU behavior unchanged.

2. **Apple Metal path**
   - Select Candle Metal for the embedder.
   - Expose a public device-aware Candle GLiNER2 load path if needed.
   - Smoke test on Apple Silicon.

3. **Windows CUDA path**
   - Pass `prefer_cuda = true` into GLiNER2 ONNX session setup.
   - Add explicit failure for `ANNO_ACCELERATOR=cuda` when CUDA is unavailable.
   - Smoke test on a real NVIDIA runner.

4. **Accelerated release sidecars**
   - Add manual/tag accelerated release workflow.
   - Package sidecar archives and checksums.
   - Attach artifacts to GitHub Releases after smoke tests pass.

5. **Follow-ups**
   - Consider Windows DirectML.
   - Consider Apple CoreML for ONNX models.
   - Consider GPU `.mcpb` packaging after binary sidecars are stable.

## 10. Acceptance criteria

- Default CPU builds and existing release artifacts are unchanged.
- New GPU features are opt-in and compile-gated.
- `anno-rag diagnose-gpu` reports compiled features, selected device/provider, and fallback status.
- `ANNO_ACCELERATOR=cpu` forces CPU on GPU-capable binaries.
- Explicit unavailable accelerator requests fail clearly.
- Apple Metal sidecar runs a real smoke on Apple Silicon before publication.
- Windows CUDA sidecar runs a real smoke on NVIDIA CUDA hardware before publication.
- Sidecar artifact names include `metal` or `cuda` so users cannot confuse them with default CPU builds.
