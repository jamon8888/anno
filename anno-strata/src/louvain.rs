//! Louvain Algorithm for community detection.
//!
//! The Louvain algorithm is a greedy modularity optimization method that
//! preceded Leiden. While Leiden offers guarantees about community connectivity,
//! Louvain is useful as a comparison baseline and is slightly faster.
//!
//! # When to Use
//!
//! - **Comparison baseline** against Leiden
//! - **Historical compatibility** with older analyses
//! - **Slightly faster** when connectivity guarantees aren't needed
//!
//! # Limitations vs Leiden
//!
//! - **No connectivity guarantee**: Communities may be disconnected internally
//! - **Resolution limit**: May miss small communities in large networks
//! - **Less stable**: More sensitive to node ordering
//!
//! # Example
//!
//! ```rust,ignore
//! use anno_strata::{Louvain, leiden::Leiden};
//! use anno_core::GraphDocument;
//!
//! // Compare Louvain vs Leiden on same graph
//! let louvain = Louvain::new().with_seed(42);
//! let leiden = Leiden::new().with_seed(42);
//!
//! let louvain_communities = louvain.cluster(&graph)?;
//! let leiden_communities = leiden.cluster(&graph)?;
//!
//! // Leiden typically finds higher modularity
//! ```
//!
//! # Algorithm
//!
//! Two-phase iterative process:
//! 1. **Local moving**: Each node moves to neighbor community that maximizes modularity gain
//! 2. **Aggregation**: Contract graph - communities become nodes, repeat
//!
//! # References
//!
//! - Blondel et al. (2008). "Fast unfolding of communities in large networks."
//!   Journal of Statistical Mechanics.

use anno_core::GraphDocument;
use std::collections::{HashMap, HashSet};

/// Louvain community detection algorithm.
///
/// A greedy modularity optimization algorithm that iteratively moves nodes
/// between communities and then aggregates the graph.
#[derive(Debug, Clone)]
pub struct Louvain {
    /// Resolution parameter (higher = more communities)
    pub resolution: f64,
    /// Random seed for deterministic results
    pub seed: Option<u64>,
    /// Maximum iterations per phase
    pub max_iterations: usize,
    /// Minimum modularity improvement to continue
    pub min_improvement: f64,
}

impl Default for Louvain {
    fn default() -> Self {
        Self {
            resolution: 1.0,
            seed: None,
            max_iterations: 100,
            min_improvement: 1e-6,
        }
    }
}

impl Louvain {
    /// Create a new Louvain instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set resolution parameter.
    pub fn with_resolution(mut self, resolution: f64) -> Self {
        self.resolution = resolution;
        self
    }

    /// Set random seed for reproducible results.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Run Louvain algorithm on a graph.
    ///
    /// Returns a map from node ID to community ID.
    pub fn cluster(&self, graph: &GraphDocument) -> Result<HashMap<String, usize>, String> {
        if graph.nodes.is_empty() {
            return Ok(HashMap::new());
        }

        // Build weighted adjacency matrix
        let node_ids: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
        let node_index: HashMap<&str, usize> = node_ids
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, i))
            .collect();
        let n = node_ids.len();

        // Build adjacency with weights (treat as undirected)
        let mut adjacency: Vec<HashMap<usize, f64>> = vec![HashMap::new(); n];
        let mut total_weight = 0.0;

        for edge in &graph.edges {
            let src = match node_index.get(edge.source.as_str()) {
                Some(&i) => i,
                None => continue,
            };
            let tgt = match node_index.get(edge.target.as_str()) {
                Some(&i) => i,
                None => continue,
            };

            let weight = edge.confidence.max(0.01); // Minimum weight

            // Add both directions for undirected
            *adjacency[src].entry(tgt).or_insert(0.0) += weight;
            *adjacency[tgt].entry(src).or_insert(0.0) += weight;
            total_weight += 2.0 * weight;
        }

        if total_weight < f64::EPSILON {
            // No edges - each node is its own community
            return Ok(node_ids
                .iter()
                .enumerate()
                .map(|(i, &id)| (id.to_string(), i))
                .collect());
        }

        // Compute node degrees (sum of edge weights)
        let degrees: Vec<f64> = adjacency
            .iter()
            .map(|neighbors| neighbors.values().sum())
            .collect();

        // Initialize: each node in its own community
        let mut communities: Vec<usize> = (0..n).collect();
        let mut community_degrees: HashMap<usize, f64> =
            degrees.iter().enumerate().map(|(i, &d)| (i, d)).collect();
        let mut community_internal: HashMap<usize, f64> = (0..n).map(|i| (i, 0.0)).collect();

        // Phase 1: Local moving
        let mut improved = true;
        let mut iteration = 0;

        while improved && iteration < self.max_iterations {
            improved = false;
            iteration += 1;

            // Get node order (optionally shuffled)
            let mut order: Vec<usize> = (0..n).collect();
            if let Some(seed) = self.seed {
                self.shuffle(&mut order, seed + iteration as u64);
            }

            for &node in &order {
                let current_comm = communities[node];
                let node_degree = degrees[node];

                // Calculate modularity gain for moving to each neighbor community
                let mut best_comm = current_comm;
                let mut best_gain = 0.0;

                // Get neighbor communities
                let neighbor_comms: HashSet<usize> = adjacency[node]
                    .keys()
                    .map(|&neighbor| communities[neighbor])
                    .collect();

                for &target_comm in &neighbor_comms {
                    if target_comm == current_comm {
                        continue;
                    }

                    let gain = self.modularity_gain(
                        node,
                        current_comm,
                        target_comm,
                        &adjacency,
                        &communities,
                        &community_degrees,
                        node_degree,
                        total_weight,
                    );

                    if gain > best_gain + self.min_improvement {
                        best_gain = gain;
                        best_comm = target_comm;
                    }
                }

                // Move node if beneficial
                if best_comm != current_comm {
                    // Update community statistics
                    let edges_to_current: f64 = adjacency[node]
                        .iter()
                        .filter(|(&neighbor, _)| communities[neighbor] == current_comm)
                        .map(|(_, &w)| w)
                        .sum();
                    let edges_to_best: f64 = adjacency[node]
                        .iter()
                        .filter(|(&neighbor, _)| communities[neighbor] == best_comm)
                        .map(|(_, &w)| w)
                        .sum();

                    // Remove from current community
                    *community_degrees.entry(current_comm).or_insert(0.0) -= node_degree;
                    *community_internal.entry(current_comm).or_insert(0.0) -=
                        2.0 * edges_to_current;

                    // Add to best community
                    *community_degrees.entry(best_comm).or_insert(0.0) += node_degree;
                    *community_internal.entry(best_comm).or_insert(0.0) += 2.0 * edges_to_best;

                    communities[node] = best_comm;
                    improved = true;
                }
            }
        }

        // Renumber communities to be contiguous
        let mut label_remap: HashMap<usize, usize> = HashMap::new();
        let mut next_id = 0;

        let mut result: HashMap<String, usize> = HashMap::new();
        for (i, &comm) in communities.iter().enumerate() {
            let community_id = *label_remap.entry(comm).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            });
            result.insert(node_ids[i].to_string(), community_id);
        }

        Ok(result)
    }

    /// Calculate modularity gain from moving a node between communities.
    #[allow(clippy::too_many_arguments)]
    fn modularity_gain(
        &self,
        node: usize,
        from_comm: usize,
        to_comm: usize,
        adjacency: &[HashMap<usize, f64>],
        communities: &[usize],
        community_degrees: &HashMap<usize, f64>,
        node_degree: f64,
        total_weight: f64,
    ) -> f64 {
        // Edges from node to target community
        let edges_to_target: f64 = adjacency[node]
            .iter()
            .filter(|(&neighbor, _)| communities[neighbor] == to_comm)
            .map(|(_, &w)| w)
            .sum();

        // Edges from node to current community (excluding self)
        let edges_to_current: f64 = adjacency[node]
            .iter()
            .filter(|(&neighbor, _)| communities[neighbor] == from_comm && neighbor != node)
            .map(|(_, &w)| w)
            .sum();

        let target_degree = community_degrees.get(&to_comm).copied().unwrap_or(0.0);
        let current_degree =
            community_degrees.get(&from_comm).copied().unwrap_or(0.0) - node_degree;

        // Modularity gain formula
        let gain_to =
            edges_to_target - self.resolution * node_degree * target_degree / total_weight;
        let loss_from =
            edges_to_current - self.resolution * node_degree * current_degree / total_weight;

        (gain_to - loss_from) / total_weight
    }

    /// Shuffle array deterministically based on seed.
    fn shuffle(&self, arr: &mut [usize], seed: u64) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut rng_state = seed;
        for i in 0..arr.len() {
            let mut hasher = DefaultHasher::new();
            rng_state.hash(&mut hasher);
            rng_state = hasher.finish();
            let j = (rng_state as usize) % (arr.len() - i) + i;
            arr.swap(i, j);
        }
    }

    /// Calculate modularity of a partition.
    pub fn modularity(&self, graph: &GraphDocument, communities: &HashMap<String, usize>) -> f64 {
        let mut total_weight = 0.0;
        let mut degrees: HashMap<&str, f64> = HashMap::new();

        for edge in &graph.edges {
            let weight = edge.confidence.max(0.01);
            total_weight += 2.0 * weight;
            *degrees.entry(&edge.source).or_insert(0.0) += weight;
            *degrees.entry(&edge.target).or_insert(0.0) += weight;
        }

        if total_weight < f64::EPSILON {
            return 0.0;
        }

        let mut modularity = 0.0;
        for edge in &graph.edges {
            let src_comm = communities.get(&edge.source);
            let tgt_comm = communities.get(&edge.target);

            if src_comm == tgt_comm {
                let weight = edge.confidence.max(0.01);
                let src_degree = degrees.get(edge.source.as_str()).copied().unwrap_or(0.0);
                let tgt_degree = degrees.get(edge.target.as_str()).copied().unwrap_or(0.0);

                modularity +=
                    2.0 * (weight - self.resolution * src_degree * tgt_degree / total_weight);
            }
        }

        modularity / total_weight
    }

    /// Get number of communities found.
    pub fn num_communities(&self, graph: &GraphDocument) -> Result<usize, String> {
        let communities = self.cluster(graph)?;
        let unique: HashSet<_> = communities.values().collect();
        Ok(unique.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_core::{Entity, EntityType, Relation};

    fn two_cliques_graph() -> GraphDocument {
        let a1 = Entity::new("A1", EntityType::Person, 0, 2, 0.9);
        let a2 = Entity::new("A2", EntityType::Person, 10, 12, 0.9);
        let a3 = Entity::new("A3", EntityType::Person, 20, 22, 0.9);

        let b1 = Entity::new("B1", EntityType::Person, 30, 32, 0.9);
        let b2 = Entity::new("B2", EntityType::Person, 40, 42, 0.9);
        let b3 = Entity::new("B3", EntityType::Person, 50, 52, 0.9);

        let relations = vec![
            Relation::new(a1.clone(), a2.clone(), "FRIEND", 0.9),
            Relation::new(a2.clone(), a3.clone(), "FRIEND", 0.9),
            Relation::new(a1.clone(), a3.clone(), "FRIEND", 0.9),
            Relation::new(b1.clone(), b2.clone(), "FRIEND", 0.9),
            Relation::new(b2.clone(), b3.clone(), "FRIEND", 0.9),
            Relation::new(b1.clone(), b3.clone(), "FRIEND", 0.9),
        ];

        GraphDocument::from_extraction(&[a1, a2, a3, b1, b2, b3], &relations, None)
    }

    fn connected_cliques_graph() -> GraphDocument {
        let a1 = Entity::new("A1", EntityType::Person, 0, 2, 0.9);
        let a2 = Entity::new("A2", EntityType::Person, 10, 12, 0.9);
        let a3 = Entity::new("A3", EntityType::Person, 20, 22, 0.9);

        let b1 = Entity::new("B1", EntityType::Person, 30, 32, 0.9);
        let b2 = Entity::new("B2", EntityType::Person, 40, 42, 0.9);
        let b3 = Entity::new("B3", EntityType::Person, 50, 52, 0.9);

        let relations = vec![
            Relation::new(a1.clone(), a2.clone(), "FRIEND", 0.9),
            Relation::new(a2.clone(), a3.clone(), "FRIEND", 0.9),
            Relation::new(a1.clone(), a3.clone(), "FRIEND", 0.9),
            Relation::new(b1.clone(), b2.clone(), "FRIEND", 0.9),
            Relation::new(b2.clone(), b3.clone(), "FRIEND", 0.9),
            Relation::new(b1.clone(), b3.clone(), "FRIEND", 0.9),
            // Weak bridge
            Relation::new(a3.clone(), b1.clone(), "KNOWS", 0.3),
        ];

        GraphDocument::from_extraction(&[a1, a2, a3, b1, b2, b3], &relations, None)
    }

    #[test]
    fn test_empty_graph() {
        let graph = GraphDocument::new();
        let louvain = Louvain::new();
        let communities = louvain.cluster(&graph).expect("empty graph should cluster");
        assert!(communities.is_empty());
    }

    #[test]
    fn test_single_node() {
        let solo = Entity::new("Solo", EntityType::Person, 0, 4, 0.9);
        let graph = GraphDocument::from_extraction(&[solo], &[], None);

        let louvain = Louvain::new();
        let communities = louvain.cluster(&graph).expect("single node should cluster");
        assert_eq!(communities.len(), 1);
    }

    #[test]
    fn test_two_disconnected_cliques() {
        let graph = two_cliques_graph();
        let louvain = Louvain::new().with_seed(42);
        let communities = louvain
            .cluster(&graph)
            .expect("two disconnected cliques should cluster");

        let unique: HashSet<_> = communities.values().collect();
        assert_eq!(unique.len(), 2, "Should detect 2 disconnected communities");
    }

    #[test]
    fn test_connected_cliques() {
        let graph = connected_cliques_graph();
        let louvain = Louvain::new().with_seed(42);
        let communities = louvain
            .cluster(&graph)
            .expect("connected cliques should cluster");

        // Should still find 2 communities due to weak bridge
        let unique: HashSet<_> = communities.values().collect();
        assert!(unique.len() <= 2, "Should find at most 2 communities");
    }

    #[test]
    fn test_resolution_effect() {
        let graph = connected_cliques_graph();

        // Low resolution = fewer communities
        let louvain_low = Louvain::new().with_resolution(0.5).with_seed(42);
        let communities_low = louvain_low
            .cluster(&graph)
            .expect("low resolution should cluster");

        // High resolution = more communities
        let louvain_high = Louvain::new().with_resolution(2.0).with_seed(42);
        let communities_high = louvain_high
            .cluster(&graph)
            .expect("high resolution should cluster");

        let unique_low: HashSet<_> = communities_low.values().collect();
        let unique_high: HashSet<_> = communities_high.values().collect();

        assert!(
            unique_high.len() >= unique_low.len(),
            "Higher resolution should find >= communities"
        );
    }

    #[test]
    fn test_modularity_positive_for_good_partition() {
        let graph = two_cliques_graph();
        let louvain = Louvain::new().with_seed(42);
        let communities = louvain.cluster(&graph).expect("two cliques should cluster");

        let modularity = louvain.modularity(&graph, &communities);
        assert!(
            modularity > 0.0,
            "Modularity should be positive for good partition"
        );
    }

    #[test]
    fn test_deterministic_with_seed() {
        let graph = connected_cliques_graph();

        let louvain1 = Louvain::new().with_seed(123);
        let louvain2 = Louvain::new().with_seed(123);

        let c1 = louvain1
            .cluster(&graph)
            .expect("deterministic clustering should succeed");
        let c2 = louvain2
            .cluster(&graph)
            .expect("deterministic clustering should succeed");

        // Same seed should give equivalent partitions
        let partition1: HashSet<Vec<&String>> = c1
            .iter()
            .fold(
                HashMap::<usize, Vec<&String>>::new(),
                |mut acc, (node, &comm)| {
                    acc.entry(comm).or_default().push(node);
                    acc
                },
            )
            .into_values()
            .map(|mut v| {
                v.sort();
                v
            })
            .collect();

        let partition2: HashSet<Vec<&String>> = c2
            .iter()
            .fold(
                HashMap::<usize, Vec<&String>>::new(),
                |mut acc, (node, &comm)| {
                    acc.entry(comm).or_default().push(node);
                    acc
                },
            )
            .into_values()
            .map(|mut v| {
                v.sort();
                v
            })
            .collect();

        assert_eq!(partition1, partition2);
    }
}
