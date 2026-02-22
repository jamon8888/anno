//! Inference abstractions shared across `anno` backends.
//!
//! This module is mostly **plumbing**: common traits, data shapes, and small
//! utilities used by multiple NER / IE backends (including “fixed-label” and
//! “open/zero-shot” styles).
//!
//! Some of the terminology and design choices correspond to well-known
//! architectures in the NER/IE literature, but the code here should be treated
//! as an implementation substrate, not a verbatim reproduction of any single
//! paper’s experiment section.
//!
//! ## Paper pointers (context only)
//!
//! - GLiNER: arXiv:2311.08526
//! - UniversalNER: arXiv:2308.03279
//! - W2NER: arXiv:2112.10070
//! - ModernBERT: arXiv:2412.13663

use std::borrow::Cow;

// =============================================================================
// Modality Types
// =============================================================================

/// Input modality for the encoder.
///
/// Supports text, images, and hybrid (OCR + visual) inputs.
/// This enables ColPali-style visual document understanding.
#[derive(Debug, Clone)]
pub enum ModalityInput<'a> {
    /// Plain text input
    Text(Cow<'a, str>),
    /// Image bytes (PNG/JPEG)
    Image {
        /// Raw image bytes
        data: Cow<'a, [u8]>,
        /// Image format hint
        format: ImageFormat,
    },
    /// Hybrid: text with visual location (e.g., OCR result)
    Hybrid {
        /// Extracted text
        text: Cow<'a, str>,
        /// Visual bounding boxes for each token/word
        visual_positions: Vec<VisualPosition>,
    },
}

/// Image format hint for decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageFormat {
    /// PNG format
    #[default]
    Png,
    /// JPEG format
    Jpeg,
    /// WebP format
    Webp,
    /// Unknown/auto-detect
    Unknown,
}

/// Visual position of a text token in an image.
#[derive(Debug, Clone, Copy)]
pub struct VisualPosition {
    /// Token/word index
    pub token_idx: u32,
    /// Normalized x coordinate (0.0-1.0)
    pub x: f32,
    /// Normalized y coordinate (0.0-1.0)
    pub y: f32,
    /// Normalized width (0.0-1.0)
    pub width: f32,
    /// Normalized height (0.0-1.0)
    pub height: f32,
    /// Page number (for multi-page documents)
    pub page: u32,
}

// =============================================================================

pub mod registry;
pub use registry::*;

pub mod encoder;
pub use encoder::*;

pub mod traits;
pub use traits::*;

pub mod late_interaction;
pub use late_interaction::*;

pub mod span;
pub use span::*;

pub mod coref;
pub use coref::*;

pub mod relation_extraction;
pub use relation_extraction::{
    extract_relation_triples, extract_relations, RelationExtractionConfig,
};

pub mod binary_embeddings;
pub use binary_embeddings::*;
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::coref::{resolve_coreferences, CoreferenceConfig};
    use super::late_interaction::{DotProductInteraction, MaxSimInteraction};
    use super::*;
    use crate::{Entity, EntityType};

    #[test]
    fn test_semantic_registry_builder() {
        let registry = SemanticRegistry::builder()
            .add_entity("person", "A human being")
            .add_entity("organization", "A company or group")
            .add_relation("WORKS_FOR", "Employment relationship")
            .build_placeholder(768);

        assert_eq!(registry.len(), 3);
        assert_eq!(registry.entity_labels().count(), 2);
        assert_eq!(registry.relation_labels().count(), 1);
    }

    #[test]
    fn test_standard_ner_registry() {
        let registry = SemanticRegistry::standard_ner(768);
        assert!(registry.len() >= 5);
        assert!(registry.label_index.contains_key("person"));
        assert!(registry.label_index.contains_key("organization"));
    }

    #[test]
    fn test_dot_product_interaction() {
        let interaction = DotProductInteraction::new();

        // 2 spans, 3 labels, hidden_dim=4
        let span_embs = vec![
            1.0, 0.0, 0.0, 0.0, // span 0
            0.0, 1.0, 0.0, 0.0, // span 1
        ];
        let label_embs = vec![
            1.0, 0.0, 0.0, 0.0, // label 0 (matches span 0)
            0.0, 1.0, 0.0, 0.0, // label 1 (matches span 1)
            0.5, 0.5, 0.0, 0.0, // label 2 (partial match both)
        ];

        let scores = interaction.compute_similarity(&span_embs, 2, &label_embs, 3, 4);

        assert_eq!(scores.len(), 6); // 2 * 3
        assert!((scores[0] - 1.0).abs() < 0.01); // span0 vs label0
        assert!((scores[4] - 1.0).abs() < 0.01); // span1 vs label1
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 0.001);

        let d = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &d) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_coreference_string_match() {
        let entities = vec![
            Entity::new("Marie Curie", EntityType::Person, 0, 11, 0.95),
            Entity::new("Curie", EntityType::Person, 50, 55, 0.90),
        ];

        let embeddings = vec![0.0f32; 2 * 768]; // Placeholder
        let clusters =
            resolve_coreferences(&entities, &embeddings, 768, &CoreferenceConfig::default());

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members.len(), 2);
        assert_eq!(clusters[0].canonical_name, "Marie Curie");
    }

    #[test]
    fn test_handshaking_matrix() {
        // 3 tokens, 2 labels, threshold 0.5
        let scores = vec![
            // token 0 with tokens 0,1,2 for labels 0,1
            0.9, 0.1, // (0,0)
            0.2, 0.8, // (0,1)
            0.1, 0.1, // (0,2)
            // token 1 with tokens 0,1,2
            0.0, 0.0, // (1,0) - skipped (lower triangle)
            0.7, 0.2, // (1,1)
            0.3, 0.6, // (1,2)
            // token 2
            0.0, 0.0, // (2,0)
            0.0, 0.0, // (2,1)
            0.1, 0.1, // (2,2)
        ];

        let matrix = HandshakingMatrix::from_dense(&scores, 3, 2, 0.5);

        // Should have cells for scores >= 0.5
        assert!(matrix.cells.len() >= 4);
    }

    #[test]
    fn test_relation_extraction() {
        let entities = vec![
            Entity::new("Steve Jobs", EntityType::Person, 0, 10, 0.95),
            Entity::new("Apple", EntityType::Organization, 20, 25, 0.90),
        ];

        let text = "Steve Jobs founded Apple Inc in 1976";

        let registry = SemanticRegistry::builder()
            .add_relation("FOUNDED", "Founded an organization")
            .build_placeholder(768);

        let config = RelationExtractionConfig::default();
        let relations = extract_relations(&entities, text, &registry, &config);

        assert!(!relations.is_empty());
        assert_eq!(relations[0].relation_type, "FOUNDED");
    }

    #[test]
    fn test_relation_extraction_uses_character_offsets_with_unicode_prefix() {
        // Unicode prefix ensures byte offsets != character offsets.
        let text = "👋 Steve Jobs founded Apple Inc.";

        // Compute character offsets explicitly (Entity spans are char-based).
        let steve_start = text.find("Steve Jobs").expect("substring present");
        // `find` returns byte offset; convert to char offset.
        let conv = crate::offset::SpanConverter::new(text);
        let steve_start_char = conv.byte_to_char(steve_start);
        let steve_end_char = steve_start_char + "Steve Jobs".chars().count();

        let apple_start = text.find("Apple").expect("substring present");
        let apple_start_char = conv.byte_to_char(apple_start);
        let apple_end_char = apple_start_char + "Apple".chars().count();

        let entities = vec![
            Entity::new(
                "Steve Jobs",
                EntityType::Person,
                steve_start_char,
                steve_end_char,
                0.95,
            ),
            Entity::new(
                "Apple",
                EntityType::Organization,
                apple_start_char,
                apple_end_char,
                0.90,
            ),
        ];

        let registry = SemanticRegistry::builder()
            .add_relation("FOUNDED", "Founded an organization")
            .build_placeholder(768);

        let config = RelationExtractionConfig::default();
        let relations = extract_relations(&entities, text, &registry, &config);

        assert!(
            !relations.is_empty(),
            "Expected FOUNDED relation to be detected"
        );
        assert_eq!(relations[0].relation_type, "FOUNDED");

        // Trigger span should exist and cover "founded" in character offsets.
        let trigger = relations[0]
            .trigger_span
            .expect("expected trigger_span to be present");
        let trigger_text: String = text
            .chars()
            .skip(trigger.0)
            .take(trigger.1.saturating_sub(trigger.0))
            .collect();
        assert_eq!(trigger_text.to_ascii_lowercase(), "founded");
    }

    // =========================================================================
    // Binary Embedding Tests
    // =========================================================================

    #[test]
    fn test_binary_hash_creation() {
        let embedding = vec![0.1, -0.2, 0.3, -0.4, 0.5, -0.6, 0.7, -0.8];
        let hash = BinaryHash::from_embedding(&embedding);

        assert_eq!(hash.dim, 8);
        // Positive values at indices 0, 2, 4, 6 should be set
        // bits[0] should have bits 0, 2, 4, 6 set = 0b01010101 = 85
        assert_eq!(hash.bits[0], 85);
    }

    #[test]
    fn test_hamming_distance_identical() {
        let embedding = vec![0.1; 64];
        let hash1 = BinaryHash::from_embedding(&embedding);
        let hash2 = BinaryHash::from_embedding(&embedding);

        assert_eq!(hash1.hamming_distance(&hash2), 0);
    }

    #[test]
    fn test_hamming_distance_opposite() {
        let embedding1 = vec![0.1; 64];
        let embedding2 = vec![-0.1; 64];
        let hash1 = BinaryHash::from_embedding(&embedding1);
        let hash2 = BinaryHash::from_embedding(&embedding2);

        assert_eq!(hash1.hamming_distance(&hash2), 64);
    }

    #[test]
    fn test_hamming_distance_half() {
        let embedding1 = vec![0.1; 64];
        let mut embedding2 = vec![0.1; 64];
        // Flip second half
        embedding2[32..64].iter_mut().for_each(|x| *x = -0.1);

        let hash1 = BinaryHash::from_embedding(&embedding1);
        let hash2 = BinaryHash::from_embedding(&embedding2);

        assert_eq!(hash1.hamming_distance(&hash2), 32);
    }

    #[test]
    fn test_binary_blocker() {
        let mut blocker = BinaryBlocker::new(5);

        // Add some hashes
        let base_embedding = vec![0.1; 64];
        let similar_embedding = {
            let mut e = vec![0.1; 64];
            e[0] = -0.1; // Flip 1 bit
            e[1] = -0.1; // Flip 2 bits
            e
        };
        let different_embedding = vec![-0.1; 64];

        blocker.add(0, BinaryHash::from_embedding(&base_embedding));
        blocker.add(1, BinaryHash::from_embedding(&similar_embedding));
        blocker.add(2, BinaryHash::from_embedding(&different_embedding));

        // Query with base
        let query = BinaryHash::from_embedding(&base_embedding);
        let candidates = blocker.query(&query);

        assert!(candidates.contains(&0), "Should find exact match");
        assert!(
            candidates.contains(&1),
            "Should find similar (2 bits different)"
        );
        assert!(
            !candidates.contains(&2),
            "Should NOT find opposite (64 bits different)"
        );
    }

    #[test]
    fn test_two_stage_retrieval() {
        // Create embeddings
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let candidates = vec![
            vec![1.0, 0.0, 0.0, 0.0],  // Identical
            vec![0.9, 0.1, 0.0, 0.0],  // Similar
            vec![-1.0, 0.0, 0.0, 0.0], // Opposite
            vec![0.0, 1.0, 0.0, 0.0],  // Orthogonal
        ];

        // Generous threshold to get candidates
        let results = two_stage_retrieval(&query, &candidates, 4, 2);

        assert!(!results.is_empty());
        // First result should be exact match
        assert_eq!(results[0].0, 0);
        assert!((results[0].1 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_approximate_cosine() {
        let embedding1 = vec![0.1; 768];
        let embedding2 = vec![0.1; 768];
        let hash1 = BinaryHash::from_embedding(&embedding1);
        let hash2 = BinaryHash::from_embedding(&embedding2);

        // Identical → approximate cosine should be ~1.0
        let approx = hash1.approximate_cosine(&hash2);
        assert!((approx - 1.0).abs() < 0.001);

        // Opposite → approximate cosine should be ~-1.0
        let embedding3 = vec![-0.1; 768];
        let hash3 = BinaryHash::from_embedding(&embedding3);
        let approx_opp = hash1.approximate_cosine(&hash3);
        assert!((approx_opp - (-1.0)).abs() < 0.001);
    }
}
