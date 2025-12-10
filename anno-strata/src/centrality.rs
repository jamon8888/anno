//! Graph centrality metrics for knowledge graphs.
//!
//! Multiple centrality algorithms capture different notions of "importance":
//!
//! | Algorithm | Question Answered | Best For |
//! |-----------|-------------------|----------|
//! | [`PageRank`] | "Connected to important things?" | General entity importance |
//! | [`Betweenness`] | "Bridges between communities?" | Finding connector entities |
//! | [`Hits`] | "Hub or authority?" | Bipartite-ish structures |
//!
//! # Which to Choose?
//!
//! - **Default**: `PageRank` - robust, handles directed graphs, good for entity salience
//! - **Bridge detection**: `Betweenness` - finds entities connecting different domains
//! - **Hub/authority**: `HITS` - when you need to distinguish general vs specific entities
//!
//! # References
//!
//! - Page, Brin et al. (1999): PageRank
//! - Freeman (1977): Betweenness centrality
//! - Kleinberg (1999): HITS (Hyperlink-Induced Topic Search)

use anno_core::GraphDocument;
use std::collections::HashMap;

// Re-export PageRank from its own module for backwards compatibility
pub use crate::pagerank::PageRank;

// =============================================================================
// Betweenness Centrality
// =============================================================================

/// Betweenness centrality: nodes that lie on shortest paths between others.
///
/// High betweenness = "bridge" or "bottleneck" entities connecting different
/// parts of the graph. Useful for finding entities that connect domains.
///
/// **Warning**: O(V × E) complexity. Expensive on large graphs.
///
/// # Example
///
/// ```rust,ignore
/// use anno_strata::centrality::Betweenness;
/// let bc = Betweenness::default();
/// let scores = bc.compute(&graph);
/// // High-scoring nodes are structural bridges
/// ```
#[derive(Debug, Clone, Default)]
pub struct Betweenness {
    /// Normalize scores to [0, 1] range
    pub normalize: bool,
}

impl Betweenness {
    /// Create a new betweenness calculator.
    pub fn new() -> Self {
        Self { normalize: true }
    }

    /// Disable normalization (raw path counts).
    pub fn without_normalization(mut self) -> Self {
        self.normalize = false;
        self
    }

    /// Compute betweenness centrality for all nodes.
    ///
    /// Uses Brandes' algorithm: O(V × E) for unweighted graphs.
    pub fn compute(&self, graph: &GraphDocument) -> HashMap<String, f64> {
        let n = graph.nodes.len();
        if n == 0 {
            return HashMap::new();
        }

        // Build adjacency list (undirected for simplicity)
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in &graph.nodes {
            adjacency.insert(&node.id, Vec::new());
        }
        for edge in &graph.edges {
            if let Some(neighbors) = adjacency.get_mut(edge.source.as_str()) {
                neighbors.push(&edge.target);
            }
            if let Some(neighbors) = adjacency.get_mut(edge.target.as_str()) {
                neighbors.push(&edge.source);
            }
        }

        // Initialize betweenness scores
        let mut betweenness: HashMap<&str, f64> =
            graph.nodes.iter().map(|n| (n.id.as_str(), 0.0)).collect();

        // Brandes' algorithm: BFS from each node
        for source in &graph.nodes {
            let source_id = source.id.as_str();

            // BFS
            let mut stack: Vec<&str> = Vec::new();
            let mut predecessors: HashMap<&str, Vec<&str>> = HashMap::new();
            let mut sigma: HashMap<&str, f64> = HashMap::new(); // # shortest paths
            let mut dist: HashMap<&str, i32> = HashMap::new();

            for node in &graph.nodes {
                predecessors.insert(&node.id, Vec::new());
                sigma.insert(&node.id, 0.0);
                dist.insert(&node.id, -1);
            }

            sigma.insert(source_id, 1.0);
            dist.insert(source_id, 0);

            let mut queue = std::collections::VecDeque::new();
            queue.push_back(source_id);

            while let Some(v) = queue.pop_front() {
                stack.push(v);
                let v_dist = *dist.get(v).unwrap_or(&-1);

                if let Some(neighbors) = adjacency.get(v) {
                    for &w in neighbors {
                        let w_dist = dist.get(w).copied().unwrap_or(-1);

                        // First visit?
                        if w_dist < 0 {
                            dist.insert(w, v_dist + 1);
                            queue.push_back(w);
                        }

                        // Shortest path through v?
                        if dist.get(w).copied().unwrap_or(-1) == v_dist + 1 {
                            let v_sigma = sigma.get(v).copied().unwrap_or(0.0);
                            *sigma.entry(w).or_insert(0.0) += v_sigma;
                            predecessors.entry(w).or_default().push(v);
                        }
                    }
                }
            }

            // Accumulation
            let mut delta: HashMap<&str, f64> =
                graph.nodes.iter().map(|n| (n.id.as_str(), 0.0)).collect();

            while let Some(w) = stack.pop() {
                let w_sigma = sigma.get(w).copied().unwrap_or(1.0);
                let w_delta = delta.get(w).copied().unwrap_or(0.0);

                if let Some(preds) = predecessors.get(w) {
                    for &v in preds {
                        let v_sigma = sigma.get(v).copied().unwrap_or(1.0);
                        let contribution = (v_sigma / w_sigma) * (1.0 + w_delta);
                        *delta.entry(v).or_insert(0.0) += contribution;
                    }
                }

                if w != source_id {
                    *betweenness.entry(w).or_insert(0.0) += w_delta;
                }
            }
        }

        // Normalize if requested (divide by (n-1)(n-2) for undirected)
        let normalization_factor = if self.normalize && n > 2 {
            2.0 / ((n - 1) * (n - 2)) as f64
        } else {
            1.0
        };

        betweenness
            .into_iter()
            .map(|(k, v)| (k.to_string(), v * normalization_factor))
            .collect()
    }

    /// Get top-k bridge nodes.
    pub fn top_k(&self, graph: &GraphDocument, k: usize) -> Vec<(String, f64)> {
        let mut scores: Vec<_> = self.compute(graph).into_iter().collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }
}

// =============================================================================
// HITS (Hubs and Authorities)
// =============================================================================

/// HITS algorithm: distinguishes hubs from authorities.
///
/// - **Authority**: pointed to by many good hubs (specific, authoritative entities)
/// - **Hub**: points to many good authorities (general, connecting entities)
///
/// Useful when your graph has a bipartite-ish structure (e.g., documents→entities,
/// categories→pages).
///
/// # Example
///
/// ```rust,ignore
/// use anno_strata::centrality::Hits;
/// let hits = Hits::default();
/// let (hubs, authorities) = hits.compute(&graph);
/// ```
#[derive(Debug, Clone)]
pub struct Hits {
    /// Maximum iterations
    pub max_iterations: usize,
    /// Convergence threshold
    pub epsilon: f64,
}

impl Default for Hits {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            epsilon: 1e-6,
        }
    }
}

impl Hits {
    /// Create a new HITS calculator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute HITS hub and authority scores.
    ///
    /// Returns (hub_scores, authority_scores).
    pub fn compute(&self, graph: &GraphDocument) -> (HashMap<String, f64>, HashMap<String, f64>) {
        let n = graph.nodes.len();
        if n == 0 {
            return (HashMap::new(), HashMap::new());
        }

        // Build directed adjacency (source → targets)
        let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();

        for node in &graph.nodes {
            outgoing.insert(&node.id, Vec::new());
            incoming.insert(&node.id, Vec::new());
        }

        for edge in &graph.edges {
            if let Some(targets) = outgoing.get_mut(edge.source.as_str()) {
                targets.push(&edge.target);
            }
            if let Some(sources) = incoming.get_mut(edge.target.as_str()) {
                sources.push(&edge.source);
            }
        }

        // Initialize scores uniformly
        let initial = 1.0 / (n as f64).sqrt();
        let mut hubs: HashMap<&str, f64> = graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), initial))
            .collect();
        let mut auths: HashMap<&str, f64> = graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), initial))
            .collect();

        // Iterate
        for _ in 0..self.max_iterations {
            let mut new_auths: HashMap<&str, f64> = HashMap::new();
            let mut new_hubs: HashMap<&str, f64> = HashMap::new();
            let mut max_diff = 0.0_f64;

            // Update authorities: auth(p) = sum of hub scores of nodes pointing to p
            for node in &graph.nodes {
                let node_id = node.id.as_str();
                let auth_score: f64 = incoming
                    .get(node_id)
                    .map(|sources| sources.iter().map(|s| hubs.get(s).unwrap_or(&0.0)).sum())
                    .unwrap_or(0.0);
                new_auths.insert(node_id, auth_score);
            }

            // Normalize authorities
            let auth_norm: f64 = new_auths
                .values()
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt()
                .max(1e-10);
            for v in new_auths.values_mut() {
                *v /= auth_norm;
            }

            // Update hubs: hub(p) = sum of authority scores of nodes p points to
            for node in &graph.nodes {
                let node_id = node.id.as_str();
                let hub_score: f64 = outgoing
                    .get(node_id)
                    .map(|targets| {
                        targets
                            .iter()
                            .map(|t| new_auths.get(t).unwrap_or(&0.0))
                            .sum()
                    })
                    .unwrap_or(0.0);
                new_hubs.insert(node_id, hub_score);
            }

            // Normalize hubs
            let hub_norm: f64 = new_hubs
                .values()
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt()
                .max(1e-10);
            for v in new_hubs.values_mut() {
                *v /= hub_norm;
            }

            // Check convergence
            for (k, &new_v) in &new_auths {
                let old_v = auths.get(k).unwrap_or(&0.0);
                max_diff = max_diff.max((new_v - old_v).abs());
            }

            auths = new_auths;
            hubs = new_hubs;

            if max_diff < self.epsilon {
                break;
            }
        }

        (
            hubs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            auths.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        )
    }

    /// Get top-k authorities.
    pub fn top_authorities(&self, graph: &GraphDocument, k: usize) -> Vec<(String, f64)> {
        let (_, auths) = self.compute(graph);
        let mut scores: Vec<_> = auths.into_iter().collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }

    /// Get top-k hubs.
    pub fn top_hubs(&self, graph: &GraphDocument, k: usize) -> Vec<(String, f64)> {
        let (hubs, _) = self.compute(graph);
        let mut scores: Vec<_> = hubs.into_iter().collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }
}

// =============================================================================
// Eigenvector Centrality
// =============================================================================

/// Eigenvector centrality: importance based on neighbor importance.
///
/// Similar to PageRank but simpler: node importance = sum of neighbor importances.
/// The principal eigenvector of the adjacency matrix.
///
/// # Comparison with PageRank
///
/// | Aspect | Eigenvector | PageRank |
/// |--------|-------------|----------|
/// | Damping | None | 0.85 typically |
/// | Sinks | Can diverge | Handled |
/// | Theory | Spectral | Random walk |
/// | Speed | Often faster | Slightly slower |
///
/// Use Eigenvector for simple, undirected graphs. Use PageRank for directed
/// graphs or when robustness to sinks matters.
///
/// # Example
///
/// ```rust,ignore
/// use anno_strata::centrality::Eigenvector;
/// let ev = Eigenvector::default();
/// let scores = ev.compute(&graph);
/// ```
#[derive(Debug, Clone)]
pub struct Eigenvector {
    /// Maximum iterations
    pub max_iterations: usize,
    /// Convergence threshold
    pub epsilon: f64,
}

impl Default for Eigenvector {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            epsilon: 1e-6,
        }
    }
}

impl Eigenvector {
    /// Create a new Eigenvector centrality calculator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum iterations.
    pub fn with_max_iterations(mut self, iterations: usize) -> Self {
        self.max_iterations = iterations;
        self
    }

    /// Compute eigenvector centrality using power iteration.
    ///
    /// Returns normalized scores (L2 norm = 1).
    pub fn compute(&self, graph: &GraphDocument) -> HashMap<String, f64> {
        let n = graph.nodes.len();
        if n == 0 {
            return HashMap::new();
        }

        // Build undirected adjacency
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in &graph.nodes {
            adjacency.insert(&node.id, Vec::new());
        }
        for edge in &graph.edges {
            if let Some(neighbors) = adjacency.get_mut(edge.source.as_str()) {
                if !neighbors.contains(&edge.target.as_str()) {
                    neighbors.push(&edge.target);
                }
            }
            if let Some(neighbors) = adjacency.get_mut(edge.target.as_str()) {
                if !neighbors.contains(&edge.source.as_str()) {
                    neighbors.push(&edge.source);
                }
            }
        }

        // Initialize uniformly
        let initial = 1.0 / (n as f64).sqrt();
        let mut scores: HashMap<&str, f64> = graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), initial))
            .collect();

        // Power iteration
        for _ in 0..self.max_iterations {
            let mut new_scores: HashMap<&str, f64> = HashMap::new();
            let mut max_diff = 0.0_f64;

            for node in &graph.nodes {
                let node_id = node.id.as_str();
                let score: f64 = adjacency
                    .get(node_id)
                    .map(|neighbors| {
                        neighbors
                            .iter()
                            .map(|n| scores.get(n).unwrap_or(&0.0))
                            .sum()
                    })
                    .unwrap_or(0.0);
                new_scores.insert(node_id, score);
            }

            // Normalize (L2)
            let norm: f64 = new_scores
                .values()
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt()
                .max(1e-10);
            for v in new_scores.values_mut() {
                *v /= norm;
            }

            // Check convergence
            for (k, &new_v) in &new_scores {
                let old_v = scores.get(k).unwrap_or(&0.0);
                max_diff = max_diff.max((new_v - old_v).abs());
            }

            scores = new_scores;

            if max_diff < self.epsilon {
                break;
            }
        }

        scores
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    /// Get top-k nodes by eigenvector centrality.
    pub fn top_k(&self, graph: &GraphDocument, k: usize) -> Vec<(String, f64)> {
        let scores = self.compute(graph);
        let mut sorted: Vec<_> = scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(k);
        sorted
    }
}

// =============================================================================
// Closeness Centrality
// =============================================================================

/// Closeness centrality: how quickly a node can reach all others.
///
/// Closeness = 1 / (sum of shortest path distances to all other nodes)
///
/// High closeness = "central" in terms of distance, can spread information
/// quickly to the entire network.
///
/// # Comparison with Other Centralities
///
/// | Metric | Closeness | Betweenness | PageRank |
/// |--------|-----------|-------------|----------|
/// | Measures | Distance to all | On shortest paths | Neighbor importance |
/// | Good for | Information spread | Bottlenecks | Influence |
///
/// # Example
///
/// ```rust,ignore
/// use anno_strata::centrality::Closeness;
/// let cl = Closeness::default();
/// let scores = cl.compute(&graph);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Closeness {
    /// Use harmonic mean (handles disconnected graphs)
    pub harmonic: bool,
}

impl Closeness {
    /// Create a new Closeness centrality calculator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use harmonic closeness (sum of 1/d instead of 1/sum(d)).
    ///
    /// This handles disconnected graphs gracefully.
    pub fn harmonic(mut self) -> Self {
        self.harmonic = true;
        self
    }

    /// Compute closeness centrality using BFS.
    ///
    /// O(V × E) for unweighted graphs.
    pub fn compute(&self, graph: &GraphDocument) -> HashMap<String, f64> {
        let n = graph.nodes.len();
        if n == 0 {
            return HashMap::new();
        }

        // Build undirected adjacency
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in &graph.nodes {
            adjacency.insert(&node.id, Vec::new());
        }
        for edge in &graph.edges {
            if let Some(neighbors) = adjacency.get_mut(edge.source.as_str()) {
                if !neighbors.contains(&edge.target.as_str()) {
                    neighbors.push(&edge.target);
                }
            }
            if let Some(neighbors) = adjacency.get_mut(edge.target.as_str()) {
                if !neighbors.contains(&edge.source.as_str()) {
                    neighbors.push(&edge.source);
                }
            }
        }

        let mut result: HashMap<String, f64> = HashMap::new();

        for source in &graph.nodes {
            let source_id = source.id.as_str();

            // BFS to compute distances
            let mut distances: HashMap<&str, usize> = HashMap::new();
            distances.insert(source_id, 0);

            let mut queue: std::collections::VecDeque<&str> = std::collections::VecDeque::new();
            queue.push_back(source_id);

            while let Some(current) = queue.pop_front() {
                let current_dist = *distances.get(current).unwrap_or(&0);

                if let Some(neighbors) = adjacency.get(current) {
                    for &neighbor in neighbors {
                        if !distances.contains_key(neighbor) {
                            distances.insert(neighbor, current_dist + 1);
                            queue.push_back(neighbor);
                        }
                    }
                }
            }

            // Calculate closeness
            let closeness = if self.harmonic {
                // Harmonic: sum of 1/d for all reachable nodes
                distances
                    .iter()
                    .filter(|(&k, _)| k != source_id)
                    .map(|(_, &d)| if d > 0 { 1.0 / d as f64 } else { 0.0 })
                    .sum::<f64>()
                    / (n - 1) as f64
            } else {
                // Classic: (n-1) / sum(d)
                let total_dist: usize = distances
                    .iter()
                    .filter(|(&k, _)| k != source_id)
                    .map(|(_, &d)| d)
                    .sum();

                if total_dist > 0 && distances.len() > 1 {
                    (distances.len() - 1) as f64 / total_dist as f64
                } else {
                    0.0
                }
            };

            result.insert(source.id.clone(), closeness);
        }

        result
    }

    /// Get top-k nodes by closeness centrality.
    pub fn top_k(&self, graph: &GraphDocument, k: usize) -> Vec<(String, f64)> {
        let scores = self.compute(graph);
        let mut sorted: Vec<_> = scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(k);
        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_core::{Entity, EntityType, Relation};

    fn chain_graph() -> GraphDocument {
        // A → B → C → D (linear chain)
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

    fn star_graph() -> GraphDocument {
        // Hub connected to many spokes
        let hub = Entity::new("Hub", EntityType::Person, 0, 3, 0.9);
        let s1 = Entity::new("S1", EntityType::Person, 10, 12, 0.9);
        let s2 = Entity::new("S2", EntityType::Person, 20, 22, 0.9);
        let s3 = Entity::new("S3", EntityType::Person, 30, 32, 0.9);
        let s4 = Entity::new("S4", EntityType::Person, 40, 42, 0.9);

        let relations = vec![
            Relation::new(hub.clone(), s1.clone(), "CONN", 0.9),
            Relation::new(hub.clone(), s2.clone(), "CONN", 0.9),
            Relation::new(hub.clone(), s3.clone(), "CONN", 0.9),
            Relation::new(hub.clone(), s4.clone(), "CONN", 0.9),
        ];

        GraphDocument::from_extraction(&[hub, s1, s2, s3, s4], &relations, None)
    }

    #[test]
    fn test_betweenness_chain() {
        let graph = chain_graph();
        let bc = Betweenness::new();
        let scores = bc.compute(&graph);

        // In a chain A-B-C-D, middle nodes (B, C) should have highest betweenness
        let b_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:b"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let c_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:c"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let a_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:a"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);

        // B and C are bridges, A is an endpoint
        assert!(
            b_score >= a_score,
            "B should have higher betweenness than A"
        );
        assert!(
            c_score >= a_score,
            "C should have higher betweenness than A"
        );
    }

    #[test]
    fn test_betweenness_star() {
        let graph = star_graph();
        let bc = Betweenness::new();
        let scores = bc.compute(&graph);

        // In a star, the hub should have highest betweenness
        let hub_score = scores
            .iter()
            .find(|(k, _)| k.contains("hub"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let spoke_scores: Vec<f64> = scores
            .iter()
            .filter(|(k, _)| k.contains("per:s"))
            .map(|(_, v)| *v)
            .collect();

        for spoke in &spoke_scores {
            assert!(hub_score >= *spoke, "Hub should have highest betweenness");
        }
    }

    #[test]
    fn test_hits_star() {
        let graph = star_graph();
        let hits = Hits::default();
        let (hubs, _auths) = hits.compute(&graph);

        // The hub node should have high hub score
        let hub_score = hubs
            .iter()
            .find(|(k, _)| k.contains("hub"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        assert!(hub_score > 0.0, "Hub should have positive hub score");
    }

    #[test]
    fn test_empty_graph() {
        let graph = GraphDocument::new();

        let bc = Betweenness::default();
        assert!(bc.compute(&graph).is_empty());

        let hits = Hits::default();
        let (h, a) = hits.compute(&graph);
        assert!(h.is_empty());
        assert!(a.is_empty());

        let ev = Eigenvector::default();
        assert!(ev.compute(&graph).is_empty());

        let cl = Closeness::default();
        assert!(cl.compute(&graph).is_empty());
    }

    #[test]
    fn test_eigenvector_star() {
        let graph = star_graph();
        let ev = Eigenvector::new();
        let scores = ev.compute(&graph);

        // Hub should have highest eigenvector centrality (connected to many)
        let hub_score = scores
            .iter()
            .find(|(k, _)| k.contains("hub"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let spoke_scores: Vec<f64> = scores
            .iter()
            .filter(|(k, _)| k.contains("per:s"))
            .map(|(_, v)| *v)
            .collect();

        for spoke in &spoke_scores {
            assert!(
                hub_score >= *spoke,
                "Hub should have highest eigenvector centrality"
            );
        }
    }

    #[test]
    fn test_eigenvector_chain() {
        let graph = chain_graph();
        let ev = Eigenvector::new();
        let scores = ev.compute(&graph);

        // In a chain, middle nodes should have higher eigenvector centrality
        // (connected to nodes with more connections)
        let b_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:b"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let a_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:a"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let d_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:d"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);

        // B is connected to both A and C, more central than endpoints
        assert!(b_score >= a_score, "B should have >= eigenvector than A");
        assert!(b_score >= d_score, "B should have >= eigenvector than D");
    }

    #[test]
    fn test_closeness_star() {
        let graph = star_graph();
        let cl = Closeness::new();
        let scores = cl.compute(&graph);

        // Hub should have highest closeness (distance 1 to all spokes)
        let hub_score = scores
            .iter()
            .find(|(k, _)| k.contains("hub"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let spoke_scores: Vec<f64> = scores
            .iter()
            .filter(|(k, _)| k.contains("per:s"))
            .map(|(_, v)| *v)
            .collect();

        for spoke in &spoke_scores {
            assert!(hub_score >= *spoke, "Hub should have highest closeness");
        }
    }

    #[test]
    fn test_closeness_chain() {
        let graph = chain_graph();
        let cl = Closeness::new();
        let scores = cl.compute(&graph);

        // In a chain A-B-C-D, middle nodes (B, C) should have highest closeness
        let b_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:b"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let c_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:c"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let a_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:a"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let d_score = scores
            .iter()
            .find(|(k, _)| k.contains("per:d"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);

        // Middle nodes should have higher closeness than endpoints
        assert!(b_score >= a_score, "B should have >= closeness than A");
        assert!(c_score >= d_score, "C should have >= closeness than D");
    }

    #[test]
    fn test_closeness_harmonic() {
        let graph = chain_graph();

        let cl_classic = Closeness::new();
        let cl_harmonic = Closeness::new().harmonic();

        let classic_scores = cl_classic.compute(&graph);
        let harmonic_scores = cl_harmonic.compute(&graph);

        // Both should have non-empty results
        assert!(!classic_scores.is_empty());
        assert!(!harmonic_scores.is_empty());

        // Both should give highest scores to middle nodes
        let classic_b = classic_scores
            .iter()
            .find(|(k, _)| k.contains("per:b"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let harmonic_b = harmonic_scores
            .iter()
            .find(|(k, _)| k.contains("per:b"))
            .map(|(_, v)| *v)
            .unwrap_or(0.0);

        assert!(classic_b > 0.0);
        assert!(harmonic_b > 0.0);
    }
}
