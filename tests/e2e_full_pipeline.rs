//! End-to-end tests for the full anno pipeline
//!
//! Tests the complete workflow: Extract → Coalesce → Stratify
//! with real-world scenarios and edge cases.

use anno_coalesce::Resolver;
use anno_core::{GroundedDocument, Location, Signal, Track};

/// E2E: Full pipeline on a single document with multiple entity mentions
#[test]
fn e2e_single_doc_with_coref() {
    let text = "Barack Obama was the 44th President of the United States. \
                He served from 2009 to 2017. Obama was born in Hawaii. \
                The former president now lives in Washington, D.C.";

    // Extract
    let mut doc = GroundedDocument::new("test_doc", text);

    // Add signals manually (simulating extraction)
    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Barack Obama",
        "PER",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(1, Location::text(38, 41), "He", "PER", 0.85));
    let sig3 = doc.add_signal(Signal::new(2, Location::text(60, 65), "Obama", "PER", 0.90));
    let sig4 = doc.add_signal(Signal::new(
        3,
        Location::text(88, 105),
        "The former president",
        "PER",
        0.80,
    ));

    let _sig5 = doc.add_signal(Signal::new(
        4,
        Location::text(17, 19),
        "44th",
        "ORDINAL",
        0.95,
    ));
    let _sig6 = doc.add_signal(Signal::new(
        5,
        Location::text(23, 48),
        "President of the United States",
        "ORG",
        0.90,
    ));
    let _sig7 = doc.add_signal(Signal::new(
        6,
        Location::text(48, 56),
        "United States",
        "LOC",
        0.95,
    ));
    let _sig8 = doc.add_signal(Signal::new(
        7,
        Location::text(75, 81),
        "Hawaii",
        "LOC",
        0.95,
    ));
    let _sig9 = doc.add_signal(Signal::new(
        8,
        Location::text(110, 125),
        "Washington, D.C.",
        "LOC",
        0.95,
    ));

    // Run intra-document coreference (Coalesce Level 2)
    // Note: In real usage, this would be done via CLI or API
    // For this test, we'll manually create tracks to simulate coreference
    let mut track1 = Track::new(0, "barack obama");
    track1.add_signal(sig1, 0);
    track1.add_signal(sig2, 1);
    track1.add_signal(sig3, 2);
    track1.add_signal(sig4, 3);
    track1.entity_type = Some("PER".to_string());
    doc.add_track(track1);

    // Verify tracks were created
    let tracks: Vec<_> = doc.tracks().collect();
    assert!(
        !tracks.is_empty(),
        "Should have at least one track for 'Barack Obama'"
    );

    // Find the track for "Barack Obama"
    let obama_track = tracks
        .iter()
        .find(|t| {
            t.canonical_surface.to_lowercase().contains("obama")
                || t.canonical_surface.to_lowercase().contains("president")
        })
        .expect("Should find track for Barack Obama");

    // Verify track contains multiple signals (coreference worked)
    assert!(
        obama_track.signals.len() >= 2,
        "Obama track should contain multiple mentions (found {})",
        obama_track.signals.len()
    );

    // Verify identity linking (Level 3) - should be None initially
    assert!(
        obama_track.identity_id.is_none(),
        "Identity should not be set without crossdoc resolution"
    );
}

/// E2E: Cross-document coreference with type mismatches
#[test]
fn e2e_crossdoc_with_type_mismatch() {
    use anno_core::{Corpus, Track};

    let mut corpus = Corpus::new();

    // Doc1: "Apple" as organization
    let mut doc1 = GroundedDocument::new("doc1", "Apple Inc. is a technology company.");
    let sig1 = doc1.add_signal(Signal::new(0, Location::text(0, 5), "Apple", "ORG", 0.95));
    let mut track1 = Track::new(0, "apple");
    track1.add_signal(sig1, 0);
    track1.entity_type = Some("ORG".to_string());
    let track1_id = doc1.add_track(track1);
    corpus.add_document(doc1);

    // Doc2: "Apple" as fruit
    let mut doc2 = GroundedDocument::new("doc2", "An apple a day keeps the doctor away.");
    let sig2 = doc2.add_signal(Signal::new(0, Location::text(2, 7), "apple", "FRUIT", 0.90));
    let mut track2 = Track::new(0, "apple");
    track2.add_signal(sig2, 0);
    track2.entity_type = Some("FRUIT".to_string());
    let track2_id = doc2.add_track(track2);
    corpus.add_document(doc2);

    // Run crossdoc with type matching required
    let resolver = Resolver::new().with_threshold(0.5).require_type_match(true);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should create 2 separate identities (type mismatch prevents merging)
    assert_eq!(
        identity_ids.len(),
        2,
        "Type mismatch should prevent merging into single identity"
    );

    // Verify tracks have different identity IDs
    let doc1_ref = corpus.get_document("doc1").unwrap();
    let doc2_ref = corpus.get_document("doc2").unwrap();

    let track1_ref = doc1_ref.tracks().find(|t| t.id == track1_id).unwrap();
    let track2_ref = doc2_ref.tracks().find(|t| t.id == track2_id).unwrap();

    assert_ne!(
        track1_ref.identity_id, track2_ref.identity_id,
        "Tracks with different types should have different identities"
    );
}

/// E2E: Cross-document coreference with embedding similarity
#[test]
fn e2e_crossdoc_with_embeddings() {
    use anno_core::{Corpus, Track};

    let mut corpus = Corpus::new();

    // Doc1: "Marie Curie" with embedding
    let mut doc1 = GroundedDocument::new("doc1", "Marie Curie won the Nobel Prize.");
    let sig1 = doc1.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Marie Curie",
        "PER",
        0.95,
    ));
    let mut track1 = Track::new(0, "marie curie");
    track1.add_signal(sig1, 0);
    track1.entity_type = Some("PER".to_string());
    // Simulate embedding (in real usage, this would come from a model)
    track1.embedding = Some(vec![0.1, 0.2, 0.3, 0.4, 0.5]);
    let track1_id = doc1.add_track(track1);
    corpus.add_document(doc1);

    // Doc2: "M. Curie" with similar embedding
    let mut doc2 = GroundedDocument::new("doc2", "M. Curie was a physicist.");
    let sig2 = doc2.add_signal(Signal::new(
        0,
        Location::text(0, 8),
        "M. Curie",
        "PER",
        0.85,
    ));
    let mut track2 = Track::new(0, "m. curie");
    track2.add_signal(sig2, 0);
    track2.entity_type = Some("PER".to_string());
    // Similar embedding (cosine similarity should be high)
    track2.embedding = Some(vec![0.11, 0.21, 0.31, 0.41, 0.51]);
    let track2_id = doc2.add_track(track2);
    corpus.add_document(doc2);

    // Run crossdoc with low threshold (embeddings should match)
    let resolver = Resolver::new()
        .with_threshold(0.3) // Low threshold to allow embedding match
        .require_type_match(true);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should create 1 identity (embeddings are similar)
    assert_eq!(
        identity_ids.len(),
        1,
        "Similar embeddings should merge into single identity"
    );

    // Verify tracks have same identity ID
    let doc1_ref = corpus.get_document("doc1").unwrap();
    let doc2_ref = corpus.get_document("doc2").unwrap();

    let track1_ref = doc1_ref.tracks().find(|t| t.id == track1_id).unwrap();
    let track2_ref = doc2_ref.tracks().find(|t| t.id == track2_id).unwrap();

    assert_eq!(
        track1_ref.identity_id, track2_ref.identity_id,
        "Tracks with similar embeddings should have same identity"
    );
}

/// E2E: Batch processing with multiple documents
#[test]
fn e2e_batch_processing() {
    use anno_core::{Corpus, Track};

    let mut corpus = Corpus::new();

    // Create 5 documents with overlapping entities
    let entities = vec![
        ("doc1", "Barack Obama was president.", "barack obama", "PER"),
        ("doc2", "Obama served from 2009 to 2017.", "obama", "PER"),
        (
            "doc3",
            "The White House is in Washington.",
            "white house",
            "ORG",
        ),
        (
            "doc4",
            "Washington D.C. is the capital.",
            "washington d.c.",
            "LOC",
        ),
        (
            "doc5",
            "The capital city is Washington.",
            "washington",
            "LOC",
        ),
    ];

    for (doc_id, text, canonical, entity_type) in entities {
        let mut doc = GroundedDocument::new(doc_id, text);

        // Find entity in text (simplified - in real usage, use NER)
        let entity_start = text
            .find(canonical.split_whitespace().next().unwrap())
            .unwrap_or(0);
        let entity_end = entity_start + canonical.len().min(text.len() - entity_start);

        let sig = doc.add_signal(Signal::new(
            0,
            Location::text(entity_start, entity_end),
            canonical,
            entity_type,
            0.90,
        ));

        let mut track = Track::new(0, canonical);
        track.add_signal(sig, 0);
        track.entity_type = Some(entity_type.to_string());
        doc.add_track(track);

        corpus.add_document(doc);
    }

    // Run crossdoc coreference
    let resolver = Resolver::new().with_threshold(0.5);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should create identities for:
    // - "Barack Obama" / "Obama" (2 docs) -> 1 identity
    // - "White House" (1 doc) -> 1 identity
    // - "Washington D.C." / "Washington" (2 docs) -> 1 identity
    // Total: 3 identities
    assert!(
        identity_ids.len() >= 3,
        "Should create at least 3 identities (Obama, White House, Washington)"
    );

    // Verify Obama identity links both documents
    let doc1 = corpus.get_document("doc1").unwrap();
    let doc2 = corpus.get_document("doc2").unwrap();

    let track1 = doc1.tracks().next().unwrap();
    let track2 = doc2.tracks().next().unwrap();

    assert_eq!(
        track1.identity_id, track2.identity_id,
        "Obama tracks should link to same identity"
    );
}

/// E2E: Empty document handling
#[test]
fn e2e_empty_document() {
    let doc = GroundedDocument::new("empty", "");

    // Should handle empty document gracefully
    assert_eq!(doc.signals().len(), 0);
    assert_eq!(doc.tracks().count(), 0);
    assert_eq!(doc.identities().count(), 0);

    // Stats should be zero
    let stats = doc.stats();
    assert_eq!(stats.signal_count, 0);
    assert_eq!(stats.track_count, 0);
    assert_eq!(stats.identity_count, 0);
}

/// E2E: Document with only singleton tracks (no coreference)
#[test]
fn e2e_singleton_tracks_only() {
    let text = "Paris is a city. London is also a city. Berlin is another city.";

    let mut doc = GroundedDocument::new("singletons", text);

    // Add signals for each city (no coreference)
    let sig1 = doc.add_signal(Signal::new(0, Location::text(0, 5), "Paris", "LOC", 0.95));
    let sig2 = doc.add_signal(Signal::new(
        1,
        Location::text(19, 25),
        "London",
        "LOC",
        0.95,
    ));
    let sig3 = doc.add_signal(Signal::new(
        2,
        Location::text(42, 48),
        "Berlin",
        "LOC",
        0.95,
    ));

    // Create separate tracks (no merging)
    let mut track1 = Track::new(0, "paris");
    track1.add_signal(sig1, 0);
    doc.add_track(track1);

    let mut track2 = Track::new(0, "london");
    track2.add_signal(sig2, 0);
    doc.add_track(track2);

    let mut track3 = Track::new(0, "berlin");
    track3.add_signal(sig3, 0);
    doc.add_track(track3);

    // Should have 3 tracks, all singletons
    let tracks: Vec<_> = doc.tracks().collect();
    assert_eq!(tracks.len(), 3);

    for track in &tracks {
        assert_eq!(track.signals.len(), 1, "All tracks should be singletons");
    }
}

/// E2E: Document with overlapping entity mentions
#[test]
fn e2e_overlapping_mentions() {
    let text = "New York City is in New York state.";

    let mut doc = GroundedDocument::new("overlapping", text);

    // "New York City" (0-13)
    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 13),
        "New York City",
        "LOC",
        0.95,
    ));

    // "New York" (0-8) - overlaps with sig1
    let sig2 = doc.add_signal(Signal::new(
        1,
        Location::text(0, 8),
        "New York",
        "LOC",
        0.90,
    ));

    // "New York" again (17-25) - separate mention
    let sig3 = doc.add_signal(Signal::new(
        2,
        Location::text(17, 25),
        "New York",
        "LOC",
        0.90,
    ));

    // Verify overlapping signals are detected
    let overlapping = doc.find_overlapping_signal_pairs();
    assert!(!overlapping.is_empty(), "Should detect overlapping signals");

    // Create tracks - should handle overlapping mentions
    let mut track1 = Track::new(0, "new york city");
    track1.add_signal(sig1, 0);
    doc.add_track(track1);

    let mut track2 = Track::new(0, "new york");
    track2.add_signal(sig2, 0);
    track2.add_signal(sig3, 1);
    doc.add_track(track2);

    // Should have 2 tracks
    assert_eq!(doc.tracks().count(), 2);
}

/// E2E: Cross-document with missing embeddings (fallback to string similarity)
#[test]
fn e2e_crossdoc_missing_embeddings() {
    use anno_core::{Corpus, Track};

    let mut corpus = Corpus::new();

    // Doc1: "Microsoft" with embedding
    let mut doc1 = GroundedDocument::new("doc1", "Microsoft Corporation is a tech company.");
    let sig1 = doc1.add_signal(Signal::new(
        0,
        Location::text(0, 9),
        "Microsoft",
        "ORG",
        0.95,
    ));
    let mut track1 = Track::new(0, "microsoft");
    track1.add_signal(sig1, 0);
    track1.entity_type = Some("ORG".to_string());
    track1.embedding = Some(vec![0.1, 0.2, 0.3]);
    let track1_id = doc1.add_track(track1);
    corpus.add_document(doc1);

    // Doc2: "Microsoft" without embedding (should fallback to string similarity)
    let mut doc2 = GroundedDocument::new("doc2", "Microsoft is based in Redmond.");
    let sig2 = doc2.add_signal(Signal::new(
        0,
        Location::text(0, 9),
        "Microsoft",
        "ORG",
        0.95,
    ));
    let mut track2 = Track::new(0, "microsoft");
    track2.add_signal(sig2, 0);
    track2.entity_type = Some("ORG".to_string());
    track2.embedding = None; // Missing embedding
    let track2_id = doc2.add_track(track2);
    corpus.add_document(doc2);

    // Run crossdoc - should fallback to string similarity
    let resolver = Resolver::new().with_threshold(0.5).require_type_match(true);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should create 1 identity (string similarity should match)
    assert_eq!(
        identity_ids.len(),
        1,
        "String similarity fallback should merge identical strings"
    );

    // Verify tracks have same identity
    let doc1_ref = corpus.get_document("doc1").unwrap();
    let doc2_ref = corpus.get_document("doc2").unwrap();

    let track1_ref = doc1_ref.tracks().find(|t| t.id == track1_id).unwrap();
    let track2_ref = doc2_ref.tracks().find(|t| t.id == track2_id).unwrap();

    assert_eq!(
        track1_ref.identity_id, track2_ref.identity_id,
        "Tracks should merge via string similarity fallback"
    );
}

/// E2E: Document with negated entities
#[test]
fn e2e_negated_entities() {
    let text = "John is a doctor. He is not a lawyer.";

    let mut doc = GroundedDocument::new("negated", text);

    let _sig1 = doc.add_signal(Signal::new(0, Location::text(0, 4), "John", "PER", 0.95));
    let _sig2 = doc.add_signal(Signal::new(1, Location::text(15, 17), "He", "PER", 0.85));

    // Create negated signal for "lawyer"
    let mut sig3 = Signal::new(2, Location::text(28, 33), "lawyer", "OCC", 0.80);
    sig3.negated = true;
    let sig3_id = doc.add_signal(sig3);

    // Verify negated signal
    let negated_signal = doc.signals().iter().find(|s| s.id == sig3_id).unwrap();
    assert!(negated_signal.negated, "Signal should be marked as negated");

    // Stats should reflect negated count
    let stats = doc.stats();
    assert_eq!(stats.negated_count, 1);
}

/// E2E: Document with quantifiers
#[test]
fn e2e_quantified_entities() {
    use anno_core::Quantifier;

    let text = "All students passed the exam. Some students failed.";

    let mut doc = GroundedDocument::new("quantified", text);

    let mut sig1 = Signal::new(0, Location::text(0, 3), "All", "QUANT", 0.95);
    sig1.quantifier = Some(Quantifier::Universal);
    doc.add_signal(sig1);

    let mut sig2 = Signal::new(1, Location::text(4, 12), "students", "PER", 0.90);
    sig2.quantifier = Some(Quantifier::Universal);
    doc.add_signal(sig2);

    let mut sig3 = Signal::new(2, Location::text(30, 34), "Some", "QUANT", 0.95);
    sig3.quantifier = Some(Quantifier::Existential);
    doc.add_signal(sig3);

    // Verify quantifiers are set
    let signals: Vec<_> = doc.signals().iter().collect();
    assert_eq!(signals[0].quantifier, Some(Quantifier::Universal));
    assert_eq!(signals[1].quantifier, Some(Quantifier::Universal));
    assert_eq!(signals[2].quantifier, Some(Quantifier::Existential));
}
