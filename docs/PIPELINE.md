# End-to-End Pipeline Guide

This doc shows how the major pieces connect, from raw text to cross-document
clusters. It complements `ARCHITECTURE.md`.

## Fast Start (CLI)

```bash
# 1) Extract entities (quick, human-readable)
anno extract "Marie Curie was born in Paris."

# 2) Extract + within-doc coref (tracks)
anno pipeline --coref "Marie Curie was born in Paris. She moved to Paris."

# 3) Cross-doc coalescing across a directory (requires `eval-advanced`)
anno cross-doc ./docs --threshold 0.6 --format tree
```

## Programmatic (within-doc → cross-doc)

```rust
use anno::eval::coref_resolver::{CoreferenceResolver, MentionRankingCoref};
use anno::eval::cdcr::{CDCRResolver, CDCRConfig};
use anno::{Entity, EntityType};

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

// Cross-doc coref (string/LSH, optionally learned cluster encoder)
let docs = vec![
    anno::eval::cdcr::Document::new("doc1", text).with_entities(resolved),
    // other documents...
];
let cdcr = CDCRResolver::new();
let clusters = cdcr.resolve(&docs);
```

## Joint Model (Durrett & Klein 2014)

Use when you need NER + coref + entity linking together.

```rust
use anno::joint::{JointConfig, JointModel};

let model = JointModel::new(JointConfig::default())?;
let result = model.analyze(text, &entities)?;
println!("chains: {}", result.chains.len());
```

To plug in custom scorers (e.g., BoxCorefProvider or your own NER/EL):

```rust
use anno::joint::{JointModelBuilder};
use anno::joint::providers::BoxCorefProvider;

let builder = JointModelBuilder::new(JointConfig::default())
    .with_coref_provider(Box::new(BoxCorefProvider::default()));
let model = builder.build()?;
```

## Cross-Context (xCoRe-style)

For long documents or cross-document clustering with learned cluster embeddings:

```rust
use anno::joint::cross_context::{CrossContextJointConfig, CrossContextJointModel, Context};
use anno::eval::cluster_encoder::{HeuristicClusterEncoder, CosineMergeScorer};

let encoder = HeuristicClusterEncoder::new(64);
let scorer = CosineMergeScorer::new(0.5);
let cc = CrossContextJointModel::new(encoder, scorer, CrossContextJointConfig::default())?;
let contexts = vec![Context::new(0, text).with_entities(entities)];
let result = cc.analyze(&contexts)?;
println!("merged clusters: {}", result.merged_clusters.len());
```

## Serving (future)

There is no HTTP/gRPC service today. Suggested minimal design:
- `POST /predict` with text → returns entities + coref chains.
- Dynamic batching for transformer backends.
- Model/version routing via headers.


