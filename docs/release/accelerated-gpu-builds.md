# Accelerated GPU Builds

The default Hacienda / anno-rag release remains the CPU build. Use an accelerated sidecar only when the machine has matching hardware and drivers.

## Artifacts

| Artifact suffix | Platform | Hardware |
| --- | --- | --- |
| `aarch64-apple-darwin-metal` | Apple Silicon macOS | Apple Metal |
| `x86_64-pc-windows-msvc-cuda` | Windows x64 | NVIDIA CUDA |

## Runtime Selection

Set `ANNO_ACCELERATOR` before starting `anno-rag`:

```text
auto   try the compiled accelerator, then fall back to CPU
cpu    force CPU
metal  require Apple Metal and fail if unavailable
cuda   require NVIDIA CUDA and fail if unavailable
```

## Diagnostics

Run:

```bash
anno-rag diagnose-gpu
```

The command prints JSON with the compiled features, requested accelerator, selected embedder device, selected detector provider, and any CPU fallback reason.

## Windows CUDA

The CUDA build is for NVIDIA GPUs. Install a compatible NVIDIA driver and CUDA runtime before using it. If `ANNO_ACCELERATOR=cuda` fails, use the CPU build or set `ANNO_ACCELERATOR=cpu`.

## Apple Metal

The Metal build is for Apple Silicon macOS. If `ANNO_ACCELERATOR=metal` fails, use the CPU build or set `ANNO_ACCELERATOR=cpu`.
