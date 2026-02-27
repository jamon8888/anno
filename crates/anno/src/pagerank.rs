//! Shared PageRank algorithm for graph-based ranking.
//!
//! This module provides a PageRank implementation that operates on
//! adjacency matrices. It's used by:
//! - `salience.rs` (TextRankSalience)
//! - `keywords.rs` (TextRankExtractor)
//! - `summarize.rs` (LexRankSummarizer)
//!
//! Delegates to [`graphops::pagerank`] for the core algorithm.

/// PageRank configuration.
#[derive(Debug, Clone)]
pub struct PageRankConfig {
    /// Damping factor (probability of following a link vs teleporting).
    /// Default: 0.85
    pub damping: f64,
    /// Maximum iterations before stopping.
    /// Default: 30
    pub max_iterations: usize,
    /// Convergence threshold (sum of absolute differences).
    /// Default: 1e-6
    pub epsilon: f64,
}

impl Default for PageRankConfig {
    fn default() -> Self {
        Self {
            damping: 0.85,
            max_iterations: 30,
            epsilon: 1e-6,
        }
    }
}

impl PageRankConfig {
    /// Create with custom damping factor.
    pub fn with_damping(mut self, damping: f64) -> Self {
        self.damping = damping.clamp(0.0, 1.0);
        self
    }

    /// Create with custom max iterations.
    pub fn with_max_iterations(mut self, iterations: usize) -> Self {
        self.max_iterations = iterations;
        self
    }
}

/// Compute PageRank scores on an adjacency matrix.
///
/// # Arguments
/// * `adjacency` - NxN weighted adjacency matrix (row i, col j = weight from i to j)
/// * `config` - PageRank parameters
///
/// # Returns
/// Vector of scores for each node, summing to approximately 1.0.
///
/// # Example
/// ```rust
/// use anno::pagerank::{pagerank, PageRankConfig};
///
/// // Simple 3-node graph: 0→1→2, 0→2
/// let adj = vec![
///     vec![0.0, 1.0, 1.0],  // node 0 connects to 1, 2
///     vec![0.0, 0.0, 1.0],  // node 1 connects to 2
///     vec![0.0, 0.0, 0.0],  // node 2 (sink)
/// ];
///
/// let scores = pagerank(&adj, &PageRankConfig::default());
/// assert_eq!(scores.len(), 3);
/// ```
pub fn pagerank(adjacency: &[Vec<f64>], config: &PageRankConfig) -> Vec<f64> {
    let g = graphops::AdjacencyMatrix(adjacency);
    let go_config = graphops::pagerank::PageRankConfig {
        damping: config.damping,
        max_iterations: config.max_iterations,
        tolerance: config.epsilon,
    };
    graphops::pagerank::pagerank_weighted(&g, go_config)
}

/// Compute PageRank and return indices sorted by score (descending).
pub fn pagerank_ranked(adjacency: &[Vec<f64>], config: &PageRankConfig) -> Vec<(usize, f64)> {
    let scores = pagerank(adjacency, config);
    let mut indexed: Vec<(usize, f64)> = scores.into_iter().enumerate().collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    indexed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let adj: Vec<Vec<f64>> = vec![];
        let scores = pagerank(&adj, &PageRankConfig::default());
        assert!(scores.is_empty());
    }

    #[test]
    fn test_single_node() {
        let adj = vec![vec![0.0]];
        let scores = pagerank(&adj, &PageRankConfig::default());
        assert_eq!(scores.len(), 1);
        assert!(scores[0] > 0.0 && scores[0] <= 1.0);
    }

    #[test]
    fn test_two_connected_nodes() {
        // Symmetric graph: 0↔1
        let adj = vec![vec![0.0, 1.0], vec![1.0, 0.0]];
        let scores = pagerank(&adj, &PageRankConfig::default());

        assert_eq!(scores.len(), 2);
        assert!((scores[0] - scores[1]).abs() < 0.01);
        assert!((scores[0] + scores[1] - 1.0).abs() < 0.1);
    }

    #[test]
    fn test_hub_spoke() {
        let adj = vec![
            vec![0.0, 1.0, 1.0, 1.0],
            vec![1.0, 0.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0, 0.0],
        ];
        let scores = pagerank(&adj, &PageRankConfig::default());

        assert_eq!(scores.len(), 4);
        assert!(scores[0] > scores[1]);
        assert!(scores[0] > scores[2]);
        assert!(scores[0] > scores[3]);
    }

    #[test]
    fn test_ranked() {
        let adj = vec![
            vec![0.0, 1.0, 1.0, 1.0],
            vec![1.0, 0.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0, 0.0],
        ];
        let ranked = pagerank_ranked(&adj, &PageRankConfig::default());
        assert_eq!(ranked[0].0, 0);
    }
}
