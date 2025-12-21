# Geometric and Topological Foundations for Entity Resolution

> Research synthesis: December 2025
> Connects advances in geometric deep learning to Anno's coreference architecture

## Executive Summary

Three geometric paradigms are emerging as theoretical foundations for coreference:

1. **Sheaf Neural Networks** — Enforce transitivity at the gradient level via the sheaf Laplacian
2. **Hyperbolic Embeddings** — Native representation of entity type hierarchies  
3. **Persistent Homology (TDA)** — Coordinate-free analysis of long-distance dependencies

Anno already has **box embeddings** (axis-aligned hyperrectangles). The question: how do these new paradigms complement or supersede boxes?

---

## The Core Problem: Transitivity

Pairwise coreference models can produce globally inconsistent predictions:

```
P(A~B) = 0.9
P(B~C) = 0.9
P(A~C) = 0.1  ← Transitivity violation!
```

Current solutions in Anno:

| Approach | Location | Mechanism | Limitation |
|----------|----------|-----------|------------|
| **Union-Find clustering** | `anno-coalesce/` | Post-hoc transitive closure | Errors propagate |
| **Iterative refinement** | `graph_coref.rs` | Transitivity bonus each round | Heuristic, not gradient |
| **Box containment** | `box_embeddings.rs` | Geometric containment is transitive | Axis-aligned (limited expressivity) |

The new paradigm: **Sheaf Neural Networks** enforce transitivity *in the loss function*, not post-hoc.

---

## 1. Sheaf Neural Networks for Coreference

### The Mathematical Setup

A **cellular sheaf** on a graph G = (V, E) assigns:
- A vector space F(v) to each node v (mention embedding space)
- A linear map F(u→v): F(u) → F(v) to each edge (restriction map)

For coreference:
- **Nodes** = mentions
- **Edges** = candidate coref links
- **Restriction maps** = learned transformations between mention coordinate systems

### The Sheaf Dirichlet Energy

The key equation:

```
E(x) = Σ_{(u,v) ∈ E} || F(u→v) · x_u - F(v→u) · x_v ||²
```

**Minimizing this energy forces consistency**: if A~B and B~C have compatible restriction maps, then A~C is implicitly enforced because the energy would be high otherwise.

### Key Insight

Standard GNNs use scalar edge weights: "how much to mix neighbor features."
Sheaf NNs use **linear maps**: "how to transform features before mixing."

This extra structure encodes directionality and type constraints:
- Pronoun → Antecedent requires a specific transformation
- Proper Noun → Proper Noun is nearly identity
- The transformation itself is learned

### Existing Implementations

#### twitter-research/neural-sheaf-diffusion (Apache 2.0)

The **official NeurIPS paper implementation** — MIT-compatible license!

- **Repository**: https://github.com/twitter-research/neural-sheaf-diffusion
- **License**: Apache 2.0 (permissive — safe to port to Anno)
- **Paper**: Bodnar et al. (2023), "Neural Sheaf Diffusion" (NeurIPS)
- **Key files**: `models/` contains sheaf layer implementations in PyTorch
- **Status**: 83 stars, actively maintained

This is the primary reference for implementation.

#### koho (Rust/Candle)

A Rust sheaf neural network library built on Candle (same backend Anno uses):

- **Repository**: https://github.com/TheMesocarp/koho
- **License**: AGPL-3.0 (copyleft — integration requires careful licensing)
- **Key features**:
  - `CellularSheaf` with k-cells and restriction maps
  - `DiffusionLayer` applying Hodge Laplacian + learned weights
  - Full training loop with Adam/SGD
- **Status**: Conceptually aligned but AGPL license is viral

**Core diffusion operation** (from koho):

```rust
pub fn diffuse(
    &self,
    sheaf: &CellularSheaf,
    k: usize,
    k_features: Matrix,
    down_included: bool,
) -> Result<Matrix, KohoError> {
    let diff = sheaf.k_hodge_laplacian(k, k_features, down_included)?;
    let weighted = self.weights.matmul(diff.inner())?;
    self.activation.activate(weighted)
}
```

### Integration Path for Anno

**Option A: Direct dependency** (blocked by AGPL)
- koho is AGPL, which is viral
- Would require Anno to be AGPL

**Option B: Clean-room reimplementation**
- Implement sheaf Laplacian construction from first principles
- Use the mathematical formulation, not koho's code
- MIT-compatible

**Option C: Interop layer**
- Call koho as a subprocess for sheaf-based clustering
- Keep Anno's core MIT-licensed

**Recommended**: Option B. The mathematics are well-documented in:
- Bodnar et al. (2023): "Neural Sheaf Diffusion"
- Hansen & Ghrist (2019): "Toward a Spectral Theory of Cellular Sheaves"

### Concrete Implementation Sketch

```rust
// anno/src/backends/sheaf_coref.rs

/// A sheaf over the coreference graph.
/// 
/// Nodes = mentions, Edges = candidate coref links.
/// Each edge has a learned restriction map.
pub struct CorefSheaf {
    /// Stalk dimension at each mention
    stalk_dim: usize,
    /// Restriction maps: edge_index -> (d x d) matrix
    restriction_maps: HashMap<(usize, usize), Tensor>,
    /// Whether maps are learned or fixed
    learned: bool,
}

impl CorefSheaf {
    /// Compute the sheaf Laplacian L = B^T * diag(weights) * B
    /// where B is the coboundary operator.
    pub fn laplacian(&self, edges: &[(usize, usize)]) -> Tensor {
        // Build coboundary matrix
        // Apply restriction maps
        // Construct Laplacian
    }
    
    /// Sheaf diffusion: x' = (I - τL)x
    /// Iterating this enforces consistency.
    pub fn diffuse(&self, x: &Tensor, tau: f32, steps: usize) -> Tensor {
        let mut y = x.clone();
        let laplacian = self.laplacian();
        for _ in 0..steps {
            y = &y - &(tau * laplacian.matmul(&y));
        }
        y
    }
}
```

---

## 2. Hyperbolic vs Box Embeddings

Anno currently uses **box embeddings** (axis-aligned hyperrectangles in Euclidean space).
The alternative: **hyperbolic embeddings** (Poincaré ball model).

### Comparison

| Property | Box Embeddings | Hyperbolic |
|----------|----------------|------------|
| **Geometry** | Euclidean, axis-aligned | Negative curvature |
| **Containment** | Explicit (min/max bounds) | Implicit (entailment cones) |
| **Hierarchy capacity** | O(2^d) nodes | O(exp(r)) nodes |
| **Interpretability** | High (visualizable) | Medium |
| **Rotation invariance** | No | Yes |
| **Current Anno status** | Implemented | Not implemented |

### When to Use Each

**Box embeddings** are better for:
- Temporal evolution (TemporalBox with velocity)
- Uncertainty quantification (volume = confidence)
- Explicit type constraints (entity ⊆ person ⊆ politician)
- Debugging (easy to visualize)

**Hyperbolic embeddings** are better for:
- Deep hierarchies (type taxonomies with many levels)
- Cross-lingual type alignment (rotation-invariant)
- Entailment-oriented tasks (A entails B iff A is in B's cone)

### Research: HMLA (Hyperbolic Multi-Head Latent Attention)

From arXiv:2507.17787 (2025):

> Projects keys and values into low-dimensional hyperbolic latent spaces, significantly reducing KV-cache size during generation compared to Euclidean equivalents.

**Implication**: For transformer-based coref models, hyperbolic attention reduces memory while preserving hierarchy.

### Integration Path

Given that Anno already has box embeddings:

1. **Keep boxes for uncertainty/temporal** — they're well-suited
2. **Add hyperbolic for type hierarchies** — complement, don't replace
3. **Hybrid approach**: Use hyperbolic for coarse type reasoning, boxes for fine-grained coreference

```rust
// Potential extension to anno-core/src/grounded.rs

pub struct HybridEmbedding {
    /// Hyperbolic embedding for type hierarchy
    pub poincare: Option<PoincareEmbedding>,
    /// Box embedding for uncertainty/temporal
    pub box_emb: Option<BoxEmbedding>,
}
```

---

## 3. Persistent Homology for Discourse Analysis

### The Idea

Apply **Topological Data Analysis (TDA)** to attention graphs or similarity matrices:
- Treat attention weights as distances
- Build a filtration (gradually connect points)
- Track which connections persist across scales
- **Persistent features = structurally important connections**

### Application to Coreference

For a long document with 100 mentions:
1. Build the pairwise similarity matrix (100x100)
2. Apply persistent homology
3. Identify:
   - **Short bars** = local connections (adjacent mentions)
   - **Long bars** = persistent connections (long-distance anaphora)

A "long bar" in degree-0 homology (connected components) indicates a coref link that survives across many similarity thresholds — likely a true coreference.

### Research: Topological BERT (arXiv:2206.15195)

> Betti numbers of the attention graph correlate more strongly with linguistic competence than raw attention weights.

**Key finding**: The *topology* of attention — not just the weights — predicts grammaticality judgments.

### Rust TDA Libraries

| Library | Status | Notes |
|---------|--------|-------|
| `symplexia` | Experimental (1 star) | JavaPlex port, incomplete |
| External Python | Mature | `giotto-tda`, `ripser` via subprocess |

**Recommended**: Start with Python TDA libraries via subprocess for prototyping, port to Rust only if it becomes a bottleneck.

### Integration Sketch

```rust
// anno/src/analysis/tda.rs

/// Compute persistent homology of the coref similarity graph.
/// Returns persistence diagram as (birth, death) pairs.
pub fn compute_persistence(
    similarity_matrix: &[Vec<f32>],
    max_dimension: usize,
) -> Vec<(f32, f32)> {
    // Call external TDA library
    // Parse output
    // Return persistence pairs
}

/// Filter edges by persistence: keep only those with
/// death - birth > threshold.
pub fn filter_by_persistence(
    edges: &[(usize, usize, f32)],
    persistence: &[(f32, f32)],
    threshold: f32,
) -> Vec<(usize, usize, f32)> {
    // Keep persistent edges
}
```

---

## 4. Enhanced Rhetorical Structure Theory (eRST)

From Computational Linguistics (2025): eRST replaces tree-based RST with **signaled graph theory**.

### Key Innovations

1. **Tree-breaking relations** — RST forced discourse into trees; eRST allows graphs
2. **Non-projective dependencies** — Cross-discourse links
3. **Concurrent relations** — Multiple simultaneous relations
4. **Explicit signaling** — Connectives as first-class graph elements

### Relevance to Coreference

Discourse structure affects coreference accessibility:
- Mentions in the same rhetorical segment are more likely coreferent
- eRST's graph structure captures this better than trees

### Current Anno Status

- `discourse/types.rs` has `DiscourseScope::preceding_clauses()`
- RST-based accessibility is not implemented
- **Gap**: eRST integration for discourse-aware coref

---

## 5. Training Dynamics: Grokking and Phase Transitions

### The Phenomenon

**Grokking**: Generalization suddenly spikes long after training accuracy plateaus.

From arXiv:2511.12768 (2025):

> Grokking maps to first-order phase transitions in thermodynamics. Models sit in a "mixed phase" (memorization) before "cooling" into a structured phase (generalization).

### Implications for Anno

1. **Don't stop training early** — Generalization may emerge late
2. **Monitor weight entropy** — Phase transition has distinct entropy signature
3. **Temperature annealing** — May help trigger grokking

### Practical Application

For learned coreference models (future work):

```rust
pub struct TrainingConfig {
    /// Continue training even after loss plateaus
    pub patience_for_grokking: usize,  // e.g., 10000 steps
    
    /// Monitor generalization separately from training loss
    pub track_validation_separately: bool,
    
    /// Use temperature annealing schedule
    pub temperature_schedule: Option<TemperatureSchedule>,
}
```

---

## Summary: Integration Priorities

| Advance | Relevance | Effort | Priority |
|---------|-----------|--------|----------|
| **Sheaf Laplacian** | Direct (transitivity) | HIGH | P1 |
| **Hyperbolic embeddings** | Complementary | MEDIUM | P2 |
| **Persistent homology** | Diagnostic | LOW | P3 |
| **eRST integration** | Discourse context | MEDIUM | P2 |
| **Grokking awareness** | Training future models | LOW | P4 |

### Immediate Actions

1. ~~Document sheaf formulation in `BOX_EMBEDDINGS.md` as future direction~~ (DONE)
2. ~~Add `CorefSheaf` stub with mathematical documentation~~ (DONE - see `anno/src/geometric/sheaf.rs`)
3. Prototype TDA analysis on existing coref outputs (Python)
4. ~~Add hyperbolic embedding types alongside boxes~~ (DONE - see `anno/src/geometric/hyperbolic.rs`)

### Code Locations

Stub implementations are in `anno/src/geometric/`:

| Module | Description | Status |
|--------|-------------|--------|
| `geometric/mod.rs` | Unified traits (`GeometricMention`, `GeometricSpace`) | Implemented |
| `geometric/hyperbolic.rs` | Poincaré ball embeddings | Stub with tests |
| `geometric/sheaf.rs` | Sheaf neural network structures | Stub with tests |
| `geometric/tda.rs` | Persistent homology types | Stub with tests |

Integration points:
- `backends/box_embeddings.rs` — References geometric alternatives in docs
- `backends/graph_coref.rs` — References sheaf approach as future direction
- `lib.rs` — Exports `geometric` module with documentation

### Deferred Actions

1. Clean-room sheaf Laplacian implementation (after prototyping)
2. eRST parser integration (external tool)
3. Grokking-aware training (when Anno has learned models)

---

## External Resources

### Evaluation Cross-Validation

For validating Anno's coreference metrics against established scorers:

| Tool | Language | License | Metrics | Link |
|------|----------|---------|---------|------|
| **scorch** | Python | MIT | MUC, B³, CEAF-e, BLANC | [LoicGrobol/scorch](https://github.com/LoicGrobol/scorch) |
| **CoNLL-2012 scripts** | Python | - | Official scorer | [explosion/conll-2012](https://github.com/explosion/conll-2012) |
| **LEA scorer** | Perl | - | LEA metric | [ns-moosavi/LEA-coreference-scorer](https://github.com/ns-moosavi/LEA-coreference-scorer) |

Anno's `anno::eval::coref_metrics` should produce results consistent with these.

### TDA for NLP Resources

Comprehensive survey of 110+ papers on topological data analysis for NLP:

- **AwesomeTDA4NLP**: [AdaUchendu/AwesomeTDA4NLP](https://github.com/AdaUchendu/AwesomeTDA4NLP)
- Survey paper: arXiv:2411.10298 (2024)

Key relevant papers for coreference/attention:
- "Artificial text detection via examining the topology of attention maps" (EMNLP 2021)
- "Hallucination Detection in LLMs via Topological Divergence on Attention Graphs" (2025)
- "Topformer: Topology-aware authorship attribution" (ECAI 2024)

---

## References

### Sheaf Theory
- Bodnar et al. (2023): "Neural Sheaf Diffusion" — NeurIPS
  - **Apache 2.0 implementation**: https://github.com/twitter-research/neural-sheaf-diffusion
- Hansen & Ghrist (2019): "Toward a Spectral Theory of Cellular Sheaves" — arXiv:1808.01513
- koho library (AGPL): https://github.com/TheMesocarp/koho

### Hyperbolic Embeddings
- arXiv:2507.17787 (2025): "Hyperbolic Multi-Head Latent Attention"
- Nickel & Kiela (2017): "Poincaré Embeddings for Learning Hierarchical Representations"
  - **Reference implementation**: https://github.com/facebookresearch/poincare-embeddings (BSD)
- HazyResearch/hyperbolics: Mixed-curvature embeddings

### Topological Data Analysis
- arXiv:2206.15195: "Topological BERT"
- arXiv:2411.10298: "Unveiling Topological Structures in Text" (TDA4NLP survey)
- Chazal & Michel (2021): "An Introduction to Topological Data Analysis"
- Python libraries: giotto-tda, ripser, gudhi

### Discourse
- Computational Linguistics (2025): "eRST: A Signaled Graph Theory of Discourse"
- aclanthology.org/2025.cl-1.3

### Training Dynamics
- arXiv:2511.12768 (2025): "Grokking as a First-Order Phase Transition"

