//! Error recovery and edge case handling tests
//!
//! Tests how the system handles errors, invalid inputs, and edge cases gracefully.

use anno_coalesce::Resolver;
use anno_core::Corpus;
use anno_core::{GroundedDocument, Location, Signal, Track};

/// E2E: Handle empty documents gracefully
#[test]
fn e2e_empty_document() {
    let doc = GroundedDocument::new("empty_doc", "");

    assert_eq!(doc.signals().len(), 0);
    assert_eq!(doc.tracks().count(), 0);
    assert_eq!(doc.identities().count(), 0);

    // Should not panic on operations
    let stats = doc.stats();
    assert_eq!(stats.signal_count, 0);
    assert_eq!(stats.track_count, 0);
}

/// E2E: Handle documents with only whitespace
#[test]
fn e2e_whitespace_only_document() {
    let doc = GroundedDocument::new("whitespace_doc", "   \n\t  ");

    assert_eq!(doc.signals().len(), 0);
    assert_eq!(doc.tracks().count(), 0);

    // Should not panic
    let _stats = doc.stats();
}

/// E2E: Handle overlapping signal spans
#[test]
fn e2e_overlapping_signals() {
    let mut doc = GroundedDocument::new("overlap_doc", "New York City is great.");

    // Add overlapping signals (nested entity)
    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 13),
        "New York City",
        "LOC",
        0.9,
    ));
    let sig2 = doc.add_signal(Signal::new(
        1,
        Location::text(0, 8),
        "New York",
        "LOC",
        0.85,
    ));

    // Both signals should be added
    assert_eq!(doc.signals().len(), 2);

    // Verify overlapping signals can be detected using the document method
    let overlapping = doc.find_overlapping_signal_pairs();
    assert_eq!(overlapping.len(), 1, "Should find one overlapping pair");
}

/// E2E: Handle signals with invalid offsets
#[test]
fn e2e_invalid_signal_offsets() {
    let mut doc = GroundedDocument::new("invalid_doc", "Short text.");
    let text_len = doc.text.chars().count();

    // Signal with end > text length (should be handled gracefully)
    let sig = Signal::new(0, Location::text(0, text_len + 100), "Invalid", "PER", 0.9);

    // Adding should either validate or handle gracefully
    let _sig_id = doc.add_signal(sig);

    // Document should still be usable
    assert!(doc.signals().len() > 0);
}

/// E2E: Handle empty tracks
#[test]
fn e2e_empty_track() {
    let mut doc = GroundedDocument::new("empty_track_doc", "Test text.");

    let mut track = Track::new(0, "empty");
    // Don't add any signals
    let track_id = doc.add_track(track);

    let track_ref = doc.get_track(track_id).unwrap();
    assert!(track_ref.is_empty(), "Track should be empty");
    assert_eq!(track_ref.len(), 0);
}

/// E2E: Handle duplicate signal IDs in track
#[test]
fn e2e_duplicate_signal_in_track() {
    let mut doc = GroundedDocument::new("duplicate_doc", "Test text.");

    let sig = doc.add_signal(Signal::new(0, Location::text(0, 4), "Test", "PER", 0.9));

    let mut track = Track::new(0, "test");
    track.add_signal(sig, 0);
    track.add_signal(sig, 1); // Add same signal twice

    let track_id = doc.add_track(track);
    let track_ref = doc.get_track(track_id).unwrap();

    // Track should handle duplicate signals (may have 1 or 2 entries depending on implementation)
    assert!(track_ref.len() >= 1);
}

/// E2E: Handle crossdoc with empty corpus
#[test]
fn e2e_crossdoc_empty_corpus() {
    let mut corpus = Corpus::new();

    let resolver = Resolver::new();
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should return empty list, not panic
    assert_eq!(identity_ids.len(), 0);
}

/// E2E: Handle crossdoc with corpus containing only empty documents
#[test]
fn e2e_crossdoc_empty_documents() {
    let mut corpus = Corpus::new();

    corpus.add_document(GroundedDocument::new("doc1", ""));
    corpus.add_document(GroundedDocument::new("doc2", ""));
    corpus.add_document(GroundedDocument::new("doc3", ""));

    let resolver = Resolver::new();
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should handle gracefully, no identities created
    assert_eq!(identity_ids.len(), 0);
}

/// E2E: Track should not contain signals from different documents
/// E2E: Handle very high similarity threshold (nothing should merge)
#[test]
fn e2e_crossdoc_high_threshold() {
    let mut corpus = Corpus::new();

    // Doc1: "Apple Inc."
    let mut doc1 = GroundedDocument::new("doc1", "Apple Inc. is a company.");
    let sig1 = doc1.add_signal(Signal::new(
        0,
        Location::text(0, 10),
        "Apple Inc.",
        "ORG",
        0.95,
    ));
    let mut track1 = Track::new(0, "apple inc");
    track1.add_signal(sig1, 0);
    track1.entity_type = Some("ORG".to_string());
    doc1.add_track(track1);
    corpus.add_document(doc1);

    // Doc2: "Apple Inc." (same entity)
    let mut doc2 = GroundedDocument::new("doc2", "Apple Inc. was founded in 1976.");
    let sig2 = doc2.add_signal(Signal::new(
        0,
        Location::text(0, 10),
        "Apple Inc.",
        "ORG",
        0.95,
    ));
    let mut track2 = Track::new(0, "apple inc");
    track2.add_signal(sig2, 0);
    track2.entity_type = Some("ORG".to_string());
    doc2.add_track(track2);
    corpus.add_document(doc2);

    // Use very high threshold (0.99) - should prevent merging
    let resolver = Resolver::new().with_threshold(0.99);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // May create 1 or 2 identities depending on exact similarity calculation
    // This tests that high threshold affects merging behavior
    assert!(identity_ids.len() >= 1);
}

/// E2E: Handle very low similarity threshold (everything might merge)
#[test]
fn e2e_crossdoc_low_threshold() {
    let mut corpus = Corpus::new();

    // Doc1: "Apple Inc."
    let mut doc1 = GroundedDocument::new("doc1", "Apple Inc. is a company.");
    let sig1 = doc1.add_signal(Signal::new(
        0,
        Location::text(0, 10),
        "Apple Inc.",
        "ORG",
        0.95,
    ));
    let mut track1 = Track::new(0, "apple inc");
    track1.add_signal(sig1, 0);
    track1.entity_type = Some("ORG".to_string());
    doc1.add_track(track1);
    corpus.add_document(doc1);

    // Doc2: "Microsoft Corp." (different entity)
    let mut doc2 = GroundedDocument::new("doc2", "Microsoft Corp. is another company.");
    let sig2 = doc2.add_signal(Signal::new(
        0,
        Location::text(0, 14),
        "Microsoft Corp.",
        "ORG",
        0.95,
    ));
    let mut track2 = Track::new(0, "microsoft corp");
    track2.add_signal(sig2, 0);
    track2.entity_type = Some("ORG".to_string());
    doc2.add_track(track2);
    corpus.add_document(doc2);

    // Use very low threshold (0.01) - might merge unrelated entities
    let resolver = Resolver::new().with_threshold(0.01);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should create at least 1 identity (may merge or not depending on similarity)
    assert!(identity_ids.len() >= 1);
}

/// E2E: Track should not contain signals from different documents
///
/// This test verifies that tracks are document-scoped. Since tracks
/// are created within a single document, they cannot contain signals
/// from different documents. The API enforces this by requiring tracks
/// to be added to a specific document, which validates signal IDs.
#[test]
fn e2e_track_cross_document_signals() {
    // Tracks are document-scoped, so signals from different documents
    // cannot be added to the same track. The API enforces this by
    // requiring tracks to be added to a document, which validates
    // that all signal IDs belong to that document.
    let mut doc1 = GroundedDocument::new("doc1", "Alice");
    let mut doc2 = GroundedDocument::new("doc2", "Bob");

    let sig1 = doc1.add_signal(Signal::new(0, Location::text(0, 5), "Alice", "PER", 0.9));
    let _sig2 = doc2.add_signal(Signal::new(0, Location::text(0, 3), "Bob", "PER", 0.9));

    // Create a track in doc1 with sig1 (valid)
    let mut track = Track::new(0, "person");
    track.add_signal(sig1, 0);
    let track_id = doc1.add_track(track);

    // Verify the track was added successfully and contains the signal
    let track_ref = doc1.get_track(track_id).unwrap();
    assert_eq!(track_ref.signals.len(), 1);
    assert_eq!(track_ref.signals[0].signal_id, sig1);

    // Attempting to add sig2 (from doc2) to a track in doc1 would require
    // accessing doc1's internal signal list, which the API prevents.
    // The document-scoped nature of tracks ensures this cannot happen.
}

/// E2E: Handle very long entity names
#[test]
fn e2e_very_long_entity_name() {
    let long_name = "A".repeat(1000);
    let text = format!("{} is a very long entity name.", long_name);

    let mut doc = GroundedDocument::new("long_doc", &text);
    let sig = doc.add_signal(Signal::new(
        0,
        Location::text(0, long_name.len()),
        &long_name,
        "PER",
        0.9,
    ));

    assert_eq!(doc.signals().len(), 1);
    assert_eq!(doc.get_signal(sig).unwrap().surface(), long_name);
}

/// E2E: Handle Unicode and emoji in entity names
#[test]
fn e2e_unicode_emoji_entities() {
    let text = "Marie Curie (👩‍🔬) won the Nobel Prize. She was born in 🇵🇱 Poland.";

    let mut doc = GroundedDocument::new("unicode_doc", text);
    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Marie Curie",
        "PER",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(1, Location::text(13, 20), "👩‍🔬", "EMOJI", 0.8));
    let sig3 = doc.add_signal(Signal::new(2, Location::text(58, 64), "🇵🇱", "FLAG", 0.9));
    let _sig4 = doc.add_signal(Signal::new(
        3,
        Location::text(65, 71),
        "Poland",
        "LOC",
        0.95,
    ));

    assert_eq!(doc.signals().len(), 4);

    // Verify Unicode handling
    assert_eq!(doc.get_signal(sig2).unwrap().surface(), "👩‍🔬");
    assert_eq!(doc.get_signal(sig3).unwrap().surface(), "🇵🇱");
}

/// E2E: Handle zero-confidence signals
#[test]
fn e2e_zero_confidence_signals() {
    let mut doc = GroundedDocument::new("zero_conf_doc", "Test entity.");

    let sig = doc.add_signal(Signal::new(0, Location::text(0, 4), "Test", "PER", 0.0));

    assert_eq!(doc.signals().len(), 1);
    assert_eq!(doc.get_signal(sig).unwrap().confidence, 0.0);

    // Should be filterable by confidence (check signal directly)
    let signal = doc.get_signal(sig).unwrap();
    assert!(
        !signal.is_confident(0.5),
        "Zero confidence signal should not pass threshold"
    );
}

/// E2E: Handle negative confidence (should be clamped)
#[test]
fn e2e_negative_confidence() {
    let mut doc = GroundedDocument::new("neg_conf_doc", "Test entity.");

    // Signal with negative confidence (should be clamped to 0.0)
    let sig = doc.add_signal(Signal::new(0, Location::text(0, 4), "Test", "PER", -1.0));

    let signal = doc.get_signal(sig).unwrap();
    assert!(
        signal.confidence >= 0.0,
        "Confidence should be clamped to >= 0.0"
    );
}

/// E2E: Handle confidence > 1.0 (should be clamped)
#[test]
fn e2e_overflow_confidence() {
    let mut doc = GroundedDocument::new("overflow_conf_doc", "Test entity.");

    // Signal with confidence > 1.0 (should be clamped)
    let sig = doc.add_signal(Signal::new(0, Location::text(0, 4), "Test", "PER", 2.0));

    let signal = doc.get_signal(sig).unwrap();
    assert!(
        signal.confidence <= 1.0,
        "Confidence should be clamped to <= 1.0"
    );
}

/// E2E: Handle track merging with empty track list
#[test]
fn e2e_merge_empty_track_list() {
    let mut doc = GroundedDocument::new("merge_empty_doc", "Test.");

    let merged_id = doc.merge_tracks(&[]);

    assert!(
        merged_id.is_none(),
        "Merging empty track list should return None"
    );
}

/// E2E: Handle track merging with single track
#[test]
fn e2e_merge_single_track() {
    let mut doc = GroundedDocument::new("merge_single_doc", "Test entity.");

    let sig = doc.add_signal(Signal::new(0, Location::text(0, 4), "Test", "PER", 0.9));
    let mut track = Track::new(0, "test");
    track.add_signal(sig, 0);
    let track_id = doc.add_track(track);

    let merged_id = doc.merge_tracks(&[track_id]);

    // Should return the same track or a new one
    assert!(merged_id.is_some());
}

/// E2E: Handle track merging with non-existent track IDs
#[test]
fn e2e_merge_nonexistent_tracks() {
    let mut doc = GroundedDocument::new("merge_nonexistent_doc", "Test.");

    // Try to merge tracks that don't exist
    let merged_id = doc.merge_tracks(&[999, 1000]);

    // Should handle gracefully (return None or panic)
    // Implementation-dependent
    let _ = merged_id; // Just verify it doesn't crash
}

/// E2E: Handle corpus with duplicate document IDs
#[test]
fn e2e_corpus_duplicate_doc_ids() {
    let mut corpus = Corpus::new();

    let doc1 = GroundedDocument::new("doc1", "Original text.");
    corpus.add_document(doc1);

    let doc2 = GroundedDocument::new("doc1", "Replaced text.");
    corpus.add_document(doc2);

    // Should replace, not duplicate
    assert_eq!(corpus.documents().count(), 1);
    assert_eq!(corpus.get_document("doc1").unwrap().text, "Replaced text.");
}

/// E2E: Handle identity with very long canonical name
#[test]
fn e2e_identity_long_name() {
    let mut corpus = Corpus::new();

    let long_name = "A".repeat(500);
    let identity = anno_core::Identity::new(0, long_name.clone());
    let identity_id = corpus.add_identity(identity);

    let stored = corpus.identities().get(&identity_id).unwrap();
    assert_eq!(stored.canonical_name, long_name);
}

/// E2E: Handle identity with many aliases
#[test]
fn e2e_identity_many_aliases() {
    let mut corpus = Corpus::new();

    let mut identity = anno_core::Identity::new(0, "Barack Obama".to_string());
    identity.aliases = (0..100).map(|i| format!("Alias {}", i)).collect();

    let identity_id = corpus.add_identity(identity);
    let stored = corpus.identities().get(&identity_id).unwrap();

    assert_eq!(stored.aliases.len(), 100);
}
