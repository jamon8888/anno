//! Integration property tests for anno-coalesce.
//!
//! These tests verify invariants across the coalesce algorithms:
//! - Clustering invariants (partitioning, bounds)
//! - Similarity function properties (symmetry, reflexivity)
//! - Conversion correctness

use anno_coalesce::{
    correlation::{greedy_agglomerative, pivot_clustering, EdgeLabel, LabeledGraph},
    hierarchical::{hierarchical_from_similarity, Linkage},
    lsh::MinHashLSH,
    streaming::{StreamingConfig, StreamingResolver},
};
use proptest::prelude::*;
use rand::SeedableRng;
use std::collections::HashSet;

// =============================================================================
// Similarity Function Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// String similarity is symmetric: sim(a, b) == sim(b, a)
    #[test]
    fn string_similarity_symmetric(
        a in "[A-Za-z ]{0,50}",
        b in "[A-Za-z ]{0,50}"
    ) {
        let sim_ab = anno_coalesce::streaming::trigram_similarity(&a, &b);
        let sim_ba = anno_coalesce::streaming::trigram_similarity(&b, &a);
        prop_assert!((sim_ab - sim_ba).abs() < 0.0001,
            "Symmetry violated: sim({:?}, {:?}) = {} but sim({:?}, {:?}) = {}",
            a, b, sim_ab, b, a, sim_ba);
    }

    /// String similarity is reflexive: sim(a, a) == 1.0 for non-empty strings
    #[test]
    fn string_similarity_reflexive(a in "[A-Za-z]{1,50}") {
        let sim = anno_coalesce::streaming::trigram_similarity(&a, &a);
        prop_assert!((sim - 1.0).abs() < 0.0001,
            "Reflexivity violated: sim({:?}, {:?}) = {} (expected 1.0)",
            a, a, sim);
    }

    /// String similarity is bounded: 0 <= sim(a, b) <= 1
    #[test]
    fn string_similarity_bounded(
        a in ".*",
        b in ".*"
    ) {
        let sim = anno_coalesce::streaming::trigram_similarity(&a, &b);
        prop_assert!(sim >= 0.0 && sim <= 1.0,
            "Bounds violated: sim({:?}, {:?}) = {} (expected [0, 1])",
            a, b, sim);
    }

    /// Cosine similarity is symmetric
    #[test]
    fn cosine_similarity_symmetric(dim in 1usize..100, seed in any::<u64>()) {
        let (a, b) = random_vectors(dim, seed);
        let sim_ab = anno_coalesce::streaming::cosine_similarity(&a, &b);
        let sim_ba = anno_coalesce::streaming::cosine_similarity(&b, &a);
        prop_assert!((sim_ab - sim_ba).abs() < 0.0001,
            "Cosine symmetry violated: {} vs {}", sim_ab, sim_ba);
    }

    /// Cosine similarity is bounded for positive vectors
    #[test]
    fn cosine_similarity_bounded(dim in 1usize..100, seed in any::<u64>()) {
        let (a, b) = random_positive_vectors(dim, seed);
        let sim = anno_coalesce::streaming::cosine_similarity(&a, &b);
        // For positive vectors, cosine is in [0, 1]
        prop_assert!(sim >= -0.001 && sim <= 1.001,
            "Cosine bounds violated: {} (expected [0, 1])", sim);
    }
}

// =============================================================================
// Clustering Invariants
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// All items appear in exactly one cluster (partitioning property)
    #[test]
    fn streaming_clusters_partition_items(
        items in prop::collection::vec("[A-Za-z]{3,15}", 1..30)
    ) {
        let mut resolver = StreamingResolver::new(StreamingConfig::default());

        for (i, item) in items.iter().enumerate() {
            resolver.add_entity(format!("doc{}", i), item, None);
        }

        // Collect all items from clusters
        let mut clustered_items = Vec::new();
        for cluster in resolver.clusters() {
            for mention in &cluster.mentions {
                clustered_items.push(mention.canonical_surface.clone());
            }
        }

        // Every input item should appear exactly once in output
        prop_assert_eq!(clustered_items.len(), items.len(),
            "Item count mismatch: {} clustered vs {} input",
            clustered_items.len(), items.len());
    }

    /// Hierarchical clustering produces valid partition at any cut
    #[test]
    fn hierarchical_valid_partition(n in 2usize..15, k in 1usize..10) {
        let k = k.min(n);
        let sims = random_similarity_matrix(n);

        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Average);
        let clusters = dendrogram.cut_to_k_clusters(k);

        // Collect all items
        let all_items: HashSet<usize> = clusters.iter().flatten().copied().collect();

        // Should be a valid partition
        prop_assert_eq!(all_items.len(), n,
            "Partition missing items: got {:?}", all_items);

        // No duplicates
        let total: usize = clusters.iter().map(|c| c.len()).sum();
        prop_assert_eq!(total, n,
            "Partition has duplicates: {} items in {} slots", n, total);
    }

    /// Correlation clustering produces valid partition
    #[test]
    fn correlation_valid_partition(n in 2usize..15, seed in any::<u64>()) {
        let graph = random_labeled_graph(n, seed);
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

        let result = pivot_clustering(&graph, &mut rng);

        // All nodes should be assigned
        prop_assert_eq!(result.assignments.len(), n,
            "Wrong assignment count");

        // All assignments valid
        for &a in &result.assignments {
            prop_assert!(a < n, "Invalid assignment: {} >= {}", a, n);
        }

        // Clusters should partition nodes
        let total: usize = result.clusters.iter().map(|c| c.len()).sum();
        prop_assert_eq!(total, n, "Partition size mismatch");
    }

    /// Greedy agglomerative produces valid partition
    #[test]
    fn greedy_valid_partition(n in 2usize..12, seed in any::<u64>()) {
        let graph = random_labeled_graph(n, seed);
        let result = greedy_agglomerative(&graph);

        let total: usize = result.clusters.iter().map(|c| c.len()).sum();
        prop_assert_eq!(total, n, "Greedy partition size mismatch");
    }
}

// =============================================================================
// LSH Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// LSH: identical strings always become candidates
    #[test]
    fn lsh_identical_strings_candidates(text in "[A-Za-z]{5,20}") {
        let mut lsh = MinHashLSH::new(Default::default());
        lsh.insert_text("1", &text);
        lsh.insert_text("2", &text);

        let pairs = lsh.candidate_pairs();
        prop_assert!(!pairs.is_empty(),
            "Identical strings should be candidates: {:?}", text);
    }

    /// LSH: estimated similarity ≈ exact for identical strings
    #[test]
    fn lsh_estimated_exact_match(text in "[A-Za-z]{5,20}") {
        let mut lsh = MinHashLSH::new(Default::default());
        lsh.insert_text("1", &text);
        lsh.insert_text("2", &text);

        let estimated = lsh.estimated_similarity(0, 1).unwrap_or(0.0);
        let exact = lsh.exact_similarity(0, 1).unwrap_or(0.0);

        // Both should be 1.0 (or very close)
        prop_assert!((exact - 1.0).abs() < 0.01,
            "Exact similarity should be 1.0, got {}", exact);
        prop_assert!(estimated > 0.8,
            "Estimated similarity should be high, got {}", estimated);
    }

    /// LSH: candidate count bounded by n*(n-1)/2
    #[test]
    fn lsh_candidate_count_bounded(items in prop::collection::vec("[A-Za-z ]{5,25}", 2..25)) {
        let mut lsh = MinHashLSH::new(Default::default());
        for (i, item) in items.iter().enumerate() {
            lsh.insert_text(i.to_string(), item);
        }

        let pairs = lsh.candidate_pairs();
        let max_pairs = items.len() * (items.len() - 1) / 2;
        prop_assert!(pairs.len() <= max_pairs,
            "Too many pairs: {} > max {}", pairs.len(), max_pairs);
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn random_vectors(dim: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let mut rng = seed;
    let a: Vec<f32> = (0..dim)
        .map(|_| {
            rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
            (rng % 2000) as f32 / 1000.0 - 1.0 // [-1, 1]
        })
        .collect();
    let b: Vec<f32> = (0..dim)
        .map(|_| {
            rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
            (rng % 2000) as f32 / 1000.0 - 1.0
        })
        .collect();
    (a, b)
}

fn random_positive_vectors(dim: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let mut rng = seed;
    let a: Vec<f32> = (0..dim)
        .map(|_| {
            rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
            (rng % 1000) as f32 / 1000.0 + 0.001 // (0, 1]
        })
        .collect();
    let b: Vec<f32> = (0..dim)
        .map(|_| {
            rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
            (rng % 1000) as f32 / 1000.0 + 0.001
        })
        .collect();
    (a, b)
}

fn random_similarity_matrix(n: usize) -> Vec<Vec<f32>> {
    let mut rng = 42u64;
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    if i == j {
                        1.0
                    } else {
                        rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                        (rng % 100) as f32 / 100.0
                    }
                })
                .collect()
        })
        .collect()
}

fn random_labeled_graph(n: usize, seed: u64) -> LabeledGraph {
    let mut graph = LabeledGraph::new(n);
    let mut rng = seed;

    for i in 0..n {
        for j in (i + 1)..n {
            rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
            if rng % 2 == 0 {
                let label = if rng % 4 < 2 {
                    EdgeLabel::Positive
                } else {
                    EdgeLabel::Negative
                };
                graph.add_edge(i, j, label);
            }
        }
    }
    graph
}

// =============================================================================
// Adversarial Input Tests (Fuzz-like)
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    // -------------------------------------------------------------------------
    // Unicode Edge Cases
    // -------------------------------------------------------------------------

    /// String similarity handles CJK characters
    #[test]
    fn string_similarity_cjk(
        a in "[\\p{Han}]{0,20}",
        b in "[\\p{Han}]{0,20}"
    ) {
        let sim = anno_coalesce::streaming::trigram_similarity(&a, &b);
        prop_assert!(sim >= 0.0 && sim <= 1.0,
            "CJK similarity out of bounds: sim({:?}, {:?}) = {}", a, b, sim);
    }

    /// String similarity handles emoji
    #[test]
    fn string_similarity_emoji(
        a in "[\\p{Emoji}]{0,10}",
        b in "[\\p{Emoji}]{0,10}"
    ) {
        let sim = anno_coalesce::streaming::trigram_similarity(&a, &b);
        prop_assert!(sim >= 0.0 && sim <= 1.0,
            "Emoji similarity out of bounds: {}", sim);
    }

    /// String similarity handles mixed scripts
    #[test]
    fn string_similarity_mixed_scripts(
        a in "[A-Za-z\\p{Han}\\p{Cyrillic}]{0,30}",
        b in "[A-Za-z\\p{Han}\\p{Cyrillic}]{0,30}"
    ) {
        let sim = anno_coalesce::streaming::trigram_similarity(&a, &b);
        prop_assert!(sim >= 0.0 && sim <= 1.0,
            "Mixed script similarity out of bounds: {}", sim);
        // Should still be symmetric
        let sim_ba = anno_coalesce::streaming::trigram_similarity(&b, &a);
        prop_assert!((sim - sim_ba).abs() < 0.001,
            "Mixed script symmetry violated");
    }

    // -------------------------------------------------------------------------
    // Boundary Conditions
    // -------------------------------------------------------------------------

    /// Empty string handling
    #[test]
    fn string_similarity_empty_strings(_seed in any::<u64>()) {
        // Empty vs empty should be 1.0 (or at least bounded)
        let sim1 = anno_coalesce::streaming::trigram_similarity("", "");
        prop_assert!(sim1 >= 0.0 && sim1 <= 1.0);

        // Empty vs non-empty should be 0.0
        let sim2 = anno_coalesce::streaming::trigram_similarity("", "hello");
        prop_assert!((sim2 - 0.0).abs() < 0.001);
    }

    /// Very long strings don't panic or hang
    #[test]
    fn string_similarity_long_strings(length in 100usize..500) {
        let a: String = "a".repeat(length);
        let b: String = "b".repeat(length);
        let c: String = "a".repeat(length);

        // Should complete without panic
        let sim_diff = anno_coalesce::streaming::trigram_similarity(&a, &b);
        let sim_same = anno_coalesce::streaming::trigram_similarity(&a, &c);

        prop_assert!(sim_diff >= 0.0 && sim_diff <= 1.0);
        prop_assert!((sim_same - 1.0).abs() < 0.001, "Same long strings should be 1.0");
    }

    /// Single character strings
    #[test]
    fn string_similarity_single_char(a in ".", b in ".") {
        let sim = anno_coalesce::streaming::trigram_similarity(&a, &b);
        prop_assert!(sim >= 0.0 && sim <= 1.0);
        if a == b {
            prop_assert!((sim - 1.0).abs() < 0.001 || sim > 0.9,
                "Identical single chars should have high similarity: {}", sim);
        }
    }

    // -------------------------------------------------------------------------
    // Streaming Resolver Edge Cases
    // -------------------------------------------------------------------------

    /// Resolver handles empty entity names gracefully
    #[test]
    fn streaming_empty_entity_names(count in 1usize..10) {
        let mut resolver = StreamingResolver::new(StreamingConfig::default());

        // Add empty strings
        for i in 0..count {
            resolver.add_entity(format!("doc{}", i), "", None);
        }

        // Should not panic, mention count correct
        prop_assert_eq!(resolver.num_mentions(), count);
        prop_assert!(resolver.num_clusters() >= 1);
    }

    /// Resolver handles unicode entity names
    #[test]
    fn streaming_unicode_entities(names in prop::collection::vec("[\\p{L}]{1,20}", 2..10)) {
        let mut resolver = StreamingResolver::new(StreamingConfig::default());

        for (i, name) in names.iter().enumerate() {
            resolver.add_entity(format!("doc{}", i), name.clone(), None);
        }

        // Basic invariants hold
        prop_assert_eq!(resolver.num_mentions(), names.len());
        prop_assert!(resolver.num_clusters() <= names.len());
        prop_assert!(resolver.num_clusters() >= 1);
    }

    /// Resolver handles rapid additions without crash
    #[test]
    fn streaming_rapid_additions(count in 50usize..200) {
        let config = StreamingConfig {
            max_clusters: 20, // Force frequent merges
            ..Default::default()
        };
        let mut resolver = StreamingResolver::new(config);

        for i in 0..count {
            let name = format!("Entity_{}", i % 50); // Some duplicates
            resolver.add_entity(format!("doc{}", i), name, None);
        }

        // Should handle merging without panic
        prop_assert_eq!(resolver.num_mentions(), count);
        prop_assert!(resolver.num_clusters() <= 20 * 2,
            "Clusters should be bounded after merging");
    }

    // -------------------------------------------------------------------------
    // LSH Edge Cases
    // -------------------------------------------------------------------------

    /// LSH handles short strings (may not produce shingles)
    #[test]
    fn lsh_short_strings(items in prop::collection::vec(".{1,3}", 2..10)) {
        let mut lsh = MinHashLSH::new(Default::default());

        for (i, item) in items.iter().enumerate() {
            lsh.insert_text(i.to_string(), item);
        }

        // Should not panic even with very short strings
        let _pairs = lsh.candidate_pairs();
    }

    /// LSH handles identical items
    #[test]
    fn lsh_many_identical(text in "[A-Za-z]{5,15}", count in 2usize..20) {
        let mut lsh = MinHashLSH::new(Default::default());

        for i in 0..count {
            lsh.insert_text(i.to_string(), &text);
        }

        // All pairs of identical strings should be candidates
        let pairs = lsh.candidate_pairs();

        // At least some pairs should be found
        prop_assert!(!pairs.is_empty(),
            "No candidates found for {} identical strings", count);
    }

    // -------------------------------------------------------------------------
    // Hierarchical Clustering Edge Cases
    // -------------------------------------------------------------------------

    /// Hierarchical clustering with identical similarities
    #[test]
    fn hierarchical_identical_similarities(n in 2usize..10) {
        // All pairwise similarities = 0.5 (except diagonal)
        let sims: Vec<Vec<f32>> = (0..n)
            .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.5 }).collect())
            .collect();

        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Average);

        // Should still produce valid clustering
        let clusters = dendrogram.cut_to_k_clusters(2.min(n));
        let total: usize = clusters.iter().map(|c| c.len()).sum();
        prop_assert_eq!(total, n);
    }

    /// Hierarchical clustering with zero similarities
    #[test]
    fn hierarchical_zero_similarities(n in 2usize..8) {
        // All pairwise similarities = 0.0 (complete dissimilarity)
        let sims: Vec<Vec<f32>> = (0..n)
            .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
            .collect();

        let dendrogram = hierarchical_from_similarity(&sims, Linkage::Single);

        // Should still produce valid clustering
        let clusters = dendrogram.cut_to_k_clusters(n);
        prop_assert_eq!(clusters.len(), n, "With zero similarity, should get n singletons");
    }
}
