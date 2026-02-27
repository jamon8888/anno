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
    /// use anno::backends::graph_coref::CorefGraph;
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
