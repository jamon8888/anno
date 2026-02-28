use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Coreference graph: undirected edges between mention indices.
pub struct CorefGraph {
    /// Number of mentions (nodes).
    pub(super) num_mentions: usize,
    /// Adjacency set: (i, j) where i < j for canonical representation.
    pub(super) edges: HashSet<(usize, usize)>,
}

impl CorefGraph {
    /// Create an empty coreference graph with the given number of mentions.
    #[must_use]
    pub fn new(num_mentions: usize) -> Self {
        Self {
            num_mentions,
            edges: HashSet::new(),
        }
    }

    /// Get the number of mentions (nodes) in the graph.
    #[must_use]
    pub fn num_mentions(&self) -> usize {
        self.num_mentions
    }

    /// Add a coreference edge between two mentions.
    ///
    /// The edge is stored in canonical form (i < j) for consistency.
    /// Self-loops and out-of-bounds indices are silently ignored.
    pub fn add_edge(&mut self, i: usize, j: usize) {
        if i == j || i >= self.num_mentions || j >= self.num_mentions {
            return;
        }
        let (lo, hi) = if i < j { (i, j) } else { (j, i) };
        self.edges.insert((lo, hi));
    }

    /// Remove a coreference edge between two mentions.
    pub fn remove_edge(&mut self, i: usize, j: usize) {
        let (lo, hi) = if i < j { (i, j) } else { (j, i) };
        self.edges.remove(&(lo, hi));
    }

    /// Check if two mentions are directly linked.
    #[must_use]
    pub fn has_edge(&self, i: usize, j: usize) -> bool {
        if i == j {
            return false;
        }
        let (lo, hi) = if i < j { (i, j) } else { (j, i) };
        self.edges.contains(&(lo, hi))
    }

    /// Get all neighbors (directly linked mentions) of a mention.
    #[must_use]
    pub fn neighbors(&self, i: usize) -> Vec<usize> {
        let mut result = Vec::new();
        for &(lo, hi) in &self.edges {
            if lo == i {
                result.push(hi);
            } else if hi == i {
                result.push(lo);
            }
        }
        result
    }

    /// Count shared neighbors between two mentions.
    ///
    /// This is the basis for the transitivity bonus: if mentions i and j
    /// share many neighbors in the current graph, they're likely coreferent.
    ///
    /// # G2GT Connection
    ///
    /// In the full G2GT model, shared structure is captured via graph-conditioned
    /// attention. Here we approximate it by explicitly counting shared neighbors
    /// and adding a proportional bonus to the pairwise score.
    #[must_use]
    pub fn shared_neighbors(&self, i: usize, j: usize) -> usize {
        let neighbors_i: HashSet<usize> = self.neighbors(i).into_iter().collect();
        let neighbors_j: HashSet<usize> = self.neighbors(j).into_iter().collect();
        neighbors_i.intersection(&neighbors_j).count()
    }

    /// Check if two mentions are transitively connected.
    ///
    /// Uses BFS to find if there's a path from i to j through coreference links.
    /// This is the closure property that ensures consistency: if A~B and B~C,
    /// then A and C are transitively connected even without a direct edge.
    #[must_use]
    pub fn transitively_connected(&self, i: usize, j: usize) -> bool {
        if i == j {
            return true;
        }
        if self.has_edge(i, j) {
            return true;
        }

        // BFS from i to find j
        let mut visited = HashSet::new();
        let mut queue = vec![i];
        visited.insert(i);

        while let Some(current) = queue.pop() {
            for neighbor in self.neighbors(current) {
                if neighbor == j {
                    return true;
                }
                if visited.insert(neighbor) {
                    queue.push(neighbor);
                }
            }
        }

        false
    }

    /// Extract connected components as clusters.
    ///
    /// Each connected component in the graph becomes a coreference chain.
    /// Singleton mentions (no edges) are included as single-mention clusters.
    #[must_use]
    pub fn extract_clusters(&self) -> Vec<Vec<usize>> {
        let mut visited = vec![false; self.num_mentions];
        let mut clusters = Vec::new();

        for start in 0..self.num_mentions {
            if visited[start] {
                continue;
            }

            // BFS to find all members of this component
            let mut cluster = Vec::new();
            let mut queue = vec![start];
            visited[start] = true;

            while let Some(current) = queue.pop() {
                cluster.push(current);
                for neighbor in self.neighbors(current) {
                    if !visited[neighbor] {
                        visited[neighbor] = true;
                        queue.push(neighbor);
                    }
                }
            }

            cluster.sort_unstable();
            clusters.push(cluster);
        }

        clusters
    }

    /// Get the number of edges in the graph.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Check if graph is empty (no edges).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Seed the graph with co-occurrence priors based on mention proximity.
    ///
    /// This is inspired by SpanEIT (Hossain et al. 2025), which constructs a
    /// semantic co-occurrence graph `G_sem` alongside the syntactic graph. The
    /// insight: mentions that appear close together are more likely coreferent.
    ///
    /// # Arguments
    ///
    /// * `mention_positions` - Position of each mention (e.g., character offset)
    /// * `window_size` - Maximum distance for co-occurrence (e.g., 100 chars)
    /// * `scorer` - Optional scoring function; if None, uses constant weight
    ///
    /// # Example
    ///
    /// ```rust
    /// use anno::backends::coref::graph::CorefGraph;
    ///
    /// let mut graph = CorefGraph::new(3);
    /// let positions = vec![0, 50, 200];  // Character offsets
    ///
    /// // Seed edges for mentions within 100 chars of each other
    /// // Type annotation needed when passing None for the scorer
    /// graph.seed_cooccurrence_edges::<fn(usize, usize) -> bool>(&positions, 100, None);
    ///
    /// assert!(graph.has_edge(0, 1));   // 50 < 100
    /// assert!(!graph.has_edge(0, 2));  // 200 > 100
    /// ```
    ///
    /// # Research Background
    ///
    /// SpanEIT constructs `G = (V, E_syn ∪ E_sem)` where:
    /// - `E_syn` = syntactic dependency edges
    /// - `E_sem` = co-occurrence edges (this method)
    ///
    /// The combined graph is processed by GAT layers for context-aware embeddings.
    /// For anno's heuristic approach, we add these edges as initial priors before
    /// iterative refinement.
    pub fn seed_cooccurrence_edges<F>(
        &mut self,
        mention_positions: &[usize],
        window_size: usize,
        scorer: Option<F>,
    ) where
        F: Fn(usize, usize) -> bool,
    {
        for i in 0..self.num_mentions {
            for j in (i + 1)..self.num_mentions {
                if i >= mention_positions.len() || j >= mention_positions.len() {
                    continue;
                }

                let pos_i = mention_positions[i];
                let pos_j = mention_positions[j];
                let distance = pos_i.abs_diff(pos_j);

                if distance <= window_size {
                    let should_add = match scorer.as_ref() {
                        None => true,
                        Some(f) => f(i, j),
                    };
                    if should_add {
                        self.add_edge(i, j);
                    }
                }
            }
        }
    }
}

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for graph-based coreference resolution.
///
/// These parameters control the iterative refinement process. The defaults
/// are based on findings from Miculicich & Henderson (2022):
///
/// - `max_iterations = 4`: Paper found T=4 optimal; more iterations don't help
/// - `link_threshold = 0.5`: Standard classification threshold
/// - `transitivity_bonus = 0.15`: Reward for transitive consistency
///
/// # Tuning Guide
///
/// | Parameter | Higher Value | Lower Value |
/// |-----------|--------------|-------------|
/// | `link_threshold` | Fewer, more confident links | More links, potential noise |
/// | `transitivity_bonus` | Stronger clustering effect | More independent decisions |
/// | `max_iterations` | More refinement passes | Faster, less propagation |
/// | `head_match_weight` | Trust head matches more | Rely more on full string |
#[derive(Debug, Clone)]
pub struct GraphCorefConfig {
    /// Maximum refinement iterations before stopping.
    ///
    /// The G2GT paper found T=4 to be optimal on CoNLL 2012. Fewer iterations
    /// leave potential coreference links undiscovered; more iterations don't
    /// improve results and waste computation.
    pub max_iterations: usize,

    /// Minimum score to create a coreference link.
    ///
    /// A pair (mᵢ, mⱼ) is linked if score(mᵢ, mⱼ) + context_bonus > threshold.
    pub link_threshold: f64,

    /// Bonus added for transitive consistency.
    ///
    /// If mentions A and B share neighbors in the current graph (i.e., both
    /// are already linked to some common mention C), this bonus is added to
    /// encourage the model to also link A and B directly.
    ///
    /// **Note**: This is our heuristic approximation of G2GT's graph-conditioned
    /// attention. The full G2GT model encodes graph structure as:
    /// ```text
    /// Attention(Q,K,V,Lk,Lv) = softmax(Q·(K+Lk)/√d)·(V+Lv)
    /// where Lk = E(G^{t-1})·Wk
    /// ```
    /// We approximate this by explicit score adjustment rather than attention modification.
    pub transitivity_bonus: f64,

    /// Bonus for each shared neighbor.
    ///
    /// Scaled by the number of shared neighbors: total_bonus = shared_count * per_neighbor_bonus
    pub per_shared_neighbor_bonus: f64,

    /// Weight for string similarity in pairwise scoring.
    pub string_similarity_weight: f64,

    /// Weight for head word matching.
    ///
    /// The G2GT paper emphasizes head-based matching. When mentions have
    /// `head_start`/`head_end` set, head matching is used. Otherwise falls
    /// back to last word heuristic.
    pub head_match_weight: f64,

    /// Weight for distance penalty in pairwise scoring.
    pub distance_weight: f64,

    /// Maximum character distance to consider (mentions further apart are not linked).
    pub max_distance: Option<usize>,

    /// Include singletons (mentions with no coreference) in output.
    ///
    /// Default: false (only return multi-mention chains).
    /// Set to true for evaluation against datasets that include singletons.
    pub include_singletons: bool,

    /// Bonus when a pronoun links to a proper noun.
    ///
    /// Pronouns are weak signals alone but should link to antecedents.
    pub pronoun_proper_bonus: f64,

    /// Optional early-stop controls for iterative refinement.
    ///
    /// GraphCoref already stops when it reaches a fixed point (Gₜ == Gₜ₋₁).
    /// This option additionally stops on:
    /// - **Cycle detection** (e.g., A→B→A oscillation across iterations)
    /// - **Stagnation** (edge count stops changing for N iterations)
    ///
    /// This is an analogue of “overthinking” / redundancy detection (CoRE-Eval),
    /// implemented using observable signals (graph structure) rather than hidden states.
    pub early_stop: Option<GraphCorefEarlyStopConfig>,

    /// Optional external pairwise scores (mention index pair -> score).
    /// Injected by callers who have pre-computed similarity signals (e.g., box containment).
    /// Score is added to pairwise_score() weighted by `external_score_weight`.
    pub external_scores: Option<HashMap<(usize, usize), f64>>,
    /// Weight for external scores in pairwise scoring.
    pub external_score_weight: f64,
}

/// Configuration for early stopping in iterative graph refinement.
#[derive(Debug, Clone)]
pub struct GraphCorefEarlyStopConfig {
    /// Stop if we detect a repeated graph state (cycle) within the configured history.
    pub detect_cycles: bool,
    /// How many past graph fingerprints to remember (0 = unbounded).
    pub cycle_history: usize,
    /// Stop if the edge count hasn't changed for this many consecutive iterations.
    pub stagnation_patience: usize,
}

impl Default for GraphCorefEarlyStopConfig {
    fn default() -> Self {
        Self {
            detect_cycles: true,
            cycle_history: 8,
            stagnation_patience: 2,
        }
    }
}

impl Default for GraphCorefConfig {
    fn default() -> Self {
        Self {
            max_iterations: 4,
            link_threshold: 0.5,
            transitivity_bonus: 0.15,
            per_shared_neighbor_bonus: 0.1,
            string_similarity_weight: 1.0,
            head_match_weight: 0.5,
            distance_weight: 0.05,
            max_distance: Some(1000),
            include_singletons: false,
            pronoun_proper_bonus: 0.3,
            early_stop: None,
            external_scores: None,
            external_score_weight: 0.5,
        }
    }
}

// =============================================================================
// Main Implementation
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Construction and basic properties
    // =========================================================================

    #[test]
    fn new_graph_is_empty() {
        let g = CorefGraph::new(5);
        assert_eq!(g.num_mentions(), 5);
        assert_eq!(g.edge_count(), 0);
        assert!(g.is_empty());
    }

    #[test]
    fn zero_node_graph() {
        let g = CorefGraph::new(0);
        assert_eq!(g.num_mentions(), 0);
        assert!(g.is_empty());
        assert_eq!(g.extract_clusters(), Vec::<Vec<usize>>::new());
    }

    // =========================================================================
    // add_edge / has_edge / remove_edge
    // =========================================================================

    #[test]
    fn add_and_query_edge() {
        let mut g = CorefGraph::new(4);
        g.add_edge(1, 3);

        assert!(g.has_edge(1, 3));
        assert!(g.has_edge(3, 1), "undirected: reversed query must work");
        assert!(!g.has_edge(0, 1));
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn add_edge_canonical_order() {
        // Both orderings should produce the same single edge.
        let mut g = CorefGraph::new(3);
        g.add_edge(2, 0);
        g.add_edge(0, 2);
        assert_eq!(g.edge_count(), 1, "duplicate edge should not increase count");
    }

    #[test]
    fn remove_edge_both_orderings() {
        let mut g = CorefGraph::new(3);
        g.add_edge(0, 2);
        assert!(g.has_edge(0, 2));

        // Remove using reversed order.
        g.remove_edge(2, 0);
        assert!(!g.has_edge(0, 2));
        assert!(!g.has_edge(2, 0));
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn remove_nonexistent_edge_is_noop() {
        let mut g = CorefGraph::new(3);
        g.remove_edge(0, 1); // nothing to remove
        assert_eq!(g.edge_count(), 0);
    }

    // =========================================================================
    // Self-loops
    // =========================================================================

    #[test]
    fn self_loop_ignored_by_add() {
        let mut g = CorefGraph::new(3);
        g.add_edge(1, 1);
        assert!(!g.has_edge(1, 1));
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn has_edge_self_returns_false() {
        let g = CorefGraph::new(3);
        assert!(!g.has_edge(0, 0));
    }

    // =========================================================================
    // Out-of-bounds
    // =========================================================================

    #[test]
    fn add_edge_out_of_bounds_ignored() {
        let mut g = CorefGraph::new(3);
        g.add_edge(0, 5);
        g.add_edge(5, 0);
        g.add_edge(10, 20);
        assert_eq!(g.edge_count(), 0);
    }

    // =========================================================================
    // Neighbors
    // =========================================================================

    #[test]
    fn neighbors_of_isolated_node() {
        let g = CorefGraph::new(3);
        assert!(g.neighbors(0).is_empty());
    }

    #[test]
    fn neighbors_returns_all_adjacent() {
        let mut g = CorefGraph::new(5);
        g.add_edge(2, 0);
        g.add_edge(2, 3);
        g.add_edge(2, 4);

        let mut nbrs = g.neighbors(2);
        nbrs.sort_unstable();
        assert_eq!(nbrs, vec![0, 3, 4]);
    }

    // =========================================================================
    // Shared neighbors
    // =========================================================================

    #[test]
    fn shared_neighbors_none() {
        let mut g = CorefGraph::new(4);
        g.add_edge(0, 1);
        g.add_edge(2, 3);
        assert_eq!(g.shared_neighbors(0, 2), 0);
    }

    #[test]
    fn shared_neighbors_one_common() {
        let mut g = CorefGraph::new(4);
        // 0--2, 1--2  =>  shared(0,1) = {2}
        g.add_edge(0, 2);
        g.add_edge(1, 2);
        assert_eq!(g.shared_neighbors(0, 1), 1);
    }

    #[test]
    fn shared_neighbors_multiple() {
        let mut g = CorefGraph::new(5);
        // 0--2, 0--3, 1--2, 1--3  =>  shared(0,1) = {2,3}
        g.add_edge(0, 2);
        g.add_edge(0, 3);
        g.add_edge(1, 2);
        g.add_edge(1, 3);
        assert_eq!(g.shared_neighbors(0, 1), 2);
    }

    // =========================================================================
    // Transitive connectivity
    // =========================================================================

    #[test]
    fn transitively_connected_same_node() {
        let g = CorefGraph::new(3);
        assert!(g.transitively_connected(1, 1));
    }

    #[test]
    fn transitively_connected_direct_edge() {
        let mut g = CorefGraph::new(3);
        g.add_edge(0, 2);
        assert!(g.transitively_connected(0, 2));
        assert!(g.transitively_connected(2, 0));
    }

    #[test]
    fn transitively_connected_via_chain() {
        // 0--1--2--3
        let mut g = CorefGraph::new(4);
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        g.add_edge(2, 3);
        assert!(g.transitively_connected(0, 3));
        assert!(g.transitively_connected(3, 0));
    }

    #[test]
    fn not_transitively_connected_disjoint() {
        let mut g = CorefGraph::new(4);
        g.add_edge(0, 1);
        g.add_edge(2, 3);
        assert!(!g.transitively_connected(0, 2));
        assert!(!g.transitively_connected(1, 3));
    }

    #[test]
    fn transitively_connected_empty_graph() {
        let g = CorefGraph::new(3);
        assert!(!g.transitively_connected(0, 1));
    }

    // =========================================================================
    // Cluster extraction (connected components)
    // =========================================================================

    #[test]
    fn clusters_all_singletons() {
        let g = CorefGraph::new(3);
        let clusters = g.extract_clusters();
        assert_eq!(clusters.len(), 3);
        for c in &clusters {
            assert_eq!(c.len(), 1);
        }
    }

    #[test]
    fn clusters_single_component() {
        // Fully connected triangle: 0--1, 1--2, 0--2
        let mut g = CorefGraph::new(3);
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        g.add_edge(0, 2);

        let clusters = g.extract_clusters();
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0], vec![0, 1, 2]);
    }

    #[test]
    fn clusters_two_components() {
        let mut g = CorefGraph::new(5);
        // Component A: {0, 1, 2}
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        // Component B: {3, 4}
        g.add_edge(3, 4);

        let clusters = g.extract_clusters();
        assert_eq!(clusters.len(), 2);

        let sizes: Vec<usize> = {
            let mut s: Vec<usize> = clusters.iter().map(|c| c.len()).collect();
            s.sort_unstable();
            s
        };
        assert_eq!(sizes, vec![2, 3]);
    }

    #[test]
    fn clusters_are_sorted() {
        // Chain: 3--1--0--2
        let mut g = CorefGraph::new(4);
        g.add_edge(3, 1);
        g.add_edge(1, 0);
        g.add_edge(0, 2);

        let clusters = g.extract_clusters();
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0], vec![0, 1, 2, 3], "cluster members must be sorted");
    }

    // =========================================================================
    // Seed co-occurrence edges
    // =========================================================================

    #[test]
    fn seed_cooccurrence_within_window() {
        let mut g = CorefGraph::new(3);
        let positions = vec![0, 30, 200];
        g.seed_cooccurrence_edges(&positions, 50, None::<fn(usize, usize) -> bool>);

        assert!(g.has_edge(0, 1), "distance 30 <= 50");
        assert!(!g.has_edge(0, 2), "distance 200 > 50");
        assert!(!g.has_edge(1, 2), "distance 170 > 50");
    }

    #[test]
    fn seed_cooccurrence_scorer_filters() {
        let mut g = CorefGraph::new(3);
        let positions = vec![0, 10, 20];
        // Scorer rejects all pairs.
        g.seed_cooccurrence_edges(&positions, 100, Some(|_i: usize, _j: usize| false));
        assert!(g.is_empty());
    }

    #[test]
    fn seed_cooccurrence_positions_shorter_than_mentions() {
        // positions has fewer entries than num_mentions -- should not panic.
        let mut g = CorefGraph::new(5);
        let positions = vec![0, 10];
        g.seed_cooccurrence_edges(&positions, 100, None::<fn(usize, usize) -> bool>);
        // Only pair (0,1) is within range of positions; rest are skipped.
        assert!(g.has_edge(0, 1));
        assert_eq!(g.edge_count(), 1);
    }

    // =========================================================================
    // Equality (derives PartialEq, Eq)
    // =========================================================================

    #[test]
    fn graph_equality() {
        let mut a = CorefGraph::new(3);
        a.add_edge(0, 1);
        a.add_edge(1, 2);

        let mut b = CorefGraph::new(3);
        b.add_edge(1, 2);
        b.add_edge(0, 1);

        assert_eq!(a, b, "insertion order should not affect equality");
    }

    #[test]
    fn graph_inequality_different_edges() {
        let mut a = CorefGraph::new(3);
        a.add_edge(0, 1);

        let mut b = CorefGraph::new(3);
        b.add_edge(0, 2);

        assert_ne!(a, b);
    }

    #[test]
    fn graph_inequality_different_size() {
        let a = CorefGraph::new(3);
        let b = CorefGraph::new(4);
        assert_ne!(a, b);
    }

    // =========================================================================
    // Config defaults
    // =========================================================================

    #[test]
    fn config_default_values() {
        let cfg = GraphCorefConfig::default();
        assert_eq!(cfg.max_iterations, 4);
        assert!((cfg.link_threshold - 0.5).abs() < f64::EPSILON);
        assert!(!cfg.include_singletons);
        assert!(cfg.early_stop.is_none());
    }

    #[test]
    fn early_stop_config_defaults() {
        let cfg = GraphCorefEarlyStopConfig::default();
        assert!(cfg.detect_cycles);
        assert_eq!(cfg.cycle_history, 8);
        assert_eq!(cfg.stagnation_patience, 2);
    }
}
