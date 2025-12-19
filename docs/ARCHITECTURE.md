# Architecture

## Structure

```
anno/
├── anno-core/      # Foundation: Entity, GroundedDocument, GraphDocument
├── anno/           # NER backends, coref, eval, CLI (src/bin/anno.rs)
├── anno-coalesce/  # Cross-document entity coalescing
└── anno-strata/    # Hierarchical clustering (Leiden, Louvain)
```

## Dependencies

```
anno-core (no workspace deps)
    ↑
    ├── anno-coalesce
    ├── anno-strata
    └── anno  (depends on anno-coalesce; optionally anno-strata)
```

Each crate is independent. Use what you need:

- `anno`: NER only
- `anno-coalesce`: Entity resolution without NER
- `anno-strata`: Clustering without NER

Or use together via the `anno` CLI binary (see `anno/src/bin/anno.rs`).

## Library

### NER

```rust
use anno::{Model, GLiNEROnnx};

let ner = GLiNEROnnx::new("onnx-community/gliner_small-v2.1")?;
let entities = ner.extract_entities(text, None)?;
```

### Cross-document Coalescing

```rust
use anno_coalesce::Resolver;

let resolver = Resolver::new();
let identities = resolver.resolve_inter_doc_coref(&mut corpus, Some(0.7), Some(true))?;
```

### Hierarchical Clustering

```rust
use anno_strata::HierarchicalLeiden;

let hierarchy = HierarchicalLeiden::cluster(&graph)?;
```

## CLI

```bash
# Extract
anno extract "Marie Curie was born in Paris."

# Coalesce (cross-doc entity resolution)
anno cross-doc ./docs --threshold 0.6
# or: anno coalesce ./docs --threshold 0.6

# Stratify (hierarchical clustering)
anno strata --input graph.json --method leiden --levels 3
```

## Pipeline

**Extract. Coalesce. Stratify.**

1. **Extract**: Detect entities in text (NER)
   - Input: raw text
   - Output: entity mentions (Signal → Track within document)

2. **Coalesce**: Cross-document entity resolution
   - Input: entities from multiple documents (Tracks)
   - Output: canonical entities (Identity) linking mentions across documents
   - Purpose: Identity resolution - "Marie Curie" in doc1 and doc2 → same Identity
   - Algorithm: Similarity-based clustering (embeddings or string similarity)
   - Example: `anno cross-doc ./docs --threshold 0.7`

3. **Stratify**: Hierarchical community detection
   - Input: graph of entities and relations (GraphDocument)
   - Output: hierarchical layers of communities at multiple resolutions
   - Purpose: Reveal abstraction levels (specific → themes → domains)
   - Algorithm: Leiden algorithm at multiple resolutions (modularity optimization)
   - Example: `anno strata --input graph.json --method leiden --levels 3`

**Key Difference**: 
- **Coalesce** = identity resolution (same entity, different documents)
- **Strata** = hierarchical organization (communities, themes, abstraction layers)
