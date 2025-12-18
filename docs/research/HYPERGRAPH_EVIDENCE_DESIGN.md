# Hypergraph-Based Evidence Aggregation for Entity Resolution

**Based on research survey: December 2025**

## Historical Context

The foundations of probabilistic evidence combination for entity resolution trace back to
**Kehler (1997)**, "Probabilistic Coreference in Information Extraction" (ACL). Kehler observed
that pairwise coreference probabilities can be globally inconsistent—if P(A~D)=0.505 and
P(C~D)=0.504, but A and C are type-incompatible, we can't satisfy both.

His solution: treat pairwise probabilities as **mass distributions** over configuration subsets,
then combine using **Dempster's Rule of Combination** (Dempster 1968). This normalizes away
conflicting mass rather than averaging, yielding a coherent distribution over full partitions.

```
Kehler's key equation (from the paper):
m₃(Aₖ) = (1/(1-κ)) Σ_{Aᵢ∩Aⱼ=Aₖ} m₁(Aᵢ)·m₂(Aⱼ)
where κ = Σ_{Aᵢ∩Aⱼ=∅} m₁(Aᵢ)·m₂(Aⱼ) is the conflict mass
```

This document extends Kehler's foundation to **hypergraph** structures (n-ary evidence) and
modern **higher-order spectral methods**, while preserving the core insight: evidence
combination requires principled handling of inconsistency.

---

## Key Research Findings

### 1. Beyond Pairwise: Hypergraph Entity Resolution

Traditional correlation clustering operates on **pairwise** edges. Recent research shows significant benefits from **hypergraph** formulations:

- **Hypergraph Stochastic Block Model** (Stephan & Zhu 2022): Spectral methods for community detection in sparse hypergraphs
- **Simplicial Complexes** (Krishnagopal & Bianconi 2021): Hodge Laplacians for detecting higher-order communities
- **1.73-approximation** (Cohen-Addad et al. 2023): Set-based rounding complements pairwise approaches

**Key insight**: Entity resolution naturally involves n-ary relationships:
- "These 5 mentions all refer to Barack Obama"
- "This transaction involves parties A, B, and C"
- "These records share this address cluster"

### 2. Evidence as Annotated Hyperedges

Instead of pairwise similarity scores, model evidence as **annotated hyperedges**:

```
Hyperedge: {mention_1, mention_3, mention_7}
Annotations:
  - source: "embedding_cluster"
  - confidence: 0.87
  - type: "same_entity"
  - features: {centroid_distance: 0.12, type_match: true}
```

This generalizes:
- **Pairwise edges**: |hyperedge| = 2
- **Set annotations**: |hyperedge| ≥ 2
- **Global annotations**: |hyperedge| = n (all entities)

### 3. Dempster-Shafer Belief Aggregation

For combining conflicting evidence, Dempster-Shafer theory provides:

```
m(A) = m₁(A) ⊕ m₂(A) = (1/(1-K)) Σ_{B∩C=A} m₁(B)·m₂(C)
```

Key advantages:
- Handles **evidential balance** (likelihood) vs **evidential weight** (credibility)
- **Correlation belief functions** (2023) resolve counterintuitive fusion with conflicts
- Natural for NLP where sources have varying reliability

**Historical connection**: Kehler (1997) was the first to apply Dempster-Shafer to NLP
coreference. He showed that iteratively combining pairwise mass distributions via Dempster's
rule is equivalent to:
1. Multiplying pairwise probabilities for each configuration
2. Normalizing by the mass assigned to impossible configurations

This elegant simplification makes the approach tractable even for moderately large coreference
sets.

### 4. Higher-Order Spectral Clustering

**Hypergraph Laplacians** enable spectral methods on n-ary relations:

- **Non-backtracking operators** (Chodrow et al. 2022): Better community detection than standard approaches
- **Ricci curvature** (Hacquard 2024): Novel edge transport for hypergraph clustering
- **Hodge decomposition**: Identifies gradient, curl, harmonic components in simplicial complexes

### 5. Ensemble Methods for Evidence Combination

Co-association matrices aggregate multiple clustering signals:

- **High-order consistency** (Gan et al. 2024): Quality-aware base cluster weighting
- **Similarity + Dissimilarity** (Zhang et al. 2024): Both positive and negative signals
- **Multi-granularity link analysis**: Hierarchical evidence aggregation

---

## Proposed API Design

### Core Abstraction: Evidence Graph

```rust
/// Evidence about entity relationships - can be pairwise or higher-order.
pub trait EvidenceGraph {
    type NodeId: Copy + Eq + Hash;
    type Annotation: Clone;
    
    /// Number of entities in the evidence graph
    fn num_nodes(&self) -> usize;
    
    /// Iterate over all hyperedges with their annotations
    fn hyperedges(&self) -> impl Iterator<Item = (HyperedgeRef<Self::NodeId>, &Self::Annotation)>;
    
    /// Get hyperedges containing a specific node
    fn hyperedges_containing(&self, node: Self::NodeId) 
        -> impl Iterator<Item = (HyperedgeRef<Self::NodeId>, &Self::Annotation)>;
    
    /// Get hyperedges of a specific arity (2 = pairwise, 3 = triples, etc.)
    fn hyperedges_of_arity(&self, arity: usize) 
        -> impl Iterator<Item = (HyperedgeRef<Self::NodeId>, &Self::Annotation)>;
}

/// Reference to a hyperedge (set of nodes)
pub struct HyperedgeRef<'a, N> {
    nodes: &'a [N],
}

impl<'a, N> HyperedgeRef<'a, N> {
    pub fn arity(&self) -> usize { self.nodes.len() }
    pub fn is_pairwise(&self) -> bool { self.nodes.len() == 2 }
    pub fn nodes(&self) -> &[N] { self.nodes }
}
```

### Evidence Annotation Types

```rust
/// Rich annotation for a hyperedge
#[derive(Debug, Clone)]
pub struct HyperedgeAnnotation {
    /// Source that produced this evidence
    pub source: EvidenceSource,
    /// Type of relationship claimed
    pub relation: RelationType,
    /// Confidence/belief mass
    pub belief: BeliefMass,
    /// Optional feature vector for this evidence
    pub features: Option<Vec<f32>>,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

/// Relationship types for hyperedges
#[derive(Debug, Clone)]
pub enum RelationType {
    /// All nodes in hyperedge refer to same entity
    SameEntity,
    /// All nodes in hyperedge are different entities
    DifferentEntities,
    /// Nodes form a related cluster (weaker than same entity)
    RelatedCluster { similarity: f32 },
    /// Nodes share an attribute (e.g., same address)
    SharedAttribute { attribute: String, value: String },
    /// Nodes appear in same context (document, transaction, etc.)
    CoOccurrence { context_type: String },
    /// Custom relation
    Custom(String),
}

/// Dempster-Shafer style belief mass
#[derive(Debug, Clone)]
pub struct BeliefMass {
    /// Belief in the hypothesis (evidence supports it)
    pub belief: f32,
    /// Plausibility (1 - disbelief)
    pub plausibility: f32,
    /// Mass assigned to this specific hypothesis
    pub mass: f32,
}

impl BeliefMass {
    /// Create from simple confidence score
    pub fn from_confidence(conf: f32) -> Self {
        Self {
            belief: conf,
            plausibility: 1.0, // No evidence against
            mass: conf,
        }
    }
    
    /// Combine with another belief mass (Dempster's rule)
    pub fn combine(&self, other: &BeliefMass) -> Self {
        // Simplified Dempster combination
        let k = self.mass * (1.0 - other.plausibility) 
              + other.mass * (1.0 - self.plausibility);
        let normalizer = 1.0 - k;
        
        if normalizer < 0.001 {
            // Total conflict - return uncertain
            Self { belief: 0.5, plausibility: 0.5, mass: 0.0 }
        } else {
            let combined_mass = (self.mass * other.mass) / normalizer;
            Self {
                belief: combined_mass,
                plausibility: 1.0 - (1.0 - self.plausibility) * (1.0 - other.plausibility),
                mass: combined_mass,
            }
        }
    }
}
```

### Concrete Implementation

```rust
/// Sparse hypergraph with weighted, annotated hyperedges
pub struct SparseHypergraph<N = usize, A = HyperedgeAnnotation> {
    num_nodes: usize,
    /// Hyperedges stored as (sorted node list, annotation)
    hyperedges: Vec<(Vec<N>, A)>,
    /// Index: node -> hyperedge indices containing it
    node_index: HashMap<N, Vec<usize>>,
    /// Index: arity -> hyperedge indices of that arity
    arity_index: HashMap<usize, Vec<usize>>,
}

impl<N: Ord + Copy + Hash, A: Clone> SparseHypergraph<N, A> {
    pub fn new(num_nodes: usize) -> Self { ... }
    
    /// Add a hyperedge (set of nodes with annotation)
    pub fn add_hyperedge(&mut self, nodes: impl IntoIterator<Item = N>, annotation: A) {
        let mut nodes: Vec<N> = nodes.into_iter().collect();
        nodes.sort();
        nodes.dedup();
        
        let idx = self.hyperedges.len();
        let arity = nodes.len();
        
        for &node in &nodes {
            self.node_index.entry(node).or_default().push(idx);
        }
        self.arity_index.entry(arity).or_default().push(idx);
        self.hyperedges.push((nodes, annotation));
    }
    
    /// Get all pairwise projections (for algorithms that need pairwise input)
    pub fn to_pairwise(&self) -> Vec<(N, N, A)> 
    where A: Clone {
        let mut pairs = Vec::new();
        for (nodes, ann) in &self.hyperedges {
            for i in 0..nodes.len() {
                for j in (i+1)..nodes.len() {
                    pairs.push((nodes[i], nodes[j], ann.clone()));
                }
            }
        }
        pairs
    }
    
    /// Build incidence matrix for spectral methods
    pub fn incidence_matrix(&self) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        // Returns (node-hyperedge incidence, hyperedge weights)
        ...
    }
}
```

### Higher-Order Clustering Algorithms

```rust
/// Trait for clustering algorithms that can handle hypergraphs
pub trait HypergraphClusterer {
    type NodeId;
    type Annotation;
    
    /// Cluster nodes given hypergraph evidence
    fn cluster(
        &self,
        graph: &impl EvidenceGraph<NodeId = Self::NodeId, Annotation = Self::Annotation>,
    ) -> ClusteringResult<Self::NodeId>;
}

/// Spectral clustering on hypergraphs using non-backtracking operator
pub struct NonBacktrackingSpectral {
    pub num_clusters: Option<usize>,
    pub use_bethe_hessian: bool,
}

/// Hypergraph correlation clustering (generalizes pairwise)
pub struct HypergraphCorrelation {
    /// Algorithm variant
    pub algorithm: HypergraphCorrelationAlgorithm,
}

pub enum HypergraphCorrelationAlgorithm {
    /// Reduce to pairwise and use standard correlation clustering
    PairwiseReduction,
    /// Use hyperedge-aware pivot selection
    HypergraphPivot,
    /// Set-based rounding (Cohen-Addad et al. 2023)
    SetBasedRounding,
    /// Preclustering + rounding combination
    PreclusteringRounding,
}

/// Belief-propagation based clustering
pub struct BeliefPropagationClusterer {
    pub max_iterations: usize,
    pub damping: f32,
    pub convergence_threshold: f32,
}
```

### Evidence Aggregation Strategies

```rust
/// Strategy for aggregating evidence from multiple hyperedges
pub trait EvidenceAggregator {
    type Annotation;
    
    /// Aggregate evidence for whether nodes should be in same cluster
    fn aggregate_for_clustering(
        &self,
        node_pair: (usize, usize),
        evidence: &[&Self::Annotation],
    ) -> ClusteringEvidence;
}

pub struct ClusteringEvidence {
    /// Aggregated belief that nodes are same entity
    pub same_entity: BeliefMass,
    /// Aggregated belief that nodes are different entities
    pub different_entities: BeliefMass,
    /// Sources that contributed
    pub sources: Vec<String>,
}

/// Dempster-Shafer aggregator
pub struct DempsterShaferAggregator;

impl EvidenceAggregator for DempsterShaferAggregator {
    type Annotation = HyperedgeAnnotation;
    
    fn aggregate_for_clustering(
        &self,
        _node_pair: (usize, usize),
        evidence: &[&HyperedgeAnnotation],
    ) -> ClusteringEvidence {
        let mut same = BeliefMass::from_confidence(0.5);
        let mut diff = BeliefMass::from_confidence(0.5);
        let mut sources = Vec::new();
        
        for ann in evidence {
            sources.push(ann.source.source_name().to_string());
            match &ann.relation {
                RelationType::SameEntity => {
                    same = same.combine(&ann.belief);
                }
                RelationType::DifferentEntities => {
                    diff = diff.combine(&ann.belief);
                }
                RelationType::RelatedCluster { similarity } => {
                    let weak_belief = BeliefMass::from_confidence(*similarity * 0.5);
                    same = same.combine(&weak_belief);
                }
                _ => {}
            }
        }
        
        ClusteringEvidence { same_entity: same, different_entities: diff, sources }
    }
}

/// Weighted voting aggregator
pub struct WeightedVotingAggregator {
    pub source_weights: HashMap<String, f32>,
    pub default_weight: f32,
}

/// Neural aggregator (learned combination)
pub struct LearnedAggregator {
    pub model_path: PathBuf,
    // ... neural network for learned aggregation
}
```

---

## Comparison with Current Implementation

| Aspect | Current (`evidence.rs`) | Proposed Hypergraph |
|--------|------------------------|---------------------|
| **Relations** | Pairwise only | n-ary hyperedges |
| **Aggregation** | Simple averaging/voting | Dempster-Shafer + learned |
| **Transitivity** | Analyzer identifies violations | Natural in hypergraph structure |
| **Algorithms** | Correlation clustering | Hypergraph spectral + correlation |
| **Scalability** | O(n²) pairwise | Sparse hyperedge representation |

---

## Implementation Roadmap

### Phase 1: Hyperedge Data Model (Week 1)
- [ ] Define `HyperedgeRef`, `HyperedgeAnnotation` types
- [ ] Implement `SparseHypergraph` storage
- [ ] Add pairwise projection for backward compatibility

### Phase 2: Belief Aggregation (Week 2)
- [ ] Implement Dempster-Shafer `BeliefMass::combine`
- [ ] Add `DempsterShaferAggregator`
- [ ] Correlation belief function for conflict handling

### Phase 3: Hypergraph Algorithms (Week 3-4)
- [ ] Non-backtracking spectral clustering
- [ ] Hypergraph-aware pivot selection
- [ ] Set-based rounding (following Cohen-Addad et al.)

### Phase 4: Integration (Week 5)
- [ ] Connect to existing `StreamingResolver`
- [ ] Add hyperedge evidence to `Resolver`
- [ ] Evaluation on multi-source datasets

---

---

## Connection to Sheaf Neural Networks

The hypergraph formulation naturally extends to **cellular sheaves** (see [`GEOMETRIC_FOUNDATIONS.md`](../design/embeddings/GEOMETRIC_FOUNDATIONS.md)):

| Hypergraph Concept | Sheaf Generalization |
|--------------------|---------------------|
| Hyperedge weight (scalar) | Restriction map (matrix) |
| Evidence aggregation | Sheaf diffusion |
| Transitivity via hyperedges | Sheaf Laplacian energy |

The sheaf formulation provides:
1. **Learned transformations**: Edge weights become linear maps
2. **Energy-based consistency**: Transitivity emerges from minimizing sheaf Dirichlet energy
3. **Gradient-level enforcement**: Consistency is in the loss, not post-hoc

For future work, the `EvidenceGraph` trait could be extended to support sheaf structure:

```rust
/// Extension for sheaf-valued evidence
pub trait SheafEvidenceGraph: EvidenceGraph {
    /// Get restriction map for edge (learned linear transformation)
    fn restriction_map(&self, edge: &HyperedgeRef<Self::NodeId>) 
        -> Option<&Tensor>;
    
    /// Compute sheaf Laplacian for spectral clustering
    fn sheaf_laplacian(&self) -> Tensor;
}
```

---

## References

1. Cohen-Addad, Lee, Li, Newman (2023). "Handling Correlated Rounding Error via Preclustering: A 1.73-approximation for Correlation Clustering"
2. Chodrow, Eikmeier, Haddock (2022). "Nonbacktracking spectral clustering of nonuniform hypergraphs"
3. Krishnagopal, Bianconi (2021). "Spectral Detection of Simplicial Communities via Hodge Laplacians"
4. Stephan, Zhu (2022). "Sparse random hypergraphs: Non-backtracking spectra and community detection"
5. Gan et al. (2024). "Clustering ensemble algorithm with high-order consistency learning"
6. Hacquard (2024). "Hypergraph clustering using Ricci curvature"
7. Dempster-Shafer theory extensions (2023). "Correlation belief function" for conflict resolution
8. Bodnar et al. (2023). "Neural Sheaf Diffusion" - NeurIPS
9. Hansen & Ghrist (2019). "Toward a Spectral Theory of Cellular Sheaves" - arXiv:1808.01513

