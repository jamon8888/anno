# Anno Workspace Architecture

**Status**: Implemented  
**Date**: 2024-12-10

## Summary

| Component | Status | Purpose |
|-----------|--------|---------|
| `subsume-core` sheaf/hyperbolic | Working | Pure geometric algebra (12 tests passing) |
| `anno-models` | Spec | Runtime abstraction for future backend consolidation |
| `anno-rel` | Working | Relation extraction types + evaluation |
| `anno/src/ingest/` | Kept | CLI ingestion, trait boundary if models need it |

## Decisions

1. **No `anno-ingest` crate** - existing `anno/src/ingest/` is sufficient
2. **`anno-models` is a specification** - defines `Runtime`/`Model` traits for migration
3. **Subsume expanded** - now covers boxes, hyperbolic, sheaf (anno-width geometric needs)
4. **Ingestion stays in anno** - avoids premature extraction; use trait if needed

## Vision

The anno ecosystem extracts structured knowledge from text through a principled pipeline:

```
Text → Extract → Coalesce → Stratify → Knowledge Graph
       (anno)   (coalesce)  (strata)
```

With geometric foundations provided by `subsume` for:
- Box embeddings (containment/entailment)
- Hyperbolic embeddings (type hierarchies)
- Sheaf networks (transitivity enforcement)
- TDA (structural diagnostics)

## Crate Hierarchy

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     GEOMETRIC FOUNDATIONS (subsume)                      │
│                                                                          │
│  subsume-core/          Pure traits, no tensor deps                     │
│  ├── box.rs             Box<S,V> trait for containment                  │
│  ├── hyperbolic.rs      HyperbolicPoint trait for hierarchies           │
│  ├── sheaf.rs           SheafGraph, RestrictionMap for transitivity     │
│  └── tda.rs             PersistenceDiagram for topology (future)        │
│                                                                          │
│  subsume-ndarray/       CPU implementations (ndarray)                   │
│  subsume-candle/        GPU implementations (candle)                    │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                         optional dependency
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        CANONICAL TYPES (anno-core)                       │
│                                                                          │
│  Entity, Track, Identity, Signal          NLP primitives                │
│  GraphDocument, Corpus                    Document containers           │
│  CoreferenceResolver trait                Coref abstraction             │
│  RelationTriple                           Relation extraction           │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              ▼                     ▼                     ▼
┌───────────────────┐  ┌───────────────────┐  ┌───────────────────┐
│   anno-ingest     │  │   anno-models     │  │     anno-rel      │
│                   │  │                   │  │                   │
│ URL resolution    │  │ Runtime trait:    │  │ Relation extract  │
│ HTML cleaning     │  │  OnnxRuntime      │  │ TPLinker          │
│ Text preparation  │  │  CandleRuntime    │  │ W2NER relations   │
│ Format detection  │  │  BurnRuntime      │  │ DocRED eval       │
│                   │  │                   │  │                   │
│ (zero ML deps)    │  │ Model trait:      │  │ Uses anno-models  │
│                   │  │  GLiNER<R>        │  │ for entity spans  │
│                   │  │  NuNER<R>         │  │                   │
│                   │  │  CRF, BiLSTM      │  │                   │
└───────────────────┘  └───────────────────┘  └───────────────────┘
              │                     │                     │
              └─────────────────────┼─────────────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                              anno                                        │
│                                                                          │
│  Facade crate - re-exports and integrates:                              │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────┐        │
│  │ pub use anno_core::*;                                        │        │
│  │ pub use anno_models::{Model, Runtime, GLiNER, NuNER, ...};  │        │
│  │ pub use anno_rel::{RelationExtractor, TPLinker};            │        │
│  │ pub use anno_ingest::{Ingest, PreparedDocument};            │        │
│  └─────────────────────────────────────────────────────────────┘        │
│                                                                          │
│  Plus anno-specific integration:                                         │
│  - joint/         Factor graphs (NER + coref + linking)                 │
│  - geometric/     Thin wrappers over subsume for NLP                    │
│  - linking/       Wikidata entity linking                               │
│  - discourse/     Shell nouns, dialogue acts                            │
│  - salience/      Entity importance ranking                             │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              ▼                     ▼                     ▼
┌───────────────────┐  ┌───────────────────┐  ┌───────────────────┐
│  anno-coalesce    │  │   anno-strata     │  │    anno-eval      │
│                   │  │                   │  │                   │
│ Cross-doc entity  │  │ Graph algorithms  │  │ Evaluation infra  │
│ resolution        │  │                   │  │                   │
│                   │  │ PageRank          │  │ NER metrics       │
│ Union-Find        │  │ Leiden clustering │  │ Coref metrics     │
│ LSH blocking      │  │ HITS              │  │ Relation metrics  │
│ Correlation clust │  │ Label propagation │  │ Benchmarks        │
│ Hierarchical      │  │                   │  │                   │
└───────────────────┘  └───────────────────┘  └───────────────────┘
                                    │
                                    ▼
                         ┌───────────────────┐
                         │    anno-cli       │
                         │                   │
                         │ Thin CLI wrapper  │
                         │ over anno + eval  │
                         └───────────────────┘
```

## Crate Purposes

### anno-core
**Purpose**: Canonical types shared by all crates  
**Dependencies**: std, serde, thiserror (minimal)  
**Size**: ~18k lines

Types:
- `Entity` - Named entity with span, type, confidence
- `Track` - Within-document coreference chain
- `Identity` - Cross-document entity (links to KB)
- `Signal` - Generic grounded annotation
- `GraphDocument` - Knowledge graph container
- `Corpus` - Multi-document container
- `CoreferenceResolver` trait

### anno-models (NEW)
**Purpose**: ML model backends with runtime abstraction  
**Dependencies**: anno-core, ort (optional), candle (optional), burn (optional)  
**Size**: ~40k lines (extracted from anno/backends)

Key abstraction:
```rust
/// Runtime for executing ML models
pub trait Runtime: Send + Sync {
    type Tensor;
    type Error: std::error::Error;
    
    fn load(&self, path: &Path) -> Result<Box<dyn Inference>, Self::Error>;
    fn device(&self) -> Device;
}

/// Model that works with any runtime
pub struct GLiNER<R: Runtime> {
    runtime: R,
    config: GLiNERConfig,
}

impl<R: Runtime> Model for GLiNER<R> {
    fn extract_entities(&self, text: &str, types: Option<&[&str]>) -> Result<Vec<Entity>>;
}
```

Benefits:
- One GLiNER implementation, works with ONNX/Candle/Burn
- Users who only want models don't need eval, CLI, etc.
- Clear feature gates: `onnx`, `candle`, `burn`

### anno-rel (NEW)
**Purpose**: Relation extraction  
**Dependencies**: anno-core, anno-models  
**Size**: ~5k lines (extracted from anno)

Types:
- `RelationExtractor` trait
- `TPLinker` - Joint entity-relation
- `W2NER` - Relation variant
- Relation evaluation metrics

### anno-ingest (NEW)
**Purpose**: Document ingestion and preparation  
**Dependencies**: anno-core, ureq (optional), scraper (optional)  
**Size**: ~3k lines (extracted from anno/ingest)

Features:
- URL resolution (HTTP, file://)
- HTML cleaning
- Text normalization
- Format detection (plaintext, HTML, PDF stub)

Why separate:
- Zero ML dependencies
- Useful independently for data pipelines
- Clear boundary: ingest → models → coalesce → strata

### anno-coalesce (existing)
**Purpose**: Cross-document entity resolution  
**Dependencies**: anno-core  
**Size**: ~9k lines

Already well-designed. Algorithms:
- Union-Find batch resolution
- LSH blocking for scalability
- Correlation clustering
- Hierarchical agglomerative
- Streaming resolution

### anno-strata (existing)
**Purpose**: Graph algorithms for knowledge graphs  
**Dependencies**: anno-core  
**Size**: ~3k lines

Already well-designed. Algorithms:
- PageRank, HITS, eigenvector centrality
- Leiden, Louvain community detection
- Label propagation
- Graph utilities

### subsume (separate repo, expanded)
**Purpose**: Geometric foundations for NLP  
**Dependencies**: minimal (ndarray/candle optional)

Current modules:
- `box_trait.rs` - Box containment
- `gumbel.rs` - Probabilistic boxes
- `distance.rs` - Depth/boundary distance
- `training.rs` - Training infrastructure

New modules:
- `sheaf.rs` - Sheaf neural networks
- `hyperbolic.rs` - Poincaré ball embeddings
- `tda.rs` - Persistence diagrams (future)

## Feature Flags

### anno-models
```toml
[features]
default = []
onnx = ["ort"]
candle = ["candle-core", "candle-nn"]
burn = ["burn", "burn-ndarray"]
metal = ["candle/metal"]
cuda = ["candle/cuda", "burn-tch"]
```

### anno
```toml
[features]
default = ["models"]
models = ["anno-models"]
rel = ["anno-rel"]
ingest = ["anno-ingest"]
eval = ["anno-eval"]
full = ["models", "rel", "ingest", "eval"]

# Runtime selection (passed through to anno-models)
onnx = ["anno-models/onnx"]
candle = ["anno-models/candle"]
burn = ["anno-models/burn"]

# Geometric (optional dependency on subsume)
subsume = ["dep:subsume-core"]
```

## Migration Path

### Phase 1: Create new crates (skeleton)
1. Create anno-models with Runtime trait
2. Create anno-ingest (move from anno/src/ingest)
3. Create anno-rel (extract from anno)

### Phase 2: Expand subsume
1. Add sheaf.rs to subsume-core
2. Add hyperbolic.rs to subsume-core
3. Implement in subsume-ndarray
4. Implement in subsume-candle (GPU)

### Phase 3: Consolidate backends
1. Implement Runtime for ONNX, Candle, Burn
2. Refactor GLiNER to GLiNER<R: Runtime>
3. Delete gliner_onnx.rs, gliner_candle.rs, etc.
4. Same for NuNER, other backends

### Phase 4: Update anno facade
1. Re-export from new crates
2. Keep backward compatibility
3. Deprecate direct backend access

## Dependency Graph (Final)

```
subsume-core (traits)
    │
    ├── subsume-ndarray (CPU)
    └── subsume-candle (GPU)
           │
anno-core ←┘
    │
    ├── anno-ingest (no ML)
    ├── anno-models (ML backends)
    │       │
    │       └── anno-rel (relations)
    │
    ├── anno-coalesce (clustering)
    └── anno-strata (graphs)
           │
    ┌──────┴──────┐
    │             │
anno (facade) ←───┘
    │
anno-eval
    │
anno-cli
```

## Open Questions

1. **Should anno-models include coref backends?**
   - Currently: coref backends in anno/backends
   - Option A: Move to anno-models (it's a "model")
   - Option B: Keep in anno (it's NLP-specific integration)
   - Leaning: Option B, coref is higher-level than NER

2. **Should joint (factor graphs) stay in anno?**
   - Yes - it integrates NER + coref + linking
   - Too intertwined to extract cleanly

3. **subsume naming for expanded scope?**
   - Keep "subsume" - the pun works ("subsumes geometric needs")
   - Sheaf/hyperbolic still relate to containment/hierarchy

4. **box-coref integration?**
   - Keep separate (Python training)
   - Its Rust inference already uses subsume
   - Document ONNX export format

## References

- Durrett & Klein (2014): Joint Entity Analysis
- Vilnis et al. (2018): Box Embeddings
- Traag et al. (2019): Leiden Algorithm
- Bodnar et al. (2022): Sheaf Neural Networks
