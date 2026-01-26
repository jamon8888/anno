//! Tests for hierarchical clustering (tier).

use anno_core::{GraphDocument, GraphEdge, GraphNode};
use anno_tier::HierarchicalLeiden;

#[test]
fn test_hierarchical_leiden_default() {
    let _leiden = HierarchicalLeiden::new();
}

#[test]
fn test_hierarchical_leiden_builder() {
    let _leiden = HierarchicalLeiden::new()
        .with_resolution(0.5)
        .with_levels(5);
}

#[test]
fn test_cluster_empty_graph() {
    let leiden = HierarchicalLeiden::new();
    let graph = GraphDocument::new();
    // Empty graph - may succeed with empty result or fail gracefully
    let _result = leiden.cluster(&graph);
}

#[test]
fn test_cluster_simple_graph() {
    let leiden = HierarchicalLeiden::new()
        .with_resolution(1.0)
        .with_levels(2);

    let mut graph = GraphDocument::new();

    // Add nodes
    graph.nodes.push(GraphNode {
        id: "n1".to_string(),
        name: "Node 1".to_string(),
        node_type: "entity".to_string(),
        properties: Default::default(),
    });
    graph.nodes.push(GraphNode {
        id: "n2".to_string(),
        name: "Node 2".to_string(),
        node_type: "entity".to_string(),
        properties: Default::default(),
    });
    graph.nodes.push(GraphNode {
        id: "n3".to_string(),
        name: "Node 3".to_string(),
        node_type: "entity".to_string(),
        properties: Default::default(),
    });

    // Add edges
    graph.edges.push(GraphEdge {
        source: "n1".to_string(),
        target: "n2".to_string(),
        relation: "connected".to_string(),
        confidence: 1.0,
        properties: Default::default(),
    });
    graph.edges.push(GraphEdge {
        source: "n2".to_string(),
        target: "n3".to_string(),
        relation: "connected".to_string(),
        confidence: 1.0,
        properties: Default::default(),
    });

    let result = leiden.cluster(&graph);
    assert!(result.is_ok(), "Clustering should succeed for simple graph");

    let clustered = result.expect("clustering should succeed");
    assert_eq!(clustered.nodes.len(), 3, "Should preserve all nodes");
}

#[test]
fn test_cluster_preserves_graph_structure() {
    let leiden = HierarchicalLeiden::new().with_levels(1);

    let mut graph = GraphDocument::new();
    graph.nodes.push(GraphNode {
        id: "a".to_string(),
        name: "A".to_string(),
        node_type: "test".to_string(),
        properties: Default::default(),
    });
    graph.nodes.push(GraphNode {
        id: "b".to_string(),
        name: "B".to_string(),
        node_type: "test".to_string(),
        properties: Default::default(),
    });
    graph.edges.push(GraphEdge {
        source: "a".to_string(),
        target: "b".to_string(),
        relation: "link".to_string(),
        confidence: 1.0,
        properties: Default::default(),
    });

    let result = leiden
        .cluster(&graph)
        .expect("Leiden clustering should succeed");

    // Original structure preserved
    assert_eq!(result.nodes.len(), 2);
    assert_eq!(result.edges.len(), 1);

    // Node IDs preserved
    assert!(result.nodes.iter().any(|n| n.id == "a"));
    assert!(result.nodes.iter().any(|n| n.id == "b"));
}
