# Type Theory and Named Entity Recognition

**A Curry-Howard Perspective on Anno's Architecture**

---

## Overview

The Curry-Howard correspondence reveals that **types are propositions** and **programs are proofs**.
This perspective illuminates anno's type hierarchy and suggests principled design decisions.

---

## 1. The Core Correspondence

### Types as Propositions

| Anno Type | Logical Proposition |
|-----------|---------------------|
| `Entity` | ∃(span, type). "an entity of this type exists at this span" |
| `Vec<Entity>` | Conjunction: "all these entities exist" |
| `Option<Entity>` | "may or may not have an entity" |
| `Result<T, E>` | Disjunction: "either T holds OR we have error E" |
| `Signal` | Atomic: "evidence of label L at location S" |
| `Track` | Conjunction: "these signals all corefer" |
| `Identity` | Universal: "this entity exists across all contexts" |

### Functions as Implications

| Function Signature | Logical Reading |
|--------------------|-----------------|
| `Model::extract_entities(&str) -> Result<Vec<Entity>>` | "Given text, I can construct entity proofs (or fail)" |
| `CoreferenceResolver::resolve(entities) -> Vec<Cluster>` | "Given entity witnesses, I can partition them" |
| `EntityLinker::link(track) -> Option<Identity>` | "Given a track, I may be able to ground it to KB" |

---

## 2. Coreference as Partition Logic

### Configurations as Logical Structures

Kehler (1997) modeled coreference as a **distribution over partitions**. Each configuration
is a conjunction of cluster memberships:

```
Configuration = { {A,B}, {C}, {D,E} }
              ≈ (A~B) ∧ ¬(A~C) ∧ ¬(B~C) ∧ (D~E) ∧ ...
```

A `CorefConfiguration` is a **proof** that this particular clustering is valid.

### Incompatibility as Negation

Type incompatibility introduces negation constraints:

```
incompatible(A, C) ⟹ ¬(A~C) holds in all valid configurations
```

The `ConfigurationDistribution` prunes configurations violating these constraints—
this is **proof search** restricted to consistent proofs.

### Weighted Disjunction

A probability distribution over configurations is a **weighted disjunction**:

```
P(config₁) · config₁ ∨ P(config₂) · config₂ ∨ ...
```

This differs from simple `Vec<Configuration>` by attaching **credence** to each alternative.
Kehler argued this is essential for downstream data fusion.

---

## 3. Evidence Combination as Proof Composition

### Evidence Sources as Independent Proofs

Each `EvidenceSource` provides evidence from an independent "proof system":

| Source | Proof Domain |
|--------|--------------|
| `StringSimilarity` | Syntactic (edit distance) |
| `Embedding` | Semantic (vector space) |
| `TypeMatch` | Ontological (category theory) |
| `KnowledgeBase` | External authority (KB entailment) |
| `ContextualCoref` | Distributional (neural model) |

### Mediation Strategies as Proof Combinators

How we combine evidence determines the logical structure:

| Strategy | Logical Interpretation |
|----------|------------------------|
| `Average` | Equal weight to all proof systems |
| `Max` | Any strong proof suffices (disjunction) |
| `Min` | Require all proofs to agree (conjunction) |
| `Product` | Independent evidence multiplies (Bayesian) |
| `Bayesian` | Likelihood ratios + prior |

### Dempster-Shafer as Conflict Resolution

Dempster's Rule handles **contradictory proofs**:

```
m₃(A) = (1/(1-κ)) Σ_{B∩C=A} m₁(B) · m₂(C)
```

When two sources disagree, Dempster normalizes away the conflicting mass.
This is more principled than averaging because it acknowledges the conflict
rather than pretending both sources are equally reliable on all cases.

---

## 4. The Signal → Track → Identity Hierarchy

### Levels of Evidential Commitment

```
Signal   (Level 1): Atomic witness, model-dependent confidence
Track    (Level 2): Conjunction of signals, coreference proof
Identity (Level 3): Universal grounding, KB-backed
```

From a proof-theoretic view:

1. **Signal detection** = constructing atomic proofs
2. **Track formation** = combining proofs via conjunction (coreference transitivity)
3. **Identity resolution** = universal generalization (KB grounding)

### Compositional Dependencies

The hierarchy enforces logical dependencies:

```
∃ Identity ⟹ ∃ Track ⟹ ∃ Signal
```

You cannot have an Identity without Tracks, nor Tracks without Signals.
This mirrors how proofs build on lemmas.

---

## 5. Invariants and Refinement Types

### Currently Runtime-Checked Invariants

These properties should always hold but aren't type-enforced:

| Invariant | Type-Level Solution |
|-----------|---------------------|
| `entity.start ≤ entity.end` | Newtype `ValidSpan(start, end)` with smart constructor |
| `confidence ∈ [0, 1]` | Newtype `Probability(f32)` or refinement type |
| `partition is valid` | Each mention in exactly one cell |
| `track.signals ⊆ document.signals` | Dependent typing / indexing |

### Toward "Make Invalid States Unrepresentable"

The Curry-Howard perspective suggests we should:

1. **Encode invariants in types** where possible
2. **Use newtypes** for semantic meaning (`Probability`, `ValidSpan`, `MentionId`)
3. **Leverage the sealed trait pattern** to control who can construct proofs
4. **Design APIs so misuse is a type error**, not a runtime panic

---

## 6. Neural Models in the Logical Framework

### Neural Proofs

A neural NER model is a **learned proof constructor**:

```
Model : Text → Evidence(Entities)
```

The model's weights encode a compressed proof search procedure. At inference,
the forward pass *constructs* entity proofs from text evidence.

### Uncertainty as Partial Proofs

The `confidence` field represents **proof strength**, not logical truth:

- `confidence = 1.0`: Strong proof (but still fallible—model isn't an oracle)
- `confidence = 0.5`: Weak proof, significant uncertainty
- `confidence = 0.0`: No proof (should probably not be returned)

This differs from classical logic where proofs are all-or-nothing.

### Calibration as Proof Quality

Kehler's cross-entropy evaluation measures whether proof strength matches reality:

```
If model says P(correct) = 0.9, it should be correct ~90% of the time
```

This is **calibration**—the alignment between stated confidence and actual accuracy.
Well-calibrated models produce "honest" proof strength estimates.

---

## 7. Triad Networks and Higher-Order Logic

### Beyond Pairwise

Triad networks (Meng & Rumshisky 2018) score mention **triples** to enforce transitivity:

```
score(A, B, C) captures: if A~B and B~C, then A~C
```

This is **second-order** reasoning about coreference relations, not just first-order
pairwise judgments.

### Hypergraph Evidence

The proposed hypergraph design (see `research/HYPERGRAPH_EVIDENCE_DESIGN.md`) extends this:

```
Hyperedge: {mention₁, mention₃, mention₇}
Annotation: "these three corefer" + confidence
```

This is evidence about **sets**, not just pairs—a move toward higher-order logic.

---

## 8. Practical Implications

### API Design Principles

1. **Function signatures are contracts**: `fn foo(A) -> B` promises "given A, I produce B"
2. **Result types encode partiality**: Extraction may fail, and the type says so
3. **Option types encode optionality**: Entity linking may not find a KB entry
4. **Generic parameters are universals**: `fn process<T: Signal>` works for any signal type

### Testing as Proof Verification

Unit tests verify that implementations match their type-level contracts:

```rust
#[test]
fn model_extracts_valid_entities() {
    let entities = model.extract_entities(text, None)?;
    for entity in entities {
        // Verify the invariants that types don't (yet) enforce
        assert!(entity.start <= entity.end);
        assert!(entity.confidence >= 0.0 && entity.confidence <= 1.0);
    }
}
```

These tests are **proof obligations** that the type system can't (yet) discharge.

### Future: Dependent Types?

Languages with dependent types (Idris, Agda, Lean) could enforce invariants like
`start ≤ end` at compile time. Rust's type system is moving in this direction with:

- Const generics
- Compile-time computation
- GATs (Generic Associated Types)

---

## References

### Curry-Howard
- Curry, Haskell (1934). "Functionality in Combinatory Logic"
- Howard, William (1980). "The Formulae-as-Types Notion of Construction"
- Wadler (2015). "Propositions as Types" (accessible introduction)

### Probabilistic Coreference
- Kehler (1997). "Probabilistic Coreference in Information Extraction"
- Dempster (1968). "A Generalization of Bayesian Inference"

### Neural Coreference
- Lee et al. (2017). "End-to-End Neural Coreference Resolution"
- Meng & Rumshisky (2018). "Triad-based Neural Network for Coreference Resolution"

### Type-Driven Design
- Yorgey (2016). "Functional Pearl: Getting a Quick Fix on Comonads"
- Kiselyov (2015). "Typed Tagless Final Interpreters"
- "Make Invalid States Unrepresentable" (Rust community idiom)

