# Box Embeddings

Complete implementation of box embeddings for coreference resolution, encoding logical invariants as geometric properties.

## Overview

Box embeddings address limitations of vector-based approaches by providing explicit constraints for transitivity, syntactic rules, temporal evolution, uncertainty, and noise. This implementation is related to the **matryoshka-box** research project (not yet published), which explores combining matryoshka embeddings (variable dimensions) with box embeddings (hierarchical reasoning with uncertainty).

## Implementation Status: ✅ Complete

### Core Infrastructure

- ✅ **BoxEmbedding** (`src/backends/box_embeddings.rs`)
  - Axis-aligned hyperrectangles with min/max bounds
  - Operations: `volume()`, `intersection_volume()`, `conditional_probability()`, `coreference_score()`
  - Helper methods: `from_vector()`, `center()`, `size()`
  - 19 comprehensive tests passing

- ✅ **BoxCorefResolver** (`src/eval/coref_resolver.rs`)
  - Implements `CoreferenceResolver` trait
  - Syntactic constraint enforcement (Principle B/C)
  - Union-find clustering with box-based scoring

### Advanced Features

- ✅ **Temporal Boxes** (BoxTE-style)
  - `TemporalBox` and `BoxVelocity` types
  - `at_time()` for time-slice operations
  - Prevents false coreference across time boundaries

- ✅ **Uncertainty-Aware Boxes** (UKGE-style)
  - `UncertainBox` with confidence derived from volume
  - `Conflict` detection for contradictory claims
  - Source trust modeling

- ✅ **Gumbel Boxes** (Noise Robustness)
  - `GumbelBox` with soft, probabilistic boundaries
  - `membership_probability()` for fuzzy membership
  - `robust_coreference()` with grid sampling

- ✅ **Interaction Modeling**
  - `interaction_strength()` for actor-action-target triples
  - `acquisition_roles()` for asymmetric relations
  - Triple intersection for event modeling

## Logical Invariants → Box Geometry

| Invariant | Box Encoding | Implementation |
|-----------|--------------|----------------|
| **Transitivity** | Box containment is transitive | `coreference_score()` uses conditional probability |
| **Syntactic Constraints** | Disjoint boxes for Principle B/C | `check_syntactic_constraints()` |
| **Temporal Evolution** | Time-sliced boxes | `TemporalBox::at_time()` |
| **Uncertainty** | Volume = confidence | `UncertainBox::confidence()` |
| **Noise Robustness** | Soft boundaries | `GumbelBox::membership_probability()` |

## Usage

### Basic Coreference Resolution

```rust
use anno::backends::box_embeddings::{BoxCorefConfig, BoxEmbedding, BoxCorefResolver};
use anno::eval::coref_resolver::CoreferenceResolver;
use anno::{Entity, EntityType};

// Entities to resolve
let entities = vec![
    Entity::new("John Smith", EntityType::Person, 0, 10, 0.9),
    Entity::new("he", EntityType::Person, 50, 52, 0.8),
];

// Create box embeddings (in practice, learned from data)
let boxes = vec![
    BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]),      // John Smith
    BoxEmbedding::new(vec![0.1, 0.1], vec![0.9, 0.9]),      // he (overlaps)
];

// Resolve
let config = BoxCorefConfig::default();
let resolver = BoxCorefResolver::new(config);
let resolved = resolver.resolve_with_boxes(&entities, &boxes);

// Check coreference
assert_eq!(resolved[0].canonical_id, resolved[1].canonical_id);
```

### Converting Vector Embeddings to Boxes

```rust
use anno::eval::coref_resolver::vectors_to_boxes;
use anno::backends::box_embeddings::BoxEmbedding;

// Vector embeddings from a text encoder
let embeddings = vec![
    0.1, 0.2, 0.3,  // Entity 0
    0.15, 0.25, 0.35,  // Entity 1
];
let hidden_dim = 3;

// Convert to boxes with fixed radius
let boxes = vectors_to_boxes(&embeddings, hidden_dim, Some(0.1));

// Or use adaptive radius (proportional to vector magnitude)
let boxes_adaptive = vectors_to_boxes(&embeddings, hidden_dim, None);
```

### Temporal Boxes (Time-Varying Entities)

```rust
use anno::backends::box_embeddings::{TemporalBox, BoxVelocity, BoxEmbedding};

// "The President" in 2012 (Obama)
let obama_base = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
let velocity = BoxVelocity::static_velocity(2);
let obama_presidency = TemporalBox::new(obama_base, velocity, (2012.0, 2016.0));

// "The President" in 2017 (Trump) - different box
let trump_base = BoxEmbedding::new(vec![5.0, 5.0], vec![6.0, 6.0]);
let trump_presidency = TemporalBox::new(trump_base, velocity, (2017.0, 2021.0));

// Check coreference at specific times
let score_2015 = obama_presidency.coreference_at_time(&trump_presidency, 2015.0);
assert_eq!(score_2015, 0.0); // Should not corefer (different times)
```

### Uncertainty-Aware Boxes (Misinformation Detection)

```rust
use anno::backends::box_embeddings::{UncertainBox, BoxEmbedding};

// High-confidence claim: "Trump is in NY" (small, precise box)
let claim_a = UncertainBox::new(
    BoxEmbedding::new(vec![0.0, 0.0], vec![0.1, 0.1]), // Small = high confidence
    0.95, // Source trust
);

// Contradictory claim: "Trump is in FL" (disjoint, high confidence)
let claim_b = UncertainBox::new(
    BoxEmbedding::new(vec![5.0, 5.0], vec![5.1, 5.1]), // Disjoint
    0.90,
);

// Detect conflict
if let Some(conflict) = claim_a.detect_conflict(&claim_b) {
    println!("Conflict detected! Severity: {:.3}", conflict.severity);
}
```

### Gumbel Boxes (Noise Robustness)

```rust
use anno::backends::box_embeddings::{GumbelBox, BoxEmbedding};

let mean_box = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
let gumbel = GumbelBox::new(mean_box, 0.1); // Temperature = 0.1 (sharp)

// Membership is probabilistic, not binary
let point = vec![0.5, 0.5];
let prob = gumbel.membership_probability(&point);
assert!(prob > 0.5); // High probability inside box

// Robust coreference tolerates slight misalignments
let box2 = BoxEmbedding::new(vec![0.05, 0.05], vec![0.95, 0.95]);
let gumbel2 = GumbelBox::new(box2, 0.1);
let score = gumbel.robust_coreference(&gumbel2, 100);
assert!(score > 0.3);
```

### Integration with Existing Code

```rust
use anno::grounded::{Identity, IdentityId};
use anno::backends::box_embeddings::BoxEmbedding;

let mut identity = Identity::new(0, "Marie Curie");
identity.box_embedding = Some(BoxEmbedding::new(
    vec![0.0, 0.0],
    vec![1.0, 1.0],
));
```

### Combining with Vector Embeddings

Box embeddings can be used alongside vector embeddings:
- **Vectors**: Fast semantic similarity (cosine similarity)
- **Boxes**: Logical constraints (transitivity, syntactic rules)

Hybrid approach: Use vectors for initial filtering, boxes for final resolution.

## Performance Considerations

- **Box operations** are more expensive than cosine similarity (O(d) vs O(d) but with more operations)
- **Temporal boxes** require time-slice computation (cache `at_time()` results)
- **Gumbel boxes** use grid sampling (adjust sample count for speed vs accuracy)

## Code Organization

```
src/
├── backends/
│   └── box_embeddings.rs      # Core types (BoxEmbedding, TemporalBox, etc.)
├── eval/
│   └── coref_resolver.rs       # BoxCorefResolver + utilities
└── grounded.rs                 # Identity.box_embedding integration

examples/
└── box_coreference.rs            # Complete example
```

## Test Coverage

All 19 tests passing:
- Core box operations (volume, intersection, conditional probability)
- Temporal boxes (at_time, coreference_at_time, velocity)
- Uncertainty boxes (confidence, conflict detection)
- Gumbel boxes (membership, robust coreference, temperature effects)
- Interaction modeling (interaction_strength, acquisition_roles)
- Helper methods (from_vector, center, size)

## Research References

- **Box Embeddings**: Vilnis et al. (2018) - Probabilistic embedding of knowledge graphs
- **BERE**: Lee et al. (2022) - Event-event relation extraction
- **BoxTE**: Messner et al. (2022) - Temporal knowledge graphs
- **UKGE**: Chen et al. (2021) - Uncertainty-aware embeddings

## Training

See [`BOX_EMBEDDINGS_TRAINING.md`](BOX_EMBEDDINGS_TRAINING.md) for mathematical details and training procedures.

## Next Steps (Research)

1. **Learning Box Embeddings**: How to learn box parameters from coreference annotations?
2. **Evaluation**: Compare box-based vs. vector-based on CoNLL-2012
3. **Hybrid Approach**: When to use boxes vs. vectors?
4. **Performance**: Optimize box operations (currently O(d) per pair)
5. **Full BoxTE**: Temporal training for time-varying entities

