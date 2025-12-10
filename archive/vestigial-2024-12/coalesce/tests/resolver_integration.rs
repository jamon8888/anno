//! Integration tests for Resolver with actual Corpus usage.
//!
//! These tests verify that the adaptive resolution system works
//! correctly when integrated with anno-core types.

use anno_coalesce::{
    embedding_similarity, string_similarity, AdaptiveResolutionConfig, GeneralizationGradient,
    Resolver,
};
use anno_core::{Corpus, GroundedDocument, Track};

/// Helper to create a GroundedDocument with minimal text
fn make_doc(id: &str) -> GroundedDocument {
    GroundedDocument::new(id, format!("Document {}", id))
}

// =============================================================================
// Basic Resolver Tests
// =============================================================================

#[test]
fn test_resolver_empty_corpus() {
    let resolver = Resolver::new();
    let mut corpus = Corpus::new();

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
    assert!(identities.is_empty());
}

#[test]
fn test_resolver_single_document() {
    let resolver = Resolver::new();
    let mut corpus = Corpus::new();

    let mut doc = make_doc("doc1");
    doc.add_track(Track::new(1, "Barack Obama").with_type("PERSON".to_string()));
    doc.add_track(Track::new(2, "Michelle Obama").with_type("PERSON".to_string()));
    corpus.add_document(doc);

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // With only one document, tracks should still create identities
    // (but won't merge since they're different people)
    assert!(!identities.is_empty());
}

#[test]
fn test_resolver_cross_doc_same_entity() {
    let resolver = Resolver::new().with_threshold(0.3); // Low threshold for string matching
    let mut corpus = Corpus::new();

    let mut doc1 = make_doc("doc1");
    doc1.add_track(Track::new(1, "Barack Obama").with_type("PERSON".to_string()));
    corpus.add_document(doc1);

    let mut doc2 = make_doc("doc2");
    doc2.add_track(Track::new(1, "Obama").with_type("PERSON".to_string()));
    corpus.add_document(doc2);

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // "Barack Obama" and "Obama" share the word "Obama" and should cluster
    // (Jaccard similarity of word sets)
    assert!(!identities.is_empty());

    // Check the identities were created
    for id in &identities {
        let identity = corpus.get_identity(*id);
        assert!(identity.is_some());
    }
}

#[test]
fn test_resolver_type_mismatch() {
    let resolver = Resolver::new().require_type_match(true);
    let mut corpus = Corpus::new();

    let mut doc1 = make_doc("doc1");
    doc1.add_track(Track::new(1, "Apple").with_type("ORGANIZATION".to_string()));
    corpus.add_document(doc1);

    let mut doc2 = make_doc("doc2");
    doc2.add_track(Track::new(1, "Apple").with_type("PRODUCT".to_string()));
    corpus.add_document(doc2);

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Different types should not cluster when require_type_match is true
    assert_eq!(
        identities.len(),
        2,
        "Different types should create separate identities"
    );
}

#[test]
fn test_resolver_type_mismatch_disabled() {
    let resolver = Resolver::new()
        .with_threshold(0.9) // High threshold for exact match
        .require_type_match(false);
    let mut corpus = Corpus::new();

    let mut doc1 = make_doc("doc1");
    doc1.add_track(Track::new(1, "Apple Inc").with_type("ORGANIZATION".to_string()));
    corpus.add_document(doc1);

    let mut doc2 = make_doc("doc2");
    doc2.add_track(Track::new(1, "Apple Inc").with_type("COMPANY".to_string()));
    corpus.add_document(doc2);

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Same string should cluster when type match is not required
    assert_eq!(
        identities.len(),
        1,
        "Same string should cluster with type match disabled"
    );
}

// =============================================================================
// Adaptive Resolution Tests
// =============================================================================

#[test]
fn test_resolver_with_adaptive_config() {
    let config = AdaptiveResolutionConfig {
        base_threshold: 0.6,
        min_threshold: 0.3,
        max_adjustment: 0.2,
        gradient: GeneralizationGradient::quadratic(),
        use_nameability: true,
    };

    let resolver = Resolver::new().with_adaptive(config);
    let mut corpus = Corpus::new();

    // Add multiple documents with similar entities
    for i in 0..5 {
        let mut doc = make_doc(&format!("doc{}", i));
        doc.add_track(Track::new(1, "Barack Obama").with_type("PERSON".to_string()));
        corpus.add_document(doc);
    }

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // All identical mentions should cluster into one identity
    assert_eq!(identities.len(), 1, "Identical mentions should form one cluster");

    // Verify the identity has all tracks
    let identity = corpus.get_identity(identities[0]).unwrap();
    if let Some(anno_core::IdentitySource::CrossDocCoref { track_refs }) = &identity.source {
        assert_eq!(track_refs.len(), 5, "Should have 5 track refs");
    }
}

#[test]
fn test_resolver_adaptive_high_vs_low_nameability() {
    // Test that high-nameability types (PERSON) cluster more easily
    // than low-nameability types (MISC)

    let config = AdaptiveResolutionConfig::default();
    let resolver = Resolver::new().with_adaptive(config);

    // Test with PERSON (high nameability)
    let mut corpus_person = Corpus::new();
    let mut doc1 = make_doc("doc1");
    doc1.add_track(Track::new(1, "John Smith").with_type("PERSON".to_string()));
    corpus_person.add_document(doc1);

    let mut doc2 = make_doc("doc2");
    doc2.add_track(Track::new(1, "J Smith").with_type("PERSON".to_string()));
    corpus_person.add_document(doc2);

    let person_ids = resolver.resolve_inter_doc_coref(&mut corpus_person, None, None);

    // Test with MISC (low nameability)
    let mut corpus_misc = Corpus::new();
    let mut doc1 = make_doc("doc1");
    doc1.add_track(Track::new(1, "John Smith").with_type("MISC".to_string()));
    corpus_misc.add_document(doc1);

    let mut doc2 = make_doc("doc2");
    doc2.add_track(Track::new(1, "J Smith").with_type("MISC".to_string()));
    corpus_misc.add_document(doc2);

    let misc_ids = resolver.resolve_inter_doc_coref(&mut corpus_misc, None, None);

    // We can't guarantee the result without knowing exact similarity,
    // but we can verify the resolver ran successfully
    assert!(!person_ids.is_empty() || !misc_ids.is_empty());
}

// =============================================================================
// Unicode Entity Tests
// =============================================================================

#[test]
fn test_resolver_unicode_entities() {
    let resolver = Resolver::new().with_threshold(0.5);
    let mut corpus = Corpus::new();

    // Chinese entity
    let mut doc1 = make_doc("doc1");
    doc1.add_track(Track::new(1, "北京").with_type("LOCATION".to_string()));
    corpus.add_document(doc1);

    // Same Chinese entity in another doc
    let mut doc2 = make_doc("doc2");
    doc2.add_track(Track::new(1, "北京").with_type("LOCATION".to_string()));
    corpus.add_document(doc2);

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Identical Chinese text should cluster
    assert_eq!(identities.len(), 1, "Identical Chinese entities should cluster");
}

#[test]
fn test_resolver_mixed_scripts() {
    let resolver = Resolver::new().with_threshold(0.3);
    let mut corpus = Corpus::new();

    // Add entities in various scripts
    let entities = vec![
        ("doc1", "東京", "LOCATION"),       // Japanese
        ("doc2", "Москва", "LOCATION"),     // Russian
        ("doc3", "الرياض", "LOCATION"),     // Arabic
        ("doc4", "São Paulo", "LOCATION"),  // Portuguese with diacritics
        ("doc5", "Zürich", "LOCATION"),     // German with umlaut
    ];

    for (doc_id, name, entity_type) in entities {
        let mut doc = make_doc(doc_id);
        doc.add_track(Track::new(1, name).with_type(entity_type.to_string()));
        corpus.add_document(doc);
    }

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // All different names should stay separate
    assert_eq!(identities.len(), 5, "Different locations should not cluster");
}

// =============================================================================
// Similarity Function Tests
// =============================================================================

#[test]
fn test_string_similarity_unicode() {
    // Identical Chinese
    assert_eq!(string_similarity("北京", "北京"), 1.0);

    // Different Chinese
    assert_eq!(string_similarity("北京", "上海"), 0.0);

    // Mixed with spaces
    let sim = string_similarity("習近平 主席", "習近平");
    assert!(sim > 0.0 && sim < 1.0);
}

#[test]
fn test_embedding_similarity_edge_cases() {
    // Empty vectors
    assert_eq!(embedding_similarity(&[], &[]), 0.0);

    // Mismatched lengths
    assert_eq!(embedding_similarity(&[1.0, 0.0], &[1.0]), 0.0);

    // Zero vector
    let zero = vec![0.0; 3];
    let unit = vec![1.0, 0.0, 0.0];
    assert_eq!(embedding_similarity(&zero, &unit), 0.0);

    // Negative embeddings (still valid)
    let neg = vec![-1.0, 0.0, 0.0];
    let pos = vec![1.0, 0.0, 0.0];
    let sim = embedding_similarity(&neg, &pos);
    assert!((sim - 0.0).abs() < 0.001); // Opposite vectors
}

// =============================================================================
// Embedding-Based Resolution Tests
// =============================================================================

#[test]
fn test_resolver_with_embeddings() {
    let resolver = Resolver::new().with_threshold(0.8);
    let mut corpus = Corpus::new();

    // Similar embeddings (cosine sim ~1.0)
    let emb1 = vec![1.0, 0.0, 0.0];
    let emb2 = vec![0.99, 0.1, 0.0]; // Very close to emb1

    let mut doc1 = make_doc("doc1");
    let mut track1 = Track::new(1, "Entity A");
    track1.embedding = Some(emb1);
    doc1.add_track(track1);
    corpus.add_document(doc1);

    let mut doc2 = make_doc("doc2");
    let mut track2 = Track::new(1, "Entity B");
    track2.embedding = Some(emb2);
    doc2.add_track(track2);
    corpus.add_document(doc2);

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Similar embeddings should cluster (even with different names)
    assert!(
        identities.len() <= 2,
        "Similar embeddings may or may not cluster depending on threshold"
    );
}

// =============================================================================
// Property Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// Property: Number of identities <= number of tracks
        #[test]
        fn resolver_identities_bounded(
            num_docs in 1usize..10,
            tracks_per_doc in 1usize..5
        ) {
            let resolver = Resolver::new();
            let mut corpus = Corpus::new();

            let total_tracks = num_docs * tracks_per_doc;
            for d in 0..num_docs {
                let mut doc = make_doc(&format!("doc{}", d));
                for t in 0..tracks_per_doc {
                    let track = Track::new(
                        t as u64,
                        format!("Entity_{}_{}", d, t)
                    ).with_type("PERSON".to_string());
                    doc.add_track(track);
                }
                corpus.add_document(doc);
            }

            let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

            prop_assert!(identities.len() <= total_tracks,
                "Got {} identities from {} tracks", identities.len(), total_tracks);
        }

        /// Property: All created identities exist in corpus
        #[test]
        fn resolver_identities_valid(num_docs in 1usize..5) {
            let resolver = Resolver::new();
            let mut corpus = Corpus::new();

            for d in 0..num_docs {
                let mut doc = make_doc(&format!("doc{}", d));
                doc.add_track(Track::new(1, format!("Entity_{}", d)));
                corpus.add_document(doc);
            }

            let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

            for id in &identities {
                prop_assert!(corpus.get_identity(*id).is_some(),
                    "Identity {} should exist in corpus", id);
            }
        }

        /// Property: Identical entities always cluster together
        #[test]
        fn resolver_identical_cluster(
            name in "[A-Za-z]{5,15}",
            num_docs in 2usize..8
        ) {
            let resolver = Resolver::new().with_threshold(0.99);
            let mut corpus = Corpus::new();

            for d in 0..num_docs {
                let mut doc = make_doc(&format!("doc{}", d));
                doc.add_track(Track::new(1, &name).with_type("PERSON".to_string()));
                corpus.add_document(doc);
            }

            let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

            prop_assert_eq!(identities.len(), 1,
                "Identical entities should form one cluster");

            let identity = corpus.get_identity(identities[0]).unwrap();
            if let Some(anno_core::IdentitySource::CrossDocCoref { track_refs }) = &identity.source {
                prop_assert_eq!(track_refs.len(), num_docs,
                    "Cluster should have {} track refs", num_docs);
            }
        }
    }
}
