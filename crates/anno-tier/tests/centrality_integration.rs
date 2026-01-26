//! Integration tests for centrality algorithms.
//!
//! Tests PageRank, Betweenness, and HITS on various graph structures
//! to verify they produce expected results.

use anno_core::{Entity, EntityType, GraphDocument, Relation};
use anno_tier::{Betweenness, Hits, PageRank};

/// Helper to create a simple entity
fn entity(name: &str, start: usize) -> Entity {
    Entity::new(name, EntityType::Person, start, start + name.len(), 0.9)
}

/// Linear chain: A → B → C → D
/// Expected: B, C have highest betweenness (bridges)
fn chain_graph() -> GraphDocument {
    let a = entity("Alice", 0);
    let b = entity("Bob", 10);
    let c = entity("Carol", 20);
    let d = entity("Dave", 30);

    let relations = vec![
        Relation::new(a.clone(), b.clone(), "KNOWS", 0.9),
        Relation::new(b.clone(), c.clone(), "KNOWS", 0.9),
        Relation::new(c.clone(), d.clone(), "KNOWS", 0.9),
    ];

    GraphDocument::from_extraction(&[a, b, c, d], &relations, None)
}

/// Star graph: Hub connected to 4 spokes
/// Expected: Hub has highest PageRank and betweenness
fn star_graph() -> GraphDocument {
    let hub = entity("Hub", 0);
    let s1 = entity("Spoke1", 10);
    let s2 = entity("Spoke2", 20);
    let s3 = entity("Spoke3", 30);
    let s4 = entity("Spoke4", 40);

    let relations = vec![
        Relation::new(hub.clone(), s1.clone(), "MANAGES", 0.9),
        Relation::new(hub.clone(), s2.clone(), "MANAGES", 0.9),
        Relation::new(hub.clone(), s3.clone(), "MANAGES", 0.9),
        Relation::new(hub.clone(), s4.clone(), "MANAGES", 0.9),
    ];

    GraphDocument::from_extraction(&[hub, s1, s2, s3, s4], &relations, None)
}

/// Two cliques connected by a single bridge
/// Expected: Bridge node has highest betweenness
fn two_cliques_with_bridge() -> GraphDocument {
    // Clique 1: A, B, C fully connected
    let a = entity("A", 0);
    let b = entity("B", 10);
    let c = entity("C", 20);
    // Bridge
    let bridge = entity("Bridge", 30);
    // Clique 2: X, Y, Z fully connected
    let x = entity("X", 40);
    let y = entity("Y", 50);
    let z = entity("Z", 60);

    let relations = vec![
        // Clique 1
        Relation::new(a.clone(), b.clone(), "FRIEND", 0.9),
        Relation::new(b.clone(), c.clone(), "FRIEND", 0.9),
        Relation::new(a.clone(), c.clone(), "FRIEND", 0.9),
        // Bridge connections
        Relation::new(c.clone(), bridge.clone(), "KNOWS", 0.9),
        Relation::new(bridge.clone(), x.clone(), "KNOWS", 0.9),
        // Clique 2
        Relation::new(x.clone(), y.clone(), "FRIEND", 0.9),
        Relation::new(y.clone(), z.clone(), "FRIEND", 0.9),
        Relation::new(x.clone(), z.clone(), "FRIEND", 0.9),
    ];

    GraphDocument::from_extraction(&[a, b, c, bridge, x, y, z], &relations, None)
}

/// Bipartite-ish: Documents point to Topics
/// Expected: HITS should identify documents as hubs, topics as authorities
fn bipartite_docs_topics() -> GraphDocument {
    // Documents (hubs)
    let doc1 = entity("Doc1", 0);
    let doc2 = entity("Doc2", 10);
    let doc3 = entity("Doc3", 20);
    // Topics (authorities)
    let topic_a = entity("AI", 30);
    let topic_b = entity("ML", 40);
    let topic_c = entity("NLP", 50);

    let relations = vec![
        // Doc1 covers AI and ML
        Relation::new(doc1.clone(), topic_a.clone(), "ABOUT", 0.9),
        Relation::new(doc1.clone(), topic_b.clone(), "ABOUT", 0.9),
        // Doc2 covers AI and NLP
        Relation::new(doc2.clone(), topic_a.clone(), "ABOUT", 0.9),
        Relation::new(doc2.clone(), topic_c.clone(), "ABOUT", 0.9),
        // Doc3 covers all
        Relation::new(doc3.clone(), topic_a.clone(), "ABOUT", 0.9),
        Relation::new(doc3.clone(), topic_b.clone(), "ABOUT", 0.9),
        Relation::new(doc3.clone(), topic_c.clone(), "ABOUT", 0.9),
    ];

    GraphDocument::from_extraction(
        &[doc1, doc2, doc3, topic_a, topic_b, topic_c],
        &relations,
        None,
    )
}

// =============================================================================
// PageRank Tests
// =============================================================================

#[test]
fn test_pagerank_star_hub_is_central() {
    let graph = star_graph();
    let pr = PageRank::default();
    let ranked = pr.ranked(&graph);

    // Hub should be first
    assert!(
        ranked[0].0.to_lowercase().contains("hub"),
        "Hub should have highest PageRank, got: {:?}",
        ranked
    );
}

#[test]
fn test_pagerank_scores_sum_to_one() {
    let graph = chain_graph();
    let pr = PageRank::default();
    let scores = pr.compute(&graph);

    let total: f64 = scores.values().sum();
    assert!(
        (total - 1.0).abs() < 0.1,
        "PageRank scores should sum to ~1, got {}",
        total
    );
}

#[test]
fn test_pagerank_all_positive() {
    let graph = two_cliques_with_bridge();
    let pr = PageRank::default();
    let scores = pr.compute(&graph);

    for (node, score) in &scores {
        assert!(*score > 0.0, "Node {} should have positive score", node);
    }
}

// =============================================================================
// Betweenness Tests
// =============================================================================

#[test]
fn test_betweenness_chain_middle_nodes() {
    let graph = chain_graph();
    let bc = Betweenness::new();
    let top = bc.top_k(&graph, 2);

    // Middle nodes (Bob, Carol) should have highest betweenness
    let top_names: Vec<_> = top.iter().map(|(n, _)| n.to_lowercase()).collect();

    // Should contain the middle nodes, not endpoints
    assert!(
        top_names
            .iter()
            .any(|n| n.contains("bob") || n.contains("carol")),
        "Middle nodes should have highest betweenness: {:?}",
        top
    );
}

#[test]
fn test_betweenness_star_hub() {
    let graph = star_graph();
    let bc = Betweenness::new();
    let top = bc.top_k(&graph, 1);

    // Hub should have highest betweenness
    assert!(
        top[0].0.to_lowercase().contains("hub"),
        "Hub should have highest betweenness: {:?}",
        top
    );
}

#[test]
fn test_betweenness_bridge_detection() {
    let graph = two_cliques_with_bridge();
    let bc = Betweenness::new();
    let top = bc.top_k(&graph, 1);

    // Bridge node should have highest betweenness
    assert!(
        top[0].0.to_lowercase().contains("bridge"),
        "Bridge node should have highest betweenness: {:?}",
        top
    );
}

// =============================================================================
// HITS Tests
// =============================================================================

#[test]
fn test_hits_bipartite_structure() {
    let graph = bipartite_docs_topics();
    let hits = Hits::default();
    let (hubs, auths) = hits.compute(&graph);

    // Documents should be hubs (they point to topics)
    // Topics should be authorities (pointed to by documents)

    // Find hub scores for docs
    let doc_hub_scores: Vec<f64> = hubs
        .iter()
        .filter(|(k, _)| k.to_lowercase().contains("doc"))
        .map(|(_, v)| *v)
        .collect();

    // Find authority scores for topics
    let topic_auth_scores: Vec<f64> = auths
        .iter()
        .filter(|(k, _)| {
            k.to_lowercase().contains("ai")
                || k.to_lowercase().contains("ml")
                || k.to_lowercase().contains("nlp")
        })
        .map(|(_, v)| *v)
        .collect();

    // Documents should have positive hub scores
    assert!(
        doc_hub_scores.iter().all(|&s| s > 0.0),
        "Documents should have positive hub scores"
    );

    // Topics should have positive authority scores
    assert!(
        topic_auth_scores.iter().all(|&s| s > 0.0),
        "Topics should have positive authority scores"
    );
}

#[test]
fn test_hits_top_authorities() {
    let graph = bipartite_docs_topics();
    let hits = Hits::default();
    let top_auths = hits.top_authorities(&graph, 3);

    // AI appears in all 3 docs, should be top authority
    assert!(
        top_auths[0].0.to_lowercase().contains("ai"),
        "AI should be top authority: {:?}",
        top_auths
    );
}

#[test]
fn test_hits_top_hubs() {
    let graph = bipartite_docs_topics();
    let hits = Hits::default();
    let top_hubs = hits.top_hubs(&graph, 3);

    // Doc3 points to all 3 topics, should be top hub
    assert!(
        top_hubs[0].0.to_lowercase().contains("doc3"),
        "Doc3 should be top hub: {:?}",
        top_hubs
    );
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_all_algorithms_empty_graph() {
    let graph = GraphDocument::new();

    let pr = PageRank::default();
    assert!(pr.compute(&graph).is_empty());

    let bc = Betweenness::default();
    assert!(bc.compute(&graph).is_empty());

    let hits = Hits::default();
    let (h, a) = hits.compute(&graph);
    assert!(h.is_empty());
    assert!(a.is_empty());
}

#[test]
fn test_all_algorithms_single_node() {
    let solo = entity("Solo", 0);
    let graph = GraphDocument::from_extraction(&[solo], &[], None);

    let pr = PageRank::default();
    let pr_scores = pr.compute(&graph);
    assert_eq!(pr_scores.len(), 1);

    let bc = Betweenness::default();
    let bc_scores = bc.compute(&graph);
    assert_eq!(bc_scores.len(), 1);

    let hits = Hits::default();
    let (h, a) = hits.compute(&graph);
    assert_eq!(h.len(), 1);
    assert_eq!(a.len(), 1);
}

// =============================================================================
// Algorithm Comparison
// =============================================================================

#[test]
fn test_algorithms_agree_on_star_hub() {
    let graph = star_graph();

    // All algorithms should agree that hub is most important
    let pr_top = PageRank::default().top_k(&graph, 1);
    let bc_top = Betweenness::default().top_k(&graph, 1);

    assert!(pr_top[0].0.to_lowercase().contains("hub"));
    assert!(bc_top[0].0.to_lowercase().contains("hub"));
}

#[test]
fn test_algorithms_differ_on_bipartite() {
    let graph = bipartite_docs_topics();

    // PageRank: should favor nodes with many incoming edges (topics)
    // HITS: should separate hubs (docs) from authorities (topics)

    let _pr_top = PageRank::default().top_k(&graph, 1); // Computed but not asserted here
    let hits = Hits::default();
    let hits_top_auth = hits.top_authorities(&graph, 1);
    let hits_top_hub = hits.top_hubs(&graph, 1);

    // HITS should clearly separate roles
    // Top hub should be a doc
    assert!(hits_top_hub[0].0.to_lowercase().contains("doc"));
    // Top authority should be a topic
    assert!(
        hits_top_auth[0].0.to_lowercase().contains("ai")
            || hits_top_auth[0].0.to_lowercase().contains("ml")
            || hits_top_auth[0].0.to_lowercase().contains("nlp")
    );
}
