# Architecture Decision: Geometric Libraries

**Status**: Decided  
**Date**: 2024-12-10  
**Decision**: Option A implemented - anno optionally depends on subsume-core

## Context

Three related projects exist:

| Project | Language | Focus | Key Types |
|---------|----------|-------|-----------|
| **anno** | Rust | NLP inference | `BoxEmbedding`, geometric stubs |
| **subsume** | Rust | Generic box geometry | `Box` trait, Gumbel, training utils |
| **box-coref** | Python | Training infrastructure | PyTorch/MLX, export to ONNX |

Current state:
- anno has its own `BoxEmbedding` (simple `Vec<f32>`, no Gumbel)
- subsume has rich `Box` trait (framework-agnostic, Gumbel support)
- box-coref trains models, exports to anno via ONNX
- anno does NOT depend on subsume

## The Question

Should anno:
1. Depend on subsume for box geometry?
2. Merge box-coref into anno?
3. Keep all three separate?

## Analysis

### Option A: Anno depends on subsume-core

```toml
# anno/Cargo.toml
[dependencies]
subsume-core = { path = "../subsume/subsume-core" }
```

**Pros**:
- DRY: subsume has 149+ tests, Gumbel boxes, training utilities
- Consistent: one definition of "box"
- Rich: anno gets volume regularization, diagnostics, etc.

**Cons**:
- Coupling: anno tied to subsume's API decisions
- Complexity: subsume's `Box` trait uses associated types (`Scalar`, `Vector`)
- Overkill?: anno's inference-only use case is simple

**Implementation**:
```rust
// anno's BoxEmbedding could implement subsume::Box
impl subsume_core::Box for BoxEmbedding {
    type Scalar = f32;
    type Vector = Vec<f32>;
    fn min(&self) -> &Self::Vector { &self.min }
    fn max(&self) -> &Self::Vector { &self.max }
    // ...
}
```

**Verdict**: Reasonable if subsume is considered stable. But subsume's trait 
design (Gumbel-focused, training-oriented) may not match anno's inference needs.

### Option B: Merge box-coref into anno

```
anno/
├── anno/           # Rust lib (unchanged)
├── (CLI in anno)   # `anno/src/bin/anno.rs` + `anno/src/cli/`  
├── anno-train/     # NEW: Python training
│   ├── pyproject.toml
│   └── training/   # From box-coref
└── docs/
```

**Pros**:
- Single repo, single source of truth
- Easier to keep in sync

**Cons**:
- Mixed Rust/Python in one repo
- Different CI/testing needs
- Python training is large (66+ files, MLX/PyTorch/cloud support)

**Verdict**: Not recommended. Training and inference have different lifecycles.
Many successful projects keep training separate (e.g., transformers vs ONNX Runtime).

### Option C: Keep separate, improve interfaces

Current approach but with:
1. Clear export format specification (ONNX + config JSON)
2. Shared documentation
3. Cross-references in code

**Pros**:
- Independence: each project evolves separately
- Focus: anno = inference, subsume = math, box-coref = training
- Simplicity: no dependency tangles

**Cons**:
- Some duplication (anno's BoxEmbedding vs subsume's Box)
- Coordination overhead

**Verdict**: Current approach. Duplication is acceptable when:
- anno's BoxEmbedding is much simpler (100 lines vs subsume's full trait)
- The semantics differ (anno doesn't need Gumbel for inference)

### Option D: Anno's geometric module uses subsume patterns

Don't depend on subsume, but adopt its design patterns:

```rust
// anno/src/geometric/box_trait.rs (NEW)
// Similar to subsume::Box but simpler
pub trait GeometricBox {
    fn min(&self) -> &[f32];
    fn max(&self) -> &[f32];
    fn volume(&self) -> f32;
    fn containment_prob(&self, other: &Self) -> f32;
}

impl GeometricBox for BoxEmbedding { ... }
impl GeometricBox for HyperbolicEmbedding { ... }  // If mapped to bounding box
```

**Pros**:
- Unified interface within anno
- No external dependency
- Can adopt subsume's design without coupling

**Cons**:
- Still some duplication
- anno's trait may diverge from subsume

**Verdict**: Worth considering. Keeps anno self-contained while learning from subsume.

## Recommendation

**Short term**: Option C (keep separate)
- anno, subsume, box-coref remain independent
- Each has clear responsibility

**Medium term**: Option D (adopt patterns)
- anno's `geometric` module defines `GeometricMention` trait (already exists)
- This trait unifies boxes, hyperbolic, sheaf representations
- No external dependency, but consistent design

**Long term (maybe)**: Option A (depend on subsume-core)
- If subsume-core stabilizes and becomes a published crate
- Anno could depend on it for advanced box operations
- But keep simple `BoxEmbedding` for basic use

## Rationale

1. **Separation of concerns**: Training (box-coref) vs inference (anno) have different needs
2. **Simplicity**: anno's BoxEmbedding is 100 lines; subsume's Box trait is richer but heavier
3. **Independence**: Each project can evolve at its own pace
4. **Future flexibility**: If subsume becomes THE box library, anno can adopt later

## What subsume ACTUALLY provides

Reading subsume carefully, it's more than "training-focused":

### Core (`subsume-core`)
- `Box` trait: Framework-agnostic geometric operations
- `GumbelBox` trait: Probabilistic boxes (training)
- Distance metrics from recent papers:
  - **Depth distance** (RegD 2025): Volume-aware distance
  - **Boundary distance** (RegD 2025): For containment chains  
  - **Vector-to-box distance** (Concept2Box 2023): Hybrid representations
- Training utilities: Metrics, diagnostics, quality assessment

### What anno's BoxEmbedding is missing
| Feature | anno | subsume |
|---------|------|---------|
| Basic operations | `volume()`, `intersection_volume()` | Same |
| Containment probability | Simple ratio | Gumbel + temperature |
| Depth distance | No | Yes (RegD 2025) |
| Boundary distance | No | Yes (RegD 2025) |
| Vector-to-box distance | No | Yes (Concept2Box 2023) |
| Training diagnostics | No | Rich (phase detection, gradients) |

### Genuine trade-off

**subsume** assumes:
- Associated types for tensors (Scalar, Vector)
- Temperature parameters everywhere
- Result types with BoxError

**anno's BoxEmbedding** is simpler:
- Always `Vec<f32>`
- No temperature (hard boxes for inference)
- Plain `f32` returns, panics on errors

The question: is the simplicity worth the missing features?

## Revised Recommendation

After deeper analysis, the cleanest path is:

### Phase 1: Experiment with subsume dependency

Try adding `subsume-core` as an optional dependency:

```toml
# anno/Cargo.toml
[dependencies]
subsume-core = { path = "../subsume/subsume-core", optional = true }

[features]
subsume = ["dep:subsume-core"]
```

Then anno's BoxEmbedding can implement subsume's Box trait when the feature is enabled:

```rust
#[cfg(feature = "subsume")]
impl subsume_core::Box for BoxEmbedding {
    type Scalar = f32;
    type Vector = Vec<f32>;
    // ...
}
```

This gives:
- Gradual adoption (feature-gated)
- Access to subsume's distance metrics
- No breaking changes to existing code

### Phase 2: Evaluate

After using subsume for a while:
- Does the trait fit anno's needs?
- Is the temperature parameter useful at inference?
- Are the distance metrics valuable?

### Phase 3: Decide

Based on experience:
- **If good fit**: Make subsume a required dependency
- **If awkward fit**: Keep anno's simple BoxEmbedding, learn from subsume's design

### box-coref: Keep separate

box-coref should remain separate because:
- Different language (Python vs Rust)
- Different purpose (training vs inference)
- Different lifecycle (experiments vs stable library)

## Action Items

1. [x] Try adding `subsume-core` as optional dependency to anno
2. [x] Implement `subsume::Box` for anno's `BoxEmbedding`
3. [ ] Add depth/boundary distance to anno's geometric module (using subsume)
4. [ ] Document export format between box-coref and anno
5. [x] Keep box-coref separate (training/inference separation is good)
6. [ ] Consider publishing subsume to crates.io for wider use

## What Was Implemented

```toml
# anno/Cargo.toml
[dependencies]
subsume-core = { path = "../../subsume/subsume-core", optional = true }

[features]
subsume = ["dep:subsume-core"]
```

```rust
// anno/src/backends/box_embeddings.rs
#[cfg(feature = "subsume")]
impl subsume_core::Box for BoxEmbedding {
    type Scalar = f32;
    type Vector = Vec<f32>;
    // ... all trait methods implemented
}
```

New methods added to BoxEmbedding:
- `intersection(&self, other) -> Self`
- `union(&self, other) -> Self`
- `overlap_prob(&self, other) -> f32`
- `distance(&self, other) -> f32`

These work without the subsume feature, but when enabled, BoxEmbedding also
implements `subsume_core::Box` for compatibility with subsume's distance metrics.
