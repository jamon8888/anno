//! Integration tests comparing different algorithms on the same graphs.
//!
//! These tests verify:
//! 1. Algorithms produce consistent results on the same graph
//! 2. Expected relationships between algorithm outputs
//! 3. Algorithm behavior on canonical graph structures

use anno_core::{Entity, EntityType, GraphDocument, Relation};
use anno_tier::{
    leiden::Leiden, Betweenness, Closeness, Eigenvector, Hits, LabelPropagation, Louvain, PageRank,
};
use std::collections::{HashMap, HashSet};

// =============================================================================
// Test Graph Builders
// =============================================================================

fn star_graph() -> GraphDocument {
    let hub = Entity::new("Hub", EntityType::Organization, 0, 3, 0.9);
    let s1 = Entity::new("S1", EntityType::Person, 10, 12, 0.9);
    let s2 = Entity::new("S2", EntityType::Person, 20, 22, 0.9);
    let s3 = Entity::new("S3", EntityType::Person, 30, 32, 0.9);
    let s4 = Entity::new("S4", EntityType::Person, 40, 42, 0.9);

    let relations = vec![
        Relation::new(hub.clone(), s1.clone(), "EMPLOYS", 0.9),
        Relation::new(hub.clone(), s2.clone(), "EMPLOYS", 0.9),
        Relation::new(hub.clone(), s3.clone(), "EMPLOYS", 0.9),
        Relation::new(hub.clone(), s4.clone(), "EMPLOYS", 0.9),
    ];

    GraphDocument::from_extraction(&[hub, s1, s2, s3, s4], &relations, None)
}

fn two_cliques_with_bridge() -> GraphDocument {
    // Clique A: A1-A2-A3
    let a1 = Entity::new("A1", EntityType::Person, 0, 2, 0.9);
    let a2 = Entity::new("A2", EntityType::Person, 10, 12, 0.9);
    let a3 = Entity::new("A3", EntityType::Person, 20, 22, 0.9);

    // Clique B: B1-B2-B3
    let b1 = Entity::new("B1", EntityType::Person, 30, 32, 0.9);
    let b2 = Entity::new("B2", EntityType::Person, 40, 42, 0.9);
    let b3 = Entity::new("B3", EntityType::Person, 50, 52, 0.9);

    // Bridge node
    let bridge = Entity::new("Bridge", EntityType::Person, 60, 66, 0.9);

    let relations = vec![
        // Clique A (fully connected)
        Relation::new(a1.clone(), a2.clone(), "FRIEND", 0.9),
        Relation::new(a2.clone(), a3.clone(), "FRIEND", 0.9),
        Relation::new(a1.clone(), a3.clone(), "FRIEND", 0.9),
        // Clique B (fully connected)
        Relation::new(b1.clone(), b2.clone(), "FRIEND", 0.9),
        Relation::new(b2.clone(), b3.clone(), "FRIEND", 0.9),
        Relation::new(b1.clone(), b3.clone(), "FRIEND", 0.9),
        // Bridge connections
        Relation::new(a3.clone(), bridge.clone(), "KNOWS", 0.9),
        Relation::new(bridge.clone(), b1.clone(), "KNOWS", 0.9),
    ];

    GraphDocument::from_extraction(&[a1, a2, a3, b1, b2, b3, bridge], &relations, None)
}

fn chain_graph() -> GraphDocument {
    // A - B - C - D - E
    let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
    let b = Entity::new("B", EntityType::Person, 10, 11, 0.9);
    let c = Entity::new("C", EntityType::Person, 20, 21, 0.9);
    let d = Entity::new("D", EntityType::Person, 30, 31, 0.9);
    let e = Entity::new("E", EntityType::Person, 40, 41, 0.9);

    let relations = vec![
        Relation::new(a.clone(), b.clone(), "NEXT", 0.9),
        Relation::new(b.clone(), c.clone(), "NEXT", 0.9),
        Relation::new(c.clone(), d.clone(), "NEXT", 0.9),
        Relation::new(d.clone(), e.clone(), "NEXT", 0.9),
    ];

    GraphDocument::from_extraction(&[a, b, c, d, e], &relations, None)
}

// =============================================================================
// Centrality Comparison Tests
// =============================================================================

#[test]
fn test_all_centralities_agree_on_star_hub() {
    let graph = star_graph();

    // All centrality measures should identify the hub as most central
    let pr = PageRank::default().compute(&graph);
    let ev = Eigenvector::default().compute(&graph);
    let bc = Betweenness::default().compute(&graph);
    let cl = Closeness::default().compute(&graph);
    let hits = Hits::default();
    let (hubs, _) = hits.compute(&graph);

    // Find hub scores
    let find_hub = |scores: &HashMap<String, f64>| {
        scores
            .iter()
            .find(|(k, _)| k.to_lowercase().contains("hub"))
            .map(|(_, &v)| v)
            .unwrap_or(0.0)
    };

    let pr_hub = find_hub(&pr);
    let ev_hub = find_hub(&ev);
    let bc_hub = find_hub(&bc);
    let cl_hub = find_hub(&cl);
    let hits_hub = find_hub(&hubs);

    // Find max score among spokes
    let max_spoke = |scores: &HashMap<String, f64>| {
        scores
            .iter()
            .filter(|(k, _)| k.to_lowercase().contains("per:s"))
            .map(|(_, &v)| v)
            .fold(0.0_f64, |a, b| a.max(b))
    };

    // Hub should be highest for all metrics
    assert!(pr_hub >= max_spoke(&pr), "PageRank: hub should be highest");
    assert!(
        ev_hub >= max_spoke(&ev),
        "Eigenvector: hub should be highest"
    );
    assert!(
        bc_hub >= max_spoke(&bc),
        "Betweenness: hub should be highest"
    );
    assert!(cl_hub >= max_spoke(&cl), "Closeness: hub should be highest");
    assert!(
        hits_hub >= max_spoke(&hubs),
        "HITS hub: hub should be highest"
    );
}

#[test]
fn test_betweenness_finds_bridge() {
    let graph = two_cliques_with_bridge();

    let bc = Betweenness::default().compute(&graph);

    // The "Bridge" node should have highest betweenness
    let bridge_score = bc
        .iter()
        .find(|(k, _)| k.to_lowercase().contains("bridge"))
        .map(|(_, &v)| v)
        .unwrap_or(0.0);

    let max_non_bridge = bc
        .iter()
        .filter(|(k, _)| !k.to_lowercase().contains("bridge"))
        .map(|(_, &v)| v)
        .fold(0.0_f64, |a, b| a.max(b));

    assert!(
        bridge_score >= max_non_bridge,
        "Bridge node should have highest betweenness: bridge={}, max_other={}",
        bridge_score,
        max_non_bridge
    );
}

#[test]
fn test_closeness_highest_in_center_of_chain() {
    let graph = chain_graph();

    let cl = Closeness::default().compute(&graph);

    // In a chain A-B-C-D-E, C (middle) should have highest closeness
    let c_score = cl
        .iter()
        .find(|(k, _)| k.to_lowercase().contains("per:c"))
        .map(|(_, &v)| v)
        .unwrap_or(0.0);

    let a_score = cl
        .iter()
        .find(|(k, _)| k.to_lowercase().contains("per:a"))
        .map(|(_, &v)| v)
        .unwrap_or(0.0);

    let e_score = cl
        .iter()
        .find(|(k, _)| k.to_lowercase().contains("per:e"))
        .map(|(_, &v)| v)
        .unwrap_or(0.0);

    assert!(c_score >= a_score, "C should have >= closeness than A");
    assert!(c_score >= e_score, "C should have >= closeness than E");
}

// =============================================================================
// Community Detection Comparison Tests
// =============================================================================

#[test]
fn test_leiden_vs_louvain_find_same_structure() {
    let graph = two_cliques_with_bridge();

    let leiden = Leiden::new().with_seed(42);
    let louvain = Louvain::new().with_seed(42);

    let leiden_comm = leiden
        .cluster(&graph)
        .expect("Leiden clustering should succeed in test");
    let louvain_comm = louvain
        .cluster(&graph)
        .expect("Louvain clustering should succeed in test");

    // Both should find approximately 2-3 communities
    let leiden_unique: HashSet<_> = leiden_comm.values().collect();
    let louvain_unique: HashSet<_> = louvain_comm.values().collect();

    assert!(
        leiden_unique.len() >= 2 && leiden_unique.len() <= 4,
        "Leiden should find 2-4 communities, found {}",
        leiden_unique.len()
    );
    assert!(
        louvain_unique.len() >= 2 && louvain_unique.len() <= 4,
        "Louvain should find 2-4 communities, found {}",
        louvain_unique.len()
    );
}

#[test]
fn test_leiden_modularity_at_least_louvain() {
    let graph = two_cliques_with_bridge();

    let leiden = Leiden::new().with_seed(42);
    let louvain = Louvain::new().with_seed(42);

    let leiden_comm = leiden
        .cluster(&graph)
        .expect("Leiden clustering should succeed in test");
    let louvain_comm = louvain
        .cluster(&graph)
        .expect("Louvain clustering should succeed in test");

    // Use louvain's modularity calculation for both (same formula)
    let leiden_mod = louvain.modularity(&graph, &leiden_comm);
    let louvain_mod = louvain.modularity(&graph, &louvain_comm);

    // Leiden should generally achieve >= modularity (it's an improvement over Louvain)
    // Allow small tolerance for randomness
    assert!(
        leiden_mod >= louvain_mod - 0.05,
        "Leiden modularity ({}) should be >= Louvain ({}) - 0.05",
        leiden_mod,
        louvain_mod
    );
}

#[test]
fn test_label_propagation_vs_leiden_coverage() {
    let graph = two_cliques_with_bridge();

    let lp = LabelPropagation::new().with_seed(42);
    let leiden = Leiden::new().with_seed(42);

    let lp_comm = lp
        .cluster(&graph)
        .expect("Label propagation clustering should succeed in test");
    let leiden_comm = leiden
        .cluster(&graph)
        .expect("Leiden clustering should succeed in test");

    // Both should cover all nodes
    assert_eq!(
        lp_comm.len(),
        graph.nodes.len(),
        "Label Propagation should cover all nodes"
    );
    assert_eq!(
        leiden_comm.len(),
        graph.nodes.len(),
        "Leiden should cover all nodes"
    );
}

// =============================================================================
// Algorithm Properties Tests
// =============================================================================

#[test]
fn test_pagerank_sums_to_approximately_one() {
    let graph = two_cliques_with_bridge();
    let pr = PageRank::default().compute(&graph);

    let total: f64 = pr.values().sum();
    assert!(
        (total - 1.0).abs() < 0.01,
        "PageRank scores should sum to ~1.0, got {}",
        total
    );
}

#[test]
fn test_eigenvector_normalized() {
    let graph = two_cliques_with_bridge();
    let ev = Eigenvector::default().compute(&graph);

    // L2 norm should be ~1.0
    let l2_norm: f64 = ev.values().map(|v| v * v).sum::<f64>().sqrt();
    assert!(
        (l2_norm - 1.0).abs() < 0.01,
        "Eigenvector should be L2 normalized, got norm {}",
        l2_norm
    );
}

#[test]
fn test_all_scores_non_negative() {
    let graph = two_cliques_with_bridge();

    let pr = PageRank::default().compute(&graph);
    let ev = Eigenvector::default().compute(&graph);
    let bc = Betweenness::default().compute(&graph);
    let cl = Closeness::default().compute(&graph);
    let (hubs, auths) = Hits::default().compute(&graph);

    for (name, scores) in [
        ("PageRank", pr),
        ("Eigenvector", ev),
        ("Betweenness", bc),
        ("Closeness", cl),
        ("HITS hubs", hubs),
        ("HITS auths", auths),
    ] {
        for (node, score) in &scores {
            assert!(
                *score >= 0.0,
                "{}: {} has negative score {}",
                name,
                node,
                score
            );
        }
    }
}

#[test]
fn test_empty_graph_all_algorithms() {
    let graph = GraphDocument::new();

    // Centrality
    assert!(PageRank::default().compute(&graph).is_empty());
    assert!(Eigenvector::default().compute(&graph).is_empty());
    assert!(Betweenness::default().compute(&graph).is_empty());
    assert!(Closeness::default().compute(&graph).is_empty());
    let (h, a) = Hits::default().compute(&graph);
    assert!(h.is_empty() && a.is_empty());

    // Community detection
    assert!(Leiden::new()
        .cluster(&graph)
        .expect("Leiden clustering should succeed on empty graph")
        .is_empty());
    assert!(Louvain::new()
        .cluster(&graph)
        .expect("Louvain clustering should succeed on empty graph")
        .is_empty());
    assert!(LabelPropagation::new()
        .cluster(&graph)
        .expect("Label propagation clustering should succeed on empty graph")
        .is_empty());
}

#[test]
fn test_single_node_all_algorithms() {
    let solo = Entity::new("Solo", EntityType::Person, 0, 4, 0.9);
    let graph = GraphDocument::from_extraction(&[solo], &[], None);

    // Centrality - single node should have some score
    assert_eq!(PageRank::default().compute(&graph).len(), 1);
    assert_eq!(Eigenvector::default().compute(&graph).len(), 1);
    assert_eq!(Betweenness::default().compute(&graph).len(), 1);
    assert_eq!(Closeness::default().compute(&graph).len(), 1);

    // Community detection - single node = single community
    assert_eq!(
        Leiden::new()
            .cluster(&graph)
            .expect("Leiden clustering should succeed on single node")
            .len(),
        1
    );
    assert_eq!(
        Louvain::new()
            .cluster(&graph)
            .expect("Louvain clustering should succeed on single node")
            .len(),
        1
    );
    assert_eq!(
        LabelPropagation::new()
            .cluster(&graph)
            .expect("Label propagation clustering should succeed on single node")
            .len(),
        1
    );
}
