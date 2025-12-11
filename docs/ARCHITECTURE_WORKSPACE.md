# Anno Workspace Architecture

**Status**: Implemented  
**Date**: 2024-12-10 (updated 2024-12-10)

## Summary

| Crate | Status | Purpose |
|-------|--------|---------|
| `anno-core` | Active | Canonical types: Entity, Track, Identity, Signal |
| `anno` | Active | NER backends, coref, eval, discourse, salience |
| `anno-coalesce` | Active | Cross-document entity resolution |
| `anno-strata` | Active | Graph algorithms (PageRank, Leiden, Louvain) |
| `anno-eval` | Active | Re-export wrapper for anno::eval |
| `anno-cli` | Active | Command-line interface |
| `anno-derive` | Active | Proc macros |

### Archived Crates (see archive/skeleton-crates-2024-12/)

| Crate | Why Archived |
|-------|--------------|
| `anno-models` | Runtime trait not integrated; backends remain in anno |
| `anno-rel` | Types duplicated in anno::backends::inference |

### External Dependencies

| Crate | Location | Status |
|-------|----------|--------|
| `subsume-core` | ../subsume | Separate repo, optional dep (currently disabled) |

## Decisions

1. **No separate `anno-ingest` crate** - existing `anno/src/ingest/` is sufficient
2. **No separate `anno-models` crate** - Runtime abstraction deferred; backends stay in anno
3. **No separate `anno-rel` crate** - RelationTriple already exists in anno
4. **Subsume stays external** - Pure geometric algebra, can be enabled when stable
5. **Training stays minimal** - box_embeddings_training.rs is exception; complex training in Python

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
│                        CANONICAL TYPES (anno-core)                       │
│                                                                          │
│  Entity, Track, Identity, Signal          NLP primitives                │
│  GraphDocument, Corpus                    Document containers           │
│  CoreferenceResolver trait                Coref abstraction             │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              ▼                     ▼                     ▼
┌───────────────────┐  ┌───────────────────┐  ┌───────────────────┐
│  anno-coalesce    │  │   anno-strata     │  │      anno         │
│                   │  │                   │  │                   │
│ Cross-doc entity  │  │ Graph algorithms  │  │ NER backends:     │
│ resolution        │  │                   │  │  GLiNER (ONNX)    │
│                   │  │ PageRank          │  │  GLiNER (Candle)  │
│ Union-Find        │  │ Leiden/Louvain    │  │  NuNER, CRF, etc  │
│ LSH blocking      │  │ Label propagation │  │                   │
│ Hierarchical      │  │ Centrality        │  │ Coref backends:   │
│ Correlation clust │  │                   │  │  MentionRanking   │
│ Streaming         │  │                   │  │  GraphCoref       │
│                   │  │                   │  │  Joint (NER+coref)│
│                   │  │                   │  │                   │
│                   │  │                   │  │ Other modules:    │
│                   │  │                   │  │  discourse/       │
│                   │  │                   │  │  salience.rs      │
│                   │  │                   │  │  linking/         │
│                   │  │                   │  │  temporal.rs      │
│                   │  │                   │  │  eval/ (gated)    │
└───────────────────┘  └───────────────────┘  └───────────────────┘
              │                     │                     │
              └─────────────────────┼─────────────────────┘
                                    ▼
                         ┌───────────────────┐
                         │    anno-eval      │  (re-export wrapper)
                         └─────────┬─────────┘
                                   │
                         ┌─────────┴─────────┐
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

### anno
**Purpose**: Main library with NER backends, coreference, and evaluation  
**Dependencies**: anno-core, anno-coalesce, anno-strata (optional)  
**Size**: ~120k lines

Modules:
- `backends/` - NER backends (GLiNER, NuNER, CRF, W2NER, etc.)
- `backends/mention_ranking.rs` - Mention-ranking coreference  
- `backends/graph_coref.rs` - Graph-based coreference
- `backends/box_embeddings.rs` - Box embedding inference
- `backends/box_embeddings_training.rs` - Box embedding training (pure Rust)
- `joint/` - Joint NER+Coref+Linking (structured CRF)
- `discourse/` - Abstract anaphora, shell nouns
- `linking/` - Entity linking to knowledge bases
- `salience.rs` - Entity importance ranking
- `temporal.rs` - Temporal parsing, diachronic NER
- `eval/` - Evaluation infrastructure (feature-gated)
- `cli/` - CLI commands (feature-gated)
- `ingest/` - URL resolution, HTML cleaning, text prep

### anno-coalesce
**Purpose**: Cross-document entity resolution  
**Dependencies**: anno-core  
**Size**: ~9k lines

Already well-designed. Algorithms:
- Union-Find batch resolution
- LSH blocking for scalability
- Correlation clustering
- Hierarchical agglomerative
- Streaming resolution

### anno-strata
**Purpose**: Graph algorithms for knowledge graphs  
**Dependencies**: anno-core, petgraph  
**Size**: ~3k lines

Algorithms:
- PageRank, eigenvector centrality
- Leiden, Louvain community detection
- Label propagation
- Graph utilities

### subsume (external, ../subsume)
**Purpose**: Pure geometric algebra for box/hyperbolic/sheaf embeddings  
**Status**: Separate repo, optional dependency (currently disabled in anno)

When integrated, provides:
- `Box` trait - Containment/entailment
- `GumbelBox` - Probabilistic boxes with dense gradients
- `HyperbolicPoint` - Poincaré ball for hierarchies
- `SheafGraph` - Transitivity enforcement

## Feature Flags

### anno
```toml
[features]
default = ["cli"]
cli = ["clap", "clap_complete", "is-terminal", "indicatif", "toml"]
eval = ["dirs", "glob"]
eval-advanced = ["eval", "rand", "ureq", "sha2", "anno-strata"]
eval-full = ["eval", "eval-bias", "eval-advanced"]
discourse = ["eval"]
onnx = ["ort", "tokenizers", "hf-hub", "ndarray", "lru"]
candle = ["candle-core", "candle-nn", "candle-transformers", "tokenizers", "hf-hub", "safetensors"]
metal = ["candle", "candle-core/metal"]
cuda = ["candle", "candle-core/cuda"]
burn = ["dep:burn", "burn-ndarray", "burn-autodiff", "tokenizers", "hf-hub", "safetensors"]
# subsume = ["dep:subsume-core"]  # Currently disabled
```

## Current Dependency Graph

```
anno-core (18k - types)
    │
    ├── anno-coalesce (9k - cross-doc resolution)
    ├── anno-strata (3k - graph algorithms)
    │
    └── anno (120k - backends, coref, eval)
            │
            ├── anno-eval (re-export wrapper)
            └── anno-cli (CLI wrapper)
```

## Resolved Questions

1. **Should anno-models include coref backends?**
   - **Resolved**: No separate anno-models crate. Backends stay in anno.
   - Reason: Runtime abstraction wasn't integrated; backends have model-specific logic.

2. **Should joint (factor graphs) stay in anno?**
   - **Yes** - it integrates NER + coref + linking; too intertwined to extract.

3. **subsume integration?**
   - **Deferred**: Dependency disabled until subsume is stable.
   - anno/geometric/ has stubs; subsume trait impl ready in box_embeddings.rs.

4. **Training in Rust?**
   - box_embeddings_training.rs provides pure Rust training (exception).
   - Complex model training stays in Python (PyTorch → export to ONNX/Safetensors).

## References

- Durrett & Klein (2014): Joint Entity Analysis
- Vilnis et al. (2018): Box Embeddings
- Traag et al. (2019): Leiden Algorithm
- Bodnar et al. (2022): Sheaf Neural Networks
