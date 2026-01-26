# Box-Coref Integration

How anno and box-coref work together for NER and coreference resolution.

**Key Principle**: Anno owns data and task evaluation. Box-coref owns training and geometric evaluation.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           DATA FLOW                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  anno (Rust toolkit)                                                        │
│  ├── datasets/               ← Dataset registry (50+ NER/coref datasets)   │
│  ├── eval/coref_metrics.rs   ← MUC, B³, CEAF, LEA, BLANC, CoNLL F1        │
│  ├── eval/coref_resolver.rs  ← BoxCorefResolver (inference)                │
│  └── backends/box_embeddings.rs ← BoxEmbedding, calibration metrics        │
│                                                                             │
│           │ Export training data (JSONL)                                    │
│           ▼                                                                 │
│                                                                             │
│  box-coref (Python training)                                                │
│  ├── data/                   ← Training data loaders                        │
│  ├── training/               ← PyTorch/Lightning training                   │
│  ├── exports/                ← box_projection_layers.safetensors            │
│  └── evaluation/             ← Box-specific metrics (containment, volume)   │
│                                                                             │
│           │ Export trained weights                                          │
│           ▼                                                                 │
│                                                                             │
│  anno inference                                                             │
│  └── Load safetensors → Project embeddings → Box coreference               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Who Owns What

### Anno Owns:
- **Dataset definitions** - All metadata in `dataset_registry.rs`
- **Dataset loaders** - Download and parse CoNLL, JSONL, BIO formats
- **Evaluation metrics** - MUC, B³, CEAF, LEA, BLANC, CoNLL F1
- **Calibration metrics** - ECE, Brier score, reliability diagrams
- **Inference-time box operations** - BoxEmbedding, coreference scores

### Box-Coref Owns:
- **Training infrastructure** - PyTorch Lightning, loss functions
- **Box geometry losses** - Gumbel containment, volume regularization
- **Multi-resolution layers** - L0/L1/L2 projection
- **Backbone fine-tuning** - SigLIP, BERT adapter

## Evaluation Comparison

| Aspect | Anno | Box-Coref |
|--------|------|-----------|
| **Metrics** | MUC, B³, CEAF, LEA, BLANC | Containment rate, volume ratio |
| **Focus** | End-to-end coref quality | Box geometry correctness |
| **Data** | NER/coref benchmarks | Hierarchy containment pairs |
| **Output** | F1 scores, CoNLL score | Loss curves, containment % |

**Key insight**: Box-coref evaluation checks *geometric* properties (is child inside parent?). 
Anno evaluation checks *task* performance (do predicted chains match gold chains?).

## Pretrained Models Status

**No publicly available pretrained box embedding models for NER/coref exist.**

Closest alternatives:
- **query2box** (Stanford): KG reasoning on FB15k, NELL - not text
- **BetaE** (Stanford): KG reasoning - not text
- **box-coref exports**: `box_projection_layers.safetensors` trained on ImageNet hierarchy

## Training Pipeline (anno → box-coref)

### Step 1: Export Hierarchy from Anno Datasets

Anno has entity type hierarchies (PER, ORG, LOC) but no explicit containment pairs.
To train box embeddings, we need (child, parent) pairs:

```rust
// Example: Create containment pairs from NER types
// PER is-a ENTITY, ORG is-a ENTITY, etc.
let pairs = vec![
    ("Barack Obama", "PER"),
    ("PER", "ENTITY"),
    ("Google", "ORG"),
    ("ORG", "ENTITY"),
];
```

### Step 2: Export Coref Chains as Containment

For coreference, each chain implies mutual containment:
- "Barack Obama" ⊆ cluster_1
- "Obama" ⊆ cluster_1
- "he" ⊆ cluster_1

```python
# Export format for box-coref training
{"mention": "Barack Obama", "cluster_id": 1, "type": "PER"}
{"mention": "Obama", "cluster_id": 1, "type": "PER"}
{"mention": "he", "cluster_id": 1, "type": "PER"}
```

### Step 3: Train Box Embeddings

Using the experiment infrastructure:

```bash
cd box-coref

# Full workflow (recommended)
just workflow gap

# Or step by step:
just workflow-export gap      # Export from anno
just validate data/gap_coref_training.jsonl
just experiment experiments/configs/gap.toml --dry-run  # Validate config
just experiment-train experiments/configs/gap.toml      # Train
just visualize                # Generate reports
```

### Step 4: Export and Use in Anno

```bash
python scripts/export_box_layers.py \
    --checkpoint experiments/runs/gap_*/checkpoints/best.ckpt \
    --output_dir exports/

# Copy to anno
cp exports/box_projection_layers.safetensors ../anno/models/
```

## Current Exports

Available at `box-coref/exports/`:
- `box_projection_layers.safetensors` (1.5 MB)
  - 768d → 128d projection layers
  - Trained on: Tiny ImageNet hierarchy + WordNet
  - Keys: `vision_l1_mu`, `vision_l1_sigma`, `text_l1_mu`, `text_l1_sigma`

## Testing the Integration

```rust
use anno::backends::{BoxEmbedding, BoxCorefConfig};
use anno::eval::coref_resolver::BoxCorefResolver;

// 1. Load box projection weights (would need safetensors loader)
let weights = load_safetensors("models/box_projection_layers.safetensors")?;

// 2. Get entity embeddings from BERT/GLiNER
let entity_embeds = bert.encode(&["Marie Curie", "she"])?;

// 3. Project to boxes
let boxes: Vec<BoxEmbedding> = entity_embeds.iter()
    .map(|e| project_to_box(e, &weights))
    .collect();

// 4. Use anno's resolver
let resolver = BoxCorefResolver::new(BoxCorefConfig::default());
let resolved = resolver.resolve_with_boxes(&entities, &boxes);

// 5. Evaluate with anno's metrics
let (p, r, f1) = muc_score(&predicted_chains, &gold_chains);
```

## Ultimate Training & Evaluation Setup

See `ULTIMATE_TRAINING_EVAL_DESIGN.md` in the `box-coref` repo for the complete design.

### Key Scripts

| Script | Purpose |
|--------|---------|
| `box-coref/scripts/anno_export_training.py` | Export anno datasets to training format |
| `box-coref/training/config_text_only.py` | Config for text-only (NER/coref) training |
| `box-coref/scripts/evaluate_unified.py` | Combined geometric + task evaluation |

### Training Flow

```
┌──────────────────────────────────────────────────────────────────────┐
│ 1. Export Training Data (anno)                                       │
├──────────────────────────────────────────────────────────────────────┤
│ # From box-coref:                                                    │
│ just workflow-export gap                                             │
│                                                                      │
│ # Or from anno directly:                                             │
│ cargo run --example export_coref_for_box_training --features eval-   │
│   advanced -- --dataset gap --output ../box-coref/data/              │
└──────────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────────┐
│ 2. Train Box Embeddings (box-coref)                                  │
├──────────────────────────────────────────────────────────────────────┤
│ # Full workflow with experiments infrastructure:                      │
│ just workflow gap                                                    │
│                                                                      │
│ # Or just training:                                                  │
│ just experiment-train experiments/configs/gap.toml                   │
└──────────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────────┐
│ 3. Visualize and Evaluate                                            │
├──────────────────────────────────────────────────────────────────────┤
│ just visualize                       # Generate plots and HTML       │
│ just eval-anno <checkpoint> gap      # Evaluate with anno metrics    │
└──────────────────────────────────────────────────────────────────────┘
```

### Evaluation Metrics

**Geometric (box-coref)**:
- Containment rate: % of pairs where child ⊂ parent (target: >80%)
- Volume ratio: parent/child volume (target: >1.5)
- Transitivity: If A⊂B and B⊂C then A⊂C (target: >90%)

**Task (anno)**:
- CoNLL F1: Average of MUC, B³, CEAFe (target: >80%)
- BLANC F1: Best discriminative metric (target: >75%)
- Stratified F1: By chain length (critical for diagnosis)

**Calibration (anno)**:
- ECE: Expected calibration error (target: <0.05)
- Brier score: Proper scoring rule (target: <0.15)

### Success Criteria

| Metric | Baseline | Target |
|--------|----------|--------|
| PreCo CoNLL F1 | 75% | >80% |
| GAP F1 | 70% | >78% |
| Containment | 50% | >80% |
| ECE | 0.10 | <0.05 |

## Research References

1. **DUCK** (EMNLP 2023): Polar box embeddings for entity linking (+7.9 F1)
2. **BoxE** (NeurIPS 2020): Box embeddings for KG completion
3. **Onoe et al.** (ACL 2021): Box embeddings for fine-grained entity types
4. **Query2Box** (ICLR 2020): Box embeddings for KG reasoning

