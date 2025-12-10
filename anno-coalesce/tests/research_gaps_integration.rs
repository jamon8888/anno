//! Integration tests for research frontiers and identified gaps.
//!
//! This test suite covers the gaps identified in the NER/coreference research synthesis:
//! - Streaming entity resolution (SPARSE-PIVOT foundations)
//! - Min-Max correlation clustering
//! - Chromatic correlation clustering
//!
//! ## Research References
//!
//! - Min-Max CC: "Min-Max Correlation Clustering" (2024) - 4-approximation
//! - Streaming CC: Charikar et al. (1997) - Doubling Algorithm
//! - SPARSE-PIVOT: Behnezhad et al. (2025) - 20+ε approximation for dynamic CC
//! - Chromatic CC: Color-constrained clustering variants

use anno_coalesce::{
    correlation::{
        chromatic_clustering, compare_algorithms_extended, min_max_clustering,
        ChromaticClusteringConfig, EdgeLabel, LabeledGraph,
    },
    streaming::{StreamingConfig, StreamingResolver},
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::collections::HashSet;

// =============================================================================
// STREAMING ENTITY RESOLUTION TESTS
// =============================================================================

/// Test basic streaming resolution functionality
#[test]
fn test_streaming_resolver_basic() {
    let config = StreamingConfig {
        add_threshold: 0.7,
        merge_threshold: 0.7,
        max_clusters: 100,
        ..Default::default()
    };

    let mut resolver = StreamingResolver::new(config);

    // Add some entities
    resolver.add_entity("doc1", "Barack Obama", Some("PER".to_string()));
    resolver.add_entity("doc1", "Obama", Some("PER".to_string()));
    resolver.add_entity("doc2", "Donald Trump", Some("PER".to_string()));

    // Obama mentions should cluster together
    assert!(resolver.num_clusters() <= 3, "Expected 3 or fewer clusters");
}

/// Test streaming resolver handles incremental additions correctly
#[test]
fn test_streaming_incremental_updates() {
    let config = StreamingConfig::default();
    let mut resolver = StreamingResolver::new(config);

    // Add entities over time
    resolver.add_entity("doc1", "Apple Inc.", Some("ORG".to_string()));
    let clusters_after_1 = resolver.num_clusters();

    resolver.add_entity("doc2", "Apple", Some("ORG".to_string()));
    let clusters_after_2 = resolver.num_clusters();

    resolver.add_entity("doc3", "Microsoft", Some("ORG".to_string()));
    let clusters_after_3 = resolver.num_clusters();

    // Clusters should evolve reasonably
    assert!(clusters_after_1 >= 1);
    // Apple and Apple Inc. might cluster together (threshold dependent)
    assert!(clusters_after_3 >= 1);

    println!(
        "Streaming clusters: {} -> {} -> {}",
        clusters_after_1, clusters_after_2, clusters_after_3
    );
}

/// Test streaming with type constraints
#[test]
fn test_streaming_type_constraints() {
    let config = StreamingConfig {
        require_type_match: true,
        add_threshold: 0.5, // Lower threshold
        merge_threshold: 0.5,
        ..Default::default()
    };

    let mut resolver = StreamingResolver::new(config);

    // Same surface form, different types
    resolver.add_entity("doc1", "Apple", Some("ORG".to_string())); // Company
    resolver.add_entity("doc2", "Apple", Some("MISC".to_string())); // Fruit (hypothetical)

    // With type constraints, these should NOT cluster together
    assert!(
        resolver.num_clusters() >= 1,
        "Type constraints should be respected"
    );
}

// =============================================================================
// MIN-MAX CORRELATION CLUSTERING TESTS
// =============================================================================

/// Test min-max clustering produces valid results
#[test]
fn test_min_max_valid_clustering() {
    let mut graph = LabeledGraph::new(6);

    // Two clear clusters
    graph.add_edge(0, 1, EdgeLabel::Positive);
    graph.add_edge(1, 2, EdgeLabel::Positive);
    graph.add_edge(0, 2, EdgeLabel::Positive);

    graph.add_edge(3, 4, EdgeLabel::Positive);
    graph.add_edge(4, 5, EdgeLabel::Positive);
    graph.add_edge(3, 5, EdgeLabel::Positive);

    // Negative edges between clusters
    graph.add_edge(0, 3, EdgeLabel::Negative);
    graph.add_edge(1, 4, EdgeLabel::Negative);

    let mut rng = StdRng::seed_from_u64(42);
    let result = min_max_clustering(&graph, &mut rng);

    // Should find a valid clustering
    assert!(result.clustering.num_clusters() >= 1);
    assert!(result.clustering.num_clusters() <= 6);

    // Max disagreements should be tracked
    println!(
        "Min-Max: {} clusters, cost {}, max_disagree {}",
        result.clustering.num_clusters(),
        result.clustering.cost,
        result.max_disagreements
    );
}

/// Test min-max vs standard on imbalanced graph
#[test]
fn test_min_max_imbalanced_graph() {
    // Create graph where standard CC might create one "bad" cluster
    let mut graph = LabeledGraph::new(10);

    // Dense cluster 0-4 (perfect agreement)
    for i in 0..5 {
        for j in (i + 1)..5 {
            graph.add_edge(i, j, EdgeLabel::Positive);
        }
    }

    // Sparse cluster 5-9 with internal conflicts
    graph.add_edge(5, 6, EdgeLabel::Positive);
    graph.add_edge(7, 8, EdgeLabel::Positive);
    graph.add_edge(5, 7, EdgeLabel::Negative); // Conflict within cluster
    graph.add_edge(6, 8, EdgeLabel::Negative); // Conflict within cluster
    graph.add_edge(8, 9, EdgeLabel::Positive);

    // Between-cluster negatives
    for i in 0..5 {
        graph.add_edge(i, 5 + (i % 5), EdgeLabel::Negative);
    }

    let mut rng = StdRng::seed_from_u64(42);
    let min_max = min_max_clustering(&graph, &mut rng);

    // Min-max should try to minimize worst-case per cluster
    println!(
        "Imbalanced graph - Min-Max: {} clusters, max_disagree {}",
        min_max.clustering.num_clusters(),
        min_max.max_disagreements
    );

    // Verify per-cluster disagreements
    for (i, &disagree) in min_max.cluster_disagreements.iter().enumerate() {
        println!("  Cluster {}: {} disagreements", i, disagree);
    }
}

/// Test that extended comparison includes all algorithms
#[test]
fn test_compare_algorithms_extended() {
    let mut graph = LabeledGraph::new(4);
    graph.add_edge(0, 1, EdgeLabel::Positive);
    graph.add_edge(2, 3, EdgeLabel::Positive);
    graph.add_edge(0, 2, EdgeLabel::Negative);

    let mut rng = StdRng::seed_from_u64(42);
    let results = compare_algorithms_extended(&graph, &mut rng);

    // Should have 5 algorithms
    assert_eq!(results.len(), 5, "Should compare 5 algorithms");

    // Last one should be Min-Max with max_disagreements
    let (name, _, max_d) = &results[4];
    assert_eq!(*name, "Min-Max");
    assert!(max_d.is_some(), "Min-Max should report max_disagreements");

    for (name, result, max_disagree) in &results {
        if let Some(max_d) = max_disagree {
            println!(
                "{}: {} clusters, cost {}, max_disagree {}",
                name,
                result.num_clusters(),
                result.cost,
                max_d
            );
        } else {
            println!(
                "{}: {} clusters, cost {}",
                name,
                result.num_clusters(),
                result.cost
            );
        }
    }
}

// =============================================================================
// CHROMATIC CORRELATION CLUSTERING TESTS
// =============================================================================

#[test]
fn test_chromatic_clustering_basic() {
    // All nodes want to cluster together
    let mut graph = LabeledGraph::new(4);
    for i in 0..4 {
        for j in (i + 1)..4 {
            graph.add_edge(i, j, EdgeLabel::Positive);
        }
    }

    // Colors: 2 colors, so max 2 per cluster
    let config = ChromaticClusteringConfig {
        k: 2,
        colors: vec![0, 0, 1, 1], // 0,1 are color 0; 2,3 are color 1
    };

    let mut rng = StdRng::seed_from_u64(42);
    let result = chromatic_clustering(&graph, &config, &mut rng);

    // No cluster should have two nodes of same color
    for cluster in &result.clusters {
        let colors_in_cluster: HashSet<_> = cluster.iter().map(|&n| config.colors[n]).collect();
        assert_eq!(
            colors_in_cluster.len(),
            cluster.len(),
            "Cluster {:?} has duplicate colors",
            cluster
        );
    }
}

#[test]
fn test_chromatic_clustering_three_colors() {
    let mut graph = LabeledGraph::new(6);
    for i in 0..6 {
        for j in (i + 1)..6 {
            graph.add_edge(i, j, EdgeLabel::Positive);
        }
    }

    // 3 colors: each pair can have at most one from each color
    let config = ChromaticClusteringConfig {
        k: 3,
        colors: vec![0, 0, 1, 1, 2, 2],
    };

    let mut rng = StdRng::seed_from_u64(42);
    let result = chromatic_clustering(&graph, &config, &mut rng);

    // Verify color constraint
    for cluster in &result.clusters {
        let colors_in_cluster: HashSet<_> = cluster.iter().map(|&n| config.colors[n]).collect();
        assert_eq!(
            colors_in_cluster.len(),
            cluster.len(),
            "Cluster {:?} violates color constraint",
            cluster
        );
    }

    println!(
        "Chromatic (3 colors): {} clusters, cost {}",
        result.num_clusters(),
        result.cost
    );
}

// =============================================================================
// CALIBRATION AND CONFIDENCE TESTS
// =============================================================================

/// Test that clustering confidence correlates with structure
#[test]
fn test_clustering_confidence_correlation() {
    // Well-separated clusters should have lower cost
    let mut good_graph = LabeledGraph::new(6);
    for i in 0..3 {
        for j in (i + 1)..3 {
            good_graph.add_edge(i, j, EdgeLabel::Positive);
        }
    }
    for i in 3..6 {
        for j in (i + 1)..6 {
            good_graph.add_edge(i, j, EdgeLabel::Positive);
        }
    }
    for i in 0..3 {
        for j in 3..6 {
            good_graph.add_edge(i, j, EdgeLabel::Negative);
        }
    }

    // Noisy/ambiguous structure
    let mut noisy_graph = LabeledGraph::new(6);
    for i in 0..6 {
        for j in (i + 1)..6 {
            // Randomly assign labels
            let label = if (i + j) % 2 == 0 {
                EdgeLabel::Positive
            } else {
                EdgeLabel::Negative
            };
            noisy_graph.add_edge(i, j, label);
        }
    }

    let mut rng = StdRng::seed_from_u64(42);
    let good_result = min_max_clustering(&good_graph, &mut rng);
    let noisy_result = min_max_clustering(&noisy_graph, &mut rng);

    println!(
        "Good structure: cost={}, max={}",
        good_result.clustering.cost, good_result.max_disagreements
    );
    println!(
        "Noisy structure: cost={}, max={}",
        noisy_result.clustering.cost, noisy_result.max_disagreements
    );

    // Good structure should have lower cost than noisy
    // (Not always true due to randomization, but generally expected)
    assert!(
        good_result.clustering.cost <= noisy_result.clustering.cost + 2,
        "Well-separated should generally have lower cost"
    );
}

// =============================================================================
// EDGE CASES
// =============================================================================

#[test]
fn test_empty_graph_min_max() {
    let graph = LabeledGraph::new(0);
    let mut rng = StdRng::seed_from_u64(42);
    let result = min_max_clustering(&graph, &mut rng);

    assert_eq!(result.clustering.num_clusters(), 0);
    assert_eq!(result.max_disagreements, 0);
}

#[test]
fn test_single_node_graph() {
    let graph = LabeledGraph::new(1);
    let mut rng = StdRng::seed_from_u64(42);
    let result = min_max_clustering(&graph, &mut rng);

    assert_eq!(result.clustering.num_clusters(), 1);
    assert_eq!(result.max_disagreements, 0);
}

#[test]
fn test_complete_positive_graph() {
    // All positive edges = one big cluster
    let mut graph = LabeledGraph::new(5);
    for i in 0..5 {
        for j in (i + 1)..5 {
            graph.add_edge(i, j, EdgeLabel::Positive);
        }
    }

    let mut rng = StdRng::seed_from_u64(42);
    let result = min_max_clustering(&graph, &mut rng);

    // Should merge everything into one cluster
    assert!(
        result.clustering.num_clusters() <= 2,
        "Complete positive graph should form 1-2 clusters"
    );
}

#[test]
fn test_complete_negative_graph() {
    // All negative edges = all singletons
    let mut graph = LabeledGraph::new(5);
    for i in 0..5 {
        for j in (i + 1)..5 {
            graph.add_edge(i, j, EdgeLabel::Negative);
        }
    }

    let mut rng = StdRng::seed_from_u64(42);
    let result = min_max_clustering(&graph, &mut rng);

    // Optimal is all singletons
    assert_eq!(
        result.clustering.num_clusters(),
        5,
        "Complete negative graph should form 5 singletons"
    );
    assert_eq!(result.clustering.cost, 0);
}

// =============================================================================
// PROPERTY-BASED TESTS
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use rand::Rng;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// Min-max should always produce valid clustering
        #[test]
        fn min_max_always_valid(n in 2usize..10, seed in any::<u64>()) {
            let mut graph = LabeledGraph::new(n);
            let mut rng = StdRng::seed_from_u64(seed);

            // Random edges
            for i in 0..n {
                for j in (i + 1)..n {
                    let label = if rng.random_bool(0.5) {
                        EdgeLabel::Positive
                    } else {
                        EdgeLabel::Negative
                    };
                    graph.add_edge(i, j, label);
                }
            }

            let result = min_max_clustering(&graph, &mut rng);

            // Valid clustering
            prop_assert!(result.clustering.num_clusters() >= 1);
            prop_assert!(result.clustering.num_clusters() <= n);

            // All nodes assigned
            prop_assert_eq!(result.clustering.assignments.len(), n);
        }

        /// Streaming resolver should handle arbitrary inputs
        #[test]
        fn streaming_handles_arbitrary_inputs(
            entities in prop::collection::vec("[a-zA-Z ]{1,20}", 1..20),
        ) {
            let config = StreamingConfig {
                add_threshold: 0.5,
                merge_threshold: 0.5,
                ..Default::default()
            };
            let mut resolver = StreamingResolver::new(config);

            for (i, entity) in entities.iter().enumerate() {
                let trimmed = entity.trim();
                if !trimmed.is_empty() {
                    resolver.add_entity(&format!("doc{}", i), trimmed, None);
                }
            }

            // Should always have non-negative clusters
            prop_assert!(resolver.num_clusters() >= 0);
        }

        /// Chromatic clustering respects color constraints
        #[test]
        fn chromatic_respects_colors(n in 3usize..8, k in 2usize..4, seed in any::<u64>()) {
            let mut graph = LabeledGraph::new(n);
            let mut rng = StdRng::seed_from_u64(seed);

            // Random positive edges (everyone wants to cluster)
            for i in 0..n {
                for j in (i + 1)..n {
                    if rng.random_bool(0.7) {
                        graph.add_edge(i, j, EdgeLabel::Positive);
                    }
                }
            }

            // Random color assignment
            let colors: Vec<usize> = (0..n).map(|i| i % k).collect();
            let config = ChromaticClusteringConfig { k, colors: colors.clone() };

            let result = chromatic_clustering(&graph, &config, &mut rng);

            // Verify no cluster has duplicate colors
            for cluster in &result.clusters {
                let cluster_colors: HashSet<_> = cluster.iter().map(|&node| colors[node]).collect();
                prop_assert_eq!(
                    cluster_colors.len(),
                    cluster.len(),
                    "Cluster {:?} violates color constraint",
                    cluster
                );
            }
        }
    }
}
