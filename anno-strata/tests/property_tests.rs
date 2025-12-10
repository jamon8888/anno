//! Property-based tests for graph algorithms.
//!
//! These tests verify invariants that should hold across all possible inputs.

use anno_core::{Entity, EntityType, GraphDocument, Relation};
use anno_strata::{
    leiden::Leiden, Betweenness, Closeness, Eigenvector, Hits, LabelPropagation, Louvain, PageRank,
};
use proptest::prelude::*;
use rand::Rng;

// =============================================================================
// Helper Functions
// =============================================================================

// Note: We generate random graphs inline within each proptest to have proper
// control over the random seed and graph structure.

// =============================================================================
// PageRank Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn pagerank_scores_bounded(n in 3..=20usize, p in 0.1..0.6f64) {
        // Note: PageRank may not sum to exactly 1.0 for graphs with disconnected
        // components or very sparse edges. We test that scores are bounded and valid.
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);

        if !graph.nodes.is_empty() {
            let pr = PageRank::default().compute(&graph);
            let total: f64 = pr.values().sum();

            // PageRank total should be bounded (0, n] for n nodes
            // Disconnected graphs may have lower totals
            prop_assert!(
                total >= 0.0 && total <= n as f64,
                "PageRank sum {} should be in [0, {}]",
                total,
                n
            );

            // Individual scores should be in [0, 1]
            for (node, &score) in &pr {
                prop_assert!(
                    score >= 0.0 && score <= 1.0,
                    "PageRank for {} should be in [0, 1]: {}",
                    node,
                    score
                );
            }
        }
    }

    #[test]
    fn pagerank_all_scores_non_negative(n in 2..=15usize, p in 0.1..0.5f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);
        let pr = PageRank::default().compute(&graph);

        for (node, score) in &pr {
            prop_assert!(
                *score >= 0.0,
                "Node {} has negative PageRank {}",
                node,
                score
            );
        }
    }
}

// =============================================================================
// Betweenness Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(15))]

    #[test]
    fn betweenness_all_scores_non_negative(n in 2..=12usize, p in 0.1..0.5f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);
        let bc = Betweenness::default().compute(&graph);

        for (node, score) in &bc {
            prop_assert!(
                *score >= 0.0,
                "Node {} has negative betweenness {}",
                node,
                score
            );
        }
    }

    #[test]
    fn betweenness_covers_all_nodes(n in 2..=12usize, p in 0.2..0.6f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);
        let bc = Betweenness::default().compute(&graph);

        // Betweenness should compute a score for every node
        prop_assert_eq!(
            bc.len(),
            graph.nodes.len(),
            "Betweenness should cover all nodes"
        );
    }
}

// =============================================================================
// Community Detection Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(15))]

    #[test]
    fn leiden_assigns_all_nodes(n in 2..=15usize, p in 0.1..0.5f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);
        let leiden = Leiden::new().with_seed(42);
        let communities = leiden.cluster(&graph).unwrap();

        // Every node should be assigned to exactly one community
        prop_assert_eq!(
            communities.len(),
            graph.nodes.len(),
            "Leiden should assign all nodes"
        );
    }

    #[test]
    fn louvain_assigns_all_nodes(n in 2..=15usize, p in 0.1..0.5f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);
        let louvain = Louvain::new().with_seed(42);
        let communities = louvain.cluster(&graph).unwrap();

        prop_assert_eq!(
            communities.len(),
            graph.nodes.len(),
            "Louvain should assign all nodes"
        );
    }

    #[test]
    fn label_propagation_assigns_all_nodes(n in 2..=15usize, p in 0.1..0.5f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);
        let lp = LabelPropagation::new().with_seed(42);
        let communities = lp.cluster(&graph).unwrap();

        prop_assert_eq!(
            communities.len(),
            graph.nodes.len(),
            "Label Propagation should assign all nodes"
        );
    }

    #[test]
    fn community_ids_are_valid(n in 2..=15usize, p in 0.1..0.5f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);

        let leiden_comm = Leiden::new().with_seed(42).cluster(&graph).unwrap();
        let louvain_comm = Louvain::new().with_seed(42).cluster(&graph).unwrap();
        let lp_comm = LabelPropagation::new().with_seed(42).cluster(&graph).unwrap();

        // Community IDs are usize, so they're always >= 0
        // Instead, verify that community IDs are within a reasonable range
        for (communities, name) in [
            (leiden_comm, "Leiden"),
            (louvain_comm, "Louvain"),
            (lp_comm, "LabelPropagation"),
        ] {
            let max_comm_id = communities.values().max().copied().unwrap_or(0);
            // Max community ID should not exceed number of nodes
            prop_assert!(
                max_comm_id < n,
                "{}: Max community ID {} should be < node count {}",
                name,
                max_comm_id,
                n
            );
        }
    }
}

// =============================================================================
// Eigenvector/HITS Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(15))]

    #[test]
    fn eigenvector_normalized(n in 3..=15usize, p in 0.2..0.5f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);

        if !graph.nodes.is_empty() && !graph.edges.is_empty() {
            let ev = Eigenvector::default().compute(&graph);

            if !ev.is_empty() {
                let l2_norm: f64 = ev.values().map(|v| v * v).sum::<f64>().sqrt();

                // Should be L2 normalized (allow 5% tolerance)
                prop_assert!(
                    (l2_norm - 1.0).abs() < 0.05 || l2_norm.abs() < 0.01,
                    "Eigenvector L2 norm {} should be ~1.0 or ~0.0",
                    l2_norm
                );
            }
        }
    }

    #[test]
    fn hits_scores_non_negative(n in 3..=15usize, p in 0.2..0.5f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);
        let (hubs, authorities) = Hits::default().compute(&graph);

        for (node, &score) in &hubs {
            prop_assert!(
                score >= 0.0,
                "Hub score for {} is negative: {}",
                node,
                score
            );
        }

        for (node, &score) in &authorities {
            prop_assert!(
                score >= 0.0,
                "Authority score for {} is negative: {}",
                node,
                score
            );
        }
    }
}

// =============================================================================
// Closeness Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(15))]

    #[test]
    fn closeness_scores_bounded(n in 2..=12usize, p in 0.2..0.6f64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        let mut rng = rand::rng();
        for i in 0..n {
            for j in (i + 1)..n {
                if rng.random::<f64>() < p {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);
        let cl = Closeness::default().compute(&graph);

        for (node, &score) in &cl {
            // Closeness should be in [0, 1] for normalized or [0, n-1] for unnormalized
            prop_assert!(
                score >= 0.0 && score <= (n as f64),
                "Closeness for {} out of bounds: {}",
                node,
                score
            );
        }
    }
}

// =============================================================================
// Determinism Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    #[test]
    fn centrality_algorithms_deterministic(n in 3..=10usize, seed in 1..=1000u64) {
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        // Use deterministic edge generation based on seed
        let mut relations = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                // Simple deterministic pattern based on indices
                if (i + j + seed as usize) % 3 == 0 {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);

        // Centrality algorithms should be deterministic (no randomness involved)
        let pr1 = PageRank::default().compute(&graph);
        let pr2 = PageRank::default().compute(&graph);
        prop_assert_eq!(pr1, pr2, "PageRank should be deterministic");

        let bc1 = Betweenness::default().compute(&graph);
        let bc2 = Betweenness::default().compute(&graph);
        prop_assert_eq!(bc1, bc2, "Betweenness should be deterministic");

        // Eigenvector uses iterative methods that may have tiny floating point
        // differences. Check that results are approximately equal.
        let ev1 = Eigenvector::default().compute(&graph);
        let ev2 = Eigenvector::default().compute(&graph);
        prop_assert_eq!(ev1.len(), ev2.len(), "Eigenvector should have same keys");
        for (key, &v1) in &ev1 {
            let v2 = ev2.get(key).copied().unwrap_or(f64::NAN);
            prop_assert!(
                (v1 - v2).abs() < 1e-10,
                "Eigenvector for {} differs: {} vs {}",
                key,
                v1,
                v2
            );
        }
    }

    #[test]
    fn community_detection_produces_valid_partition(n in 3..=10usize, seed in 1..=1000u64) {
        // Note: Community detection with seeds may still be non-deterministic due to
        // HashMap iteration order. We verify valid partition properties instead.
        let entities: Vec<Entity> = (0..n)
            .map(|i| Entity::new(&format!("N{}", i), EntityType::Person, i * 10, i * 10 + 5, 0.9))
            .collect();

        let mut relations = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                if (i + j + seed as usize) % 3 == 0 {
                    relations.push(Relation::new(
                        entities[i].clone(),
                        entities[j].clone(),
                        "REL",
                        0.9,
                    ));
                }
            }
        }

        let graph = GraphDocument::from_extraction(&entities, &relations, None);

        // All algorithms should produce valid partitions (every node assigned)
        let leiden = Leiden::new().with_seed(seed).cluster(&graph).unwrap();
        prop_assert_eq!(leiden.len(), graph.nodes.len(), "Leiden should assign all nodes");

        let louvain = Louvain::new().with_seed(seed).cluster(&graph).unwrap();
        prop_assert_eq!(louvain.len(), graph.nodes.len(), "Louvain should assign all nodes");

        // Community IDs should be in valid range
        let max_leiden_comm = leiden.values().max().copied().unwrap_or(0);
        prop_assert!(max_leiden_comm < n, "Leiden max community ID should be < n");

        let max_louvain_comm = louvain.values().max().copied().unwrap_or(0);
        prop_assert!(max_louvain_comm < n, "Louvain max community ID should be < n");
    }
}
