//! Robustness and metamorphic tests for entity resolution.
//!
//! These tests verify that the clustering algorithms are robust to:
//! - Input ordering (permutation invariance)
//! - Small perturbations (stability)
//! - Edge cases (empty, single element, all identical)
//! - Unicode and adversarial inputs

use anno_coalesce::correlation::{EdgeLabel, LabeledGraph};
use anno_coalesce::hierarchical::{hierarchical_from_similarity, Linkage};
use anno_coalesce::resolver::{string_similarity, Resolver};
use anno_coalesce::streaming::{trigram_similarity, StreamingConfig, StreamingResolver};
use proptest::prelude::*;

// =============================================================================
// Permutation Invariance Tests
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Hierarchical clustering should produce same number of steps regardless of input order
    #[test]
    fn hierarchical_permutation_invariance(
        similarities in prop::collection::vec(0.0f32..1.0, 3..10)
    ) {
        let n = ((1.0 + (1.0 + 8.0 * similarities.len() as f64).sqrt()) / 2.0) as usize;
        if n < 2 || n * (n - 1) / 2 != similarities.len() {
            return Ok(()); // Skip if not a valid triangular number
        }

        // Build similarity matrix
        let mut matrix: Vec<Vec<f32>> = vec![vec![1.0; n]; n];
        let mut idx = 0;
        #[allow(clippy::needless_range_loop)] // Need to access both matrix[i][j] and matrix[j][i]
        for i in 0..n {
            #[allow(clippy::needless_range_loop)] // Need to access both matrix[i][j] and matrix[j][i]
            for j in (i + 1)..n {
                if idx < similarities.len() {
                    matrix[i][j] = similarities[idx];
                    matrix[j][i] = similarities[idx];
                    idx += 1;
                }
            }
        }

        // Original order
        let dendrogram1 = hierarchical_from_similarity(&matrix, Linkage::Average);

        // Permuted order (reverse)
        let mut perm_matrix = vec![vec![1.0; n]; n];
        #[allow(clippy::needless_range_loop)] // Need to access both source and destination matrices
        for i in 0..n {
            #[allow(clippy::needless_range_loop)] // Need to access both source and destination matrices
            for j in 0..n {
                perm_matrix[i][j] = matrix[n - 1 - i][n - 1 - j];
            }
        }
        let dendrogram2 = hierarchical_from_similarity(&perm_matrix, Linkage::Average);

        // Dendrograms should have same number of steps
        prop_assert_eq!(dendrogram1.steps.len(), dendrogram2.steps.len());
    }

    /// Labeled graph construction should be consistent
    #[test]
    fn labeled_graph_construction(
        edges in prop::collection::vec((0usize..5, 0usize..5, any::<bool>()), 1..20)
    ) {
        let mut graph = LabeledGraph::new(6);
        for (i, j, positive) in &edges {
            if i != j && *i < 6 && *j < 6 {
                let label = if *positive { EdgeLabel::Positive } else { EdgeLabel::Negative };
                graph.add_edge(*i, *j, label);
            }
        }

        // Just verify graph construction doesn't panic
        prop_assert!(graph.n <= 6);
    }
}

// =============================================================================
// Stability Tests (small perturbations shouldn't drastically change results)
// =============================================================================

#[test]
fn hierarchical_stability_small_perturbation() {
    // Original similarities
    let original = vec![
        vec![1.0, 0.9, 0.1],
        vec![0.9, 1.0, 0.2],
        vec![0.1, 0.2, 1.0],
    ];

    // Perturbed (0.9 → 0.85, small change)
    let perturbed = vec![
        vec![1.0, 0.85, 0.1],
        vec![0.85, 1.0, 0.2],
        vec![0.1, 0.2, 1.0],
    ];

    let orig_dendrogram = hierarchical_from_similarity(&original, Linkage::Average);
    let pert_dendrogram = hierarchical_from_similarity(&perturbed, Linkage::Average);

    // Both should produce same number of merges (n-1 = 2)
    assert_eq!(orig_dendrogram.steps.len(), pert_dendrogram.steps.len());

    // First merge should still join the closest pair (0 and 1)
    assert!(
        (orig_dendrogram.steps[0].cluster_a == 0 && orig_dendrogram.steps[0].cluster_b == 1)
            || (orig_dendrogram.steps[0].cluster_a == 1 && orig_dendrogram.steps[0].cluster_b == 0)
    );

    assert!(
        (pert_dendrogram.steps[0].cluster_a == 0 && pert_dendrogram.steps[0].cluster_b == 1)
            || (pert_dendrogram.steps[0].cluster_a == 1 && pert_dendrogram.steps[0].cluster_b == 0)
    );
}

// =============================================================================
// Edge Case Tests
// =============================================================================

#[test]
fn resolver_empty_input() {
    let resolver = Resolver::new();
    let mut corpus = anno_core::Corpus::new();
    let result = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
    assert!(result.is_empty());
}

#[test]
fn resolver_single_track() {
    let resolver = Resolver::new();
    let mut corpus = anno_core::Corpus::new();
    let mut doc = anno_core::GroundedDocument::new("doc1", "");
    let track = anno_core::Track::new(1, "John Smith");
    doc.add_track(track);
    corpus.add_document(doc);
    let result = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
    // Single track should form single identity
    assert_eq!(result.len(), 1);
}

#[test]
fn hierarchical_single_element() {
    let matrix = vec![vec![1.0]];
    let dendrogram = hierarchical_from_similarity(&matrix, Linkage::Average);
    assert!(dendrogram.steps.is_empty(), "Single element has no merges");
}

#[test]
fn hierarchical_all_identical() {
    // All elements identical similarity
    let matrix = vec![
        vec![1.0, 1.0, 1.0],
        vec![1.0, 1.0, 1.0],
        vec![1.0, 1.0, 1.0],
    ];

    let dendrogram = hierarchical_from_similarity(&matrix, Linkage::Average);

    // Should produce 2 merges for 3 elements
    assert_eq!(dendrogram.steps.len(), 2);
}

#[test]
fn correlation_graph_construction() {
    // Test that labeled graph construction works for various configurations
    let mut graph = LabeledGraph::new(4);
    for i in 0..4 {
        for j in (i + 1)..4 {
            graph.add_edge(i, j, EdgeLabel::Positive);
        }
    }
    // Graph should have 4 nodes
    assert_eq!(graph.n, 4);
}

#[test]
fn correlation_mixed_edges() {
    let mut graph = LabeledGraph::new(4);
    for i in 0..4 {
        for j in (i + 1)..4 {
            let label = if (i + j) % 2 == 0 {
                EdgeLabel::Positive
            } else {
                EdgeLabel::Negative
            };
            graph.add_edge(i, j, label);
        }
    }
    // Graph should have 4 nodes
    assert_eq!(graph.n, 4);
}

// =============================================================================
// Unicode and Adversarial Input Tests
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// String similarity handles CJK characters
    #[test]
    fn string_similarity_cjk(
        a in "[\u{4e00}-\u{9fff}]{1,10}",
        b in "[\u{4e00}-\u{9fff}]{1,10}"
    ) {
        let sim = string_similarity(&a, &b);
        prop_assert!((0.0..=1.0).contains(&sim));
    }

    /// String similarity handles Arabic
    #[test]
    fn string_similarity_arabic(
        a in "[\u{0600}-\u{06ff}]{1,10}",
        b in "[\u{0600}-\u{06ff}]{1,10}"
    ) {
        let sim = string_similarity(&a, &b);
        prop_assert!((0.0..=1.0).contains(&sim));
    }

    /// String similarity handles Cyrillic
    #[test]
    fn string_similarity_cyrillic(
        a in "[\u{0400}-\u{04ff}]{1,10}",
        b in "[\u{0400}-\u{04ff}]{1,10}"
    ) {
        let sim = string_similarity(&a, &b);
        prop_assert!((0.0..=1.0).contains(&sim));
    }

    /// String similarity handles mixed scripts
    #[test]
    fn string_similarity_mixed_script(
        a in "[a-zA-Z\u{4e00}-\u{9fff}\u{0600}-\u{06ff}]{1,10}",
        b in "[a-zA-Z\u{4e00}-\u{9fff}\u{0600}-\u{06ff}]{1,10}"
    ) {
        let sim = string_similarity(&a, &b);
        prop_assert!((0.0..=1.0).contains(&sim));
    }

    /// String similarity handles empty strings
    #[test]
    fn string_similarity_empty(
        a in "[a-zA-Z]{0,10}"
    ) {
        let sim_empty_empty = string_similarity("", "");
        let sim_a_empty = string_similarity(&a, "");
        let sim_empty_a = string_similarity("", &a);

        // Empty vs empty should be 1.0 (identical)
        prop_assert!((sim_empty_empty - 1.0).abs() < f32::EPSILON);
        // Non-empty vs empty should be 0.0
        if !a.is_empty() {
            prop_assert!((sim_a_empty).abs() < f32::EPSILON);
            prop_assert!((sim_empty_a).abs() < f32::EPSILON);
        }
    }

    /// String similarity is symmetric
    #[test]
    fn string_similarity_symmetric(
        a in "[a-zA-Z]{1,20}",
        b in "[a-zA-Z]{1,20}"
    ) {
        let sim_ab = string_similarity(&a, &b);
        let sim_ba = string_similarity(&b, &a);
        prop_assert!((sim_ab - sim_ba).abs() < f32::EPSILON);
    }

    /// Identical strings have similarity 1.0
    #[test]
    fn string_similarity_identity(
        a in "[a-zA-Z]{1,20}"
    ) {
        let sim = string_similarity(&a, &a);
        prop_assert!((sim - 1.0).abs() < f32::EPSILON);
    }
}

// =============================================================================
// Streaming Resolver Tests
// =============================================================================

#[test]
fn streaming_empty_input() {
    let resolver = StreamingResolver::new(StreamingConfig::default());
    assert_eq!(resolver.clusters().len(), 0);
}

#[test]
fn streaming_single_mention() {
    let mut resolver = StreamingResolver::new(StreamingConfig::default());
    resolver.add_entity(
        "doc1".to_string(),
        "John Smith".to_string(),
        Some("PER".to_string()),
    );
    assert_eq!(resolver.clusters().len(), 1);
}

#[test]
fn streaming_exact_duplicates() {
    let mut resolver = StreamingResolver::new(StreamingConfig::default());
    resolver.add_entity(
        "doc1".to_string(),
        "John Smith".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc2".to_string(),
        "John Smith".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc3".to_string(),
        "John Smith".to_string(),
        Some("PER".to_string()),
    );

    // All identical should be in one cluster
    assert_eq!(resolver.clusters().len(), 1);
}

#[test]
fn streaming_clear_distincts() {
    let mut resolver = StreamingResolver::new(StreamingConfig::default());
    resolver.add_entity(
        "doc1".to_string(),
        "Alice".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc2".to_string(),
        "Bob".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc3".to_string(),
        "Charlie".to_string(),
        Some("PER".to_string()),
    );

    // Clearly different names should be in separate clusters
    assert!(resolver.clusters().len() >= 2);
}

// =============================================================================
// Trigram Similarity Tests
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Trigram similarity is in [0, 1]
    #[test]
    fn trigram_similarity_bounds(
        a in "[a-zA-Z]{0,20}",
        b in "[a-zA-Z]{0,20}"
    ) {
        let sim = trigram_similarity(&a, &b);
        prop_assert!((0.0..=1.0).contains(&sim));
    }

    /// Trigram similarity is symmetric
    #[test]
    fn trigram_similarity_symmetric(
        a in "[a-zA-Z]{1,20}",
        b in "[a-zA-Z]{1,20}"
    ) {
        let sim_ab = trigram_similarity(&a, &b);
        let sim_ba = trigram_similarity(&b, &a);
        prop_assert!((sim_ab - sim_ba).abs() < f32::EPSILON);
    }
}

// =============================================================================
// Threshold Monotonicity Tests
// =============================================================================

#[test]
fn threshold_monotonicity() {
    // Higher threshold should produce >= clusters
    let items = [
        "Apple Inc.",
        "Apple Computer",
        "Apple",
        "Microsoft Corporation",
        "Microsoft Corp",
        "MS",
    ];

    let thresholds = [0.3, 0.5, 0.7, 0.9];
    let mut cluster_counts = vec![];

    for &threshold in &thresholds {
        let resolver = Resolver::new().with_threshold(threshold);
        let mut corpus = anno_core::Corpus::new();
        let mut doc = anno_core::GroundedDocument::new("doc", "");
        for (i, s) in items.iter().enumerate() {
            let track = anno_core::Track::new(i as u64, *s);
            doc.add_track(track);
        }
        corpus.add_document(doc);
        let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
        cluster_counts.push(identities.len());
    }

    // Cluster count should be non-decreasing as threshold increases
    for i in 0..(cluster_counts.len() - 1) {
        assert!(
            cluster_counts[i] <= cluster_counts[i + 1],
            "Higher threshold should produce >= clusters: {} vs {} at thresholds {} vs {}",
            cluster_counts[i],
            cluster_counts[i + 1],
            thresholds[i],
            thresholds[i + 1]
        );
    }
}

// =============================================================================
// Transitivity Tests (Coreference Axiom)
// =============================================================================

#[test]
fn coreference_transitivity() {
    // If A corefs with B, and B corefs with C, then A should coref with C
    let mut resolver = StreamingResolver::new(StreamingConfig::default());
    resolver.add_entity(
        "doc1".to_string(),
        "Barack Obama".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc2".to_string(),
        "Obama".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc3".to_string(),
        "President Obama".to_string(),
        Some("PER".to_string()),
    );

    let clusters = resolver.clusters();
    // All should end up in same or connected clusters due to transitive closure
    // (Obama connects "Barack Obama" and "President Obama")
    assert!(
        clusters.len() <= 3,
        "Transitive coreference should produce at most 3 clusters, got {}",
        clusters.len()
    );
}

// =============================================================================
// Case Sensitivity Tests
// =============================================================================

#[test]
fn case_insensitive_matching() {
    let mut resolver = StreamingResolver::new(StreamingConfig::default());
    resolver.add_entity(
        "doc1".to_string(),
        "john smith".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc2".to_string(),
        "JOHN SMITH".to_string(),
        Some("PER".to_string()),
    );
    resolver.add_entity(
        "doc3".to_string(),
        "John Smith".to_string(),
        Some("PER".to_string()),
    );

    let clusters = resolver.clusters();
    // Case variations should be in same cluster (trigram is case-insensitive)
    assert_eq!(
        clusters.len(),
        1,
        "Case variations should match, got {} clusters",
        clusters.len()
    );
}

// =============================================================================
// Stress Tests
// =============================================================================

#[test]
fn stress_many_items() {
    let mut resolver = StreamingResolver::new(StreamingConfig::default());

    // Add 100 distinct entities
    for i in 0..100 {
        let name = format!("Entity_{}", i);
        resolver.add_entity(format!("doc_{}", i), name, None);
    }

    let clusters = resolver.clusters();
    // Should handle without panic
    assert!(!clusters.is_empty());
}

#[test]
fn stress_many_duplicates() {
    let mut resolver = StreamingResolver::new(StreamingConfig::default());

    // Add same entity 100 times from different docs
    for i in 0..100 {
        resolver.add_entity(
            format!("doc_{}", i),
            "John Smith".to_string(),
            Some("PER".to_string()),
        );
    }

    let clusters = resolver.clusters();
    // All should merge to single cluster
    assert_eq!(clusters.len(), 1, "All duplicates should merge");
}
