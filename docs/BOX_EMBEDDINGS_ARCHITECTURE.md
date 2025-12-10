# Box Embeddings Architecture

This document describes the architecture for box embeddings in `anno` and related projects.

## Overview

**Anno is self-contained.** It has its own box embedding implementation in `backends/box_embeddings.rs` that doesn't depend on external projects.

**box-coref is a separate research project** that uses `subsume` for advanced box operations and `anno` for evaluation metrics. It depends on anno, not the other way around.

## Dependency Hierarchy

```
┌─────────────────────────────────────────────────────────────────────┐
│                    PROJECT RELATIONSHIPS                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                     anno (toolkit)                            │  │
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │  │
│  │  backends/box_embeddings.rs:                                  │  │
│  │  • BoxEmbedding         (axis-aligned hyperrectangles)        │  │
│  │  • GumbelBox            (probabilistic boundaries)            │  │
│  │  • TemporalBox          (time-varying entities)               │  │
│  │  • UncertainBox         (confidence + conflict detection)     │  │
│  │  • Calibration metrics  (ECE, Brier, reliability diagrams)    │  │
│  │                                                                │  │
│  │  eval/coref_metrics.rs:                                       │  │
│  │  • MUC, B³, CEAF, LEA, BLANC metrics                          │  │
│  │  • CorefEvaluation      (aggregate metrics)                   │  │
│  │                                                                │  │
│  │  eval/coref_resolver.rs:                                      │  │
│  │  • BoxCorefResolver     (uses BoxEmbedding)                   │  │
│  │  • SimpleCorefResolver  (rule-based)                          │  │
│  │                                                                │  │
│  │  STANDALONE - no external box embedding dependencies          │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              ↑                                      │
│                         (depends on)                                │
│                              │                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │              box-coref (research project)                     │  │
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │  │
│  │  python/:                                                     │  │
│  │  • PyTorch training pipeline                                  │  │
│  │  • Multi-dataset experiments                                  │  │
│  │                                                                │  │
│  │  inference/rust/:                                             │  │
│  │  • BoxCorefEmbedding    (multi-resolution L0→L1→L2)          │  │
│  │  • BoxCorefIndex        (search with logical filtering)       │  │
│  │  • BoxEmbeddingTrainer  (Rust training alternative)           │  │
│  │                                                                │  │
│  │  DEPENDS ON:                                                  │  │
│  │  • anno (types, eval metrics)                                 │  │
│  │  • subsume (advanced box math)                                │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                      │
│                         (depends on)                                │
│                              ↓                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                  subsume (box math library)                   │  │
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │  │
│  │  subsume-core:                                                │  │
│  │  • Box trait            • GumbelBox trait                     │  │
│  │  • containment_prob     • Training utilities                  │  │
│  │  • BoxE scoring         • Calibration metrics                 │  │
│  │                                                                │  │
│  │  subsume-ndarray:                                             │  │
│  │  • NdarrayBox           • NdarrayGumbelBox                    │  │
│  │  • Optimizers (Adam, AdamW, SGD)                              │  │
│  │                                                                │  │
│  │  STANDALONE - pure geometry, no NLP concepts                  │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### 1. Anno Is Self-Contained

Anno's `box_embeddings.rs` provides everything needed for basic box-based coreference:
- `BoxEmbedding`: Volume, intersection, containment probability, coreference scoring
- `GumbelBox`: Probabilistic boundaries with temperature-controlled softness
- `TemporalBox`: Time-varying entities with velocity
- `UncertainBox`: Confidence modeling and conflict detection
- Calibration: ECE, Brier score, reliability diagrams

**Rationale:** Most users don't need advanced training. The simple implementation covers inference use cases without adding external dependencies.

### 2. box-coref Extends Anno (Not Vice Versa)

box-coref is a research project that:
- Trains sophisticated box embeddings (multi-resolution, self-adversarial sampling)
- Uses `subsume` for advanced geometric operations
- Uses `anno` for evaluation (MUC, B³, CEAF, LEA, BLANC)

**Rationale:** Research code belongs in research repos. Anno stays stable and lightweight.

### 3. Clear Dependency Flow

```
subsume (standalone) ← box-coref → anno (standalone)
```

- `anno` doesn't know about `box-coref` or `subsume`
- `box-coref` depends on both
- No circular dependencies

## Usage Patterns

### Basic Box Coreference (anno only)

```rust
use anno::backends::{BoxEmbedding, BoxCorefConfig};
use anno::eval::coref_resolver::BoxCorefResolver;

// Create boxes from embeddings
let box_a = BoxEmbedding::from_vector(&embedding_a, 0.1);
let box_b = BoxEmbedding::from_vector(&embedding_b, 0.1);

// Score coreference
let score = box_a.coreference_score(&box_b);

// Or use the resolver
let config = BoxCorefConfig::default();
let resolver = BoxCorefResolver::new(config);
let clusters = resolver.resolve(&entities);
```

### Calibration Metrics (anno only)

```rust
use anno::backends::{expected_calibration_error, CalibrationReport};

let predictions = vec![
    (0.9, true),   // 90% confident, correct
    (0.8, false),  // 80% confident, wrong
    (0.3, false),  // 30% confident, wrong (well-calibrated)
];

let ece = expected_calibration_error(&predictions, 10);
let report = CalibrationReport::from_predictions(&predictions, 10);
println!("ECE: {:.3}, Well-calibrated: {}", report.ece, report.is_well_calibrated(0.05));
```

### Advanced Training (box-coref)

```rust
// In box-coref/inference/rust
use box_coref_inference::{BoxEmbeddingTrainer, TrainingConfig};
use anno::eval::{CorefEvaluation, CorefChain};

// Train
let mut trainer = BoxEmbeddingTrainer::new(config, dim, None);
trainer.train(&examples);

// Evaluate using anno's metrics
let predicted: Vec<CorefChain> = trainer.predict(&mentions);
let eval = CorefEvaluation::compute(&predicted, &gold);
println!("{}", eval.summary_line());
```

## When to Use What

| Use Case | Solution |
|----------|----------|
| Basic coreference scoring | `anno::backends::BoxEmbedding` |
| Uncertainty modeling | `anno::backends::UncertainBox` |
| Temporal entities | `anno::backends::TemporalBox` |
| Calibration metrics | `anno::backends::CalibrationReport` |
| Coref evaluation | `anno::eval::CorefEvaluation` |
| Training box embeddings | `box-coref` (separate repo) |
| Advanced box operations | `subsume` (for research use) |

## Benefits of This Architecture

1. **Self-contained toolkit**: Anno works without external dependencies
2. **Research separation**: Experimental code stays in research repos
3. **Stable interfaces**: Anno's types don't change based on research progress
4. **Clear evaluation flow**: box-coref → anno for metrics
5. **No circular dependencies**: Dependency flow is strictly one-way

## Pretrained Models

**No publicly available pretrained box embedding models exist for NER/coref.**

The closest alternatives are:
- **query2box** (Stanford, 209 stars): KG reasoning on FB15k, NELL - not for text
- **KGReasoning** (Stanford, 305 stars): BetaE + Query2Box for KG reasoning
- **box-coref exports**: `box_projection_layers.safetensors` trained on ImageNet hierarchy

### Current Exports

Available at `box-coref/exports/`:
```
box_projection_layers.safetensors  (1.5 MB)
├── text_l1_mu.weight     [128, 768]  - Text -> Box center
├── text_l1_mu.bias       [128]
├── text_l1_sigma.weight  [128, 768]  - Text -> Box delta
├── text_l1_sigma.bias    [128]
├── vision_l1_mu.weight   [128, 768]  - Vision -> Box center
├── vision_l1_mu.bias     [128]
├── vision_l1_sigma.weight[128, 768]  - Vision -> Box delta
└── vision_l1_sigma.bias  [128]
```

Trained on: Tiny ImageNet hierarchy + WordNet (containment relationships)

### Using Pretrained Weights

```rust
// Conceptual (would need safetensors loader)
let weights = load_safetensors("box_projection_layers.safetensors")?;

// Project a 768d embedding to box
fn project_to_box(embed: &[f32], weights: &Weights) -> BoxEmbedding {
    let center = linear(&embed, &weights["text_l1_mu.weight"], &weights["text_l1_mu.bias"]);
    let delta = linear(&embed, &weights["text_l1_sigma.weight"], &weights["text_l1_sigma.bias"]);
    let delta = clamp(&delta, 0.05, 10.0);  // Valid box bounds
    BoxEmbedding::from_center_delta(&center, &delta)
}
```

## Training on Coref Data

See `docs/BOX_COREF_INTEGRATION.md` for the full pipeline:
1. Export coref clusters from anno datasets
2. Convert to containment pairs (mention ⊆ cluster)
3. Train in box-coref
4. Export and use in anno

## References

- Vilnis et al. (2018): "Probabilistic Embedding of Knowledge Graphs with Box Lattice Measures"
- Ren et al. (2020): "Query2box: Reasoning over KGs using Box Embeddings" (ICLR)
- Lee et al. (2022): "Box Embeddings for Event-Event Relation Extraction" (BERE)
- Messner et al. (2022): "Temporal Knowledge Graph Completion with Box Embeddings" (BoxTE)
- Chen et al. (2021): "Uncertainty-Aware Knowledge Graph Embeddings" (UKGE)
