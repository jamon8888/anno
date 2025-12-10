# FRECO Integration Design

Framing-divergent Event Coreference for anno.

Based on: Zhao et al. (EMNLP 2025) "Seeing the Same Story Differently: Framing-Divergent Event Coreference for Computational Framing Analysis"

## Overview

FRECO identifies pairs of event mentions that:
1. Refer to the same real-world occurrence
2. Differ in framing (lexical choice, causal attribution, valence, perspective)

Unlike traditional event coreference which treats variation as noise, FRECO treats **framing divergence as the signal**.

## Key Concepts

### Framing Divergence Types

| Type | Example | Detection Signal |
|------|---------|------------------|
| Lexical | "hunted down" vs "pursued" | Synonym with connotation shift |
| Valence | "dispersed" vs "refused to leave" | Opposite emotional tone |
| Granularity | "lost his job" vs "mass layoffs" | Instance vs aggregate |
| Abstraction | "challenged authority" vs "demanded accountability" | Concrete vs abstract |
| Causal | "self-defense" vs "unprovoked attack" | Different attribution |
| Agency | "was killed" vs "died" | Passive vs intransitive |

### Event Coreference Relations

FRECO uses relaxed coreference following Hovy et al. (2013) and TAC KBP "event hoppers":

- **Full**: Identical event, same arguments
- **Subevent**: One event is part of another
- **Concept-Instance**: Abstract vs concrete realization
- **Membership**: One among many

All qualify for FRECO if framing diverges.

## Architecture

```
                    ┌─────────────────────┐
                    │   Document Corpus   │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │  Event Extraction   │
                    │  (SRL + triggers)   │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │  Candidate Pairing  │
                    │  (CDEC cross-enc)   │
                    └──────────┬──────────┘
                               │
              ┌────────────────┴────────────────┐
              │                                 │
    ┌─────────▼─────────┐             ┌─────────▼─────────┐
    │  FRECO Classifier │             │  Bootstrapped     │
    │  (SFT + DPO)      │◄────────────│  Mining Pipeline  │
    └─────────┬─────────┘             └───────────────────┘
              │
    ┌─────────▼─────────┐
    │  Attitude Labels  │
    │  (supp/skep/neut) │
    └───────────────────┘
```

## Integration with anno

### New Types (anno-core)

```rust
// anno_core::types::framing

pub enum FramingAttitude {
    Supportive,
    Skeptical,
    Neutral,
}

pub enum FramingDivergenceType {
    Lexical,
    Valence,
    Granularity,
    Abstraction,
    Causal,
    Participant,
    Agency,
    // ...
}

pub struct EventMention {
    pub trigger: String,
    pub sentence: String,
    pub arguments: Vec<EventArgument>,  // SRL roles
    pub attitude: Option<FramingAttitude>,
}

pub struct FrecoPair {
    pub event_a: EventMention,
    pub event_b: EventMention,
    pub label: Option<bool>,
    pub divergence_type: Option<FramingDivergenceType>,
    pub similarity_score: Option<f64>,
}
```

### Evaluation Metrics (anno)

```rust
// anno::eval::freco_metrics

pub struct FrecoMetrics {
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub mcc: f64,  // Matthews Correlation Coefficient
}

pub struct FrecoMiningMetrics {
    pub precision_at_k: HashMap<usize, f64>,
    pub recall_at_k: HashMap<usize, f64>,
    pub average_precision: f64,
}

pub struct AttitudeMetrics {
    pub accuracy: f64,
    pub macro_f1: f64,
    pub cohens_kappa: f64,
}

pub struct CrossTopicEvaluation {
    pub per_topic: HashMap<String, FrecoMetrics>,
    pub f1_mean: f64,
    pub f1_std: f64,
}
```

### Connection to Existing Modules

| anno Module | FRECO Connection |
|-------------|------------------|
| `eval::cdcr` | Candidate pair scoring with cross-encoder |
| `eval::coref` | Coreference chain types and metrics |
| `eval::inter_doc_coref` | Cross-document clustering |
| `linking::candidate` | String similarity metrics |
| `lang` | Multilingual event extraction |

## Evaluation Protocol

### Classification Task

Binary: is this pair (event_a, event_b) FRECO?

**Leave-One-Topic-Out**:
- Train on 3 topics, test on 1
- Tests generalization beyond topical cues
- Report mean ± std F1 across folds

### Mining Task

Given candidate pool, retrieve FRECO pairs:
1. Score all pairs with CDEC cross-encoder
2. Filter by similarity threshold (e.g., 0.3)
3. Apply FRECO classifier
4. Rank by confidence

**Metrics**: P@K, R@K, Average Precision

### Bootstrapped Mining

Semi-supervised expansion:
1. Start with small gold set
2. Mine high-confidence pairs (threshold 0.9)
3. Retrain classifier with expanded set
4. Lower threshold, repeat

**Stopping criteria**:
- Validation loss plateau/increase
- Jaccard with previous round < threshold
- Few new positive pairs

## Implementation Roadmap

### Phase 1: Types & Metrics (Complete)

- [x] `anno_core::types::framing` - Core types
- [x] `anno::eval::freco_metrics` - Evaluation metrics

### Phase 2: Event Extraction

- [ ] SRL integration for event arguments
- [ ] Event trigger detection
- [ ] Cross-document event mention linking

### Phase 3: FRECO Classification

- [ ] CDEC cross-encoder integration
- [ ] Candidate pair filtering
- [ ] Training data loader (FRECO corpus format)

### Phase 4: Bootstrapped Mining

- [ ] Mining pipeline
- [ ] Convergence detection
- [ ] Quality estimation

### Phase 5: Downstream Integration

- [ ] Media attitude detection
- [ ] Framing analysis reports
- [ ] Cross-topic generalization testing

## Research Connections

### Related Papers

- Zhao et al. (2024): Media attitude detection via framing analysis
- Yu et al. (2022): Pairwise event coreference (CDEC)
- Hovy et al. (2013): Quasi-identity in event coreference
- Mitamura et al. (2017): TAC KBP event hoppers

### Datasets

| Dataset | Topics | Pairs | Positive Rate |
|---------|--------|-------|---------------|
| FRECO (Zhao 2025) | 4 (Putin, Al-Shifa, HK, Rittenhouse) | 3,800 | 46.5% |
| RECB | Same 4 | - | - |

### Model Results (from paper)

| Model | SFT | DPO | SFT→DPO |
|-------|-----|-----|---------|
| Llama-3.2-3B | 75.2 | 77.8 | 77.5 |
| Llama-3.1-8B | 76.7 | 79.5 | 79.2 |
| + SRL | +1-3 | +1-2 | +1-2 |

Key finding: DPO helps with hard negatives (semantically similar but not FRECO).

## Open Questions

1. **Multilingual FRECO**: How do framing patterns differ across languages/cultures?

2. **Implicit framing**: Some framing is not in word choice but in what's omitted (selection framing).

3. **Framing categorization**: Can we automatically label divergence type (lexical, causal, etc.)?

4. **Historical texts**: How does FRECO apply to ancient texts with ideological variation?

5. **Scale**: Paper's bootstrapped mining found ~6K new pairs. Can we scale to millions?

## Usage Example

```rust
use anno::eval::freco_metrics::{FrecoEvaluator, FrecoMetrics};
use anno_core::types::{EventMention, FrecoPair, FramingAttitude};

// Create event mentions
let e1 = EventMention::new("e1", "doc1", "raid", 0, 4, "The raid at the hospital...")
    .with_attitude(FramingAttitude::Skeptical);
let e2 = EventMention::new("e2", "doc2", "operation", 0, 9, "The operation was conducted...")
    .with_attitude(FramingAttitude::Supportive);

// Create FRECO pair
let pair = FrecoPair::new(e1, e2)
    .with_label(true)
    .with_similarity_score(0.85);

// Evaluate
let evaluator = FrecoEvaluator::new();
let predictions = vec![(0.85, true), (0.60, false), (0.75, true)];
let metrics = evaluator.evaluate_classification(&predictions);

println!("F1: {:.3}", metrics.f1);
println!("MCC: {:.3}", metrics.mcc());
```

## References

```bibtex
@inproceedings{zhao2025freco,
  title={Seeing the Same Story Differently: Framing-Divergent Event Coreference 
         for Computational Framing Analysis},
  author={Zhao, Jin and Hu, Xinrui and Xue, Nianwen},
  booktitle={EMNLP},
  year={2025}
}

@inproceedings{hovy2013events,
  title={Events are not simple: Identity, non-identity, and quasi-identity},
  author={Hovy, Eduard and Mitamura, Teruko and Verdejo, Felisa and Araki, Jun and Philpot, Andrew},
  booktitle={Workshop on Events},
  year={2013}
}
```

