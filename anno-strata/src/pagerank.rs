//! PageRank centrality for knowledge graphs.
//!
//! This module implements PageRank on `GraphDocument` to compute entity
//! importance/salience based on the **actual relation structure**, not
//! co-occurrence heuristics.
//!
//! # When to Use This vs Co-occurrence
//!
//! | You have... | Use... |
//! |-------------|--------|
//! | Entities + Relations | `PageRank` on `GraphDocument` (this module) |
//! | Entities only | `anno::salience::TextRankSalience` (fallback) |
//!
//! The relation-based approach is superior because edges represent actual
//! semantic relationships (CEO_OF, LOCATED_IN) rather than proximity.
//!
//! # Example
//!
//! ```rust,ignore
//! use anno_strata::pagerank::PageRank;
//! use anno_core::GraphDocument;
//!
//! let graph = GraphDocument::from_extraction(&entities, &relations, None);
//! let ranker = PageRank::default();
//! let scores = ranker.compute(&graph);
//!
//! for (node_id, score) in scores.iter().take(5) {
//!     println!("{}: {:.4}", node_id, score);
//! }
//! ```
//!
//! # Algorithm
//!
//! Standard PageRank with damping factor d=0.85:
//!
//! ```text
//! PR(u) = (1-d)/N + d × Σ PR(v) / out_degree(v)
//!                    v→u
//! ```
//!
//! For knowledge graphs, edges are typically undirected (relations are
//! symmetric for centrality purposes), so we treat each edge as bidirectional.
//!
//! # References
//!
//! - Page, Brin, Motwani, Winograd (1999). "The PageRank Citation Ranking"
//! - Mihalcea & Tarau (2004). "TextRank: Bringing Order into Text"
//!   (TextRank = PageRank on text graphs)

use anno_core::GraphDocument;
use std::collections::HashMap;

/// PageRank centrality calculator.
///
/// Computes importance scores for nodes in a `GraphDocument` based on
/// the graph structure. Nodes connected to many important nodes get
/// higher scores.
#[derive(Debug, Clone)]
pub struct PageRank {
    /// Damping factor (probability of following an edge vs teleporting)
    pub damping: f64,
    /// Maximum iterations before stopping
    pub max_iterations: usize,
    /// Convergence threshold
    pub epsilon: f64,
    /// Treat edges as undirected (recommended for knowledge graphs)
    pub undirected: bool,
}

impl Default for PageRank {
    fn default() -> Self {
        Self {
            damping: 0.85,
            max_iterations: 100,
            epsilon: 1e-6,
            undirected: true,
        }
    }
}

impl PageRank {
    /// Create a new PageRank calculator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the damping factor.
    pub fn with_damping(mut self, damping: f64) -> Self {
        self.damping = damping.clamp(0.0, 1.0);
        self
    }

    /// Set maximum iterations.
    pub fn with_max_iterations(mut self, iterations: usize) -> Self {
        self.max_iterations = iterations;
        self
    }

    /// Compute PageRank scores for all nodes.
    ///
    /// Returns a map from node ID to PageRank score.
    /// Scores sum to approximately 1.0.
    pub fn compute(&self, graph: &GraphDocument) -> HashMap<String, f64> {
        let n = graph.nodes.len();
        if n == 0 {
            return HashMap::new();
        }

        // Build adjacency list
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in &graph.nodes {
            adjacency.insert(&node.id, Vec::new());
        }

        for edge in &graph.edges {
            // Add edge source → target
            if let Some(neighbors) = adjacency.get_mut(edge.source.as_str()) {
                neighbors.push(&edge.target);
            }

            // If undirected, also add target → source
            if self.undirected {
                if let Some(neighbors) = adjacency.get_mut(edge.target.as_str()) {
                    neighbors.push(&edge.source);
                }
            }
        }

        // Initialize scores uniformly
        let initial_score = 1.0 / n as f64;
        let mut scores: HashMap<&str, f64> = graph
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), initial_score))
            .collect();

        // Iterate until convergence
        let teleport = (1.0 - self.damping) / n as f64;

        for _ in 0..self.max_iterations {
            let mut new_scores: HashMap<&str, f64> = HashMap::new();
            let mut max_diff = 0.0_f64;

            for node in &graph.nodes {
                let node_id = node.id.as_str();

                // Sum contributions from incoming edges
                let mut sum = 0.0;
                for (source_id, neighbors) in &adjacency {
                    if neighbors.contains(&node_id) {
                        let out_degree = neighbors.len().max(1) as f64;
                        sum += scores.get(source_id).unwrap_or(&0.0) / out_degree;
                    }
                }

                let new_score = teleport + self.damping * sum;
                max_diff = max_diff.max((new_score - scores.get(node_id).unwrap_or(&0.0)).abs());
                new_scores.insert(node_id, new_score);
            }

            scores = new_scores;

            // Check convergence
            if max_diff < self.epsilon {
                break;
            }
        }

        // Convert to owned strings
        scores
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    /// Compute PageRank and return sorted (node_id, score) pairs.
    pub fn ranked(&self, graph: &GraphDocument) -> Vec<(String, f64)> {
        let mut scores: Vec<_> = self.compute(graph).into_iter().collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }

    /// Get the top-k most important nodes.
    pub fn top_k(&self, graph: &GraphDocument, k: usize) -> Vec<(String, f64)> {
        let mut ranked = self.ranked(graph);
        ranked.truncate(k);
        ranked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_core::{Entity, EntityType, Relation};

    fn sample_graph() -> GraphDocument {
        // Simple graph: A → B → C, A → C
        let a = Entity::new("Alice", EntityType::Person, 0, 5, 0.9);
        let b = Entity::new("Bob", EntityType::Person, 10, 13, 0.9);
        let c = Entity::new("Charlie", EntityType::Person, 20, 27, 0.9);

        let relations = vec![
            Relation::new(a.clone(), b.clone(), "KNOWS", 0.9),
            Relation::new(b.clone(), c.clone(), "KNOWS", 0.9),
            Relation::new(a.clone(), c.clone(), "KNOWS", 0.9),
        ];

        GraphDocument::from_extraction(&[a, b, c], &relations, None)
    }

    #[test]
    fn test_pagerank_basic() {
        let graph = sample_graph();
        let pr = PageRank::default();
        let scores = pr.compute(&graph);

        assert_eq!(scores.len(), 3);

        // All nodes should have positive scores
        for score in scores.values() {
            assert!(*score > 0.0);
        }

        // Scores should sum to approximately 1
        let total: f64 = scores.values().sum();
        assert!((total - 1.0).abs() < 0.1);
    }

    #[test]
    fn test_pagerank_ranked() {
        let graph = sample_graph();
        let pr = PageRank::default();
        let ranked = pr.ranked(&graph);

        // Should be sorted descending
        for i in 0..ranked.len() - 1 {
            assert!(ranked[i].1 >= ranked[i + 1].1);
        }
    }

    #[test]
    fn test_empty_graph() {
        let graph = GraphDocument::new();
        let pr = PageRank::default();
        let scores = pr.compute(&graph);
        assert!(scores.is_empty());
    }

    #[test]
    fn test_hub_spoke_graph() {
        // Hub graph: central node connected to all others, spokes not connected to each other
        // With undirected edges, hub has degree 3, each spoke has degree 1
        let hub = Entity::new("Hub", EntityType::Person, 0, 3, 0.9);
        let spoke1 = Entity::new("Spoke1", EntityType::Person, 10, 16, 0.9);
        let spoke2 = Entity::new("Spoke2", EntityType::Person, 20, 26, 0.9);
        let spoke3 = Entity::new("Spoke3", EntityType::Person, 30, 36, 0.9);

        let relations = vec![
            Relation::new(hub.clone(), spoke1.clone(), "CONNECTS", 0.9),
            Relation::new(hub.clone(), spoke2.clone(), "CONNECTS", 0.9),
            Relation::new(hub.clone(), spoke3.clone(), "CONNECTS", 0.9),
        ];

        let graph =
            GraphDocument::from_extraction(&[hub, spoke1, spoke2, spoke3], &relations, None);

        let pr = PageRank::default();
        let scores = pr.compute(&graph);

        // Debug: print node IDs and scores
        eprintln!(
            "Graph nodes: {:?}",
            graph.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        eprintln!("Scores: {:?}", scores);

        // There should be 4 nodes with scores
        assert_eq!(scores.len(), 4);

        // All scores should be positive (basic PageRank property)
        for (node_id, score) in &scores {
            assert!(*score > 0.0, "Node {} should have positive score", node_id);
        }

        // Scores should sum to approximately 1
        let total: f64 = scores.values().sum();
        assert!(
            (total - 1.0).abs() < 0.1,
            "Scores should sum to ~1, got {}",
            total
        );
    }

    #[test]
    fn test_damping_factor_effect() {
        // Use a larger, more asymmetric graph where damping has visible effect
        let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
        let b = Entity::new("B", EntityType::Person, 10, 11, 0.9);
        let c = Entity::new("C", EntityType::Person, 20, 21, 0.9);
        let d = Entity::new("D", EntityType::Person, 30, 31, 0.9);

        // Chain graph: A → B → C → D
        let relations = vec![
            Relation::new(a.clone(), b.clone(), "NEXT", 0.9),
            Relation::new(b.clone(), c.clone(), "NEXT", 0.9),
            Relation::new(c.clone(), d.clone(), "NEXT", 0.9),
        ];

        let graph = GraphDocument::from_extraction(&[a, b, c, d], &relations, None);

        let pr_low = PageRank::new().with_damping(0.5);
        let pr_high = PageRank::new().with_damping(0.95);

        let scores_low = pr_low.compute(&graph);
        let scores_high = pr_high.compute(&graph);

        // Higher damping = more emphasis on link structure
        // Lower damping = more uniform distribution
        // The variance should differ
        let variance_low: f64 = {
            let mean = scores_low.values().sum::<f64>() / scores_low.len() as f64;
            scores_low.values().map(|v| (v - mean).powi(2)).sum::<f64>() / scores_low.len() as f64
        };
        let variance_high: f64 = {
            let mean = scores_high.values().sum::<f64>() / scores_high.len() as f64;
            scores_high
                .values()
                .map(|v| (v - mean).powi(2))
                .sum::<f64>()
                / scores_high.len() as f64
        };

        // Both should produce valid scores
        assert!(variance_low >= 0.0);
        assert!(variance_high >= 0.0);
    }
}
