# Uncertain Reference and Deferred Resolution

## Overview

This module implements **epsilon-term semantics** for coreference resolution,
following Israel's (1994) insight that discourse referent identity is often
uncertain until later context resolves the ambiguity.

## The Problem: Indeterminacy in Media Res

Consider:

```
When she arrived, Mary greeted John.
```

At "she", we don't yet know the referent. Options:

1. **Immediate resolution** (greedy): Pick the best candidate now
2. **Deferred resolution** (lazy): Wait for more context
3. **Probabilistic**: Maintain a distribution over candidates

The epsilon-term approach maintains uncertainty explicitly, refining as context
arrives and resolving when forced.

## Quick Start

```rust
use anno::discourse::uncertain_reference::{
    UncertainReference, ReferenceCandidate, CandidateSource,
    ResolutionStrategy, resolve_uncertain,
};

// Create uncertain reference for a pronoun
let mut reference = UncertainReference::new("she");

// Add candidates with weights (log-odds or probabilities)
reference.add_candidate(
    ReferenceCandidate::new(1, "Mary", 0.6)
        .with_source(CandidateSource::Discourse)
);
reference.add_candidate(
    ReferenceCandidate::new(2, "Jane", 0.4)
        .with_source(CandidateSource::Discourse)
);

// Later, new evidence arrives
reference.update_evidence(1, 0.3);  // Boost Mary
reference.update_evidence(2, -0.1); // Demote Jane

// Check uncertainty level
println!("Entropy: {:.2}", reference.entropy());
println!("Ambiguous? {}", reference.is_ambiguous(0.4));

// Resolve when needed
let resolved = reference.resolve();
println!("Resolved to: {:?}", resolved.map(|r| r.description));
```

## Core Types

### ReferenceCandidate

A candidate referent with associated metadata:

```rust
let candidate = ReferenceCandidate::new(entity_id, "John Smith", weight)
    .with_source(CandidateSource::Discourse)
    .satisfies("gender:masculine")
    .satisfies("number:singular")
    .violates("animacy:inanimate");  // Hard constraint violation
```

Candidate sources:
- `Discourse`: Explicit antecedent in text
- `WorldKnowledge`: Known from common ground
- `Bridging`: Inferrable (e.g., "the door" after "a room")
- `Accommodation`: Adding to discourse model
- `Cataphoric`: Forward reference (resolved later)

### UncertainReference

Maintains a distribution over candidates:

```rust
let mut reference = UncertainReference::new("the issue")
    .cataphoric()  // Forward-referencing
    .at_position(42);  // Discourse position

// Add constraints
reference.add_constraint(ConstraintKind::Number, "singular", true);  // Hard
reference.add_constraint(ConstraintKind::Salience, "high", false);   // Soft
```

Key methods:
- `add_candidate()` — Add or merge a candidate
- `update_evidence()` — Adjust candidate weight
- `prune()` — Remove low-weight candidates
- `prune_violations()` — Remove candidates with hard constraint violations
- `entropy()` — Uncertainty measure (higher = more uncertain)
- `is_ambiguous()` — Multiple high-probability candidates?
- `resolve()` — Force resolution to best candidate
- `probabilities()` — Softmax distribution over candidates

## Resolution Strategies

```rust
use anno::discourse::uncertain_reference::ResolutionStrategy;

// Immediate resolution
let strategy = ResolutionStrategy::Greedy;

// Wait until forced
let strategy = ResolutionStrategy::Deferred;

// Maintain full distribution
let strategy = ResolutionStrategy::Probabilistic;

// Resolve only when confident (e.g., >90%)
let strategy = ResolutionStrategy::Confident(90);

// Apply strategy
let result = resolve_uncertain(&mut reference, strategy);
```

## Deferred Resolution Context

Track multiple uncertain references across a discourse:

```rust
use anno::discourse::uncertain_reference::DeferredResolutionContext;

let mut context = DeferredResolutionContext::new();

// Add uncertain references as they're encountered
context.add_uncertain(pronoun_reference);
context.add_uncertain(definite_np_reference);

// Record entity mentions (for tracking discourse model)
context.record_mention(1);  // Entity 1 mentioned at current position
context.advance();          // Move to next position

// When new entities become available, try resolving cataphoric references
context.try_resolve_cataphoric(&[
    (3, "Mary Smith".to_string(), 0.9),
]);

// At discourse end, force resolution of all pending
context.resolve_all();

// Get statistics
let stats = context.statistics();
println!("Resolved: {}/{}", stats.resolved, stats.total);
println!("Ambiguous: {}", stats.ambiguous);
println!("Cataphoric: {}", stats.cataphoric);
println!("Average entropy: {:.2}", stats.avg_entropy);
```

## Constraint System

Define and check reference constraints:

```rust
use anno::discourse::uncertain_reference::{
    ReferenceConstraint, ConstraintKind,
};

// Hard constraints must be satisfied
reference.add_constraint(ConstraintKind::Gender, "feminine", true);
reference.add_constraint(ConstraintKind::Number, "singular", true);

// Soft constraints are preferences
reference.add_constraint(ConstraintKind::Salience, "high", false);
reference.add_constraint(ConstraintKind::Recency, "recent", false);

// Candidates track which constraints they satisfy/violate
let candidate = ReferenceCandidate::new(1, "Mary", 0.8)
    .satisfies("gender:feminine")
    .satisfies("number:singular")
    .satisfies("animacy:animate");

// Prune candidates that violate hard constraints
reference.prune_violations();
```

Constraint kinds:
- `Gender`, `Number`, `Person` — Agreement features
- `Animacy` — Animate vs inanimate requirement
- `SemanticType` — Must be person/organization/location/etc.
- `Binding` — Syntactic binding theory constraints
- `Salience`, `Recency` — Discourse-level preferences

## Entropy and Ambiguity

Measure uncertainty in the candidate distribution:

```rust
// Entropy: 0 = certain, log2(n) = maximally uncertain
let h = reference.entropy();

// Get probability distribution (softmax over weights)
let probs = reference.probabilities();
for (entity_id, prob) in probs {
    println!("Entity {}: {:.1}%", entity_id, prob * 100.0);
}

// Check if ambiguous at threshold
if reference.is_ambiguous(0.3) {
    println!("Multiple candidates above 30%!");
}
```

## Use Cases

### Cataphoric Resolution

Forward-referencing pronouns ("When *she* arrived, Mary..."):

```rust
// Create cataphoric reference at position 5
let mut cataphoric = UncertainReference::new("she")
    .cataphoric()
    .at_position(5);

// ... later, when we see "Mary" at position 12 ...
cataphoric.add_candidate(
    ReferenceCandidate::new(1, "Mary", 0.9)
        .with_source(CandidateSource::Cataphoric)
);

// Now resolve
let resolved = cataphoric.resolve();
```

### Bridging Inference

Inferrable referents ("John entered *the room*. The door was open."):

```rust
// "the door" is inferrable from "the room"
let mut bridging = UncertainReference::new("the door");

bridging.add_candidate(
    ReferenceCandidate::new(2, "door of room#1", 0.85)
        .with_source(CandidateSource::Bridging)
);
```

### Ambiguous Pronouns

"John told Bill that *he* was leaving":

```rust
let mut ambiguous = UncertainReference::new("he");

// Both John and Bill are valid candidates
ambiguous.add_candidate(ReferenceCandidate::new(1, "John", 0.5));
ambiguous.add_candidate(ReferenceCandidate::new(2, "Bill", 0.5));

// Use probabilistic strategy to maintain uncertainty
let strategy = ResolutionStrategy::Probabilistic;

// Or resolve only when highly confident
let strategy = ResolutionStrategy::Confident(80);
```

## Theoretical Background

### Epsilon-Terms (Hilbert)

For a predicate A[x], the epsilon-term ε_x(A[x]) denotes "some A" without
specifying which. In coreference:

- **Introduction**: "a man" introduces ε_x(man(x))
- **Refinement**: Context narrows possible values
- **Resolution**: Binding determined by discourse end

### Connection to Israel (1994)

Israel observed that proof-system parameters have "indeterminate" identity
until subsequent proof structure determines their binding. Similarly, discourse
referents have uncertain identity until resolved.

### Comparison to Other Approaches

| Approach | When Resolved | Uncertainty Modeling |
|----------|---------------|---------------------|
| Mention-ranking | Immediately | Implicit in scores |
| Entity-centric | At cluster merge | Cluster membership |
| **Epsilon-term** | When forced | Explicit distribution |

## See Also

- `anno/src/discourse/uncertain_reference.rs` — Implementation
- `anno/src/discourse/centering.rs` — Centering theory integration
- `docs/notes/research/theory/DYNAMIC_SEMANTICS_THEORY.md` — Theoretical foundations
- `docs/notes/research/systems/ABSTRACT_ANAPHORA_RESEARCH.md` — Abstract anaphora handling

## References

```bibtex
@article{israel1994dynamic,
  title={The Very Idea of Dynamic Semantics},
  author={Israel, David},
  journal={Proceedings of the Ninth Amsterdam Colloquium},
  year={1994}
}

@book{hilbert1939grundlagen,
  title={Grundlagen der Mathematik},
  author={Hilbert, David and Bernays, Paul},
  volume={2},
  year={1939},
  note={Epsilon calculus introduced here}
}

@article{kehler1997current,
  title={Current Theories of Centering for Pronoun Interpretation},
  author={Kehler, Andrew},
  journal={Computational Linguistics},
  volume={23},
  number={1},
  year={1997}
}
```
