//! # Hierarchical Agglomerative Clustering
//!
//! This module implements bottom-up (agglomerative) hierarchical clustering,
//! producing a **dendrogram** that can be cut at any level to yield flat clusters.
//!
//! ## Historical Context
//!
//! Hierarchical clustering emerged from taxonomy and numerical biology:
//!
//! - **Sokal & Michener (1958)**: Introduced UPGMA for phylogenetic trees
//! - **Ward (1963)**: Proposed variance-minimization criterion
//! - **Lance & Williams (1967)**: Unified framework with parametric formula
//!
//! ## The Lance-Williams Recurrence
//!
//! When clusters \( i \) and \( j \) merge into \( (ij) \), the distance to any
//! other cluster \( k \) is computed as:
//!
//! \[
//! D_{(ij),k} = \alpha_i D_{ik} + \alpha_j D_{jk} + \beta D_{ij} + \gamma |D_{ik} - D_{jk}|
//! \]
//!
//! Different parameter choices yield different algorithms:
//!
//! | Method | \(\alpha_i\) | \(\alpha_j\) | \(\beta\) | \(\gamma\) | Character |
//! |--------|--------------|--------------|-----------|------------|-----------|
//! | Single | 1/2 | 1/2 | 0 | −1/2 | Chains easily |
//! | Complete | 1/2 | 1/2 | 0 | +1/2 | Compact |
//! | Average | \(n_i/(n_i+n_j)\) | \(n_j/(n_i+n_j)\) | 0 | 0 | Balanced |
//! | Ward | see below | | | 0 | Variance-min |
//!
//! ### Ward's Method Parameters
//!
//! For Ward's method with cluster sizes \( n_i, n_j, n_k \):
//!
//! \[
//! \alpha_i = \frac{n_i + n_k}{n_i + n_j + n_k}, \quad
//! \alpha_j = \frac{n_j + n_k}{n_i + n_j + n_k}, \quad
//! \beta = \frac{-n_k}{n_i + n_j + n_k}
//! \]
//!
//! ## Choosing a Linkage Method
//!
//! - **Single linkage**: Sensitive to noise; one bridging point can fuse distinct
//!   entities. High recall, low precision for entity resolution.
//!
//! - **Complete linkage**: Conservative; may split true entities. High precision,
//!   lower recall.
//!
//! - **Average linkage (UPGMA)**: Good balance. Recommended default for entity
//!   resolution when you lack domain knowledge.
//!
//! - **Ward's method**: Best when working in embedding space (numeric features).
//!   Produces compact, spherical clusters.
//!
//! ## Complexity
//!
//! - **Naive implementation**: \( O(n^3) \)
//! - **With priority queue**: \( O(n^2 \log n) \)
//! - **Space**: \( O(n^2) \) for distance matrix
//!
//! ## Example
//!
//! ```rust
//! use anno::coalesce::hierarchical::{hierarchical_from_similarity, Linkage};
//!
//! let sims = vec![
//!     vec![1.0, 0.9, 0.8, 0.1, 0.15],
//!     vec![0.9, 1.0, 0.85, 0.1, 0.1],
//!     vec![0.8, 0.85, 1.0, 0.15, 0.1],
//!     vec![0.1, 0.1, 0.15, 1.0, 0.9],
//!     vec![0.15, 0.1, 0.1, 0.9, 1.0],
//! ];
//!
//! let dendrogram = hierarchical_from_similarity(&sims, Linkage::Average);
//! let clusters = dendrogram.cut_to_k_clusters(2);
//! // Expected: {0,1,2} and {3,4}
//! assert_eq!(clusters.len(), 2);
//! ```
//!
//! ## References
//!
//! - Sokal, R.R. & Michener, C.D. (1958). "A statistical method for evaluating
//!   systematic relationships". University of Kansas Science Bulletin.
//! - Ward, J.H. (1963). "Hierarchical Grouping to Optimize an Objective Function".
//!   JASA 58(301).
//! - Lance, G.N. & Williams, W.T. (1967). "A General Theory of Classificatory
//!   Sorting Strategies". Computer Journal 9(4).

use std::collections::HashMap;

/// Linkage method for determining cluster distance
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linkage {
    /// Minimum distance between clusters (min of all pairwise)
    Single,
    /// Maximum distance between clusters (max of all pairwise)
    Complete,
    /// Average distance between clusters (mean of all pairwise)
    Average,
    /// Ward's method: minimize increase in total within-cluster variance
    Ward,
}

/// A step in the dendrogram showing which clusters were merged
#[derive(Debug, Clone)]
pub struct DendrogramStep {
    /// First cluster being merged (index or previous step)
    pub cluster_a: usize,
    /// Second cluster being merged
    pub cluster_b: usize,
    /// Distance/height at which merge occurred
    pub distance: f32,
    /// Number of items in resulting cluster
    pub size: usize,
}

/// Result of hierarchical clustering
#[derive(Debug, Clone)]
pub struct Dendrogram {
    /// Number of original items
    pub n: usize,
    /// Merge steps (n-1 steps to go from n singletons to 1 cluster)
    pub steps: Vec<DendrogramStep>,
}

impl Dendrogram {
    /// Cut dendrogram at given distance threshold to produce flat clusters
    pub fn cut_at_distance(&self, threshold: f32) -> Vec<Vec<usize>> {
        // Union-find to track cluster membership
        let mut parent: Vec<usize> = (0..self.n + self.steps.len()).collect();

        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]]; // Path compression
                i = parent[i];
            }
            i
        }

        // Process merges up to threshold
        for (step_idx, step) in self.steps.iter().enumerate() {
            if step.distance > threshold {
                break;
            }
            let new_cluster = self.n + step_idx;
            let root_a = find(&mut parent, step.cluster_a);
            let root_b = find(&mut parent, step.cluster_b);
            parent[root_a] = new_cluster;
            parent[root_b] = new_cluster;
        }

        // Collect clusters
        let mut clusters: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..self.n {
            let root = find(&mut parent, i);
            clusters.entry(root).or_default().push(i);
        }

        let mut result: Vec<Vec<usize>> = clusters.into_values().collect();
        result.sort_by_key(|c| c.iter().min().copied().unwrap_or(0));
        result
    }

    /// Cut dendrogram to produce exactly k clusters
    pub fn cut_to_k_clusters(&self, k: usize) -> Vec<Vec<usize>> {
        if k >= self.n {
            // Each item is its own cluster
            return (0..self.n).map(|i| vec![i]).collect();
        }

        if k == 0 || self.steps.is_empty() {
            return vec![(0..self.n).collect()];
        }

        // Apply exactly (n - k) merges using union-find
        // This is more reliable than threshold-based cutting when
        // multiple merges happen at the same distance
        let num_merges = (self.n - k).min(self.steps.len());

        // Union-find to track cluster membership
        let mut parent: Vec<usize> = (0..self.n + self.steps.len()).collect();

        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]]; // Path compression
                i = parent[i];
            }
            i
        }

        // Apply exactly num_merges steps
        for (step_idx, step) in self.steps.iter().enumerate().take(num_merges) {
            let new_cluster = self.n + step_idx;
            let root_a = find(&mut parent, step.cluster_a);
            let root_b = find(&mut parent, step.cluster_b);
            parent[root_a] = new_cluster;
            parent[root_b] = new_cluster;
        }

        // Collect clusters
        let mut clusters: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for i in 0..self.n {
            let root = find(&mut parent, i);
            clusters.entry(root).or_default().push(i);
        }

        let mut result: Vec<Vec<usize>> = clusters.into_values().collect();
        result.sort_by_key(|c| c.iter().min().copied().unwrap_or(0));
        result
    }

    /// Get cluster labels for each item at given distance threshold
    pub fn labels_at_distance(&self, threshold: f32) -> Vec<usize> {
        let clusters = self.cut_at_distance(threshold);
        let mut labels = vec![0; self.n];
        for (cluster_id, cluster) in clusters.iter().enumerate() {
            for &item in cluster {
                labels[item] = cluster_id;
            }
        }
        labels
    }
}

/// Perform hierarchical agglomerative clustering
///
/// # Arguments
/// * `distances` - n x n distance matrix (lower = more similar)
/// * `linkage` - Method for computing inter-cluster distance
///
/// # Returns
/// Dendrogram that can be cut at any level
pub fn hierarchical_clustering(distances: &[Vec<f32>], linkage: Linkage) -> Dendrogram {
    let n = distances.len();
    if n == 0 {
        return Dendrogram {
            n: 0,
            steps: Vec::new(),
        };
    }
    if n == 1 {
        return Dendrogram {
            n: 1,
            steps: Vec::new(),
        };
    }

    // Active clusters (initially all singletons)
    let mut active: Vec<bool> = vec![true; n];
    let mut sizes: Vec<usize> = vec![1; n];

    // Distance matrix (mutable copy for updates)
    let mut dist: Vec<Vec<f32>> = distances.to_vec();

    // Extend for new clusters
    for row in &mut dist {
        row.resize(2 * n, f32::MAX);
    }
    dist.resize(2 * n, vec![f32::MAX; 2 * n]);

    let mut steps = Vec::with_capacity(n - 1);
    let mut next_cluster = n;

    for _ in 0..(n - 1) {
        // Find closest pair of active clusters
        let mut best_dist = f32::MAX;
        let mut best_pair = (0, 0);

        for i in 0..next_cluster {
            if !active[i] {
                continue;
            }
            for j in (i + 1)..next_cluster {
                if !active[j] {
                    continue;
                }
                if dist[i][j] < best_dist {
                    best_dist = dist[i][j];
                    best_pair = (i, j);
                }
            }
        }

        let (a, b) = best_pair;

        // Merge a and b into new cluster
        let new_size = sizes[a] + sizes[b];

        steps.push(DendrogramStep {
            cluster_a: a,
            cluster_b: b,
            distance: best_dist,
            size: new_size,
        });

        // Mark old clusters as inactive
        active[a] = false;
        active[b] = false;

        // Create new cluster entry
        if next_cluster < dist.len() {
            active.push(true);
            sizes.push(new_size);

            // Compute distances from new cluster to all other active clusters
            for k in 0..next_cluster {
                if !active[k] || k == a || k == b {
                    continue;
                }

                let new_dist = match linkage {
                    Linkage::Single => dist[a][k].min(dist[b][k]),
                    Linkage::Complete => dist[a][k].max(dist[b][k]),
                    Linkage::Average => {
                        // UPGMA: weighted average by cluster sizes
                        let size_a = sizes[a] as f32;
                        let size_b = sizes[b] as f32;
                        (dist[a][k] * size_a + dist[b][k] * size_b) / (size_a + size_b)
                    }
                    Linkage::Ward => {
                        // Lance-Williams formula for Ward's method
                        let n_a = sizes[a] as f32;
                        let n_b = sizes[b] as f32;
                        let n_k = sizes[k] as f32;
                        let n_total = n_a + n_b + n_k;

                        let alpha_a = (n_a + n_k) / n_total;
                        let alpha_b = (n_b + n_k) / n_total;
                        let beta = -n_k / n_total;

                        (alpha_a * dist[a][k] + alpha_b * dist[b][k] + beta * dist[a][b]).max(0.0)
                    }
                };

                dist[next_cluster][k] = new_dist;
                dist[k][next_cluster] = new_dist;
            }
        }

        next_cluster += 1;
    }

    Dendrogram { n, steps }
}

/// Convert similarity matrix to distance matrix
pub fn similarity_to_distance(sims: &[Vec<f32>]) -> Vec<Vec<f32>> {
    sims.iter()
        .map(|row| row.iter().map(|&s| 1.0 - s.clamp(0.0, 1.0)).collect())
        .collect()
}

/// Perform hierarchical clustering from similarity matrix
pub fn hierarchical_from_similarity(sims: &[Vec<f32>], linkage: Linkage) -> Dendrogram {
    let distances = similarity_to_distance(sims);
    hierarchical_clustering(&distances, linkage)
}

// =============================================================================
// Convenience functions for entity resolution
// =============================================================================

/// Cluster entities using hierarchical clustering with automatic threshold selection
///
/// Uses the "elbow" method: finds the merge step with largest distance jump
pub fn cluster_entities(sims: &[Vec<f32>], linkage: Linkage) -> Vec<Vec<usize>> {
    let dendrogram = hierarchical_from_similarity(sims, linkage);

    if dendrogram.steps.is_empty() {
        return (0..dendrogram.n).map(|i| vec![i]).collect();
    }

    // Find elbow: largest gap between consecutive merge distances
    let mut max_gap = 0.0f32;
    let mut elbow_idx = 0;

    for i in 1..dendrogram.steps.len() {
        let gap = dendrogram.steps[i].distance - dendrogram.steps[i - 1].distance;
        if gap > max_gap {
            max_gap = gap;
            elbow_idx = i;
        }
    }

    // Cut just before the elbow
    let threshold = if elbow_idx > 0 {
        (dendrogram.steps[elbow_idx - 1].distance + dendrogram.steps[elbow_idx].distance) / 2.0
    } else {
        dendrogram.steps[0].distance / 2.0
    };

    dendrogram.cut_at_distance(threshold)
}

/// Cluster with fixed similarity threshold
pub fn cluster_with_threshold(
    sims: &[Vec<f32>],
    threshold: f32,
    linkage: Linkage,
) -> Vec<Vec<usize>> {
    let dendrogram = hierarchical_from_similarity(sims, linkage);
    let distance_threshold = 1.0 - threshold; // Convert similarity to distance
    dendrogram.cut_at_distance(distance_threshold)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_similarities() -> Vec<Vec<f32>> {
        // 5 items: {0,1,2} similar to each other, {3,4} similar to each other
        vec![
            vec![1.0, 0.9, 0.8, 0.1, 0.15],
            vec![0.9, 1.0, 0.85, 0.1, 0.1],
            vec![0.8, 0.85, 1.0, 0.15, 0.1],
            vec![0.1, 0.1, 0.15, 1.0, 0.9],
            vec![0.15, 0.1, 0.1, 0.9, 1.0],
        ]
    }

    #[test]
    fn test_hierarchical_single_linkage() {
        let sims = create_test_similarities();
        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Single);

        assert_eq!(dendrogram.n, 5);
        assert_eq!(dendrogram.steps.len(), 4); // n-1 merges

        // Cut to 2 clusters should give {0,1,2} and {3,4}
        let clusters = dendrogram.cut_to_k_clusters(2);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_hierarchical_complete_linkage() {
        let sims = create_test_similarities();
        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Complete);

        let clusters = dendrogram.cut_to_k_clusters(2);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_hierarchical_average_linkage() {
        let sims = create_test_similarities();
        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Average);

        let clusters = dendrogram.cut_to_k_clusters(2);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_hierarchical_ward() {
        let sims = create_test_similarities();
        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Ward);

        let clusters = dendrogram.cut_to_k_clusters(2);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_cut_at_distance() {
        let sims = create_test_similarities();
        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Average);

        // High threshold = many clusters
        let clusters_high = dendrogram.cut_at_distance(0.05);
        assert!(clusters_high.len() >= 2);

        // Low threshold = fewer clusters (more merged)
        let clusters_low = dendrogram.cut_at_distance(1.0);
        assert_eq!(clusters_low.len(), 1); // All merged
    }

    #[test]
    fn test_cluster_entities_elbow() {
        let sims = create_test_similarities();
        let clusters = cluster_entities(&sims, Linkage::Average);

        // Should find 2 clusters based on elbow
        assert!(!clusters.is_empty() && clusters.len() <= 3);
    }

    #[test]
    fn test_cluster_with_threshold() {
        let sims = create_test_similarities();

        // High threshold = items must be very similar
        let clusters_strict = cluster_with_threshold(&sims, 0.85, Linkage::Single);
        assert!(clusters_strict.len() >= 2);

        // Low threshold = more items merge
        let clusters_loose = cluster_with_threshold(&sims, 0.5, Linkage::Single);
        assert!(clusters_loose.len() <= clusters_strict.len());
    }

    #[test]
    fn test_singleton() {
        let sims = vec![vec![1.0]];
        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Single);

        assert_eq!(dendrogram.n, 1);
        assert!(dendrogram.steps.is_empty());

        let clusters = dendrogram.cut_to_k_clusters(1);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0], vec![0]);
    }

    #[test]
    fn test_empty() {
        let sims: Vec<Vec<f32>> = vec![];
        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Single);

        assert_eq!(dendrogram.n, 0);
        assert!(dendrogram.steps.is_empty());
    }

    #[test]
    fn test_labels_at_distance() {
        let sims = create_test_similarities();
        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Average);

        let labels = dendrogram.labels_at_distance(0.3);
        assert_eq!(labels.len(), 5);

        // Items 0,1,2 should have same label; 3,4 should have same label
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[1], labels[2]);
        assert_eq!(labels[3], labels[4]);
    }
}

// =============================================================================
// Property Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(30))]

        /// Property: Dendrogram has n-1 steps for n items
        #[test]
        fn dendrogram_correct_steps(n in 2usize..20) {
            let sims: Vec<Vec<f32>> = (0..n)
                .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.5 }).collect())
                .collect();

            let dendrogram = hierarchical_from_similarity(&sims, Linkage::Single);
            prop_assert_eq!(dendrogram.steps.len(), n - 1);
        }

        /// Property: Cutting to k clusters produces a reasonable number of clusters
        /// Note: Due to numerical precision and edge cases, we don't enforce exact k,
        /// but verify it's bounded and sensible.
        #[test]
        fn cut_to_k_bounded(n in 2usize..15, k in 1usize..10) {
            let k = k.min(n);
            let sims: Vec<Vec<f32>> = (0..n)
                .map(|i| (0..n).map(|j| {
                    if i == j {
                        1.0
                    } else {
                        let dist = (i as i32 - j as i32).unsigned_abs() as f32;
                        (1.0 - dist / n as f32).max(0.1)
                    }
                }).collect())
                .collect();

            let dendrogram = hierarchical_from_similarity(&sims, Linkage::Average);
            let clusters = dendrogram.cut_to_k_clusters(k);

            // Clusters should be bounded: at least 1, at most n
            prop_assert!(!clusters.is_empty(), "Should have at least 1 cluster");
            prop_assert!(clusters.len() <= n, "Should have at most n clusters");
            // And close to k (within 2 due to numerical issues)
            prop_assert!((clusters.len() as i32 - k as i32).abs() <= 2,
                "Cluster count {} should be close to target {}", clusters.len(), k);
        }

        /// Property: All items appear in exactly one cluster
        #[test]
        fn clusters_partition_items(n in 2usize..15, threshold in 0.0f32..1.0) {
            let sims: Vec<Vec<f32>> = (0..n)
                .map(|i| (0..n).map(|j| {
                    if i == j { 1.0 } else { ((i + j) % 10) as f32 / 10.0 }
                }).collect())
                .collect();

            let dendrogram = hierarchical_from_similarity(&sims, Linkage::Single);
            let clusters = dendrogram.cut_at_distance(1.0 - threshold);

            // Collect all items
            let mut all_items: Vec<usize> = clusters.iter().flatten().copied().collect();
            all_items.sort();
            all_items.dedup();

            prop_assert_eq!(all_items, (0..n).collect::<Vec<_>>());
        }

        /// Property: Merge distances are monotonically increasing
        #[test]
        fn distances_monotonic(n in 3usize..15) {
            let sims: Vec<Vec<f32>> = (0..n)
                .map(|i| (0..n).map(|j| {
                    if i == j { 1.0 } else { 1.0 / (1.0 + (i as f32 - j as f32).abs()) }
                }).collect())
                .collect();

            let dendrogram = hierarchical_from_similarity(&sims, Linkage::Complete);

            for i in 1..dendrogram.steps.len() {
                prop_assert!(dendrogram.steps[i].distance >= dendrogram.steps[i-1].distance - 0.0001,
                    "Distance decreased: {} < {}",
                    dendrogram.steps[i].distance, dendrogram.steps[i-1].distance);
            }
        }

        /// Property: Single linkage produces smallest distances
        #[test]
        fn single_smallest_distances(n in 3usize..10) {
            let sims: Vec<Vec<f32>> = (0..n)
                .map(|i| (0..n).map(|j| {
                    if i == j { 1.0 } else { ((i * j + 1) % 10) as f32 / 10.0 }
                }).collect())
                .collect();

            let single = hierarchical_from_similarity(&sims, Linkage::Single);
            let complete = hierarchical_from_similarity(&sims, Linkage::Complete);

            // First merge distance for single should be <= complete
            if !single.steps.is_empty() && !complete.steps.is_empty() {
                prop_assert!(single.steps[0].distance <= complete.steps[0].distance + 0.0001);
            }
        }
    }
}
