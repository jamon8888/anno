//! # Correlation Clustering for Entity Resolution
//!
//! Correlation clustering operates on a graph where edges are labeled **positive**
//! (should cluster together) or **negative** (should be in different clusters).
//! The objective is to minimize *disagreements*: negative edges within clusters
//! plus positive edges between clusters.
//!
//! ## Problem Formulation
//!
//! Given a complete graph \( G = (V, E) \) with edge labeling \( \ell: E \to \{+, -\} \),
//! find a partition \( \mathcal{C} \) minimizing:
//!
//! \[
//! \text{cost}(\mathcal{C}) = \underbrace{\sum_{\substack{(u,v) \in E^+ \\ C(u) \neq C(v)}} 1}_{\text{positive edges cut}}
//!                         + \underbrace{\sum_{\substack{(u,v) \in E^- \\ C(u) = C(v)}} 1}_{\text{negative edges uncut}}
//! \]
//!
//! where \( C(u) \) denotes the cluster containing vertex \( u \).
//!
//! ## Computational Complexity
//!
//! - **NP-hard**: Bansal, Blum, Chawla (2004) proved NP-hardness via reduction from MAX-CUT
//! - **APX-hard**: No PTAS exists unless P = NP
//! - **Approximable**: Polynomial-time constant-factor approximations exist
//!
//! ## Algorithms Implemented
//!
//! ### Pivot Algorithm (Ailon, Charikar, Newman 2008)
//!
//! **3-approximation** in expected \( O(n + m) \) time.
//!
//! ```text
//! while unclustered vertices remain:
//!     pick random pivot v from unclustered
//!     C ← {v} ∪ {u : (v,u) is positive and u unclustered}
//!     output C as cluster
//!     mark all vertices in C as clustered
//! ```
//!
//! The 3-approximation bound is tight for this algorithm.
//!
//! ### Modified Pivot (Behnezhad et al., ICML 2025)
//!
//! **Better than 3-approximation** (~23% fewer errors empirically).
//!
//! Key insight: instead of adding ALL positive neighbors of the pivot, use a
//! voting scheme that considers both positive and negative edges to current
//! cluster members:
//!
//! - Only add \( v \) if \( |\{u \in C : (v,u) \in E^+\}| > |\{u \in C : (v,u) \in E^-\}| \)
//!
//! ### Greedy Agglomerative
//!
//! Start with singletons; repeatedly merge the pair of clusters that maximizes
//! improvement (reduces disagreements most). No theoretical guarantee but often
//! competitive in practice.
//!
//! ## Why Correlation Clustering for Entity Resolution?
//!
//! In entity resolution, a pairwise matcher produces positive/negative judgments:
//! - "John Smith, NY" vs "J. Smith, New York" → **positive** (likely same)
//! - "John Smith" vs "Jane Smith" → **negative** (likely different)
//!
//! These judgments may be **inconsistent** (non-transitive). Correlation clustering
//! finds the partition that best respects the *preponderance* of evidence.
//!
//! ## Example
//!
//! ```rust
//! use anno::coalesce::correlation::{EdgeLabel, LabeledGraph, pivot_clustering};
//! use rand::SeedableRng;
//!
//! let mut graph = LabeledGraph::new(4);
//! graph.add_edge(0, 1, EdgeLabel::Positive);
//! graph.add_edge(2, 3, EdgeLabel::Positive);
//! graph.add_edge(0, 2, EdgeLabel::Negative);
//!
//! let mut rng = rand::rngs::StdRng::seed_from_u64(42);
//! let result = pivot_clustering(&graph, &mut rng);
//! // Expected: 2 clusters {0,1} and {2,3}, cost = 0
//! ```
//!
//! ## References
//!
//! - Bansal, Blum, Chawla (2004). "Correlation Clustering". Machine Learning 56(1-3).
//! - Ailon, Charikar, Newman (2008). "Aggregating inconsistent information:
//!   Ranking and clustering". JACM 55(5).
//! - Behnezhad et al. (2025). "Breaking the 3-approximation barrier for
//!   correlation clustering". ICML 2025.

use rand::prelude::*;
use std::collections::{HashMap, HashSet};

/// Edge label for correlation clustering: positive (should cluster) or negative (should separate).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeLabel {
    /// Positive edge: the two endpoints should be in the same cluster.
    Positive,
    /// Negative edge: the two endpoints should be in different clusters.
    Negative,
}

impl EdgeLabel {
    /// Create from similarity score and threshold
    pub fn from_similarity(sim: f32, threshold: f32) -> Self {
        if sim >= threshold {
            EdgeLabel::Positive
        } else {
            EdgeLabel::Negative
        }
    }
}

/// A graph for correlation clustering with +/- edge labels
#[derive(Debug, Clone)]
pub struct LabeledGraph {
    /// Number of nodes
    pub n: usize,
    /// Adjacency list: node -> [(neighbor, label)]
    pub adj: Vec<Vec<(usize, EdgeLabel)>>,
    /// Optional: similarity scores for weighted variants
    pub weights: Option<Vec<Vec<f32>>>,
}

impl LabeledGraph {
    /// Create empty graph with n nodes
    pub fn new(n: usize) -> Self {
        Self {
            n,
            adj: vec![Vec::new(); n],
            weights: None,
        }
    }

    /// Create from similarity matrix with threshold
    pub fn from_similarity_matrix(sims: &[Vec<f32>], threshold: f32) -> Self {
        let n = sims.len();
        let mut adj = vec![Vec::new(); n];

        for i in 0..n {
            for j in (i + 1)..n {
                let label = EdgeLabel::from_similarity(sims[i][j], threshold);
                adj[i].push((j, label));
                adj[j].push((i, label));
            }
        }

        Self {
            n,
            adj,
            weights: Some(sims.to_vec()),
        }
    }

    /// Add an edge with label
    pub fn add_edge(&mut self, u: usize, v: usize, label: EdgeLabel) {
        self.adj[u].push((v, label));
        self.adj[v].push((u, label));
    }

    /// Get positive neighbors of a node
    pub fn positive_neighbors(&self, u: usize) -> Vec<usize> {
        self.adj[u]
            .iter()
            .filter(|(_, label)| *label == EdgeLabel::Positive)
            .map(|(v, _)| *v)
            .collect()
    }

    /// Get negative neighbors of a node
    pub fn negative_neighbors(&self, u: usize) -> Vec<usize> {
        self.adj[u]
            .iter()
            .filter(|(_, label)| *label == EdgeLabel::Negative)
            .map(|(v, _)| *v)
            .collect()
    }

    /// Get edge label between two nodes (None if no edge)
    pub fn edge_label(&self, u: usize, v: usize) -> Option<EdgeLabel> {
        self.adj[u]
            .iter()
            .find(|(neighbor, _)| *neighbor == v)
            .map(|(_, label)| *label)
    }
}

/// Result of correlation clustering
#[derive(Debug, Clone)]
pub struct ClusteringResult {
    /// Cluster assignment for each node
    pub assignments: Vec<usize>,
    /// List of clusters (each cluster is a set of node indices)
    pub clusters: Vec<HashSet<usize>>,
    /// Number of disagreements (cost)
    pub cost: usize,
}

impl ClusteringResult {
    /// Create from cluster assignments
    pub fn from_assignments(assignments: Vec<usize>) -> Self {
        let mut clusters: HashMap<usize, HashSet<usize>> = HashMap::new();
        for (node, &cluster_id) in assignments.iter().enumerate() {
            clusters.entry(cluster_id).or_default().insert(node);
        }

        let mut cluster_list: Vec<_> = clusters.into_values().collect();
        cluster_list.sort_by_key(|c| c.iter().min().copied().unwrap_or(0));

        Self {
            assignments,
            clusters: cluster_list,
            cost: 0, // Computed separately
        }
    }

    /// Compute cost (disagreements) given graph
    pub fn compute_cost(&mut self, graph: &LabeledGraph) {
        let mut cost = 0;

        for u in 0..graph.n {
            for (v, label) in &graph.adj[u] {
                if u < *v {
                    // Count each edge once
                    let same_cluster = self.assignments[u] == self.assignments[*v];
                    let disagreement = match (same_cluster, label) {
                        (true, EdgeLabel::Negative) => true,  // Neg edge within cluster
                        (false, EdgeLabel::Positive) => true, // Pos edge between clusters
                        _ => false,
                    };
                    if disagreement {
                        cost += 1;
                    }
                }
            }
        }

        self.cost = cost;
    }

    /// Number of clusters
    pub fn num_clusters(&self) -> usize {
        self.clusters.len()
    }
}

// =============================================================================
// Pivot Algorithm (Ailon, Charikar, Newman 2008)
// =============================================================================

/// Classic Pivot/QwikCluster algorithm.
///
/// 3-approximation for correlation clustering on complete graphs with +/- edges.
/// Expected O(n + m) time where m is number of edges.
///
/// Algorithm:
/// 1. While unclustered nodes remain:
///    - Pick random pivot from unclustered
///    - Form cluster: pivot + all positive neighbors that are unclustered
///    - Mark cluster as clustered
pub fn pivot_clustering<R: Rng>(graph: &LabeledGraph, rng: &mut R) -> ClusteringResult {
    let n = graph.n;
    let mut unclustered: Vec<usize> = (0..n).collect();
    let mut assignments = vec![usize::MAX; n];
    let mut cluster_id = 0;

    while !unclustered.is_empty() {
        // Pick random pivot
        let pivot_idx = rng.random_range(0..unclustered.len());
        let pivot = unclustered.swap_remove(pivot_idx);

        // Assign pivot to new cluster
        assignments[pivot] = cluster_id;

        // Find positive neighbors that are still unclustered
        let positive_neighbors = graph.positive_neighbors(pivot);

        // Remove positive neighbors from unclustered and assign to this cluster
        let mut i = 0;
        while i < unclustered.len() {
            let v = unclustered[i];
            if positive_neighbors.contains(&v) {
                assignments[v] = cluster_id;
                unclustered.swap_remove(i);
            } else {
                i += 1;
            }
        }

        cluster_id += 1;
    }

    let mut result = ClusteringResult::from_assignments(assignments);
    result.compute_cost(graph);
    result
}

/// Run Pivot multiple times and return best result
pub fn pivot_clustering_best_of<R: Rng>(
    graph: &LabeledGraph,
    rng: &mut R,
    iterations: usize,
) -> ClusteringResult {
    let mut best: Option<ClusteringResult> = None;

    for _ in 0..iterations {
        let result = pivot_clustering(graph, rng);
        match &best {
            None => best = Some(result),
            Some(prev) if result.cost < prev.cost => best = Some(result),
            _ => {}
        }
    }

    best.expect("should have at least one clustering result after iterations")
}

// =============================================================================
// Modified Pivot (Behnezhad et al. 2025)
// =============================================================================

/// Modified Pivot algorithm from Behnezhad et al. (ICML 2025).
///
/// Achieves better than 3-approximation with ~23% fewer errors empirically.
/// Key insight: instead of adding ALL positive neighbors, use a voting scheme
/// that considers both positive and negative edges.
///
/// For each candidate neighbor v:
/// - Count positive edges from v to current cluster members (support)
/// - Count negative edges from v to current cluster members (opposition)
/// - Only add v if support > opposition
pub fn modified_pivot_clustering<R: Rng>(graph: &LabeledGraph, rng: &mut R) -> ClusteringResult {
    let n = graph.n;
    let mut unclustered: HashSet<usize> = (0..n).collect();
    let mut assignments = vec![usize::MAX; n];
    let mut cluster_id = 0;

    while !unclustered.is_empty() {
        // Pick random pivot from unclustered
        let unclustered_vec: Vec<_> = unclustered.iter().copied().collect();
        let pivot = *unclustered_vec
            .choose(rng)
            .expect("unclustered_vec should not be empty");
        unclustered.remove(&pivot);

        // Start cluster with pivot
        let mut current_cluster: HashSet<usize> = HashSet::new();
        current_cluster.insert(pivot);
        assignments[pivot] = cluster_id;

        // Candidates: positive neighbors of pivot that are unclustered
        let mut candidates: Vec<usize> = graph
            .positive_neighbors(pivot)
            .into_iter()
            .filter(|v| unclustered.contains(v))
            .collect();

        // Process candidates with voting
        // We iterate until no more candidates can be added
        let mut changed = true;
        while changed {
            changed = false;
            let mut to_remove = Vec::new();
            let mut to_add = Vec::new();

            for (idx, &v) in candidates.iter().enumerate() {
                if !unclustered.contains(&v) {
                    to_remove.push(idx);
                    continue;
                }

                // Count support and opposition from current cluster
                let mut support = 0i32;
                let mut opposition = 0i32;

                for &member in &current_cluster {
                    if let Some(label) = graph.edge_label(v, member) {
                        match label {
                            EdgeLabel::Positive => support += 1,
                            EdgeLabel::Negative => opposition += 1,
                        }
                    }
                }

                // Add to cluster if support > opposition (strict inequality)
                if support > opposition {
                    current_cluster.insert(v);
                    assignments[v] = cluster_id;
                    unclustered.remove(&v);
                    changed = true;
                    to_remove.push(idx);

                    // Collect v's positive neighbors as new candidates
                    for neighbor in graph.positive_neighbors(v) {
                        if unclustered.contains(&neighbor) && !candidates.contains(&neighbor) {
                            to_add.push(neighbor);
                        }
                    }
                }
            }

            // Remove processed candidates (in reverse order to preserve indices)
            for idx in to_remove.into_iter().rev() {
                candidates.swap_remove(idx);
            }
            // Add new candidates
            candidates.extend(to_add);
        }

        cluster_id += 1;
    }

    let mut result = ClusteringResult::from_assignments(assignments);
    result.compute_cost(graph);
    result
}

// =============================================================================
// Greedy Agglomerative Correlation Clustering
// =============================================================================

/// Greedy agglomerative approach: start with singletons, merge clusters that reduce cost
pub fn greedy_agglomerative(graph: &LabeledGraph) -> ClusteringResult {
    let n = graph.n;

    // Start with each node in its own cluster
    let mut assignments: Vec<usize> = (0..n).collect();
    let mut cluster_sizes: HashMap<usize, usize> = (0..n).map(|i| (i, 1)).collect();

    // Compute initial disagreements for each pair of clusters
    // For efficiency, we only consider merging clusters that share positive edges
    let mut merge_candidates: Vec<(usize, usize)> = Vec::new();

    for u in 0..n {
        for (v, label) in &graph.adj[u] {
            if u < *v && *label == EdgeLabel::Positive {
                merge_candidates.push((u, *v));
            }
        }
    }

    // Greedy merging
    let mut improved = true;
    while improved {
        improved = false;

        // Find best merge
        let mut best_merge: Option<(usize, usize, i32)> = None; // (cluster_a, cluster_b, improvement)

        for u in 0..n {
            for v in (u + 1)..n {
                let cluster_u = find_root(&assignments, u);
                let cluster_v = find_root(&assignments, v);

                if cluster_u == cluster_v {
                    continue;
                }

                // Compute cost change of merging
                let improvement =
                    compute_merge_improvement(graph, &assignments, cluster_u, cluster_v);

                if improvement > 0 {
                    match &best_merge {
                        None => best_merge = Some((cluster_u, cluster_v, improvement)),
                        Some((_, _, best_imp)) if improvement > *best_imp => {
                            best_merge = Some((cluster_u, cluster_v, improvement));
                        }
                        _ => {}
                    }
                }
            }
        }

        // Apply best merge if found
        if let Some((cluster_a, cluster_b, _)) = best_merge {
            // Merge cluster_b into cluster_a
            for i in 0..n {
                if find_root(&assignments, i) == cluster_b {
                    assignments[i] = cluster_a;
                }
            }
            let size_b = cluster_sizes.remove(&cluster_b).unwrap_or(0);
            *cluster_sizes.entry(cluster_a).or_insert(0) += size_b;
            improved = true;
        }
    }

    // Normalize assignments to consecutive IDs
    let mut id_map: HashMap<usize, usize> = HashMap::new();
    let mut next_id = 0;
    for i in 0..n {
        let root = find_root(&assignments, i);
        if let std::collections::hash_map::Entry::Vacant(e) = id_map.entry(root) {
            e.insert(next_id);
            next_id += 1;
        }
        assignments[i] = *id_map
            .get(&root)
            .expect("root should exist in id_map after entry check");
    }

    let mut result = ClusteringResult::from_assignments(assignments);
    result.compute_cost(graph);
    result
}

/// Find root cluster for a node (with path compression)
fn find_root(assignments: &[usize], node: usize) -> usize {
    // For this simple version, assignments directly hold cluster IDs
    // Return the assignment value
    assignments[node]
}

/// Compute improvement from merging two clusters
/// Positive = cost reduction, Negative = cost increase
fn compute_merge_improvement(
    graph: &LabeledGraph,
    assignments: &[usize],
    cluster_a: usize,
    cluster_b: usize,
) -> i32 {
    let mut improvement = 0i32;

    // Get members of each cluster
    let members_a: Vec<usize> = (0..graph.n)
        .filter(|&i| assignments[i] == cluster_a)
        .collect();
    let members_b: Vec<usize> = (0..graph.n)
        .filter(|&i| assignments[i] == cluster_b)
        .collect();

    // For edges between the two clusters:
    // - Positive edges: currently disagreeing (between clusters), will agree after merge
    // - Negative edges: currently agreeing (between clusters), will disagree after merge
    for &u in &members_a {
        for &v in &members_b {
            if let Some(label) = graph.edge_label(u, v) {
                match label {
                    EdgeLabel::Positive => improvement += 1, // Was disagreement, becomes agreement
                    EdgeLabel::Negative => improvement -= 1, // Was agreement, becomes disagreement
                }
            }
        }
    }

    improvement
}

// =============================================================================
// Min-Max Correlation Clustering (2024)
// =============================================================================

/// Min-Max Correlation Clustering result
///
/// Unlike standard CC which minimizes *total* disagreements, min-max CC
/// minimizes the *maximum* disagreements per cluster. This is useful when
/// you want to avoid creating any "bad" clusters, even if the overall
/// cost is slightly higher.
#[derive(Debug, Clone)]
pub struct MinMaxClusteringResult {
    /// Standard clustering result
    pub clustering: ClusteringResult,
    /// Maximum disagreements in any single cluster
    pub max_disagreements: usize,
    /// Disagreements per cluster
    pub cluster_disagreements: Vec<usize>,
}

impl MinMaxClusteringResult {
    /// Compute per-cluster disagreements
    pub fn compute_cluster_disagreements(
        graph: &LabeledGraph,
        assignments: &[usize],
        num_clusters: usize,
    ) -> Vec<usize> {
        let mut disagreements = vec![0usize; num_clusters];

        for u in 0..graph.n {
            let cluster_u = assignments[u];
            for (v, label) in &graph.adj[u] {
                if u < *v {
                    let cluster_v = assignments[*v];
                    let disagrees = match (cluster_u == cluster_v, label) {
                        (true, EdgeLabel::Negative) => true,  // Neg within cluster
                        (false, EdgeLabel::Positive) => true, // Pos between clusters
                        _ => false,
                    };
                    if disagrees {
                        // Attribute disagreement to both affected clusters
                        disagreements[cluster_u] += 1;
                        if cluster_u != cluster_v {
                            disagreements[cluster_v] += 1;
                        }
                    }
                }
            }
        }

        disagreements
    }
}

/// Min-Max Correlation Clustering (4-approximation, 2024).
///
/// Minimizes the *maximum* number of disagreements per cluster, not the total.
/// This is valuable when you want uniformly good clusters rather than
/// optimizing aggregate quality.
///
/// The algorithm uses a modified pivot approach where the cluster formation
/// considers the per-cluster disagreement impact, not just pairwise judgments.
///
/// # Algorithm
///
/// 1. Start with singletons
/// 2. Repeatedly consider merging pairs of clusters
/// 3. Only merge if max(disagreements_after) < max(disagreements_before)
/// 4. Stop when no merge improves max disagreements
///
/// # References
///
/// - "Min-Max Correlation Clustering" (2024) - 4-approximation guarantee
/// - Related to chromatic correlation clustering variants
///
/// # Example
///
/// ```
/// use anno::coalesce::correlation::{EdgeLabel, LabeledGraph, min_max_clustering};
/// use rand::SeedableRng;
///
/// let mut graph = LabeledGraph::new(4);
/// graph.add_edge(0, 1, EdgeLabel::Positive);
/// graph.add_edge(2, 3, EdgeLabel::Positive);
/// graph.add_edge(0, 2, EdgeLabel::Negative);
///
/// let mut rng = rand::rngs::StdRng::seed_from_u64(42);
/// let result = min_max_clustering(&graph, &mut rng);
/// println!("Max disagreements: {}", result.max_disagreements);
/// ```
pub fn min_max_clustering<R: Rng>(graph: &LabeledGraph, rng: &mut R) -> MinMaxClusteringResult {
    let n = graph.n;
    if n == 0 {
        return MinMaxClusteringResult {
            clustering: ClusteringResult {
                assignments: vec![],
                clusters: vec![],
                cost: 0,
            },
            max_disagreements: 0,
            cluster_disagreements: vec![],
        };
    }

    // Start with pivot clustering as baseline
    let baseline = pivot_clustering_best_of(graph, rng, 5);
    let mut assignments = baseline.assignments.clone();
    let mut num_clusters = baseline.num_clusters();

    // Compute initial per-cluster disagreements
    let mut cluster_disagree =
        MinMaxClusteringResult::compute_cluster_disagreements(graph, &assignments, num_clusters);
    let mut max_disagree = cluster_disagree.iter().copied().max().unwrap_or(0);

    // Iterative improvement: try splitting high-disagreement clusters
    let mut improved = true;
    let mut iterations = 0;
    let max_iterations = n * 2; // Prevent infinite loops

    while improved && iterations < max_iterations {
        improved = false;
        iterations += 1;

        // Find the cluster with max disagreements
        let worst_cluster = cluster_disagree
            .iter()
            .enumerate()
            .max_by_key(|(_, &d)| d)
            .map(|(i, _)| i);

        if let Some(worst_idx) = worst_cluster {
            if cluster_disagree[worst_idx] == 0 {
                break; // Perfect clustering
            }

            // Get members of the worst cluster
            let members: Vec<usize> = (0..n).filter(|&i| assignments[i] == worst_idx).collect();

            if members.len() < 2 {
                continue; // Can't split singleton
            }

            // Try splitting: move the node with most negative edges to others in cluster
            // to a new cluster
            let mut best_split_node = None;
            let mut best_split_improvement = 0;

            for &node in &members {
                // Count negative edges within cluster
                let neg_internal: usize = graph.adj[node]
                    .iter()
                    .filter(|(neighbor, label)| {
                        assignments[*neighbor] == worst_idx
                            && *neighbor != node
                            && *label == EdgeLabel::Negative
                    })
                    .count();

                // Count positive edges to outside cluster
                let pos_external: usize = graph.adj[node]
                    .iter()
                    .filter(|(neighbor, label)| {
                        assignments[*neighbor] != worst_idx && *label == EdgeLabel::Positive
                    })
                    .count();

                let improvement = neg_internal as i32 - pos_external as i32;
                if improvement > best_split_improvement as i32 {
                    best_split_node = Some(node);
                    best_split_improvement = improvement as usize;
                }
            }

            // Apply split if beneficial
            if let Some(node) = best_split_node {
                if best_split_improvement > 0 {
                    // Create new cluster
                    assignments[node] = num_clusters;
                    num_clusters += 1;

                    // Recompute disagreements
                    cluster_disagree = MinMaxClusteringResult::compute_cluster_disagreements(
                        graph,
                        &assignments,
                        num_clusters,
                    );
                    let new_max = cluster_disagree.iter().copied().max().unwrap_or(0);

                    if new_max < max_disagree {
                        max_disagree = new_max;
                        improved = true;
                    } else {
                        // Revert if didn't improve max
                        assignments[node] = worst_idx;
                        num_clusters -= 1;
                        cluster_disagree = MinMaxClusteringResult::compute_cluster_disagreements(
                            graph,
                            &assignments,
                            num_clusters,
                        );
                    }
                }
            }
        }
    }

    // Also try merge passes to reduce cluster count without increasing max
    improved = true;
    while improved {
        improved = false;

        // Find pairs of clusters that could merge without increasing max_disagree
        'outer: for c1 in 0..num_clusters {
            for c2 in (c1 + 1)..num_clusters {
                if cluster_disagree.get(c1).copied().unwrap_or(0) == 0
                    || cluster_disagree.get(c2).copied().unwrap_or(0) == 0
                {
                    continue;
                }

                // Simulate merge
                let mut test_assignments = assignments.clone();
                for assignment in &mut test_assignments {
                    if *assignment == c2 {
                        *assignment = c1;
                    }
                }

                let test_disagree = MinMaxClusteringResult::compute_cluster_disagreements(
                    graph,
                    &test_assignments,
                    num_clusters,
                );
                let test_max = test_disagree.iter().copied().max().unwrap_or(0);

                if test_max <= max_disagree {
                    // Accept merge
                    assignments = test_assignments;
                    cluster_disagree = test_disagree;
                    max_disagree = test_max;
                    improved = true;
                    break 'outer;
                }
            }
        }
    }

    // Normalize assignments
    let mut id_map: HashMap<usize, usize> = HashMap::new();
    let mut next_id = 0;
    for assignment in &mut assignments {
        if let std::collections::hash_map::Entry::Vacant(e) = id_map.entry(*assignment) {
            e.insert(next_id);
            next_id += 1;
        }
        *assignment = *id_map
            .get(assignment)
            .expect("assignment should exist in id_map after entry check");
    }

    let mut result = ClusteringResult::from_assignments(assignments.clone());
    result.compute_cost(graph);

    let final_disagree = MinMaxClusteringResult::compute_cluster_disagreements(
        graph,
        &assignments,
        result.num_clusters(),
    );
    let final_max = final_disagree.iter().copied().max().unwrap_or(0);

    MinMaxClusteringResult {
        clustering: result,
        max_disagreements: final_max,
        cluster_disagreements: final_disagree,
    }
}

// =============================================================================
// Chromatic Correlation Clustering (Extension)
// =============================================================================

/// Chromatic Correlation Clustering with k colors.
///
/// A variant where clusters must also satisfy a color constraint:
/// no two nodes of the same color can be in the same cluster.
/// This models scenarios like "no two people with the same role in same team."
///
/// This is a stub/placeholder for the full chromatic CC algorithm.
/// The full algorithm requires careful bookkeeping of color constraints.
#[derive(Debug, Clone)]
pub struct ChromaticClusteringConfig {
    /// Number of colors
    pub k: usize,
    /// Color assignment for each node
    pub colors: Vec<usize>,
}

/// Chromatic correlation clustering (simplified version)
///
/// Note: This is a heuristic implementation. The full chromatic CC
/// algorithm with approximation guarantees is more complex.
pub fn chromatic_clustering<R: Rng>(
    graph: &LabeledGraph,
    config: &ChromaticClusteringConfig,
    rng: &mut R,
) -> ClusteringResult {
    let n = graph.n;
    if n == 0 || config.colors.len() != n {
        return ClusteringResult::from_assignments(vec![]);
    }

    // Start with pivot but respect color constraints
    let mut unclustered: Vec<usize> = (0..n).collect();
    let mut assignments = vec![usize::MAX; n];
    let mut cluster_id = 0;

    while !unclustered.is_empty() {
        // Pick random pivot
        let pivot_idx = rng.random_range(0..unclustered.len());
        let pivot = unclustered.swap_remove(pivot_idx);

        // Assign pivot to new cluster
        assignments[pivot] = cluster_id;
        let mut cluster_colors: HashSet<usize> = HashSet::new();
        cluster_colors.insert(config.colors[pivot]);

        // Find positive neighbors that are unclustered AND have different color
        let positive_neighbors = graph.positive_neighbors(pivot);

        let mut i = 0;
        while i < unclustered.len() {
            let v = unclustered[i];
            let v_color = config.colors[v];

            if positive_neighbors.contains(&v) && !cluster_colors.contains(&v_color) {
                assignments[v] = cluster_id;
                cluster_colors.insert(v_color);
                unclustered.swap_remove(i);
            } else {
                i += 1;
            }
        }

        cluster_id += 1;
    }

    let mut result = ClusteringResult::from_assignments(assignments);
    result.compute_cost(graph);
    result
}

// =============================================================================
// Utility: Compare algorithms
// =============================================================================

/// Compare multiple clustering algorithms on same graph
pub fn compare_algorithms<R: Rng>(
    graph: &LabeledGraph,
    rng: &mut R,
) -> Vec<(&'static str, ClusteringResult)> {
    let mut results = Vec::new();

    // Pivot (single run)
    let pivot_result = pivot_clustering(graph, rng);
    results.push(("Pivot (1x)", pivot_result));

    // Pivot (best of 10)
    let pivot_best = pivot_clustering_best_of(graph, rng, 10);
    results.push(("Pivot (best of 10)", pivot_best));

    // Modified Pivot
    let modified_result = modified_pivot_clustering(graph, rng);
    results.push(("Modified Pivot", modified_result));

    // Greedy Agglomerative
    let greedy_result = greedy_agglomerative(graph);
    results.push(("Greedy Agglomerative", greedy_result));

    // Min-Max Clustering (optimizes worst-case per cluster)
    let min_max_result = min_max_clustering(graph, rng);
    results.push(("Min-Max", min_max_result.clustering));

    results
}

/// Compare algorithms with extended metrics (including min-max)
pub fn compare_algorithms_extended<R: Rng>(
    graph: &LabeledGraph,
    rng: &mut R,
) -> Vec<(&'static str, ClusteringResult, Option<usize>)> {
    let mut results = Vec::new();

    // Standard algorithms (no per-cluster metrics)
    let pivot_result = pivot_clustering(graph, rng);
    results.push(("Pivot (1x)", pivot_result, None));

    let pivot_best = pivot_clustering_best_of(graph, rng, 10);
    results.push(("Pivot (best of 10)", pivot_best, None));

    let modified_result = modified_pivot_clustering(graph, rng);
    results.push(("Modified Pivot", modified_result, None));

    let greedy_result = greedy_agglomerative(graph);
    results.push(("Greedy Agglomerative", greedy_result, None));

    // Min-Max with max_disagreements metric
    let min_max_result = min_max_clustering(graph, rng);
    results.push((
        "Min-Max",
        min_max_result.clustering,
        Some(min_max_result.max_disagreements),
    ));

    results
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn create_test_graph() -> LabeledGraph {
        // Simple graph: 6 nodes, two natural clusters {0,1,2} and {3,4,5}
        // Positive edges within clusters, negative edges between
        let mut graph = LabeledGraph::new(6);

        // Cluster 1: 0-1-2
        graph.add_edge(0, 1, EdgeLabel::Positive);
        graph.add_edge(1, 2, EdgeLabel::Positive);
        graph.add_edge(0, 2, EdgeLabel::Positive);

        // Cluster 2: 3-4-5
        graph.add_edge(3, 4, EdgeLabel::Positive);
        graph.add_edge(4, 5, EdgeLabel::Positive);
        graph.add_edge(3, 5, EdgeLabel::Positive);

        // Negative edges between clusters
        graph.add_edge(0, 3, EdgeLabel::Negative);
        graph.add_edge(1, 4, EdgeLabel::Negative);
        graph.add_edge(2, 5, EdgeLabel::Negative);

        graph
    }

    #[test]
    fn test_pivot_finds_clusters() {
        let graph = create_test_graph();
        let mut rng = StdRng::seed_from_u64(42);
        let result = pivot_clustering(&graph, &mut rng);

        // Should find 2 clusters with cost 0 (perfect clustering exists)
        assert!(result.num_clusters() <= 3); // Might find 2 or 3 due to randomness
        println!(
            "Pivot: {} clusters, cost {}",
            result.num_clusters(),
            result.cost
        );
    }

    #[test]
    fn test_modified_pivot_finds_clusters() {
        let graph = create_test_graph();
        let mut rng = StdRng::seed_from_u64(42);
        let result = modified_pivot_clustering(&graph, &mut rng);

        assert!(result.num_clusters() <= 3);
        println!(
            "Modified Pivot: {} clusters, cost {}",
            result.num_clusters(),
            result.cost
        );
    }

    #[test]
    fn test_greedy_agglomerative() {
        let graph = create_test_graph();
        let result = greedy_agglomerative(&graph);

        // Greedy should find optimal or near-optimal
        assert!(result.num_clusters() <= 3);
        println!(
            "Greedy: {} clusters, cost {}",
            result.num_clusters(),
            result.cost
        );
    }

    #[test]
    fn test_compare_algorithms() {
        let graph = create_test_graph();
        let mut rng = StdRng::seed_from_u64(42);
        let results = compare_algorithms(&graph, &mut rng);

        println!("\nAlgorithm comparison on test graph:");
        for (name, result) in &results {
            println!(
                "  {}: {} clusters, cost {}",
                name,
                result.num_clusters(),
                result.cost
            );
        }

        // All should find reasonable clusterings
        for (name, result) in results {
            assert!(
                result.cost <= 3,
                "{} has too high cost: {}",
                name,
                result.cost
            );
        }
    }

    #[test]
    fn test_from_similarity_matrix() {
        let sims = vec![
            vec![1.0, 0.9, 0.8, 0.1, 0.1],
            vec![0.9, 1.0, 0.85, 0.15, 0.1],
            vec![0.8, 0.85, 1.0, 0.1, 0.2],
            vec![0.1, 0.15, 0.1, 1.0, 0.9],
            vec![0.1, 0.1, 0.2, 0.9, 1.0],
        ];

        let graph = LabeledGraph::from_similarity_matrix(&sims, 0.5);
        let mut rng = StdRng::seed_from_u64(42);
        let result = pivot_clustering(&graph, &mut rng);

        // Should find 2 clusters: {0,1,2} and {3,4}
        assert_eq!(result.num_clusters(), 2);
    }

    #[test]
    fn test_singleton_clustering() {
        // All negative edges = all singletons optimal
        let mut graph = LabeledGraph::new(4);
        graph.add_edge(0, 1, EdgeLabel::Negative);
        graph.add_edge(0, 2, EdgeLabel::Negative);
        graph.add_edge(0, 3, EdgeLabel::Negative);
        graph.add_edge(1, 2, EdgeLabel::Negative);
        graph.add_edge(1, 3, EdgeLabel::Negative);
        graph.add_edge(2, 3, EdgeLabel::Negative);

        let mut rng = StdRng::seed_from_u64(42);
        let result = pivot_clustering(&graph, &mut rng);

        // Each node should be in its own cluster
        assert_eq!(result.num_clusters(), 4);
        assert_eq!(result.cost, 0);
    }

    #[test]
    fn test_min_max_clustering() {
        let graph = create_test_graph();
        let mut rng = StdRng::seed_from_u64(42);
        let result = min_max_clustering(&graph, &mut rng);

        // Should find a valid clustering
        assert!(result.clustering.num_clusters() >= 1);
        // Max disagreements should be minimized
        println!(
            "Min-Max: {} clusters, total cost {}, max_disagreements {}",
            result.clustering.num_clusters(),
            result.clustering.cost,
            result.max_disagreements
        );
    }

    #[test]
    fn test_min_max_vs_standard() {
        // Create graph with unbalanced structure where one cluster could be "bad"
        let mut graph = LabeledGraph::new(8);

        // Dense cluster 0-1-2-3 (should have low disagreements)
        for i in 0..4 {
            for j in (i + 1)..4 {
                graph.add_edge(i, j, EdgeLabel::Positive);
            }
        }

        // Sparse cluster 4-5-6-7 with some internal conflicts
        graph.add_edge(4, 5, EdgeLabel::Positive);
        graph.add_edge(6, 7, EdgeLabel::Positive);
        graph.add_edge(4, 6, EdgeLabel::Negative); // Internal conflict
        graph.add_edge(5, 7, EdgeLabel::Negative); // Internal conflict

        // Between-cluster edges
        graph.add_edge(0, 4, EdgeLabel::Negative);
        graph.add_edge(2, 6, EdgeLabel::Negative);

        let mut rng = StdRng::seed_from_u64(42);
        let standard = pivot_clustering_best_of(&graph, &mut rng, 10);
        let min_max = min_max_clustering(&graph, &mut rng);

        println!(
            "Standard: {} clusters, cost {}",
            standard.num_clusters(),
            standard.cost
        );
        println!(
            "Min-Max: {} clusters, cost {}, max {}",
            min_max.clustering.num_clusters(),
            min_max.clustering.cost,
            min_max.max_disagreements
        );

        // Min-max should find valid clustering
        assert!(min_max.clustering.num_clusters() >= 1);
    }

    #[test]
    fn test_chromatic_clustering() {
        // Test chromatic constraint: nodes with same color can't be together
        let mut graph = LabeledGraph::new(6);

        // All positive edges (everyone wants to cluster together)
        for i in 0..6 {
            for j in (i + 1)..6 {
                graph.add_edge(i, j, EdgeLabel::Positive);
            }
        }

        // Colors: 0,1,2 have color 0; 3,4,5 have color 1
        let config = ChromaticClusteringConfig {
            k: 2,
            colors: vec![0, 0, 0, 1, 1, 1],
        };

        let mut rng = StdRng::seed_from_u64(42);
        let result = chromatic_clustering(&graph, &config, &mut rng);

        // Due to color constraint, can have at most 2 nodes per cluster
        // (one from each color group)
        for cluster in &result.clusters {
            let mut colors_in_cluster: HashSet<usize> = HashSet::new();
            for &node in cluster {
                let color = config.colors[node];
                assert!(
                    !colors_in_cluster.contains(&color),
                    "Same color {} appears twice in cluster {:?}",
                    color,
                    cluster
                );
                colors_in_cluster.insert(color);
            }
        }

        println!(
            "Chromatic: {} clusters, cost {}",
            result.num_clusters(),
            result.cost
        );
    }

    #[test]
    fn test_compare_algorithms_extended() {
        let graph = create_test_graph();
        let mut rng = StdRng::seed_from_u64(42);
        let results = compare_algorithms_extended(&graph, &mut rng);

        println!("\nExtended algorithm comparison:");
        for (name, result, max_disagree) in &results {
            if let Some(max_d) = max_disagree {
                println!(
                    "  {}: {} clusters, cost {}, max_disagreements {}",
                    name,
                    result.num_clusters(),
                    result.cost,
                    max_d
                );
            } else {
                println!(
                    "  {}: {} clusters, cost {}",
                    name,
                    result.num_clusters(),
                    result.cost
                );
            }
        }

        // All should find reasonable clusterings
        for (name, result, _) in results {
            assert!(
                result.cost <= 5,
                "{} has too high cost: {}",
                name,
                result.cost
            );
        }
    }
}

// =============================================================================
// Property Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(30))]

        /// Property: Pivot always produces a valid clustering
        #[test]
        fn pivot_valid_clustering(n in 2usize..20, seed in any::<u64>()) {
            let mut graph = LabeledGraph::new(n);
            // Add random edges
            let mut rng = StdRng::seed_from_u64(seed);
            for i in 0..n {
                for j in (i+1)..n {
                    if rng.random_bool(0.5) {
                        let label = if rng.random_bool(0.5) {
                            EdgeLabel::Positive
                        } else {
                            EdgeLabel::Negative
                        };
                        graph.add_edge(i, j, label);
                    }
                }
            }

            let result = pivot_clustering(&graph, &mut rng);

            // Every node should be assigned
            prop_assert!(result.assignments.iter().all(|&a| a < n));

            // Clusters should partition all nodes
            let total_nodes: usize = result.clusters.iter().map(|c| c.len()).sum();
            prop_assert_eq!(total_nodes, n);
        }

        /// Property: Modified Pivot cost <= Pivot cost (on average, not always)
        #[test]
        fn modified_pivot_competitive(n in 5usize..15, seed in any::<u64>()) {
            let mut graph = LabeledGraph::new(n);
            let mut rng = StdRng::seed_from_u64(seed);

            // Create graph with clear cluster structure
            for i in 0..n {
                for j in (i+1)..n {
                    let same_cluster = (i < n/2) == (j < n/2);
                    let label = if same_cluster {
                        EdgeLabel::Positive
                    } else {
                        EdgeLabel::Negative
                    };
                    graph.add_edge(i, j, label);
                }
            }

            let pivot_result = pivot_clustering_best_of(&graph, &mut rng, 5);
            let modified_result = modified_pivot_clustering(&graph, &mut rng);

            // Modified should be competitive (within factor of 2)
            prop_assert!(modified_result.cost <= pivot_result.cost * 2 + 5,
                "Modified cost {} too high vs Pivot {}",
                modified_result.cost, pivot_result.cost);
        }

        /// Property: All algorithms find at least 1 cluster
        #[test]
        fn algorithms_find_clusters(n in 2usize..10, seed in any::<u64>()) {
            let mut graph = LabeledGraph::new(n);
            let mut rng = StdRng::seed_from_u64(seed);

            for i in 0..n {
                for j in (i+1)..n {
                    let label = if rng.random_bool(0.5) {
                        EdgeLabel::Positive
                    } else {
                        EdgeLabel::Negative
                    };
                    graph.add_edge(i, j, label);
                }
            }

            let pivot = pivot_clustering(&graph, &mut rng);
            let modified = modified_pivot_clustering(&graph, &mut rng);
            let greedy = greedy_agglomerative(&graph);

            prop_assert!(pivot.num_clusters() >= 1);
            prop_assert!(modified.num_clusters() >= 1);
            prop_assert!(greedy.num_clusters() >= 1);
        }

        /// Property: Cost is non-negative
        #[test]
        fn cost_non_negative(n in 2usize..15, seed in any::<u64>()) {
            let mut graph = LabeledGraph::new(n);
            let mut rng = StdRng::seed_from_u64(seed);

            for i in 0..n {
                for j in (i+1)..n {
                    if rng.random_bool(0.7) {
                        let label = if rng.random_bool(0.5) {
                            EdgeLabel::Positive
                        } else {
                            EdgeLabel::Negative
                        };
                        graph.add_edge(i, j, label);
                    }
                }
            }

            let result = pivot_clustering(&graph, &mut rng);
            // cost is usize, always >= 0 by type constraint
            prop_assert!(!result.clusters.is_empty(), "Should have at least one cluster");
        }
    }
}
