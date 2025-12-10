# anno-models

Runtime-agnostic ML model backends for NER.

## Status

**Skeleton with working traits.** The `Model` and `Runtime` traits are defined
and usable. Implementations are minimal - real backends live in `anno/src/backends/`
until migration.

## Purpose

Eliminate the N×M problem:

```
Current (anno/backends/):          Target (anno-models):
├── gliner_onnx.rs                 ├── GLiNER<R: Runtime>
├── gliner_candle.rs      →        ├── NuNER<R: Runtime>  
├── gliner_poly.rs                 └── CRF<R: Runtime>
├── nuner.rs                           ↓
└── ...                            impl Runtime for OnnxRuntime
                                   impl Runtime for CandleRuntime
                                   impl Runtime for BurnRuntime
```

## Usage

For now, use `anno::Model` directly. When writing new backends, consider
implementing against `anno_models::Model` for future compatibility.

```rust
// Future:
use anno_models::{Model, GLiNERConfig, OnnxRuntime};

let runtime = OnnxRuntime::new()?;
let model = GLiNER::load(runtime, GLiNERConfig::base())?;
let entities = model.extract("Steve Jobs founded Apple.", &["person", "org"])?;
```

## Migration Path

1. New backends implement `anno_models::Model`
2. Adapter makes them compatible with `anno::Model`
3. Eventually anno re-exports from anno-models
4. Delete duplicate backends from anno

## Features

- `onnx` - ONNX Runtime (production, broad hardware)
- `candle` - Candle (pure Rust, good Metal support)
- `burn` - Burn (WebGPU, training capable)
- `metal` - Metal acceleration
- `cuda` - CUDA acceleration
