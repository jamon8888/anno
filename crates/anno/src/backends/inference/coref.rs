//! Coreference resolution utilities and data structures.

use std::collections::HashMap;
use crate::Entity;

// Coreference Resolution
// =============================================================================

/// A coreference cluster (mentions referring to same entity).
#[derive(Debug, Clone)]
pub struct CoreferenceCluster {
    /// Cluster ID
    pub id: u64,
    /// Member entities (indices into entity list)
    pub members: Vec<usize>,
    /// Representative entity index (most informative mention)
    pub representative: usize,
    /// Canonical name (from representative)
    pub canonical_name: String,
}

/// Configuration for coreference resolution.
#[derive(Debug, Clone)]
pub struct CoreferenceConfig {
    /// Minimum cosine similarity to link mentions
    pub similarity_threshold: f32,
    /// Maximum token distance between coreferent mentions
    pub max_distance: Option<usize>,
    /// Whether to use exact string matching as a signal
    pub use_string_match: bool,
}

impl Default for CoreferenceConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.85,
            max_distance: Some(500),
            use_string_match: true,
        }
    }
}

/// Resolve coreferences between entities using embedding similarity.
///
/// # Algorithm
///
/// 1. Compute pairwise cosine similarity between entity embeddings
/// 2. Link entities above threshold (with optional distance constraint)
/// 3. Build clusters via transitive closure
/// 4. Select representative (longest/most informative mention)
///
/// # Example
///
/// Input entities: ["Lynn Conway", "She", "The engineer", "Conway"]
/// Output clusters: [{0, 1, 2, 3}] with canonical_name = "Lynn Conway"
pub fn resolve_coreferences(
    entities: &[Entity],
    embeddings: &[f32], // [num_entities, hidden_dim]
    hidden_dim: usize,
    config: &CoreferenceConfig,
) -> Vec<CoreferenceCluster> {
    let n = entities.len();
    if n == 0 {
        return vec![];
    }

    // Union-find for clustering
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }

    fn union(parent: &mut [usize], i: usize, j: usize) {
        let pi = find(parent, i);
        let pj = find(parent, j);
        if pi != pj {
            parent[pi] = pj;
        }
    }

    // Check all pairs
    for i in 0..n {
        for j in (i + 1)..n {
            // String match check (fast path)
            if config.use_string_match {
                let text_i = entities[i].text.to_lowercase();
                let text_j = entities[j].text.to_lowercase();
                if text_i == text_j || text_i.contains(&text_j) || text_j.contains(&text_i) {
                    // Same entity type required
                    if entities[i].entity_type == entities[j].entity_type {
                        union(&mut parent, i, j);
                        continue;
                    }
                }
            }

            // Distance check
            if let Some(max_dist) = config.max_distance {
                let dist = if entities[i].end <= entities[j].start {
                    entities[j].start - entities[i].end
                } else {
                    entities[i].start.saturating_sub(entities[j].end)
                };
                if dist > max_dist {
                    continue;
                }
            }

            // Embedding similarity
            if embeddings.len() >= (j + 1) * hidden_dim {
                let emb_i = &embeddings[i * hidden_dim..(i + 1) * hidden_dim];
                let emb_j = &embeddings[j * hidden_dim..(j + 1) * hidden_dim];

                let similarity = cosine_similarity(emb_i, emb_j);

                if similarity >= config.similarity_threshold {
                    // Same entity type required
                    if entities[i].entity_type == entities[j].entity_type {
                        union(&mut parent, i, j);
                    }
                }
            }
        }
    }

    // Build clusters
    let mut cluster_members: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        cluster_members.entry(root).or_default().push(i);
    }

    // Convert to CoreferenceCluster
    let mut clusters = Vec::new();
    let mut cluster_id = 0u64;

    for (_root, members) in cluster_members {
        if members.len() > 1 {
            // Find representative (longest mention)
            let representative = *members
                .iter()
                .max_by_key(|&&i| entities[i].text.len())
                .unwrap_or(&members[0]);

            clusters.push(CoreferenceCluster {
                id: cluster_id,
                members,
                representative,
                canonical_name: entities[representative].text.clone(),
            });
            cluster_id += 1;
        }
    }

    clusters
}

/// Compute cosine similarity between two vectors.
///
/// Returns a value in [-1.0, 1.0] where:
/// - 1.0 = identical direction
/// - 0.0 = orthogonal
/// - -1.0 = opposite direction
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a > 0.0 && norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}





