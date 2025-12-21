# Clustering Architecture Design

**Implementation Status**: Core evidence types implemented in `coalesce/src/evidence.rs`.
Integrated with `StreamingResolver` via `StreamingConfig::with_evidence()`.
See also `../../research/HYPERGRAPH_EVIDENCE_DESIGN.md` for future hypergraph-based extensions.

## Current State Issues

### 1. No Signal Mediation

Currently, confidence signals are combined naively:
- Simple averaging: `(c1 + c2) / 2.0`
- First-wins: uses first track's confidence
- No source weighting, uncertainty propagation, or evidence accumulation

### 2. Transitivity Violation Handling

Pairwise similarity judgments often violate transitivity. Current approaches:

```
A ≈ B (0.9 similarity)
B ≈ C (0.8 similarity)
A ≈ C (0.3 similarity)  ← PROBLEM: Should A and C be in same cluster?
```

| Algorithm | Behavior |
|-----------|----------|
| Union-Find | Forces transitivity (A=B ∧ B=C → A=C regardless of A≈C) |
| Correlation | Minimizes disagreements, may keep A,C separate |
| Hierarchical | Depends on linkage (single chains, complete splits) |

### 3. Algorithm Selection

No unified interface for choosing algorithms based on data characteristics.

---

## Proposed Design: Evidence-Based Mediation

### Core Concept: Evidence Accumulation

Instead of binary similarity, accumulate **evidence** for and against coreference:

```rust
/// Evidence for/against a pair being coreferent
pub struct PairEvidence {
    /// Sources that produced this evidence
    pub sources: Vec<EvidenceSource>,
    /// Positive evidence score (sum of confidences favoring coreference)
    pub positive: f32,
    /// Negative evidence score (sum of confidences against coreference)
    pub negative: f32,
    /// Feature-level contributions
    pub features: HashMap<String, f32>,
}

pub enum EvidenceSource {
    StringSimilarity { method: String, score: f32 },
    Embedding { model: String, score: f32 },
    TypeMatch { matched: bool, type_a: String, type_b: String },
    KnowledgeBase { kb_id: Option<String>, linked: bool },
    ContextualCoref { model: String, score: f32 },
}
```

### Mediation Strategies

```rust
pub enum MediationStrategy {
    /// Simple majority voting
    Voting,
    /// Weighted by source reliability
    SourceWeighted { weights: HashMap<String, f32> },
    /// Bayesian combination with priors
    Bayesian { prior: f32 },
    /// Learned combination (if training data available)
    Learned { model: Box<dyn MediationModel> },
}

pub trait MediationModel {
    fn combine(&self, evidence: &PairEvidence) -> f32;
}
```

### Handling Transitivity Violations

**Key insight**: Transitivity violations carry information about cluster structure.

```rust
/// Analyze transitivity patterns to inform clustering
pub struct TransitivityAnalyzer {
    edges: HashMap<(usize, usize), f32>,
}

impl TransitivityAnalyzer {
    /// Detect triangles with violated transitivity
    pub fn find_violations(&self, threshold: f32) -> Vec<TransitivityViolation> {
        // For each triple (a, b, c) where:
        //   sim(a,b) >= threshold
        //   sim(b,c) >= threshold
        //   sim(a,c) < threshold
        // This suggests cluster boundary between a and c
    }
    
    /// Score cluster quality by transitivity consistency
    pub fn transitivity_score(&self, clusters: &[Vec<usize>]) -> f32 {
        // Count violated triangles within clusters
        // Lower is better
    }
}
```

### Unified Clustering Interface

```rust
pub trait ClusteringAlgorithm {
    /// Cluster items given pairwise similarities
    fn cluster(
        &self,
        n: usize,
        similarities: impl Fn(usize, usize) -> f32,
    ) -> ClusteringResult;
    
    /// Algorithm characteristics for auto-selection
    fn characteristics(&self) -> AlgorithmCharacteristics;
}

pub struct AlgorithmCharacteristics {
    pub complexity: Complexity,           // O(n²), O(n log n), etc.
    pub handles_noise: bool,              // DBSCAN-like
    pub respects_transitivity: bool,      // Union-find: yes, correlation: no
    pub deterministic: bool,              // Hierarchical: yes, pivot: no
    pub memory_bounded: bool,             // Streaming: yes
    pub requires_threshold: bool,         // Union-find: yes, hierarchical: no
}

/// Auto-select algorithm based on data characteristics
pub fn select_algorithm(
    n: usize,
    is_streaming: bool,
    similarity_variance: f32,
) -> Box<dyn ClusteringAlgorithm> {
    match (n, is_streaming, similarity_variance > 0.3) {
        (_, true, _) => Box::new(StreamingResolver::default()),
        (n, _, _) if n > 10_000 => Box::new(LSHBlockedResolver::default()),
        (_, _, true) => Box::new(CorrelationClusterer::modified_pivot()),
        _ => Box::new(HierarchicalClusterer::average_linkage()),
    }
}
```

---

## Cluster Confidence Model

### Cluster-Level Confidence

```rust
pub struct ClusterConfidence {
    /// Overall cluster coherence (internal similarity)
    pub coherence: f32,
    /// Separation from nearest cluster
    pub separation: f32,
    /// Transitivity consistency within cluster
    pub transitivity: f32,
    /// Evidence strength (weighted sum of contributing evidence)
    pub evidence_strength: f32,
}

impl ClusterConfidence {
    pub fn aggregate(&self) -> f32 {
        // Geometric mean to penalize any low component
        (self.coherence * self.separation * self.transitivity * self.evidence_strength).powf(0.25)
    }
}
```

### Per-Mention Confidence in Cluster

```rust
pub struct MembershipConfidence {
    /// Member's similarity to cluster centroid
    pub centroid_similarity: f32,
    /// Member's average similarity to other members
    pub avg_internal_similarity: f32,
    /// Member's max similarity to other clusters
    pub max_external_similarity: f32,
    /// Confidence that this member belongs in this cluster
    pub membership_score: f32,
}
```

---

## Implementation Plan

### Phase 1: Evidence Model (Week 1)
- [ ] Define `PairEvidence` and `EvidenceSource` types
- [ ] Implement evidence accumulation in resolver
- [ ] Add source tracking to similarity computations

### Phase 2: Mediation (Week 2)
- [ ] Implement `MediationStrategy` enum
- [ ] Add `SourceWeighted` with configurable weights
- [ ] Implement `Bayesian` combination

### Phase 3: Transitivity Analysis (Week 3)
- [ ] Implement `TransitivityAnalyzer`
- [ ] Add violation detection to clustering
- [ ] Use violations to inform cluster boundaries

### Phase 4: Unified Interface (Week 4)
- [ ] Define `ClusteringAlgorithm` trait
- [ ] Wrap existing algorithms in trait
- [ ] Implement auto-selection based on characteristics

---

## References

- Bansal, Blum, Chawla (2004). "Correlation Clustering"
- Behnezhad et al. (2025). "Breaking the 3-approximation barrier"
- Dempster-Shafer theory for evidence combination
- Ensemble clustering literature

