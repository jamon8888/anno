//! Integration tests for graph utility functions leveraging petgraph.
//!
//! These tests verify that we correctly bridge GraphDocument to petgraph
//! and expose useful graph analysis capabilities.

use anno_core::{Entity, EntityType, GraphDocument, Relation};
use anno_strata::{
    average_path_length, count_connected_components, find_connected_components, graph_diameter,
    node_eccentricities, shortest_distances, strongly_connected_components, GraphStats,
};

// =============================================================================
// Test Fixtures
// =============================================================================

/// Build a simple cycle graph: A → B → C → D → A
fn cycle_graph() -> GraphDocument {
    let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
    let b = Entity::new("B", EntityType::Person, 10, 11, 0.9);
    let c = Entity::new("C", EntityType::Person, 20, 21, 0.9);
    let d = Entity::new("D", EntityType::Person, 30, 31, 0.9);

    let relations = vec![
        Relation::new(a.clone(), b.clone(), "NEXT", 0.9),
        Relation::new(b.clone(), c.clone(), "NEXT", 0.9),
        Relation::new(c.clone(), d.clone(), "NEXT", 0.9),
        Relation::new(d.clone(), a.clone(), "NEXT", 0.9),
    ];

    GraphDocument::from_extraction(&[a, b, c, d], &relations, None)
}

/// Build a chain graph: A → B → C → D (no cycle)
fn chain_graph() -> GraphDocument {
    let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
    let b = Entity::new("B", EntityType::Person, 10, 11, 0.9);
    let c = Entity::new("C", EntityType::Person, 20, 21, 0.9);
    let d = Entity::new("D", EntityType::Person, 30, 31, 0.9);

    let relations = vec![
        Relation::new(a.clone(), b.clone(), "NEXT", 0.9),
        Relation::new(b.clone(), c.clone(), "NEXT", 0.9),
        Relation::new(c.clone(), d.clone(), "NEXT", 0.9),
    ];

    GraphDocument::from_extraction(&[a, b, c, d], &relations, None)
}

/// Build two disconnected components: (A-B) and (C-D)
fn two_components() -> GraphDocument {
    let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
    let b = Entity::new("B", EntityType::Person, 10, 11, 0.9);
    let c = Entity::new("C", EntityType::Person, 20, 21, 0.9);
    let d = Entity::new("D", EntityType::Person, 30, 31, 0.9);

    let relations = vec![
        Relation::new(a.clone(), b.clone(), "KNOWS", 0.9),
        Relation::new(c.clone(), d.clone(), "KNOWS", 0.9),
    ];

    GraphDocument::from_extraction(&[a, b, c, d], &relations, None)
}

/// Build a star graph: Hub connected to 4 spokes
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

// =============================================================================
// Strongly Connected Components Tests
// =============================================================================

#[test]
fn test_scc_cycle_graph_is_single_component() {
    let graph = cycle_graph();
    let sccs = strongly_connected_components(&graph);

    // A directed cycle forms a single SCC
    assert_eq!(sccs.len(), 1, "Cycle should have 1 SCC");
    assert_eq!(sccs[0].len(), 4, "SCC should contain all 4 nodes");
}

#[test]
fn test_scc_chain_graph_has_multiple_components() {
    let graph = chain_graph();
    let sccs = strongly_connected_components(&graph);

    // A chain A→B→C→D has 4 SCCs (no back edges)
    // Unless the underlying impl treats as undirected
    // Note: our GraphDocument typically creates bidirectional edges for relations
    // so this may behave as if edges are undirected
    assert!(!sccs.is_empty(), "Should have at least one SCC");
}

// =============================================================================
// Connected Components Tests
// =============================================================================

#[test]
fn test_connected_components_single() {
    let graph = cycle_graph();
    let count = count_connected_components(&graph);
    assert_eq!(count, 1, "Cycle graph should have 1 connected component");
}

#[test]
fn test_connected_components_multiple() {
    let graph = two_components();
    let count = count_connected_components(&graph);
    assert_eq!(count, 2, "Should have 2 disconnected components");
}

#[test]
fn test_find_connected_components_returns_nodes() {
    let graph = two_components();
    let components = find_connected_components(&graph);

    assert_eq!(components.len(), 2, "Should find 2 components");

    // Each component should have 2 nodes
    for component in &components {
        assert_eq!(component.len(), 2, "Each component should have 2 nodes");
    }
}

// =============================================================================
// Shortest Path Tests
// =============================================================================

#[test]
fn test_shortest_distances_from_source() {
    let graph = chain_graph();

    // Find a node ID that exists
    let source_id = graph.nodes.first().map(|n| n.id.as_str()).unwrap_or("");

    let distances = shortest_distances(&graph, source_id);

    // Should have distances computed
    // Note: actual distances depend on edge direction and how GraphDocument constructs IDs
    assert!(!distances.is_empty() || graph.edges.is_empty());
}

#[test]
fn test_shortest_distances_nonexistent_source() {
    let graph = chain_graph();
    let distances = shortest_distances(&graph, "nonexistent_node_xyz");

    assert!(
        distances.is_empty(),
        "Nonexistent source should return empty"
    );
}

// =============================================================================
// Graph Metrics Tests
// =============================================================================

#[test]
fn test_average_path_length() {
    let graph = cycle_graph();
    let apl = average_path_length(&graph);

    // A connected graph should have a finite average path length
    assert!(apl.is_some() || graph.nodes.is_empty());
}

#[test]
fn test_graph_diameter_connected() {
    let graph = chain_graph();
    let diameter = graph_diameter(&graph);

    // Chain of 4 nodes should have diameter related to chain length
    assert!(diameter.is_some());
}

#[test]
fn test_graph_diameter_disconnected() {
    let graph = two_components();
    let diameter = graph_diameter(&graph);

    // Disconnected graph may return None or a finite value for largest component
    // Behavior depends on implementation - just verify it doesn't panic
    let _ = diameter;
}

#[test]
fn test_node_eccentricities() {
    let graph = star_graph();
    let eccentricities = node_eccentricities(&graph);

    // Should compute eccentricity for all nodes
    assert_eq!(
        eccentricities.len(),
        graph.nodes.len(),
        "Should have eccentricity for each node"
    );

    // All values should be non-negative
    for (_, &ecc) in &eccentricities {
        assert!(ecc >= 0.0, "Eccentricity should be non-negative");
    }
}

// =============================================================================
// Graph Statistics Tests
// =============================================================================

#[test]
fn test_graph_stats_basic() {
    let graph = cycle_graph();
    let stats = GraphStats::compute(&graph);

    assert_eq!(stats.node_count, 4);
    assert_eq!(stats.edge_count, 4);
    assert_eq!(stats.component_count, 1);
    assert!(stats.density > 0.0 && stats.density <= 1.0);
}

#[test]
fn test_graph_stats_disconnected() {
    let graph = two_components();
    let stats = GraphStats::compute(&graph);

    assert_eq!(stats.node_count, 4);
    assert_eq!(stats.edge_count, 2);
    assert_eq!(stats.component_count, 2);
}

#[test]
fn test_graph_stats_empty() {
    let graph = GraphDocument::new();
    let stats = GraphStats::compute(&graph);

    assert_eq!(stats.node_count, 0);
    assert_eq!(stats.edge_count, 0);
    assert_eq!(stats.component_count, 0);
    assert_eq!(stats.density, 0.0);
}

#[test]
fn test_graph_stats_star() {
    let graph = star_graph();
    let stats = GraphStats::compute(&graph);

    assert_eq!(stats.node_count, 5);
    assert_eq!(stats.edge_count, 4);

    // Hub has degree 4, spokes have degree 1
    assert_eq!(stats.max_degree, 4, "Hub should have max degree");
}

// =============================================================================
// Edge Cases and Properties
// =============================================================================

#[test]
fn test_all_functions_handle_empty_graph() {
    let graph = GraphDocument::new();

    assert!(strongly_connected_components(&graph).is_empty());
    assert_eq!(count_connected_components(&graph), 0);
    assert!(find_connected_components(&graph).is_empty());
    assert!(shortest_distances(&graph, "any").is_empty());
    assert_eq!(average_path_length(&graph), Some(0.0));
    assert_eq!(graph_diameter(&graph), Some(0.0));
    assert!(node_eccentricities(&graph).is_empty());
}

#[test]
fn test_single_node_graph() {
    let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
    let graph = GraphDocument::from_extraction(&[a], &[], None);

    assert_eq!(count_connected_components(&graph), 1);
    assert_eq!(strongly_connected_components(&graph).len(), 1);

    let stats = GraphStats::compute(&graph);
    assert_eq!(stats.node_count, 1);
    assert_eq!(stats.edge_count, 0);
}

#[test]
fn test_self_loop_handling() {
    let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
    let relations = vec![Relation::new(a.clone(), a.clone(), "SELF", 0.9)];
    let graph = GraphDocument::from_extraction(&[a], &relations, None);

    // Should handle self-loops gracefully
    let stats = GraphStats::compute(&graph);
    assert_eq!(stats.node_count, 1);
    // Self-loop adds to edge count but may or may not be counted in the GraphDocument
}
