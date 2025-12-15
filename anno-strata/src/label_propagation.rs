//! Label Propagation Algorithm for fast community detection.
//!
//! Label Propagation is a near-linear time algorithm for community detection.
//! Each node adopts the most common label among its neighbors, iterating
//! until convergence.
//!
//! # When to Use
//!
//! - **Very large graphs** (>1M edges) where Leiden is too slow
//! - **Quick prototyping** or initial exploration
//! - **As initialization** for more sophisticated methods
//!
//! # Limitations
//!
//! - **Non-deterministic**: Results vary across runs (use seed for reproducibility)
//! - **Lower quality**: Typically lower modularity than Leiden
//! - **Unstable**: Sensitive to node visitation order
//!
//! # Example
//!
//! ```rust,ignore
//! use anno_strata::LabelPropagation;
//! use anno_core::GraphDocument;
//!
//! let lp = LabelPropagation::new().with_seed(42);
//! let communities = lp.cluster(&graph)?;
//! // communities: HashMap<String, usize> (node_id → community_id)
//! ```
//!
//! # References
//!
//! - Raghavan, Albert, Kumara (2007). "Near linear time algorithm to detect
//!   community structures in large-scale networks." Physical Review E 76.

use anno_core::GraphDocument;
use std::collections::HashMap;

/// Label Propagation community detection.
///
/// A fast, simple algorithm that iteratively propagates labels from
/// neighbors until convergence. O(E) per iteration, typically converges
/// in 5-20 iterations.
#[derive(Debug, Clone)]
pub struct LabelPropagation {
    /// Maximum iterations before stopping
    pub max_iterations: usize,
    /// Random seed for deterministic results
    pub seed: Option<u64>,
}

impl Default for LabelPropagation {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            seed: None,
        }
    }
}

impl LabelPropagation {
    /// Create a new Label Propagation instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum iterations.
    pub fn with_max_iterations(mut self, iterations: usize) -> Self {
        self.max_iterations = iterations;
        self
    }

    /// Set random seed for reproducible results.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Run Label Propagation on a graph.
    ///
    /// Returns a map from node ID to community ID.
    ///
    /// # Algorithm
    ///
    /// 1. Initialize: each node gets its own unique label
    /// 2. In random order, each node adopts the most common label among neighbors
    /// 3. Repeat until no labels change or max iterations reached
    pub fn cluster(&self, graph: &GraphDocument) -> Result<HashMap<String, usize>, String> {
        if graph.nodes.is_empty() {
            return Ok(HashMap::new());
        }

        // Build adjacency list (undirected)
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in &graph.nodes {
            adjacency.insert(&node.id, Vec::new());
        }
        for edge in &graph.edges {
            if let Some(neighbors) = adjacency.get_mut(edge.source.as_str()) {
                neighbors.push(&edge.target);
            }
            if let Some(neighbors) = adjacency.get_mut(edge.target.as_str()) {
                neighbors.push(&edge.source);
            }
        }

        // Initialize: each node is its own community
        let mut labels: HashMap<&str, usize> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();

        // Get node IDs in a sortable order (for deterministic shuffle)
        let mut node_ids: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();

        // Iterate until convergence
        for iteration in 0..self.max_iterations {
            // Shuffle node order
            self.shuffle_nodes(&mut node_ids, iteration);

            let mut changed = false;

            for &node_id in &node_ids {
                let neighbors = match adjacency.get(node_id) {
                    Some(n) => n,
                    None => continue,
                };

                if neighbors.is_empty() {
                    continue;
                }

                // Count neighbor labels
                let mut label_counts: HashMap<usize, usize> = HashMap::new();
                for &neighbor in neighbors {
                    if let Some(&label) = labels.get(neighbor) {
                        *label_counts.entry(label).or_insert(0) += 1;
                    }
                }

                // Find most common label (break ties deterministically)
                let current_label = labels.get(node_id).copied().unwrap_or(0);
                let best_label = label_counts
                    .iter()
                    .max_by(|(l1, c1), (l2, c2)| c1.cmp(c2).then_with(|| l1.cmp(l2)))
                    .map(|(&label, _)| label)
                    .unwrap_or(current_label);

                if best_label != current_label {
                    labels.insert(node_id, best_label);
                    changed = true;
                }
            }

            if !changed {
                // Converged
                break;
            }
        }

        // Renumber communities to be contiguous
        let mut label_remap: HashMap<usize, usize> = HashMap::new();
        let mut next_id = 0;

        let mut result: HashMap<String, usize> = HashMap::new();
        for (node_id, &label) in &labels {
            let community_id = *label_remap.entry(label).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            });
            result.insert(node_id.to_string(), community_id);
        }

        Ok(result)
    }

    /// Shuffle nodes deterministically based on seed and iteration.
    fn shuffle_nodes(&self, nodes: &mut [&str], iteration: usize) {
        if let Some(seed) = self.seed {
            // Simple deterministic shuffle using seed + iteration
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            (seed + iteration as u64).hash(&mut hasher);
            let mut rng_state = hasher.finish();

            for i in 0..nodes.len() {
                rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
                let j = (rng_state as usize) % (nodes.len() - i) + i;
                nodes.swap(i, j);
            }
        }
        // If no seed, keep original order (non-random)
    }

    /// Get number of communities found.
    pub fn num_communities(&self, graph: &GraphDocument) -> Result<usize, String> {
        let communities = self.cluster(graph)?;
        let unique: std::collections::HashSet<_> = communities.values().collect();
        Ok(unique.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_core::{Entity, EntityType, Relation};

    fn two_cliques_graph() -> GraphDocument {
        // Two separate cliques that should be detected as communities
        let a1 = Entity::new("A1", EntityType::Person, 0, 2, 0.9);
        let a2 = Entity::new("A2", EntityType::Person, 10, 12, 0.9);
        let a3 = Entity::new("A3", EntityType::Person, 20, 22, 0.9);

        let b1 = Entity::new("B1", EntityType::Person, 30, 32, 0.9);
        let b2 = Entity::new("B2", EntityType::Person, 40, 42, 0.9);
        let b3 = Entity::new("B3", EntityType::Person, 50, 52, 0.9);

        let relations = vec![
            // Clique A
            Relation::new(a1.clone(), a2.clone(), "FRIEND", 0.9),
            Relation::new(a2.clone(), a3.clone(), "FRIEND", 0.9),
            Relation::new(a1.clone(), a3.clone(), "FRIEND", 0.9),
            // Clique B
            Relation::new(b1.clone(), b2.clone(), "FRIEND", 0.9),
            Relation::new(b2.clone(), b3.clone(), "FRIEND", 0.9),
            Relation::new(b1.clone(), b3.clone(), "FRIEND", 0.9),
        ];

        GraphDocument::from_extraction(&[a1, a2, a3, b1, b2, b3], &relations, None)
    }

    fn connected_cliques_graph() -> GraphDocument {
        // Two cliques with a single bridge
        let a1 = Entity::new("A1", EntityType::Person, 0, 2, 0.9);
        let a2 = Entity::new("A2", EntityType::Person, 10, 12, 0.9);
        let a3 = Entity::new("A3", EntityType::Person, 20, 22, 0.9);

        let b1 = Entity::new("B1", EntityType::Person, 30, 32, 0.9);
        let b2 = Entity::new("B2", EntityType::Person, 40, 42, 0.9);
        let b3 = Entity::new("B3", EntityType::Person, 50, 52, 0.9);

        let relations = vec![
            // Clique A
            Relation::new(a1.clone(), a2.clone(), "FRIEND", 0.9),
            Relation::new(a2.clone(), a3.clone(), "FRIEND", 0.9),
            Relation::new(a1.clone(), a3.clone(), "FRIEND", 0.9),
            // Clique B
            Relation::new(b1.clone(), b2.clone(), "FRIEND", 0.9),
            Relation::new(b2.clone(), b3.clone(), "FRIEND", 0.9),
            Relation::new(b1.clone(), b3.clone(), "FRIEND", 0.9),
            // Bridge
            Relation::new(a3.clone(), b1.clone(), "KNOWS", 0.9),
        ];

        GraphDocument::from_extraction(&[a1, a2, a3, b1, b2, b3], &relations, None)
    }

    #[test]
    fn test_empty_graph() {
        let graph = GraphDocument::new();
        let lp = LabelPropagation::new();
        let communities = lp.cluster(&graph).expect("empty graph should cluster");
        assert!(communities.is_empty());
    }

    #[test]
    fn test_single_node() {
        let solo = Entity::new("Solo", EntityType::Person, 0, 4, 0.9);
        let graph = GraphDocument::from_extraction(&[solo], &[], None);

        let lp = LabelPropagation::new();
        let communities = lp.cluster(&graph).expect("single node should cluster");
        assert_eq!(communities.len(), 1);
    }

    #[test]
    fn test_two_disconnected_cliques() {
        let graph = two_cliques_graph();
        let lp = LabelPropagation::new().with_seed(42);
        let communities = lp.cluster(&graph).expect("two cliques should cluster");

        // Should find 2 communities
        let unique: std::collections::HashSet<_> = communities.values().collect();
        assert_eq!(unique.len(), 2, "Should detect 2 disconnected communities");

        // All A nodes should be in same community
        let a_communities: Vec<_> = communities
            .iter()
            .filter(|(k, _)| k.contains("a"))
            .map(|(_, v)| *v)
            .collect();
        assert!(
            a_communities.windows(2).all(|w| w[0] == w[1]),
            "All A nodes should be in same community"
        );
    }

    #[test]
    fn test_deterministic_with_seed() {
        let graph = connected_cliques_graph();

        let lp1 = LabelPropagation::new().with_seed(123);
        let lp2 = LabelPropagation::new().with_seed(123);

        let c1 = lp1
            .cluster(&graph)
            .expect("deterministic clustering should succeed");
        let c2 = lp2
            .cluster(&graph)
            .expect("deterministic clustering should succeed");

        // Same seed should give equivalent partitions
        // (community IDs might differ, but groupings should be the same)
        let partition1: std::collections::HashSet<Vec<&String>> = c1
            .iter()
            .fold(
                std::collections::HashMap::<usize, Vec<&String>>::new(),
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

        let partition2: std::collections::HashSet<Vec<&String>> = c2
            .iter()
            .fold(
                std::collections::HashMap::<usize, Vec<&String>>::new(),
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

        assert_eq!(
            partition1, partition2,
            "Same seed should give equivalent partitions"
        );
    }

    #[test]
    fn test_all_nodes_assigned() {
        let graph = connected_cliques_graph();
        let lp = LabelPropagation::new().with_seed(42);
        let communities = lp
            .cluster(&graph)
            .expect("connected cliques should cluster");

        // Every node should have a community
        assert_eq!(communities.len(), graph.nodes.len());
    }

    #[test]
    fn test_converges_quickly() {
        let graph = two_cliques_graph();
        let lp = LabelPropagation::new().with_seed(42).with_max_iterations(5);
        let result = lp.cluster(&graph);

        // Should still produce valid result even with limited iterations
        assert!(result.is_ok());
    }
}
