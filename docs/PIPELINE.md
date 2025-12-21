# End-to-End Pipeline Guide

This doc shows how the major pieces connect, from raw text to cross-document
clusters. It complements `ARCHITECTURE.md`.

## Fast Start (CLI)

Prefer `--text` / `--file` over positional text for now; see `docs/guides/BUGS.md` for known CLI input pitfalls.

```bash
# 1) Extract entities (quick, human-readable)
anno extract --text "Marie Curie was born in Paris."

# 2) Extract + within-doc coref (tracks)
anno pipeline --coref --text "Marie Curie was born in Paris. She moved to Paris."

# 3) Cross-doc coalescing across a directory (requires `eval-advanced`)
anno cross-doc ./docs --threshold 0.6 --format tree
```

## Programmatic (within-doc → cross-doc)

```rust
use anno::eval::cdcr::{CDCRResolver, Document};
use anno::{Entity, EntityType, MentionRankingCoref};
use anno_core::CoreferenceResolver;

// Within-doc coref (feature-based)
let coref = MentionRankingCoref::new();
let text = "Barack Obama met Macron. Obama spoke in Paris.";
let entities = vec![
    Entity::new("Barack Obama", EntityType::Person, 0, 12, 0.9),
    Entity::new("Obama", EntityType::Person, 23, 28, 0.9),
    Entity::new("Macron", EntityType::Person, 33, 39, 0.9),
    Entity::new("Paris", EntityType::Location, 52, 57, 0.9),
];
let resolved = coref.resolve(&entities);

// Cross-doc clustering (string similarity + blocking)
let docs = vec![
    Document::new("doc1", text).with_entities(resolved),
    // other documents...
];
let cdcr = CDCRResolver::new();
let clusters = cdcr.resolve(&docs);
```

## Advanced / research

If you want deeper design/research notes:

- `docs/notes/design/joint/JOINT_MODEL_DESIGN.md`
- `docs/notes/research/systems/XCORE_INTEGRATION.md`


