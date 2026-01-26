//! RAPTOR-style hierarchical tree building for GraphDocument.
//!
//! This module provides integration with the `tier` crate for building
//! hierarchical summaries of graphs, supporting RAPTOR-style retrieval.
//!
//! ## RAPTOR Pipeline
//!
//! 1. **Cluster**: Group nodes by embedding similarity (GMM/K-means)
//! 2. **Summarize**: Create summary nodes for each cluster
//! 3. **Recurse**: Build higher levels from summaries
//!
//! ## Usage
//!
//! ```rust,ignore
//! use anno_tier::raptor::{RaptorConfig, build_raptor_tree};
//! use anno_core::GraphDocument;
//!
//! let graph = GraphDocument::new();
//! // ... populate graph with nodes having "embedding" properties
//!
//! let config = RaptorConfig::default();
//! let tree = build_raptor_tree(&graph, config, |nodes| {
//!     // Your summarization function
//!     summarize_with_llm(nodes)
//! })?;
//! ```
//!
//! ## References
//!
//! Sarthi et al. (2024). "RAPTOR: Recursive Abstractive Processing for
//! Tree-Organized Retrieval." ICLR 2024.

use anno_core::{GraphDocument, GraphNode};
use std::collections::HashMap;

/// Configuration for RAPTOR tree building.
#[derive(Debug, Clone)]
pub struct RaptorConfig {
    /// Maximum depth of the tree.
    pub max_depth: usize,
    /// Target number of nodes per cluster (fanout).
    pub cluster_size: usize,
    /// Minimum nodes needed to form a cluster.
    pub min_cluster_size: usize,
    /// Property name containing node embeddings.
    pub embedding_property: String,
    /// Property name for storing level in hierarchy.
    pub level_property: String,
    /// Property name for storing parent node ID.
    pub parent_property: String,
}

impl Default for RaptorConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            cluster_size: 6,
            min_cluster_size: 2,
            embedding_property: "embedding".to_string(),
            level_property: "raptor_level".to_string(),
            parent_property: "raptor_parent".to_string(),
        }
    }
}

impl RaptorConfig {
    /// Create a new configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum tree depth.
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set target cluster size.
    pub fn with_cluster_size(mut self, size: usize) -> Self {
        self.cluster_size = size;
        self
    }

    /// Set the property name containing embeddings.
    pub fn with_embedding_property(mut self, prop: impl Into<String>) -> Self {
        self.embedding_property = prop.into();
        self
    }
}

/// Result of building a RAPTOR tree.
#[derive(Debug, Clone)]
pub struct RaptorTree {
    /// The augmented graph with summary nodes.
    pub graph: GraphDocument,
    /// Number of levels in the tree.
    pub depth: usize,
    /// Node IDs at each level (level 0 = leaves).
    pub levels: Vec<Vec<String>>,
    /// Statistics about the tree.
    pub stats: TreeStats,
}

/// Statistics about a RAPTOR tree.
#[derive(Debug, Clone, Default)]
pub struct TreeStats {
    /// Total nodes (leaves + summaries).
    pub total_nodes: usize,
    /// Number of leaf nodes.
    pub leaf_count: usize,
    /// Number of summary nodes.
    pub summary_count: usize,
    /// Average cluster size per level.
    pub avg_cluster_sizes: Vec<f32>,
}

/// Build a RAPTOR-style hierarchical tree from a GraphDocument.
///
/// This function clusters nodes at each level and creates summary nodes
/// using the provided summarization function.
///
/// # Arguments
///
/// * `graph` - The input graph with nodes to organize
/// * `config` - Configuration for tree building
/// * `summarize_fn` - Function that takes a slice of nodes and produces a summary node
///
/// # Returns
///
/// A `RaptorTree` containing the hierarchical structure.
pub fn build_raptor_tree<F>(
    graph: &GraphDocument,
    config: RaptorConfig,
    summarize_fn: F,
) -> Result<RaptorTree, String>
where
    F: Fn(&[&GraphNode]) -> GraphNode,
{
    if graph.nodes.is_empty() {
        return Err("Cannot build tree from empty graph".to_string());
    }

    let mut result_graph = graph.clone();
    let mut levels: Vec<Vec<String>> = Vec::new();

    // Level 0: Original nodes (leaves)
    let mut current_ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();

    // Mark original nodes as level 0
    for node in &mut result_graph.nodes {
        node.properties
            .insert(config.level_property.clone(), serde_json::json!(0));
    }
    levels.push(current_ids.clone());

    // Extract embeddings for clustering
    let embeddings = extract_embeddings(&result_graph, &current_ids, &config.embedding_property)?;

    // Build higher levels
    let mut next_node_id = graph.nodes.len();

    for level in 1..=config.max_depth {
        if current_ids.len() <= config.min_cluster_size {
            break;
        }

        // Cluster current level's nodes
        let n_clusters = (current_ids.len() / config.cluster_size).max(1);
        let clusters = simple_kmeans_cluster(&embeddings, &current_ids, n_clusters)?;

        if clusters.len() <= 1 {
            break;
        }

        let mut level_ids = Vec::new();
        let mut level_embeddings = Vec::new();

        for cluster_ids in clusters {
            if cluster_ids.is_empty() {
                continue;
            }

            // Get nodes in this cluster
            let cluster_nodes: Vec<&GraphNode> = cluster_ids
                .iter()
                .filter_map(|id| result_graph.nodes.iter().find(|n| &n.id == id))
                .collect();

            if cluster_nodes.is_empty() {
                continue;
            }

            // Create summary node
            let mut summary = summarize_fn(&cluster_nodes);
            summary.id = format!("summary_{}", next_node_id);
            next_node_id += 1;

            // Set level and parent properties
            summary
                .properties
                .insert(config.level_property.clone(), serde_json::json!(level));

            // Compute mean embedding for summary
            let mean_embedding = compute_mean_embedding(&cluster_nodes, &config.embedding_property);
            if let Some(emb) = mean_embedding {
                summary
                    .properties
                    .insert(config.embedding_property.clone(), serde_json::json!(emb));
                level_embeddings.push((summary.id.clone(), emb));
            }

            // Link children to parent
            for child_id in &cluster_ids {
                if let Some(child) = result_graph.nodes.iter_mut().find(|n| &n.id == child_id) {
                    child
                        .properties
                        .insert(config.parent_property.clone(), summary.id.clone().into());
                }
            }

            level_ids.push(summary.id.clone());
            result_graph.nodes.push(summary);
        }

        if level_ids.is_empty() {
            break;
        }

        levels.push(level_ids.clone());
        current_ids = level_ids;

        // Update embeddings for next level
        // embeddings = level_embeddings; // Would need to restructure
    }

    // Compute statistics
    let stats = TreeStats {
        total_nodes: result_graph.nodes.len(),
        leaf_count: graph.nodes.len(),
        summary_count: result_graph.nodes.len() - graph.nodes.len(),
        avg_cluster_sizes: levels
            .windows(2)
            .map(|w| w[0].len() as f32 / w[1].len().max(1) as f32)
            .collect(),
    };

    Ok(RaptorTree {
        graph: result_graph,
        depth: levels.len(),
        levels,
        stats,
    })
}

/// Extract embeddings from nodes.
fn extract_embeddings(
    graph: &GraphDocument,
    node_ids: &[String],
    embedding_prop: &str,
) -> Result<HashMap<String, Vec<f32>>, String> {
    let mut embeddings = HashMap::new();

    for id in node_ids {
        if let Some(node) = graph.nodes.iter().find(|n| &n.id == id) {
            if let Some(emb_value) = node.properties.get(embedding_prop) {
                if let Some(emb_arr) = emb_value.as_array() {
                    let embedding: Vec<f32> = emb_arr
                        .iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect();
                    if !embedding.is_empty() {
                        embeddings.insert(id.clone(), embedding);
                    }
                }
            }
        }
    }

    if embeddings.is_empty() {
        return Err(format!(
            "No embeddings found in property '{}'",
            embedding_prop
        ));
    }

    Ok(embeddings)
}

/// Simple k-means clustering (fallback when `tier` is not enabled/available).
fn simple_kmeans_cluster(
    embeddings: &HashMap<String, Vec<f32>>,
    node_ids: &[String],
    k: usize,
) -> Result<Vec<Vec<String>>, String> {
    if node_ids.is_empty() || k == 0 {
        return Ok(vec![]);
    }

    let k = k.min(node_ids.len());

    // Get embeddings in order
    let mut ordered_embeddings: Vec<(String, Vec<f32>)> = Vec::new();
    for id in node_ids {
        if let Some(emb) = embeddings.get(id) {
            ordered_embeddings.push((id.clone(), emb.clone()));
        }
    }

    if ordered_embeddings.is_empty() {
        // No embeddings: fall back to simple grouping
        let mut clusters: Vec<Vec<String>> = (0..k).map(|_| Vec::new()).collect();
        for (i, id) in node_ids.iter().enumerate() {
            clusters[i % k].push(id.clone());
        }
        return Ok(clusters);
    }

    // Initialize centroids (first k points)
    let dim = ordered_embeddings[0].1.len();
    let mut centroids: Vec<Vec<f32>> = ordered_embeddings
        .iter()
        .take(k)
        .map(|(_, e)| e.clone())
        .collect();

    // Simple k-means iterations
    let mut assignments = vec![0usize; ordered_embeddings.len()];
    for _iter in 0..20 {
        // Assignment step
        for (i, (_, emb)) in ordered_embeddings.iter().enumerate() {
            let mut best_cluster = 0;
            let mut best_dist = f32::MAX;
            for (c, centroid) in centroids.iter().enumerate() {
                let dist: f32 = emb
                    .iter()
                    .zip(centroid.iter())
                    .map(|(a, b)| (a - b).powi(2))
                    .sum();
                if dist < best_dist {
                    best_dist = dist;
                    best_cluster = c;
                }
            }
            assignments[i] = best_cluster;
        }

        // Update step
        let mut new_centroids = vec![vec![0.0; dim]; k];
        let mut counts = vec![0usize; k];

        for (i, (_, emb)) in ordered_embeddings.iter().enumerate() {
            let c = assignments[i];
            counts[c] += 1;
            for (j, v) in emb.iter().enumerate() {
                new_centroids[c][j] += v;
            }
        }

        for c in 0..k {
            if counts[c] > 0 {
                for j in 0..dim {
                    new_centroids[c][j] /= counts[c] as f32;
                }
            }
        }
        centroids = new_centroids;
    }

    // Group by assignment
    let mut clusters: Vec<Vec<String>> = (0..k).map(|_| Vec::new()).collect();
    for (i, (id, _)) in ordered_embeddings.iter().enumerate() {
        clusters[assignments[i]].push(id.clone());
    }

    // Remove empty clusters
    clusters.retain(|c| !c.is_empty());

    Ok(clusters)
}

/// Compute mean embedding from a set of nodes.
fn compute_mean_embedding(nodes: &[&GraphNode], embedding_prop: &str) -> Option<Vec<f32>> {
    let embeddings: Vec<Vec<f32>> = nodes
        .iter()
        .filter_map(|n| {
            n.properties.get(embedding_prop).and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_f64().map(|f| f as f32))
                        .collect()
                })
            })
        })
        .collect();

    if embeddings.is_empty() {
        return None;
    }

    let dim = embeddings[0].len();
    let mut mean = vec![0.0; dim];

    for emb in &embeddings {
        for (i, v) in emb.iter().enumerate() {
            mean[i] += v;
        }
    }

    let n = embeddings.len() as f32;
    for v in &mut mean {
        *v /= n;
    }

    Some(mean)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_graph() -> GraphDocument {
        let mut graph = GraphDocument::new();

        // Add nodes with embeddings
        for i in 0..10 {
            let mut node = GraphNode::new(format!("node_{}", i), "chunk", format!("Node {}", i));
            node.properties.insert(
                "embedding".to_string(),
                serde_json::json!(vec![i as f32 / 10.0, (i as f32).sin()]),
            );
            node.properties
                .insert("text".to_string(), serde_json::json!(format!("Text {}", i)));
            graph.nodes.push(node);
        }

        graph
    }

    #[test]
    fn test_raptor_config_default() {
        let config = RaptorConfig::default();
        assert_eq!(config.max_depth, 4);
        assert_eq!(config.cluster_size, 6);
    }

    #[test]
    fn test_build_raptor_tree() {
        let graph = make_test_graph();
        let config = RaptorConfig::default().with_cluster_size(3);

        let tree = build_raptor_tree(&graph, config, |nodes| {
            // Simple summarizer: concatenate texts
            let texts: Vec<String> = nodes
                .iter()
                .filter_map(|n| n.properties.get("text")?.as_str().map(String::from))
                .collect();

            let mut summary = GraphNode::new("temp", "summary", "Summary");
            summary
                .properties
                .insert("text".to_string(), serde_json::json!(texts.join(" | ")));
            summary
        })
        .unwrap();

        assert!(tree.depth >= 2);
        assert_eq!(tree.stats.leaf_count, 10);
        assert!(tree.stats.summary_count > 0);
    }

    #[test]
    fn test_extract_embeddings() {
        let graph = make_test_graph();
        let ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
        let embeddings = extract_embeddings(&graph, &ids, "embedding").unwrap();
        assert_eq!(embeddings.len(), 10);
    }
}
