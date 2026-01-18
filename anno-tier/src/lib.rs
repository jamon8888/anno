//! # anno-tier
//!
//! **Graph algorithms for any node/edge structure: centrality and community detection.**
//!
//! This crate implements graph-theoretic algorithms that operate on `GraphDocument`.
//! The key insight: **nodes can be anything** - entities, documents, sentences,
//! chunks, concepts, or events. The algorithms only see structure.
//!
//! ## What Can Be Nodes?
//!
//! | Node Type | Edge Type | Use Case |
//! |-----------|-----------|----------|
//! | Entities | Relations | Entity importance in knowledge graphs |
//! | Documents | Citations/Similarity | Influential document detection |
//! | Sentences | Similarity | Extractive summarization (LexRank) |
//! | Chunks | Embedding similarity | RAPTOR-style hierarchical RAG |
//! | Concepts | Ontology relations | Concept importance in taxonomies |
//! | Events | Causal/temporal | Pivotal event detection |
//!
//! ## Centrality: "Which nodes are structurally important?"
//!
//! | Algorithm | Question | Best For |
//! |-----------|----------|----------|
//! | [`PageRank`] | "Connected to important things?" | General importance |
//! | [`Eigenvector`] | "Connected to high-degree nodes?" | Simpler PageRank variant |
//! | [`Betweenness`] | "Bridges communities?" | Finding connectors |
//! | [`Closeness`] | "How quickly can I reach everyone?" | Information spread |
//! | [`Hits`] | "Hub or authority?" | Bipartite structures |
//!
//! ## Community Detection: "How do nodes cluster?"
//!
//! | Algorithm | Type | Best For |
//! |-----------|------|----------|
//! | [`HierarchicalLeiden`] | Modularity | Best quality, guarantees connected |
//! | [`Louvain`] | Modularity | Comparison baseline to Leiden |
//! | [`LabelPropagation`] | O(E) | Very fast, approximate |
//!
//! ## Design Principle
//!
//! Tier is **node-type agnostic**. It doesn't know if nodes are entities,
//! sentences, or chunks - it only sees the graph structure. This means:
//!
//! - Use the same `PageRank` for entity salience OR document influence
//! - Use the same `Leiden` for entity clustering OR chunk grouping
//! - Build `GraphDocument` however makes sense for your use case
//!
//! **Extract. Coalesce. Stratify.** — This crate implements the "Stratify" step.
//!
//! ---
//!
//! # The Community Detection Problem
//!
//! Given a graph \( G = (V, E) \), we seek a partition of vertices into communities
//! such that vertices within communities are densely connected, while connections
//! between communities are sparse. The quality is measured by **modularity**:
//!
//! \[
//! Q = \frac{1}{2m} \sum_{ij} \left[ A_{ij} - \gamma \frac{k_i k_j}{2m} \right] \delta(c_i, c_j)
//! \]
//!
//! Where:
//! - \( m \) = total edge weight
//! - \( A_{ij} \) = adjacency matrix
//! - \( k_i \) = degree of node \( i \)
//! - \( c_i \) = community assignment of node \( i \)
//! - \( \gamma \) = resolution parameter
//! - \( \delta \) = Kronecker delta (1 if same community)
//!
//! ---
//!
//! # Algorithms
//!
//! ## Leiden Algorithm
//!
//! An improvement over Louvain that guarantees **well-connected communities**.
//!
//! **Three phases:**
//! 1. **Local moving**: Nodes move to neighboring communities that improve modularity
//! 2. **Refinement**: Split communities to ensure connectivity
//! 3. **Aggregation**: Contract graph, nodes become communities, repeat
//!
//! **Complexity:** \( O(n \log n) \) expected for sparse graphs
//!
//! **Reference:** Traag, Waltman, van Eck (2019). "From Louvain to Leiden:
//! guaranteeing well-connected communities." Scientific Reports 9, 5233.
//!
//! ## Hierarchical Leiden
//!
//! Applies Leiden at multiple resolution parameters to reveal hierarchical structure:
//! - High resolution (\( \gamma > 1 \)): Many small communities
//! - Low resolution (\( \gamma < 1 \)): Few large communities
//!
//! The result is a **dendrogram of communities** annotated on the graph.
//!
//! ---
//!
//! # Relationship to `coalesce`
//!
//! | Aspect | `coalesce` | `tier` |
//! |--------|------------|----------|
//! | Input | Entity mentions | Knowledge graph |
//! | Output | Entity clusters | Community hierarchy |
//! | Algorithm | Union-Find, HAC, Correlation | Leiden, Louvain |
//! | Use case | Entity resolution | Graph summarization |
//!
//! **When to use which:**
//! - `coalesce`: Clustering text mentions (NER output) into entities
//! - `tier`: Finding communities in a constructed knowledge graph
//!
//! These are complementary:
//! 1. Extract entities with NER (`anno`)
//! 2. Cluster entity mentions (`coalesce`)
//! 3. Build knowledge graph with relations
//! 4. Discover communities (`tier`)
//!
//! ---
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use anno_tier::HierarchicalLeiden;
//! use anno_core::GraphDocument;
//!
//! // Build a knowledge graph
//! let mut graph = GraphDocument::new();
//! // ... add nodes and edges ...
//!
//! // Discover hierarchical community structure
//! let clusterer = HierarchicalLeiden::new()
//!     .with_resolution(1.0)  // Default resolution
//!     .with_levels(3);       // 3 levels of hierarchy
//!
//! let annotated_graph = clusterer.cluster(&graph)?;
//!
//! // Each node now has `level_0_community`, `level_1_community`, etc. properties
//! for node in &annotated_graph.nodes {
//!     if let Some(community) = node.properties.get("level_0_community") {
//!         println!("{} is in community {}", node.name, community);
//!     }
//! }
//! ```
//!
//! ---
//!
//! # Mathematical Background
//!
//! ## Modularity Maximization
//!
//! Finding the optimal partition is NP-hard. Leiden uses a greedy approach:
//!
//! 1. **Initialization**: Each node in its own community
//! 2. **Local search**: For each node, compute modularity gain from moving to
//!    each neighbor's community. Move if gain > 0.
//! 3. **Refinement**: Ensure communities are internally connected
//! 4. **Aggregation**: Build meta-graph, repeat until no improvement
//!
//! ## Resolution Parameter
//!
//! The resolution \( \gamma \) controls community granularity:
//!
//! \[
//! \Delta Q = \frac{w_{in}}{m} - \gamma \frac{k_{in} \cdot k_{out}}{(2m)^2}
//! \]
//!
//! - \( \gamma = 1 \): Standard modularity (default)
//! - \( \gamma > 1 \): Prefer smaller communities
//! - \( \gamma < 1 \): Prefer larger communities
//!
//! ---
//!
//! # References
//!
//! - Traag, Waltman, van Eck (2019). "From Louvain to Leiden: guaranteeing
//!   well-connected communities." Scientific Reports 9, 5233.
//! - Blondel et al. (2008). "Fast unfolding of communities in large networks."
//!   J. Stat. Mech. P10008.
//! - Newman & Girvan (2004). "Finding and evaluating community structure in networks."
//!   Phys. Rev. E 69, 026113.
//!
//! ---
//!
//! # Graph Utilities
//!
//! The [`graph_utils`] module provides tools for graph analysis and interoperability
//! with the [`petgraph`] crate:
//!
//! | Function | Purpose |
//! |----------|---------|
//! | [`count_connected_components`] | Number of weakly connected components |
//! | [`find_connected_components`] | List of nodes in each component |
//! | [`strongly_connected_components`] | SCCs via Tarjan's algorithm |
//! | [`shortest_distances`] | Dijkstra's shortest paths from a source |
//! | [`average_path_length`] | Mean shortest path (graph cohesion) |
//! | [`graph_diameter`] | Longest shortest path (∞ if disconnected) |
//! | [`node_eccentricities`] | Max distance from each node |
//! | [`GraphStats`] | Comprehensive statistics in one call |
//!
//! These utilities are valuable for understanding graph structure before
//! applying centrality or community detection algorithms.
//!
//! [`count_connected_components`]: graph_utils::count_connected_components
//! [`find_connected_components`]: graph_utils::find_connected_components
//! [`strongly_connected_components`]: graph_utils::strongly_connected_components
//! [`shortest_distances`]: graph_utils::shortest_distances
//! [`average_path_length`]: graph_utils::average_path_length
//! [`graph_diameter`]: graph_utils::graph_diameter
//! [`node_eccentricities`]: graph_utils::node_eccentricities
//! [`GraphStats`]: graph_utils::GraphStats
//! [`petgraph`]: https://docs.rs/petgraph

#![warn(missing_docs)]

pub mod centrality;
pub mod graph_utils;
pub mod label_propagation;
pub mod leiden;
pub mod louvain;
pub mod pagerank;

#[cfg(feature = "raptor")]
pub mod raptor;

// Re-export main types
pub use centrality::{Betweenness, Closeness, Eigenvector, Hits};
pub use graph_utils::{
    average_path_length, count_connected_components, find_connected_components, graph_diameter,
    node_eccentricities, shortest_distances, strongly_connected_components, GraphStats,
};
pub use label_propagation::LabelPropagation;
pub use louvain::Louvain;
pub use pagerank::PageRank;

#[cfg(feature = "raptor")]
pub use raptor::{build_raptor_tree, RaptorConfig, RaptorTree, TreeStats};

use anno_core::GraphDocument;

/// Hierarchical Leiden clustering.
///
/// This implements the Leiden algorithm for hierarchical community detection
/// in knowledge graphs, revealing tier of abstraction.
///
/// The algorithm is applied recursively at multiple resolutions to build
/// a hierarchical dendrogram of communities.
#[derive(Debug, Clone)]
pub struct HierarchicalLeiden {
    resolution: f32,
    levels: usize,
}

impl HierarchicalLeiden {
    /// Create a new hierarchical Leiden clusterer with default settings.
    pub fn new() -> Self {
        Self {
            resolution: 1.0,
            levels: 3,
        }
    }

    /// Set the resolution parameter for clustering.
    ///
    /// Higher resolution values lead to more, smaller communities.
    /// Lower values lead to fewer, larger communities.
    pub fn with_resolution(mut self, resolution: f32) -> Self {
        self.resolution = resolution;
        self
    }

    /// Set the number of hierarchical levels to compute.
    ///
    /// Each level represents a different granularity of community structure.
    pub fn with_levels(mut self, levels: usize) -> Self {
        self.levels = levels;
        self
    }

    /// Cluster a graph document using hierarchical Leiden algorithm.
    ///
    /// This implementation:
    /// 1. Builds a graph from the GraphDocument
    /// 2. Applies Leiden algorithm at multiple resolutions
    /// 3. Creates hierarchical community structure
    /// 4. Returns a new GraphDocument with community annotations
    ///
    /// # Errors
    ///
    /// Returns an error if the graph is empty or invalid.
    pub fn cluster(&self, graph: &GraphDocument) -> Result<GraphDocument, String> {
        use leiden::Leiden;

        // Apply Leiden algorithm at multiple resolutions for hierarchical structure
        let mut result = graph.clone();
        let mut all_communities = Vec::new();

        for level in 0..self.levels {
            let resolution = self.resolution * (2.0_f32.powi(level as i32));
            let leiden = Leiden::new().with_resolution(resolution);
            let communities = leiden.cluster(graph)?;
            all_communities.push((level, communities));
        }

        // Annotate nodes with community assignments at each level
        for (level, communities) in all_communities {
            for node in &mut result.nodes {
                if let Some(community_id) = communities.get(&node.id) {
                    node.properties
                        .insert(format!("level_{}_community", level), (*community_id).into());
                }
            }
        }

        Ok(result)
    }
}

impl Default for HierarchicalLeiden {
    fn default() -> Self {
        Self::new()
    }
}
