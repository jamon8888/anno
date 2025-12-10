//! Inter-document entity coalescing.

use anno_core::{Corpus, Identity, IdentityId, IdentitySource, TrackId, TrackRef};
use std::collections::HashMap;

use crate::alignment::{entity_type_nameability, AdaptiveResolutionConfig, AlignmentScore};

/// Coalescer for inter-document entity resolution.
#[derive(Debug, Clone)]
pub struct Resolver {
    similarity_threshold: f32,
    require_type_match: bool,
    /// Optional adaptive resolution config for dynamic thresholds
    adaptive_config: Option<AdaptiveResolutionConfig>,
}

impl Resolver {
    /// Create a new resolver with default settings.
    pub fn new() -> Self {
        Self {
            similarity_threshold: 0.7,
            require_type_match: true,
            adaptive_config: None,
        }
    }

    /// Create a new resolver with custom settings.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    /// Set whether to require entity type match for clustering.
    pub fn require_type_match(mut self, require: bool) -> Self {
        self.require_type_match = require;
        self
    }

    /// Enable adaptive thresholds based on conceptual alignment.
    ///
    /// When enabled, the resolver adjusts similarity thresholds dynamically:
    /// - High-nameability types (PERSON, LOCATION) get lower thresholds
    /// - Well-evidenced clusters (many matches) get lower thresholds
    /// - Distance-based decay follows Shepard's Universal Law
    ///
    /// # Research Background
    ///
    /// Based on "Ad hoc conventions generalize to new referents" (Ji et al., 2025):
    /// referential conventions reflect conceptual alignment that generalizes to
    /// similar entities, not just arbitrary labels for specific referents.
    ///
    /// # Example
    ///
    /// ```rust
    /// use anno_coalesce::{Resolver, AdaptiveResolutionConfig};
    ///
    /// let resolver = Resolver::new()
    ///     .with_adaptive(AdaptiveResolutionConfig::default());
    /// ```
    pub fn with_adaptive(mut self, config: AdaptiveResolutionConfig) -> Self {
        self.adaptive_config = Some(config);
        self
    }

    /// Compute effective threshold for a pair comparison.
    ///
    /// If adaptive config is set, adjusts threshold based on:
    /// - Entity type nameability (prior consensus)
    /// - Cluster alignment score (accumulated evidence)
    /// - Current similarity (distance-based decay)
    fn effective_threshold(
        &self,
        base_threshold: f32,
        entity_type: Option<&str>,
        alignment: &AlignmentScore,
        similarity: f32,
    ) -> f32 {
        match &self.adaptive_config {
            Some(config) => {
                // Use adaptive config but override base threshold
                let mut adapted_config = config.clone();
                adapted_config.base_threshold = base_threshold;
                let nameability = entity_type.map(entity_type_nameability);
                adapted_config.compute_threshold(alignment, similarity, nameability)
            }
            None => base_threshold,
        }
    }

    /// Coalesce inter-document entities across all documents in a corpus.
    ///
    /// This method clusters tracks from different documents that refer to the same
    /// real-world entity, creating `Identity` instances without KB links.
    ///
    /// # Algorithm
    ///
    /// 1. Extract all tracks from all documents
    /// 2. Compute track embeddings (if available) or use string similarity
    /// 3. Cluster tracks using similarity threshold
    /// 4. Create Identity for each cluster
    /// 5. Link tracks to identities
    ///
    /// # Parameters
    ///
    /// * `corpus` - The corpus containing documents to resolve
    /// * `similarity_threshold` - Minimum similarity (0.0-1.0) to cluster tracks
    /// * `require_type_match` - Only cluster tracks with same entity type
    ///
    /// # Returns
    ///
    /// Vector of created identities, each linked to tracks from multiple documents.
    pub fn resolve_inter_doc_coref(
        &self,
        corpus: &mut Corpus,
        similarity_threshold: Option<f32>,
        require_type_match: Option<bool>,
    ) -> Vec<IdentityId> {
        let threshold = similarity_threshold.unwrap_or(self.similarity_threshold);
        let type_match = require_type_match.unwrap_or(self.require_type_match);

        // 1. Collect all track data (clone what we need to avoid borrow conflicts)
        #[derive(Debug, Clone)]
        struct TrackData {
            track_ref: TrackRef,
            canonical_surface: String,
            entity_type: Option<String>,
            cluster_confidence: f32,
            embedding: Option<Vec<f32>>,
        }

        let mut track_data: Vec<TrackData> = Vec::new();
        // Collect document IDs first to avoid borrow checker issues
        let doc_ids: Vec<String> = corpus.documents().map(|d| d.id.clone()).collect();
        for doc_id in doc_ids {
            if let Some(doc) = corpus.get_document(&doc_id) {
                for track in doc.tracks() {
                    if let Some(track_ref) = doc.track_ref(track.id) {
                        track_data.push(TrackData {
                            track_ref,
                            canonical_surface: track.canonical_surface.clone(),
                            entity_type: track.entity_type.clone(),
                            cluster_confidence: track.cluster_confidence,
                            embedding: track.embedding.clone(),
                        });
                    }
                }
            }
        }

        if track_data.is_empty() {
            return vec![];
        }

        // 2. Cluster tracks using string similarity or embeddings
        // Uses embeddings if available (from track.embedding), otherwise falls back to string similarity
        let mut union_find: Vec<usize> = (0..track_data.len()).collect();

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

        // Track alignment scores per cluster root (for adaptive thresholds)
        let mut cluster_alignments: HashMap<usize, AlignmentScore> = HashMap::new();

        // Compare all pairs
        for i in 0..track_data.len() {
            for j in (i + 1)..track_data.len() {
                let track_a = &track_data[i];
                let track_b = &track_data[j];

                // Type check
                if type_match && track_a.entity_type != track_b.entity_type {
                    continue;
                }

                // Compute similarity: prefer embeddings if BOTH available, fallback to string similarity
                // Edge case: If only one track has an embedding, we can't compare embeddings directly,
                // so we fall back to string similarity for consistency.
                let similarity =
                    if let (Some(emb_a), Some(emb_b)) = (&track_a.embedding, &track_b.embedding) {
                        // Both have embeddings: use cosine similarity
                        embedding_similarity(emb_a, emb_b)
                    } else {
                        // One or both missing embeddings: fallback to string similarity
                        // This handles: (Some, None), (None, Some), (None, None)
                        string_similarity(&track_a.canonical_surface, &track_b.canonical_surface)
                    };

                // Compute effective threshold (adaptive or fixed)
                let root_i = find(&mut union_find, i);
                let alignment = cluster_alignments
                    .entry(root_i)
                    .or_insert_with(AlignmentScore::new);
                let effective_thresh = self.effective_threshold(
                    threshold,
                    track_a.entity_type.as_deref(),
                    alignment,
                    similarity,
                );

                if similarity >= effective_thresh {
                    union(&mut union_find, i, j);
                    // Record the match for adaptive threshold tracking
                    alignment.record_match(similarity);
                }
            }
        }

        // 3. Build clusters
        let mut cluster_map: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..track_data.len() {
            let root = find(&mut union_find, i);
            cluster_map.entry(root).or_default().push(i);
        }

        // 4. Create identities for each cluster
        // Note: Singleton clusters (clusters with only one track) still create identities.
        // This allows tracking entities that appear only once across documents.
        let mut created_ids = Vec::new();
        for (_, member_indices) in cluster_map.iter() {
            if member_indices.is_empty() {
                continue;
            }

            // Safe: we just checked is_empty() above, so member_indices[0] is valid
            let first_idx = member_indices[0];
            let first_track = &track_data[first_idx];

            // Collect all track refs in this cluster
            let track_refs_in_cluster: Vec<TrackRef> = member_indices
                .iter()
                .map(|&idx| track_data[idx].track_ref.clone())
                .collect();

            // Create identity
            let identity = Identity {
                id: corpus.next_identity_id(), // Will be set by add_identity
                canonical_name: first_track.canonical_surface.clone(),
                entity_type: first_track.entity_type.clone(),
                kb_id: None,
                kb_name: None,
                description: None,
                embedding: first_track.embedding.clone(),
                box_embedding: None,
                aliases: Vec::new(),
                confidence: first_track.cluster_confidence,
                source: Some(IdentitySource::CrossDocCoref {
                    track_refs: track_refs_in_cluster,
                }),
            };

            let identity_id = corpus.add_identity(identity);
            created_ids.push(identity_id);

            // 5. Link tracks to identity
            // Collect doc_id and track_id pairs first to avoid borrow conflicts
            let links: Vec<(String, TrackId)> = member_indices
                .iter()
                .map(|&idx| {
                    let track_ref = &track_data[idx].track_ref;
                    (track_ref.doc_id.clone(), track_ref.track_id)
                })
                .collect();

            for (doc_id, track_id) in links {
                if let Some(doc) = corpus.get_document_mut(&doc_id) {
                    doc.link_track_to_identity(track_id, identity_id);
                } else {
                    // Document was removed or doesn't exist - this is a data consistency issue
                    // Log warning but continue with other tracks
                    log::warn!(
                        "Document '{}' not found when linking track {} to identity {}",
                        doc_id,
                        track_id,
                        identity_id
                    );
                }
            }
        }

        created_ids
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

// String similarity is now provided by the similarity module.
// Re-export for backward compatibility.
pub use crate::similarity::string_similarity;

/// Compute embedding similarity using cosine similarity.
///
/// Returns a value in [0.0, 1.0] where 1.0 is identical.
///
/// Formula: `cosine(a, b) = (a · b) / (||a|| × ||b||)`, normalized to [0, 1].
/// Measures angle between vectors, not magnitude, making it suitable for embeddings.
///
/// # Example
///
/// ```
/// use anno_coalesce::embedding_similarity;
///
/// let emb1 = vec![1.0, 0.0, 0.0];
/// let emb2 = vec![1.0, 0.0, 0.0];
/// let sim = embedding_similarity(&emb1, &emb2);
/// assert_eq!(sim, 1.0);
/// ```
pub fn embedding_similarity(emb_a: &[f32], emb_b: &[f32]) -> f32 {
    if emb_a.len() != emb_b.len() || emb_a.is_empty() {
        return 0.0;
    }

    // Cosine similarity
    let dot_product: f32 = emb_a.iter().zip(emb_b.iter()).map(|(a, b)| a * b).sum();
    let norm_a: f32 = emb_a.iter().map(|a| a * a).sum::<f32>().sqrt();
    let norm_b: f32 = emb_b.iter().map(|b| b * b).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    // Normalize to [0, 1] range (cosine similarity is [-1, 1])
    (dot_product / (norm_a * norm_b) + 1.0) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AdaptiveResolutionConfig;

    #[test]
    fn test_resolver_default() {
        let resolver = Resolver::new();
        assert!((resolver.similarity_threshold - 0.7).abs() < 0.001);
        assert!(resolver.require_type_match);
        assert!(resolver.adaptive_config.is_none());
    }

    #[test]
    fn test_resolver_with_adaptive() {
        let config = AdaptiveResolutionConfig::default();
        let resolver = Resolver::new().with_adaptive(config.clone());

        assert!(resolver.adaptive_config.is_some());
        let adaptive = resolver.adaptive_config.unwrap();
        assert!((adaptive.base_threshold - config.base_threshold).abs() < 0.001);
    }

    #[test]
    fn test_effective_threshold_no_adaptive() {
        let resolver = Resolver::new();
        let alignment = AlignmentScore::new();

        // Without adaptive config, threshold should be the base
        let threshold = resolver.effective_threshold(0.8, Some("PERSON"), &alignment, 0.9);
        assert!((threshold - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_effective_threshold_with_adaptive() {
        let config = AdaptiveResolutionConfig::default();
        let resolver = Resolver::new().with_adaptive(config);

        // With some alignment evidence
        let mut alignment = AlignmentScore::new();
        alignment.record_match(0.85);
        alignment.record_match(0.90);

        // High nameability type (PERSON) with evidence should lower threshold
        let threshold = resolver.effective_threshold(0.7, Some("PERSON"), &alignment, 0.8);

        // Should be lower than base threshold (0.7)
        assert!(threshold <= 0.7, "Threshold should be reduced: {}", threshold);
    }

    #[test]
    fn test_effective_threshold_nameability_effect() {
        let config = AdaptiveResolutionConfig::default();
        let resolver = Resolver::new().with_adaptive(config);
        let alignment = AlignmentScore::new();

        let person_thresh = resolver.effective_threshold(0.7, Some("PERSON"), &alignment, 0.8);
        let misc_thresh = resolver.effective_threshold(0.7, Some("MISC"), &alignment, 0.8);

        // PERSON has higher nameability → should have lower threshold
        assert!(
            person_thresh <= misc_thresh,
            "PERSON threshold ({}) should be <= MISC threshold ({})",
            person_thresh,
            misc_thresh
        );
    }

    #[test]
    fn test_string_similarity() {
        // Identical strings
        assert_eq!(string_similarity("John Smith", "John Smith"), 1.0);

        // Partial overlap
        let sim = string_similarity("John Smith", "John");
        assert!(sim > 0.0 && sim < 1.0);

        // No overlap
        assert_eq!(string_similarity("John", "Mary"), 0.0);

        // Empty strings
        assert_eq!(string_similarity("", ""), 1.0);
        assert_eq!(string_similarity("John", ""), 0.0);
    }

    #[test]
    fn test_embedding_similarity() {
        // Identical embeddings
        let emb = vec![1.0, 0.0, 0.0];
        assert!((embedding_similarity(&emb, &emb) - 1.0).abs() < 0.001);

        // Orthogonal embeddings
        let emb1 = vec![1.0, 0.0, 0.0];
        let emb2 = vec![0.0, 1.0, 0.0];
        assert!((embedding_similarity(&emb1, &emb2) - 0.5).abs() < 0.001);

        // Opposite embeddings
        let emb1 = vec![1.0, 0.0, 0.0];
        let emb2 = vec![-1.0, 0.0, 0.0];
        assert!((embedding_similarity(&emb1, &emb2) - 0.0).abs() < 0.001);

        // Empty/mismatched embeddings
        assert_eq!(embedding_similarity(&[], &[]), 0.0);
        assert_eq!(embedding_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }
}
