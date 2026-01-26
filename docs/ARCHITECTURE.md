# Architecture

## Structure

```
anno/
├── anno-core/      # Foundation: Entity, GroundedDocument, GraphDocument
├── anno/           # NER backends, coref, eval, CLI (src/bin/anno.rs)
├── anno-coalesce/  # Cross-document entity coalescing
```

## Dependencies

```
anno-core (no workspace deps)
    ↑
    ├── anno-coalesce
    └── anno  (depends on anno-coalesce)
```

Each crate is independent. Use what you need:

- `anno`: NER only
- `anno-coalesce`: Entity resolution without NER

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

This capability has been de-scoped from the main workspace to reduce maintenance surface.

## CLI

```bash
# Extract
anno extract "Marie Curie was born in Paris."

# Coalesce (cross-doc entity resolution)
anno cross-doc ./docs --threshold 0.6
# or: anno coalesce ./docs --threshold 0.6

# Stratify (hierarchical clustering)
#
# This capability has been de-scoped from the main workspace to reduce maintenance surface.
```

## Pipeline

**Extract. Coalesce.**

1. **Extract**: Detect entities in text (NER)
   - Input: raw text
   - Output: entity mentions (Signal → Track within document)

2. **Coalesce**: Cross-document entity resolution
   - Input: entities from multiple documents (Tracks)
   - Output: canonical entities (Identity) linking mentions across documents
   - Purpose: Identity resolution - "Marie Curie" in doc1 and doc2 → same Identity
   - Algorithm: Similarity-based clustering (embeddings or string similarity)
   - Example: `anno cross-doc ./docs --threshold 0.7`

**Key Difference**: 
- **Coalesce** = identity resolution (same entity, different documents)
