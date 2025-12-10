//! Sheaf neural networks for gradient-level transitivity enforcement.
//!
//! Standard coreference models produce pairwise scores that can violate transitivity:
//!
//! ```text
//! P(A~B) = 0.9, P(B~C) = 0.9, P(A~C) = 0.1  ← Inconsistent!
//! ```
//!
//! Sheaf neural networks address this by encoding transitivity in the loss function,
//! not as a post-hoc constraint.
//!
//! # Mathematical Background
//!
//! A **cellular sheaf** on a graph G = (V, E) assigns:
//! - A vector space F(v) to each node v (the "stalk")
//! - A linear map F(u→v): F(u) → F(v) to each edge (the "restriction map")
//!
//! For coreference:
//! - **Nodes** = entity mentions
//! - **Edges** = candidate coreference links
//! - **Stalks** = mention embedding spaces
//! - **Restriction maps** = learned transformations between mention representations
//!
//! # The Sheaf Dirichlet Energy
//!
//! The key insight: transitivity emerges from minimizing the **sheaf Dirichlet energy**:
//!
//! ```text
//! E(x) = Σ_{(u,v) ∈ E} || F(u→v) · x_u - F(v→u) · x_v ||²
//! ```
//!
//! If A~B and B~C with compatible restriction maps, then the energy is only low
//! if A~C is also consistent—transitivity is enforced at the gradient level.
//!
//! # Comparison with Current Anno Approaches
//!
//! | Approach | Transitivity | When Applied |
//! |----------|-------------|--------------|
//! | Union-Find | Hard closure | Post-hoc clustering |
//! | Graph Coref | Heuristic bonus | Each refinement round |
//! | Box containment | Geometric | Inference time |
//! | **Sheaf energy** | **Gradient-level** | **Training time** |
//!
//! # Relationship to Other Anno Modules
//!
//! This module connects to several existing Anno abstractions:
//!
//! - **Hypergraph Evidence** (`docs/HYPERGRAPH_EVIDENCE_DESIGN.md`):
//!   The `SheafEvidenceGraph` trait extends `EvidenceGraph` with restriction maps.
//!   Scalar hyperedge weights become learned linear transformations.
//!
//! - **Box Embeddings** (`backends/box_embeddings.rs`):
//!   Complementary, not competing. Boxes model uncertainty/temporal evolution;
//!   sheaves model transitivity constraints.
//!
//! - **Graph Coref** (`backends/graph_coref.rs`):
//!   Iterative refinement approximates sheaf diffusion heuristically.
//!   Sheaf NN would replace the transitivity bonus with gradient-level enforcement.
//!
//! # External Projects
//!
//! - **box-coref** (separate repo): Training infrastructure that could use sheaf losses
//! - **subsume** (separate repo): Pure geometry library, no sheaf concepts
//!
//! # Dependencies: What We Use vs What We Don't
//!
//! **Uses only standard library + serde**:
//! - `Vec<f32>` for embeddings and restriction map weights
//! - `HashMap` for graph structure
//! - `serde` for serialization (optional persistence)
//!
//! **Does NOT depend on**:
//! - `subsume` (separate pure-geometry library)
//! - `candle` / `ndarray` / tensor libraries (GPU ops are feature-gated stubs)
//! - External ML frameworks
//!
//! This keeps the module self-contained and lightweight. The trade-off:
//! no GPU acceleration until Candle feature is implemented.
//!
//! # Implementation Status: STUB
//!
//! This module provides trait definitions and placeholder implementations.
//! Uses `Vec<f32>` for prototyping. Full implementation requires:
//!
//! - Candle tensors for GPU acceleration (feature-gated)
//! - Port from `twitter-research/neural-sheaf-diffusion` (Apache 2.0)
//!
//! See `docs/GEOMETRIC_FOUNDATIONS.md` for the implementation roadmap.
//!
//! # References
//!
//! - Bodnar et al. (2023): "Neural Sheaf Diffusion" — NeurIPS
//! - Hansen & Ghrist (2019): "Toward a Spectral Theory of Cellular Sheaves"
//! - Kehler (1997): "Probabilistic Coreference in Information Extraction" — ACL
//! - Reference implementation: <https://github.com/twitter-research/neural-sheaf-diffusion>

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Core Types
// ============================================================================

/// Configuration for sheaf diffusion layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheafDiffusionConfig {
    /// Dimension of the stalk spaces (mention embedding dimension).
    pub stalk_dim: usize,

    /// Dimension of the restriction maps (can differ from stalk_dim).
    pub restriction_dim: usize,

    /// Number of diffusion layers.
    pub num_layers: usize,

    /// Whether to include downward (node → edge) diffusion.
    pub include_down: bool,

    /// Regularization weight for the Dirichlet energy.
    pub energy_weight: f32,

    /// Learning rate for restriction map optimization.
    pub lr: f32,
}

impl Default for SheafDiffusionConfig {
    fn default() -> Self {
        Self {
            stalk_dim: 64,
            restriction_dim: 64,
            num_layers: 2,
            include_down: true,
            energy_weight: 0.1,
            lr: 0.001,
        }
    }
}

/// A restriction map between two stalks.
///
/// In coreference, this is a learned linear transformation that maps
/// one mention's representation to another's coordinate system.
///
/// If mentions are coreferent, their restriction maps should be compatible
/// (i.e., F(u→v) · x_u ≈ F(v→u) · x_v).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestrictionMap {
    /// Source node ID.
    pub source: usize,

    /// Target node ID.
    pub target: usize,

    /// The linear map as a flattened matrix (row-major).
    /// Shape: (restriction_dim, stalk_dim)
    pub weights: Vec<f32>,

    /// Input dimension (stalk_dim of source).
    pub in_dim: usize,

    /// Output dimension (restriction_dim).
    pub out_dim: usize,
}

impl RestrictionMap {
    /// Create a new restriction map with random initialization.
    pub fn new(source: usize, target: usize, in_dim: usize, out_dim: usize) -> Self {
        // Xavier initialization: scale by sqrt(2 / (in + out))
        let scale = (2.0 / (in_dim + out_dim) as f32).sqrt();
        let weights: Vec<f32> = (0..in_dim * out_dim)
            .map(|i| {
                // Deterministic pseudo-random based on indices
                let seed = (source * 1000 + target * 100 + i) as f32;
                ((seed * 0.6180339887).fract() - 0.5) * 2.0 * scale
            })
            .collect();

        Self {
            source,
            target,
            weights,
            in_dim,
            out_dim,
        }
    }

    /// Create an identity-like restriction map.
    pub fn identity(source: usize, target: usize, dim: usize) -> Self {
        let mut weights = vec![0.0; dim * dim];
        for i in 0..dim {
            weights[i * dim + i] = 1.0;
        }
        Self {
            source,
            target,
            weights,
            in_dim: dim,
            out_dim: dim,
        }
    }

    /// Apply the restriction map to a vector.
    ///
    /// Computes F(source→target) · x
    pub fn apply(&self, x: &[f32]) -> Vec<f32> {
        assert_eq!(
            x.len(),
            self.in_dim,
            "Input dimension mismatch: expected {}, got {}",
            self.in_dim,
            x.len()
        );

        let mut result = vec![0.0; self.out_dim];
        for i in 0..self.out_dim {
            for j in 0..self.in_dim {
                result[i] += self.weights[i * self.in_dim + j] * x[j];
            }
        }
        result
    }
}

/// A graph with sheaf structure for coreference resolution.
///
/// Nodes are entity mentions, edges are candidate coreference links.
/// Each edge has bidirectional restriction maps.
#[derive(Debug, Clone)]
pub struct SheafGraph {
    /// Number of nodes (mentions).
    pub num_nodes: usize,

    /// Node features: node_id → embedding vector.
    node_features: HashMap<usize, Vec<f32>>,

    /// Edges as (source, target) pairs.
    edges: Vec<(usize, usize)>,

    /// Restriction maps: edge_index → (forward_map, backward_map).
    restriction_maps: HashMap<usize, (RestrictionMap, RestrictionMap)>,

    /// Configuration.
    config: SheafDiffusionConfig,
}

impl SheafGraph {
    /// Create a new sheaf graph with the given configuration.
    pub fn new(config: SheafDiffusionConfig) -> Self {
        Self {
            num_nodes: 0,
            node_features: HashMap::new(),
            edges: Vec::new(),
            restriction_maps: HashMap::new(),
            config,
        }
    }

    /// Add a node with its feature vector.
    pub fn add_node(&mut self, id: usize, features: Vec<f32>) {
        assert_eq!(
            features.len(),
            self.config.stalk_dim,
            "Feature dimension must match stalk_dim"
        );
        self.node_features.insert(id, features);
        self.num_nodes = self.num_nodes.max(id + 1);
    }

    /// Add an edge with learned restriction maps.
    pub fn add_edge(&mut self, source: usize, target: usize) {
        let edge_idx = self.edges.len();
        self.edges.push((source, target));

        // Create bidirectional restriction maps
        let forward = RestrictionMap::new(
            source,
            target,
            self.config.stalk_dim,
            self.config.restriction_dim,
        );
        let backward = RestrictionMap::new(
            target,
            source,
            self.config.stalk_dim,
            self.config.restriction_dim,
        );

        self.restriction_maps.insert(edge_idx, (forward, backward));
    }

    /// Get the number of edges.
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Compute the sheaf Dirichlet energy.
    ///
    /// E(x) = Σ_{(u,v) ∈ E} || F(u→v) · x_u - F(v→u) · x_v ||²
    ///
    /// Lower energy = more consistent coreference predictions.
    pub fn dirichlet_energy(&self) -> f32 {
        let mut energy = 0.0;

        for (edge_idx, (source, target)) in self.edges.iter().enumerate() {
            let x_u = match self.node_features.get(source) {
                Some(f) => f,
                None => continue,
            };
            let x_v = match self.node_features.get(target) {
                Some(f) => f,
                None => continue,
            };

            let (forward, backward) = match self.restriction_maps.get(&edge_idx) {
                Some(maps) => maps,
                None => continue,
            };

            // F(u→v) · x_u
            let fu_xu = forward.apply(x_u);
            // F(v→u) · x_v
            let fv_xv = backward.apply(x_v);

            // || F(u→v) · x_u - F(v→u) · x_v ||²
            let diff_sq: f32 = fu_xu
                .iter()
                .zip(fv_xv.iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum();

            energy += diff_sq;
        }

        energy
    }

    /// Get coreference score for an edge based on restriction map consistency.
    ///
    /// High consistency (low local energy) = likely coreferent.
    pub fn edge_coref_score(&self, edge_idx: usize) -> Option<f32> {
        let (source, target) = self.edges.get(edge_idx)?;
        let x_u = self.node_features.get(source)?;
        let x_v = self.node_features.get(target)?;
        let (forward, backward) = self.restriction_maps.get(&edge_idx)?;

        let fu_xu = forward.apply(x_u);
        let fv_xv = backward.apply(x_v);

        let diff_sq: f32 = fu_xu
            .iter()
            .zip(fv_xv.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum();

        // Convert energy to score: high energy = low score
        // Using exponential: score = exp(-energy)
        Some((-diff_sq).exp())
    }
}

// ============================================================================
// Sheaf Diffusion Layer (Stub)
// ============================================================================

/// A sheaf diffusion layer that propagates information while respecting sheaf structure.
///
/// # Status: STUB
///
/// Full implementation requires:
/// - Hodge Laplacian computation
/// - Learnable layer weights
/// - Activation functions
///
/// Reference: `twitter-research/neural-sheaf-diffusion/models/`
pub struct SheafDiffusionLayer {
    /// Layer configuration.
    pub config: SheafDiffusionConfig,

    /// Layer index (for multi-layer networks).
    pub layer_idx: usize,
}

impl SheafDiffusionLayer {
    /// Create a new diffusion layer.
    pub fn new(config: SheafDiffusionConfig, layer_idx: usize) -> Self {
        Self { config, layer_idx }
    }

    /// Apply one step of sheaf diffusion.
    ///
    /// # Status: STUB
    ///
    /// In full implementation:
    /// 1. Compute Hodge Laplacian: L = B^T * diag(edge_weights) * B
    /// 2. Apply diffusion: X' = (I - αL) * X
    /// 3. Apply learned weights and activation
    pub fn forward(&self, _graph: &SheafGraph) -> SheafGraph {
        // TODO: Implement sheaf diffusion
        // See: twitter-research/neural-sheaf-diffusion/models/diffuse.py
        unimplemented!("Sheaf diffusion forward pass - see docs/GEOMETRIC_FOUNDATIONS.md")
    }
}

// ============================================================================
// Integration with Anno Coreference
// ============================================================================

/// Trait for coreference resolvers that can use sheaf structure.
///
/// # Future Use
///
/// ```rust,ignore
/// impl SheafCoref for MentionRankingCoref {
///     fn to_sheaf_graph(&self, mentions: &[Mention]) -> SheafGraph {
///         // Convert mention-pair scores to sheaf structure
///     }
///
///     fn refine_with_sheaf(&mut self, graph: &SheafGraph) {
///         // Use Dirichlet energy to adjust scores
///     }
/// }
/// ```
pub trait SheafCoref {
    /// Convert coreference structure to a sheaf graph.
    fn to_sheaf_graph(&self, config: &SheafDiffusionConfig) -> SheafGraph;

    /// Refine coreference scores using sheaf energy minimization.
    fn refine_with_sheaf(&mut self, graph: &SheafGraph) -> Result<(), SheafError>;

    /// Compute the transitivity violation score (sheaf Dirichlet energy).
    fn transitivity_energy(&self, config: &SheafDiffusionConfig) -> f32 {
        self.to_sheaf_graph(config).dirichlet_energy()
    }
}

/// Error type for sheaf operations.
#[derive(Debug, Clone)]
pub enum SheafError {
    /// Graph structure is invalid.
    InvalidGraph(String),
    /// Numerical error during computation.
    NumericalError(String),
    /// Configuration error.
    ConfigError(String),
}

impl std::fmt::Display for SheafError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGraph(msg) => write!(f, "Invalid sheaf graph: {}", msg),
            Self::NumericalError(msg) => write!(f, "Numerical error: {}", msg),
            Self::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for SheafError {}

// ============================================================================
// Candle Integration (Feature-Gated)
// ============================================================================

/// Candle-based sheaf operations for GPU acceleration.
///
/// When the `candle` feature is enabled, these functions use Candle tensors
/// for efficient GPU computation. Otherwise, they fall back to CPU `Vec<f32>`.
///
/// # Implementation Path
///
/// To implement full sheaf diffusion with Candle:
///
/// 1. **Port restriction maps to tensors**:
///    ```rust,ignore
///    use candle_core::{Device, Tensor};
///    
///    pub struct TensorRestrictionMap {
///        weights: Tensor,  // [out_dim, in_dim]
///    }
///    ```
///
/// 2. **Implement coboundary operator**:
///    Build the incidence matrix B where B[e, v] = ±1 if edge e touches node v.
///
/// 3. **Compute sheaf Laplacian**:
///    L = B^T * diag(restriction_maps) * B
///
/// 4. **Diffusion step**:
///    x' = x - τ * L * x
///
/// Reference: `twitter-research/neural-sheaf-diffusion/models/diffuse.py`
#[cfg(feature = "candle")]
pub mod candle_ops {
    //! Candle-based sheaf operations.
    //!
    //! This module is a placeholder for GPU-accelerated sheaf operations.
    //! See `twitter-research/neural-sheaf-diffusion` for reference implementation.

    /// Placeholder for Candle-based sheaf graph.
    ///
    /// Full implementation would store restriction maps as `candle_core::Tensor`.
    pub struct CandleSheafGraph {
        // TODO: Add Candle tensor storage
        // restriction_maps: HashMap<(usize, usize), Tensor>,
        // node_features: Tensor,  // [num_nodes, stalk_dim]
    }

    impl CandleSheafGraph {
        /// Compute sheaf Laplacian as a Candle tensor.
        ///
        /// # Status: STUB
        pub fn sheaf_laplacian(&self) -> Result<(), super::SheafError> {
            // TODO: Implement L = B^T * diag(F) * B
            unimplemented!("Candle sheaf Laplacian - see docs/GEOMETRIC_FOUNDATIONS.md")
        }

        /// Run one step of sheaf diffusion.
        ///
        /// # Status: STUB
        pub fn diffuse_step(&self, _tau: f32) -> Result<(), super::SheafError> {
            // TODO: Implement x' = x - τ * L * x
            unimplemented!("Candle sheaf diffusion - see docs/GEOMETRIC_FOUNDATIONS.md")
        }
    }
}

// ============================================================================
// Integration with Hypergraph Evidence
// ============================================================================

/// Connection to the hypergraph evidence framework.
///
/// The `SheafEvidenceGraph` trait from `docs/HYPERGRAPH_EVIDENCE_DESIGN.md`
/// extends the `EvidenceGraph` trait with sheaf structure. This is the
/// integration point for combining sheaf neural networks with hypergraph
/// evidence aggregation.
///
/// # Existing Evidence Infrastructure
///
/// Anno already has evidence combination in `anno-coalesce`:
///
/// - `anno_coalesce::evidence::EvidenceSource` — Multiple signal types
/// - `anno_coalesce::evidence::PairEvidence` — Accumulates pairwise evidence
/// - `anno_coalesce::evidence::MediationStrategy` — Combination strategies
///
/// These implement Dempster-Shafer style combination (Kehler 1997).
/// The sheaf approach would **extend** this by:
///
/// 1. Replacing scalar edge weights with learned restriction maps
/// 2. Adding gradient-level transitivity enforcement
/// 3. Computing Dirichlet energy as a global consistency measure
///
/// # Design
///
/// ```rust,ignore
/// // From HYPERGRAPH_EVIDENCE_DESIGN.md:
/// pub trait SheafEvidenceGraph: EvidenceGraph {
///     fn restriction_map(&self, edge: &HyperedgeRef<Self::NodeId>)
///         -> Option<&Tensor>;
///     fn sheaf_laplacian(&self) -> Tensor;
/// }
/// ```
///
/// The `SheafGraph` in this module implements the core sheaf operations.
/// To integrate with hypergraph evidence:
///
/// 1. `SheafGraph` provides the mathematical operations (Dirichlet energy, diffusion)
/// 2. `EvidenceGraph` (from hypergraph design) provides the annotation structure
/// 3. `anno_coalesce::evidence` provides the existing combination strategies
/// 4. `SheafEvidenceGraph` combines all three
///
/// # Implementation Status
///
/// - `SheafGraph`: Basic operations implemented (this module)
/// - `anno_coalesce::evidence`: Implemented (Dempster-Shafer combination)
/// - `EvidenceGraph`: Design documented (HYPERGRAPH_EVIDENCE_DESIGN.md)
/// - `SheafEvidenceGraph`: Not yet implemented (future work)
pub mod integration {
    //! Integration points with other Anno modules.

    /// Marker trait for types that can be upgraded to sheaf structure.
    ///
    /// Types that implement this can have their edge weights replaced
    /// with learned restriction maps.
    pub trait SheafUpgradeable {
        /// The dimension of node features (stalk dimension).
        fn stalk_dim(&self) -> usize;

        /// Number of edges that would become restriction maps.
        fn num_edges(&self) -> usize;

        /// Get edge endpoints for constructing sheaf structure.
        fn edge_endpoints(&self) -> Vec<(usize, usize)>;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_restriction_map_identity() {
        let map = RestrictionMap::identity(0, 1, 4);
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y = map.apply(&x);

        for (a, b) in x.iter().zip(y.iter()) {
            assert!((a - b).abs() < 1e-5, "Identity map should preserve input");
        }
    }

    #[test]
    fn test_restriction_map_dimensions() {
        let map = RestrictionMap::new(0, 1, 8, 4);
        assert_eq!(map.in_dim, 8);
        assert_eq!(map.out_dim, 4);

        let x = vec![1.0; 8];
        let y = map.apply(&x);
        assert_eq!(y.len(), 4);
    }

    #[test]
    fn test_sheaf_graph_creation() {
        let config = SheafDiffusionConfig::default();
        let mut graph = SheafGraph::new(config.clone());

        // Add nodes with features
        graph.add_node(0, vec![0.1; config.stalk_dim]);
        graph.add_node(1, vec![0.2; config.stalk_dim]);
        graph.add_node(2, vec![0.3; config.stalk_dim]);

        // Add edges
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);

        assert_eq!(graph.num_nodes, 3);
        assert_eq!(graph.num_edges(), 2);
    }

    #[test]
    fn test_dirichlet_energy_computed() {
        let config = SheafDiffusionConfig {
            stalk_dim: 4,
            restriction_dim: 4,
            ..Default::default()
        };
        let mut graph = SheafGraph::new(config.clone());

        // Add identical nodes (should have low energy with identity maps)
        let features = vec![0.5; config.stalk_dim];
        graph.add_node(0, features.clone());
        graph.add_node(1, features.clone());

        graph.add_edge(0, 1);

        let energy = graph.dirichlet_energy();
        // Energy should be finite
        assert!(energy.is_finite(), "Energy should be finite: {}", energy);
    }

    #[test]
    fn test_edge_coref_score() {
        let config = SheafDiffusionConfig {
            stalk_dim: 4,
            restriction_dim: 4,
            ..Default::default()
        };
        let mut graph = SheafGraph::new(config.clone());

        graph.add_node(0, vec![0.5; config.stalk_dim]);
        graph.add_node(1, vec![0.5; config.stalk_dim]);
        graph.add_edge(0, 1);

        let score = graph.edge_coref_score(0);
        assert!(score.is_some());
        let score = score.unwrap();
        assert!(
            score >= 0.0 && score <= 1.0,
            "Score should be in [0, 1]: {}",
            score
        );
    }
}
