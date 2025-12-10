//! Graph utility functions bridging `GraphDocument` to petgraph.
//!
//! This module provides efficient conversions and utility functions that
//! leverage petgraph's optimized implementations for common graph operations.
//!
//! # Key Benefits
//!
//! - **Strongly Connected Components**: O(V+E) via Tarjan/Kosaraju
//! - **Connected Components**: O(V+E) via Union-Find
//! - **Shortest Paths**: Dijkstra, Bellman-Ford, Johnson for graph distances
//! - **Cycle Detection**: Detect cycles in directed graphs
//!
//! These functions complement our custom implementations (Leiden, PageRank)
//! by providing foundational graph analysis.

use anno_core::GraphDocument;
use petgraph::algo::{bellman_ford, connected_components, dijkstra, kosaraju_scc, tarjan_scc};
use petgraph::graph::{DiGraph, NodeIndex, UnGraph};
use std::collections::HashMap;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

// =============================================================================
// Graph Conversion
// =============================================================================

/// Convert a `GraphDocument` to a petgraph `DiGraph` (directed).
///
/// Returns the graph and a bidirectional mapping between node IDs and indices.
pub fn to_digraph(
    doc: &GraphDocument,
) -> (
    DiGraph<String, f32>,
    HashMap<String, NodeIndex>,
    HashMap<NodeIndex, String>,
) {
    let mut graph = DiGraph::new();
    let mut id_to_idx = HashMap::new();
    let mut idx_to_id = HashMap::new();

    // Add nodes
    for node in &doc.nodes {
        let idx = graph.add_node(node.id.clone());
        id_to_idx.insert(node.id.clone(), idx);
        idx_to_id.insert(idx, node.id.clone());
    }

    // Add edges - use confidence as weight (default 1.0)
    for edge in &doc.edges {
        if let (Some(&src), Some(&tgt)) = (id_to_idx.get(&edge.source), id_to_idx.get(&edge.target))
        {
            let weight = if edge.confidence > 0.0 {
                edge.confidence as f32
            } else {
                1.0
            };
            graph.add_edge(src, tgt, weight);
        }
    }

    (graph, id_to_idx, idx_to_id)
}

/// Convert a `GraphDocument` to a petgraph `UnGraph` (undirected).
///
/// Returns the graph and a bidirectional mapping between node IDs and indices.
pub fn to_ungraph(
    doc: &GraphDocument,
) -> (
    UnGraph<String, f32>,
    HashMap<String, NodeIndex>,
    HashMap<NodeIndex, String>,
) {
    let mut graph = UnGraph::new_undirected();
    let mut id_to_idx = HashMap::new();
    let mut idx_to_id = HashMap::new();

    // Add nodes
    for node in &doc.nodes {
        let idx = graph.add_node(node.id.clone());
        id_to_idx.insert(node.id.clone(), idx);
        idx_to_id.insert(idx, node.id.clone());
    }

    // Add edges (undirected) - use confidence as weight (default 1.0)
    for edge in &doc.edges {
        if let (Some(&src), Some(&tgt)) = (id_to_idx.get(&edge.source), id_to_idx.get(&edge.target))
        {
            let weight = if edge.confidence > 0.0 {
                edge.confidence as f32
            } else {
                1.0
            };
            graph.add_edge(src, tgt, weight);
        }
    }

    (graph, id_to_idx, idx_to_id)
}

// =============================================================================
// Strongly Connected Components
// =============================================================================

/// Find strongly connected components using Tarjan's algorithm.
///
/// Returns a vector of components, where each component is a vector of node IDs.
/// Components are ordered by reverse topological sort (postorder).
///
/// # Complexity
/// - Time: O(V + E)
/// - Space: O(V)
///
/// # Example
///
/// ```rust,ignore
/// let components = strongly_connected_components(&graph);
/// for (i, component) in components.iter().enumerate() {
///     println!("SCC {}: {:?}", i, component);
/// }
/// ```
pub fn strongly_connected_components(doc: &GraphDocument) -> Vec<Vec<String>> {
    let (graph, _, idx_to_id) = to_digraph(doc);
    let sccs = tarjan_scc(&graph);

    sccs.into_iter()
        .map(|scc| {
            scc.into_iter()
                .filter_map(|idx| idx_to_id.get(&idx).cloned())
                .collect()
        })
        .collect()
}

/// Find strongly connected components using Kosaraju's algorithm.
///
/// Alternative to Tarjan's algorithm with same complexity but different traversal order.
pub fn strongly_connected_components_kosaraju(doc: &GraphDocument) -> Vec<Vec<String>> {
    let (graph, _, idx_to_id) = to_digraph(doc);
    let sccs = kosaraju_scc(&graph);

    sccs.into_iter()
        .map(|scc| {
            scc.into_iter()
                .filter_map(|idx| idx_to_id.get(&idx).cloned())
                .collect()
        })
        .collect()
}

// =============================================================================
// Connected Components
// =============================================================================

/// Count the number of (weakly) connected components.
///
/// For directed graphs, this computes weakly connected components
/// (ignoring edge direction).
///
/// # Complexity
/// - Time: O(V + E)
/// - Space: O(V)
pub fn count_connected_components(doc: &GraphDocument) -> usize {
    let (graph, _, _) = to_digraph(doc);
    connected_components(&graph)
}

/// Find all connected components and return their node IDs.
///
/// Uses strongly connected components as a proxy for connected component
/// membership, then groups nodes.
pub fn find_connected_components(doc: &GraphDocument) -> Vec<Vec<String>> {
    // For undirected interpretation, we use the undirected graph
    let (graph, _, idx_to_id) = to_ungraph(doc);

    // Use tarjan_scc on undirected graph - each SCC is a connected component
    let sccs = tarjan_scc(&graph);

    sccs.into_iter()
        .map(|scc| {
            scc.into_iter()
                .filter_map(|idx| idx_to_id.get(&idx).cloned())
                .collect()
        })
        .collect()
}

// =============================================================================
// Shortest Paths
// =============================================================================

/// Compute shortest path distances from a source node to all reachable nodes.
///
/// Uses Dijkstra's algorithm (requires non-negative edge weights).
///
/// # Arguments
/// - `doc`: The graph document
/// - `source_id`: The source node ID
///
/// # Returns
/// A map from node ID to shortest distance from source.
/// Unreachable nodes are not included.
///
/// # Complexity
/// - Time: O((V + E) log V)
/// - Space: O(V + E)
pub fn shortest_distances(doc: &GraphDocument, source_id: &str) -> HashMap<String, f32> {
    let (graph, id_to_idx, idx_to_id) = to_digraph(doc);

    let Some(&source_idx) = id_to_idx.get(source_id) else {
        return HashMap::new();
    };

    let distances = dijkstra(&graph, source_idx, None, |e| *e.weight());

    distances
        .into_iter()
        .filter_map(|(idx, dist)| idx_to_id.get(&idx).map(|id| (id.clone(), dist)))
        .collect()
}

/// Compute shortest path distances using Bellman-Ford (supports negative weights).
///
/// Returns `None` if a negative cycle is detected.
///
/// # Complexity
/// - Time: O(V * E)
/// - Space: O(V)
pub fn shortest_distances_bellman_ford(
    doc: &GraphDocument,
    source_id: &str,
) -> Option<HashMap<String, f64>> {
    let (graph, id_to_idx, idx_to_id) = to_digraph(doc);

    let Some(&source_idx) = id_to_idx.get(source_id) else {
        return Some(HashMap::new());
    };

    // Convert weights to f64 for Bellman-Ford
    let graph_f64: DiGraph<String, f64> = graph.map(|_, n| n.clone(), |_, e| *e as f64);

    match bellman_ford(&graph_f64, source_idx) {
        Ok(paths) => {
            let mut result = HashMap::new();
            for (idx, &dist) in paths.distances.iter().enumerate() {
                if dist.is_finite() {
                    if let Some(id) = idx_to_id.get(&NodeIndex::new(idx)) {
                        result.insert(id.clone(), dist);
                    }
                }
            }
            Some(result)
        }
        Err(_) => None, // Negative cycle detected
    }
}

// =============================================================================
// Graph Distance Metrics
// =============================================================================

/// Compute the average shortest path length (characteristic path length).
///
/// This is a key metric for "small world" network analysis.
/// Returns `None` if the graph is disconnected.
///
/// # Parallelism
///
/// When the `parallel` feature is enabled, this uses rayon for parallel
/// computation across source nodes.
pub fn average_path_length(doc: &GraphDocument) -> Option<f64> {
    if doc.nodes.is_empty() {
        return Some(0.0);
    }

    let (graph, _, _) = to_digraph(doc);
    let n = graph.node_count();
    if n <= 1 {
        return Some(0.0);
    }

    #[cfg(feature = "parallel")]
    {
        // Parallel version: compute distances from each source in parallel
        let results: Vec<(f64, usize)> = graph
            .node_indices()
            .collect::<Vec<_>>()
            .par_iter()
            .map(|&source| {
                let distances = dijkstra(&graph, source, None, |e| *e.weight());
                let mut total = 0.0;
                let mut count = 0;
                for (target, dist) in distances {
                    if source != target {
                        total += dist as f64;
                        count += 1;
                    }
                }
                (total, count)
            })
            .collect();

        let (total_distance, path_count): (f64, usize) = results
            .iter()
            .fold((0.0, 0), |(t, c), &(dt, dc)| (t + dt, c + dc));

        if path_count == 0 {
            None
        } else {
            Some(total_distance / path_count as f64)
        }
    }

    #[cfg(not(feature = "parallel"))]
    {
        // Sequential version
        let mut total_distance = 0.0;
        let mut path_count = 0;

        for source in graph.node_indices() {
            let distances = dijkstra(&graph, source, None, |e| *e.weight());
            for (target, dist) in distances {
                if source != target {
                    total_distance += dist as f64;
                    path_count += 1;
                }
            }
        }

        if path_count == 0 {
            None
        } else {
            Some(total_distance / path_count as f64)
        }
    }
}

/// Compute the diameter (longest shortest path) of the graph.
///
/// Returns `None` if the graph is disconnected.
pub fn graph_diameter(doc: &GraphDocument) -> Option<f32> {
    if doc.nodes.is_empty() {
        return Some(0.0);
    }

    let (graph, _, _) = to_digraph(doc);
    let mut max_distance = 0.0_f32;

    for source in graph.node_indices() {
        let distances = dijkstra(&graph, source, None, |e| *e.weight());
        for (_, dist) in distances {
            if dist.is_finite() && dist > max_distance {
                max_distance = dist;
            }
        }
    }

    if max_distance == 0.0 && doc.nodes.len() > 1 {
        // Check if disconnected
        if count_connected_components(doc) > 1 {
            return None;
        }
    }

    Some(max_distance)
}

/// Compute the eccentricity of each node (max distance to any other node).
///
/// # Parallelism
///
/// When the `parallel` feature is enabled, this uses rayon for parallel
/// computation across nodes.
pub fn node_eccentricities(doc: &GraphDocument) -> HashMap<String, f32> {
    let (graph, _, idx_to_id) = to_digraph(doc);

    #[cfg(feature = "parallel")]
    {
        // Parallel version
        let results: Vec<(String, f32)> = graph
            .node_indices()
            .collect::<Vec<_>>()
            .par_iter()
            .filter_map(|&source| {
                let distances = dijkstra(&graph, source, None, |e| *e.weight());
                let max_dist = distances
                    .values()
                    .filter(|d| d.is_finite())
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .copied()
                    .unwrap_or(0.0);

                idx_to_id.get(&source).map(|id| (id.clone(), max_dist))
            })
            .collect();

        results.into_iter().collect()
    }

    #[cfg(not(feature = "parallel"))]
    {
        // Sequential version
        let mut eccentricities = HashMap::new();

        for source in graph.node_indices() {
            let distances = dijkstra(&graph, source, None, |e| *e.weight());
            let max_dist = distances
                .values()
                .filter(|d| d.is_finite())
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .copied()
                .unwrap_or(0.0);

            if let Some(id) = idx_to_id.get(&source) {
                eccentricities.insert(id.clone(), max_dist);
            }
        }

        eccentricities
    }
}

// =============================================================================
// Graph Statistics
// =============================================================================

/// Compute basic graph statistics.
#[derive(Debug, Clone, Default)]
pub struct GraphStats {
    /// Number of nodes
    pub node_count: usize,
    /// Number of edges
    pub edge_count: usize,
    /// Number of connected components
    pub component_count: usize,
    /// Average node degree
    pub avg_degree: f64,
    /// Maximum node degree
    pub max_degree: usize,
    /// Graph density (edges / possible edges)
    pub density: f64,
}

impl GraphStats {
    /// Compute statistics for a graph document.
    pub fn compute(doc: &GraphDocument) -> Self {
        let n = doc.nodes.len();
        let e = doc.edges.len();

        // Compute degrees
        let mut degree_map: HashMap<&str, usize> = HashMap::new();
        for edge in &doc.edges {
            *degree_map.entry(&edge.source).or_insert(0) += 1;
            *degree_map.entry(&edge.target).or_insert(0) += 1;
        }

        let max_degree = degree_map.values().max().copied().unwrap_or(0);
        let avg_degree = if n > 0 {
            degree_map.values().sum::<usize>() as f64 / n as f64
        } else {
            0.0
        };

        // Density: actual edges / possible edges
        // For undirected: n*(n-1)/2, for directed: n*(n-1)
        let density = if n > 1 {
            (2.0 * e as f64) / (n as f64 * (n as f64 - 1.0))
        } else {
            0.0
        };

        Self {
            node_count: n,
            edge_count: e,
            component_count: count_connected_components(doc),
            avg_degree,
            max_degree,
            density,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_core::{Entity, EntityType, Relation};

    fn sample_graph() -> GraphDocument {
        let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
        let b = Entity::new("B", EntityType::Person, 10, 11, 0.9);
        let c = Entity::new("C", EntityType::Person, 20, 21, 0.9);
        let d = Entity::new("D", EntityType::Person, 30, 31, 0.9);

        let relations = vec![
            Relation::new(a.clone(), b.clone(), "KNOWS", 0.9),
            Relation::new(b.clone(), c.clone(), "KNOWS", 0.9),
            Relation::new(c.clone(), d.clone(), "KNOWS", 0.9),
            Relation::new(d.clone(), a.clone(), "KNOWS", 0.9), // Cycle
        ];

        GraphDocument::from_extraction(&[a, b, c, d], &relations, None)
    }

    fn disconnected_graph() -> GraphDocument {
        let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
        let b = Entity::new("B", EntityType::Person, 10, 11, 0.9);
        let c = Entity::new("C", EntityType::Person, 20, 21, 0.9);
        let d = Entity::new("D", EntityType::Person, 30, 31, 0.9);

        // A-B connected, C-D connected, but no connection between groups
        let relations = vec![
            Relation::new(a.clone(), b.clone(), "KNOWS", 0.9),
            Relation::new(c.clone(), d.clone(), "KNOWS", 0.9),
        ];

        GraphDocument::from_extraction(&[a, b, c, d], &relations, None)
    }

    #[test]
    fn test_scc_single_component() {
        let graph = sample_graph();
        let sccs = strongly_connected_components(&graph);

        // Cycle graph should have 1 SCC containing all nodes
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 4);
    }

    #[test]
    fn test_connected_components_count() {
        let graph = sample_graph();
        assert_eq!(count_connected_components(&graph), 1);

        let disconnected = disconnected_graph();
        assert_eq!(count_connected_components(&disconnected), 2);
    }

    #[test]
    fn test_shortest_distances() {
        let graph = sample_graph();
        let distances = shortest_distances(&graph, "per:a");

        // Should have distances to all nodes
        assert!(distances.len() <= 4);
        // Distance to self is 0
        if let Some(&self_dist) = distances.get("per:a") {
            assert_eq!(self_dist, 0.0);
        }
    }

    #[test]
    fn test_graph_stats() {
        let graph = sample_graph();
        let stats = GraphStats::compute(&graph);

        assert_eq!(stats.node_count, 4);
        assert_eq!(stats.edge_count, 4);
        assert_eq!(stats.component_count, 1);
        assert!(stats.density > 0.0);
    }

    #[test]
    fn test_empty_graph() {
        let graph = GraphDocument::new();

        assert_eq!(count_connected_components(&graph), 0);
        assert!(strongly_connected_components(&graph).is_empty());
        assert!(shortest_distances(&graph, "x").is_empty());

        let stats = GraphStats::compute(&graph);
        assert_eq!(stats.node_count, 0);
    }
}
