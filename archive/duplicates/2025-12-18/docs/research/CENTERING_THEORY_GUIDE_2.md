# Centering Theory Implementation Guide

## Quick Start

```rust
use anno::discourse::centering::{
    CenteringState, CenteringTransition, ForwardCenter, GrammaticalRole,
    InformationStatus, CenteringConfig, track_centers, analyze_coherence,
};

// Build utterances as lists of forward centers (entities mentioned)
let utterances = vec![
    // U1: "John saw Mary."
    vec![
        ForwardCenter::new(1, "John", 1.0)
            .with_role(GrammaticalRole::Subject)
            .with_info_status(InformationStatus::New),
        ForwardCenter::new(2, "Mary", 0.8)
            .with_role(GrammaticalRole::DirectObject)
            .with_info_status(InformationStatus::New),
    ],
    // U2: "He gave her a book."
    vec![
        ForwardCenter::new(1, "He", 0.9)
            .with_role(GrammaticalRole::Subject)
            .with_info_status(InformationStatus::Evoked),
        ForwardCenter::new(2, "her", 0.7)
            .with_role(GrammaticalRole::IndirectObject)
            .with_info_status(InformationStatus::Evoked),
        ForwardCenter::new(3, "a book", 0.6)
            .with_role(GrammaticalRole::DirectObject)
            .with_info_status(InformationStatus::New),
    ],
];

// Track centers through the discourse
let config = CenteringConfig::default();
let states = track_centers(&utterances, &config);

// Analyze coherence
let analysis = analyze_coherence(&states);
println!("Average coherence: {:.2}", analysis.avg_coherence);
println!("Continuity ratio: {:.2}", analysis.continuity_ratio);
```

## Core Concepts

### Forward-Looking Centers (Cf)

Each utterance has a set of **forward-looking centers**—the entities it mentions,
ranked by salience. Ranking factors:

1. **Grammatical role**: Subject > Direct Object > Indirect Object > Oblique
2. **Information status**: Evoked (mentioned before) > Unused (world knowledge) > New
3. **Explicit salience score**: From external NER/mention detection

```rust
let fc = ForwardCenter::new(entity_id, surface_form, base_salience)
    .with_role(GrammaticalRole::Subject)
    .with_info_status(InformationStatus::Evoked);

// Effective salience combines these factors
let score = fc.effective_salience();
```

### Backward-Looking Center (Cb)

The **backward-looking center** Cb(U_n) is the highest-ranked member of Cf(U_{n-1})
that is realized in U_n. It represents what the discourse is "about."

```
U1: "John saw Mary."        Cf = [John, Mary]    Cb = None (discourse-initial)
U2: "He gave her a book."   Cf = [He, her, book] Cb = John (highest from U1's Cf in U2)
```

### Preferred Center (Cp)

The **preferred center** Cp(U_n) is the highest-ranked member of Cf(U_n).
It predicts what the next utterance will likely be about.

### Transition Types

| Transition | Condition | Interpretation |
|------------|-----------|----------------|
| CONTINUE | Cb(U_n) = Cb(U_{n-1}) AND Cb = Cp | Same topic, remains most salient |
| RETAIN | Cb(U_n) = Cb(U_{n-1}) AND Cb ≠ Cp | Same topic, but shift signaled |
| SMOOTH-SHIFT | Cb(U_n) ≠ Cb(U_{n-1}) AND Cb = Cp | New topic, established smoothly |
| ROUGH-SHIFT | Cb(U_n) ≠ Cb(U_{n-1}) AND Cb ≠ Cp | New topic, not yet established |

**Preference ordering**: CONTINUE > RETAIN > SMOOTH-SHIFT > ROUGH-SHIFT

This predicts processing difficulty: texts with more CONTINUEs are easier to read.

## CT + Recency (Jiang et al. 2022)

Research shows vanilla CT provides limited benefit for neural coreference, but
CT augmented with recency captures significant signal.

```rust
let config = CenteringConfig {
    use_recency: true,
    recency_decay: 0.5,  // Exponential decay factor
    recency_window: 5,   // Look back 5 utterances
    ..Default::default()
};

let states = track_centers(&utterances, &config);

// Recency scores are available on each state
for (entity_id, score) in &states[2].recency_scores {
    println!("Entity {}: recency score {:.2}", entity_id, score);
}
```

## Integration with Coreference

Use centering scores to rank antecedent candidates:

```rust
use anno::discourse::centering::score_antecedents;

// For a pronoun in utterance 3, get antecedent scores
let scores = score_antecedents(3, &states, &config);

// Higher scores = better antecedent candidates
for (entity_id, score) in scores.iter() {
    println!("Entity {} score: {:.3}", entity_id, score);
}
```

## Information Status

Following Prince (1981) and Strube's hearer-old/hearer-new distinction:

| Status | Description | Example |
|--------|-------------|---------|
| `New` | First mention, indefinite | "a man walked in" |
| `Inferrable` | First mention but inferrable | "the door" (after "a room") |
| `Evoked` | Previously mentioned | "the man" (after "a man") |
| `Unused` | Known from world knowledge | "the sun", "the president" |

**Hearer-old entities** (Evoked, Unused, Inferrable) rank higher than hearer-new
(New) in the S-list model.

## Coherence Analysis

Analyze discourse structure through centering patterns:

```rust
let analysis = analyze_coherence(&states);

// Transition distribution
for (transition, count) in &analysis.transition_counts {
    println!("{}: {}", transition, count);
}

// Coherence metrics
println!("Average coherence: {:.2}", analysis.avg_coherence);
println!("Continuity ratio: {:.2}", analysis.continuity_ratio);
println!("Longest continuity run: {}", analysis.max_continuity_run);
println!("Number of shifts: {}", analysis.shift_count);
```

## Theoretical Background

### Israel's Critique (1994)

Israel noted that the "extent" of a discourse referent is not statically
determinable—its binding depends on subsequent discourse. Centering operationalizes
this by tracking which entities remain "centered" across utterances.

### Connection to S-List (Strube 1998)

Strube's "Never Look Back" model simplifies centering to a single salience list
that processes incrementally. Anno's `EntityMemory` in incremental coreference
implements this pattern.

### BFP Algorithm (1987)

The Brennan-Friedman-Pollard algorithm defines the transition rules. For
discourse-initial utterances (no previous Cb), establishing Cb=Cp is treated
as CONTINUE (smooth establishment), otherwise RETAIN.

## See Also

- `anno/src/discourse/centering.rs` — Implementation
- `anno/src/discourse/uncertain_reference.rs` — Epsilon-term deferred resolution
- `anno/src/eval/incremental_coref.rs` — EntityMemory with LRU/LFU
- `docs/research/DYNAMIC_SEMANTICS_THEORY.md` — Theoretical foundations

## References

```bibtex
@inproceedings{grosz1995centering,
  title={Centering: A Framework for Modeling Local Coherence},
  author={Grosz, Barbara J and Joshi, Aravind K and Weinstein, Scott},
  booktitle={Computational Linguistics},
  volume={21},
  number={2},
  year={1995}
}

@inproceedings{strube1998never,
  title={Never Look Back: An Alternative to Centering},
  author={Strube, Michael},
  booktitle={COLING-ACL},
  year={1998}
}

@article{jiang2022centering,
  title={Investigating the Role of Centering Theory in Neural Coreference},
  author={Jiang, Yuchen Eleanor and Cotterell, Ryan and Sachan, Mrinmaya},
  journal={arXiv:2210.14678},
  year={2022}
}

@article{brennan1987centering,
  title={A Centering Approach to Pronouns},
  author={Brennan, Susan E and Friedman, Marilyn Walker and Pollard, Carl J},
  booktitle={ACL},
  year={1987}
}
```
