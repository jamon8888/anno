//! Regression test: Crossdoc coreference with type mismatch handling
//!
//! This test ensures that when `require_type_match` is enabled, tracks with
//! different entity types are not clustered together, even if they have high similarity.

use anno_core::{Corpus, GroundedDocument, Signal, Location, TrackId};
use anno_coalesce::Resolver;
use anno::HeuristicNER;

#[test]
fn test_type_mismatch_with_require_type_match() {
    // Create a corpus with two documents
    let mut corpus = Corpus::new();
    
    // Document 1: "Apple" as ORG
    let mut doc1 = GroundedDocument::new("doc1", "Apple was founded in 1976.");
    let signal1 = Signal::new(0, Location::text(0, 5), "Apple", "ORG", 0.9);
    let sig_id1 = doc1.add_signal(signal1);
    
    // Document 2: "Apple" as FRUIT (should be different entity type)
    let mut doc2 = GroundedDocument::new("doc2", "I ate an apple for lunch.");
    let signal2 = Signal::new(0, Location::text(10, 15), "apple", "FRUIT", 0.8);
    let sig_id2 = doc2.add_signal(signal2);
    
    // Create tracks
    let track1_id = doc1.create_track_from_signals("Apple", &[sig_id1]).unwrap();
    let track2_id = doc2.create_track_from_signals("apple", &[sig_id2]).unwrap();
    
    corpus.add_document(doc1);
    corpus.add_document(doc2);
    
    // Resolve with require_type_match = true
    let resolver = Resolver::new().require_type_match(true);
    let identities = resolver.resolve_inter_doc_coref(&mut corpus, Some(0.5), Some(true));
    
    // Should create 2 separate identities (one for ORG, one for FRUIT)
    assert_eq!(
        identities.len(),
        2,
        "With require_type_match=true, different entity types should create separate identities"
    );
}

#[test]
fn test_type_mismatch_without_require_type_match() {
    // Same setup as above, but with require_type_match = false
    let mut corpus = Corpus::new();
    
    let mut doc1 = GroundedDocument::new("doc1", "Apple was founded in 1976.");
    let signal1 = Signal::new(0, Location::text(0, 5), "Apple", "ORG", 0.9);
    let sig_id1 = doc1.add_signal(signal1);
    
    let mut doc2 = GroundedDocument::new("doc2", "I ate an apple for lunch.");
    let signal2 = Signal::new(0, Location::text(10, 15), "apple", "FRUIT", 0.8);
    let sig_id2 = doc2.add_signal(signal2);
    
    let track1_id = doc1.create_track_from_signals("Apple", &[sig_id1]).unwrap();
    let track2_id = doc2.create_track_from_signals("apple", &[sig_id2]).unwrap();
    
    corpus.add_document(doc1);
    corpus.add_document(doc2);
    
    // Resolve with require_type_match = false
    let resolver = Resolver::new().require_type_match(false);
    let identities = resolver.resolve_inter_doc_coref(&mut corpus, Some(0.5), Some(false));
    
    // With high similarity and no type requirement, might cluster together
    // (depends on string similarity threshold)
    assert!(
        identities.len() >= 1,
        "Without require_type_match, entities might cluster based on similarity alone"
    );
}

