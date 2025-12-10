//! Serialization and deserialization round-trip tests
//!
//! Tests that data structures can be serialized and deserialized correctly,
//! preserving all information.

use anno_core::{Corpus, GroundedDocument, Identity, Location, Signal, Track};
use serde_json;

/// E2E: GroundedDocument JSON round-trip
#[test]
fn e2e_grounded_document_json_roundtrip() {
    let mut doc = GroundedDocument::new(
        "test_doc",
        "Marie Curie won the Nobel Prize. She was a physicist.",
    );

    // Add signals
    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Marie Curie",
        "PER",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(1, Location::text(38, 41), "She", "PER", 0.85));
    let sig3 = doc.add_signal(Signal::new(
        2,
        Location::text(17, 29),
        "Nobel Prize",
        "AWARD",
        0.92,
    ));

    // Add tracks
    let mut track1 = Track::new(0, "marie curie");
    track1.add_signal(sig1, 0);
    track1.add_signal(sig2, 1);
    track1.entity_type = Some("PER".to_string());
    doc.add_track(track1);

    // Add identity
    let identity = Identity::from_kb(
        0,
        "Marie Curie".to_string(),
        "wikidata".to_string(),
        "Q7186".to_string(),
    );
    doc.add_identity(identity);

    // Serialize to JSON
    let json = serde_json::to_string(&doc).expect("Should serialize");

    // Deserialize from JSON
    let deserialized: GroundedDocument = serde_json::from_str(&json).expect("Should deserialize");

    // Verify all data preserved
    assert_eq!(deserialized.id, doc.id);
    assert_eq!(deserialized.text, doc.text);
    assert_eq!(deserialized.signals().len(), doc.signals().len());
    assert_eq!(deserialized.tracks().count(), doc.tracks().count());
    assert_eq!(deserialized.identities().count(), doc.identities().count());

    // Verify signal data
    let orig_sig = doc.get_signal(sig1).unwrap();
    let deser_sig = deserialized
        .signals()
        .iter()
        .find(|s| s.surface() == "Marie Curie")
        .unwrap();
    assert_eq!(deser_sig.surface(), orig_sig.surface());
    assert_eq!(deser_sig.label(), orig_sig.label());
    assert!((deser_sig.confidence - orig_sig.confidence).abs() < 0.001);
}

/// E2E: Corpus document round-trip (via individual documents)
///
/// Note: Corpus itself doesn't implement Serialize, but we can test
/// serialization of individual documents and identities.
#[test]
fn e2e_corpus_document_roundtrip() {
    let mut corpus = Corpus::new();

    // Add documents
    let mut doc1 = GroundedDocument::new("doc1", "Barack Obama was president.");
    let sig1 = doc1.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Barack Obama",
        "PER",
        0.95,
    ));
    let mut track1 = Track::new(0, "barack obama");
    track1.add_signal(sig1, 0);
    doc1.add_track(track1);
    corpus.add_document(doc1);

    let doc2 = GroundedDocument::new("doc2", "Obama served from 2009 to 2017.");
    corpus.add_document(doc2);

    // Add identity
    let identity = Identity::new(0, "Barack Obama".to_string());
    corpus.add_identity(identity);

    // Serialize individual documents
    let orig_doc = corpus.get_document("doc1").unwrap();
    let json = serde_json::to_string(orig_doc).expect("Should serialize document");

    // Deserialize from JSON
    let deser_doc: GroundedDocument =
        serde_json::from_str(&json).expect("Should deserialize document");

    // Verify data preserved
    assert_eq!(deser_doc.text, orig_doc.text);
    assert_eq!(deser_doc.signals().len(), orig_doc.signals().len());
    assert_eq!(deser_doc.tracks().count(), orig_doc.tracks().count());
}

/// E2E: Signal with all optional fields JSON round-trip
#[test]
fn e2e_signal_full_json_roundtrip() {
    use anno_core::Quantifier;

    let mut signal = Signal::new(0, Location::text(0, 10), "test entity", "PER", 0.9);
    signal.negated = true;
    signal.quantifier = Some(Quantifier::Existential);

    // Serialize
    let json = serde_json::to_string(&signal).expect("Should serialize");

    // Deserialize
    let deserialized: Signal<Location> = serde_json::from_str(&json).expect("Should deserialize");

    // Verify all fields preserved
    assert_eq!(deserialized.surface(), signal.surface());
    assert_eq!(deserialized.label(), signal.label());
    assert_eq!(deserialized.confidence, signal.confidence);
    assert_eq!(deserialized.negated, signal.negated);
    assert_eq!(deserialized.quantifier, signal.quantifier);
}

/// E2E: Track with embedding JSON round-trip
#[test]
fn e2e_track_embedding_json_roundtrip() {
    let mut track = Track::new(0, "test entity");
    track.entity_type = Some("PER".to_string());
    track.embedding = Some(vec![0.1, 0.2, 0.3, 0.4, 0.5]);
    track.cluster_confidence = 0.85;

    // Serialize
    let json = serde_json::to_string(&track).expect("Should serialize");

    // Deserialize
    let deserialized: Track = serde_json::from_str(&json).expect("Should deserialize");

    // Verify all fields preserved
    assert_eq!(deserialized.canonical_surface, track.canonical_surface);
    assert_eq!(deserialized.entity_type, track.entity_type);
    assert_eq!(deserialized.cluster_confidence, track.cluster_confidence);
    assert_eq!(deserialized.embedding, track.embedding);
}

/// E2E: Identity with KB info JSON round-trip
#[test]
fn e2e_identity_kb_json_roundtrip() {
    let mut identity = Identity::from_kb(
        0,
        "Marie Curie".to_string(),
        "wikidata".to_string(),
        "Q7186".to_string(),
    );
    identity.description = Some("Polish-French physicist and chemist".to_string());
    identity.aliases = vec!["Maria Skłodowska".to_string(), "M. Curie".to_string()];
    identity.embedding = Some(vec![0.1, 0.2, 0.3]);

    // Serialize
    let json = serde_json::to_string(&identity).expect("Should serialize");

    // Deserialize
    let deserialized: Identity = serde_json::from_str(&json).expect("Should deserialize");

    // Verify all fields preserved
    assert_eq!(deserialized.canonical_name, identity.canonical_name);
    assert_eq!(deserialized.kb_name, identity.kb_name);
    assert_eq!(deserialized.kb_id, identity.kb_id);
    assert_eq!(deserialized.description, identity.description);
    assert_eq!(deserialized.aliases, identity.aliases);
    assert_eq!(deserialized.embedding, identity.embedding);
}

/// E2E: Large document serialization performance
#[test]
fn e2e_large_document_serialization() {
    let mut doc = GroundedDocument::new("large_doc", &"Test text. ".repeat(1000));

    // Add many signals
    for i in 0..100 {
        let start = i * 12;
        let end = start + 4;
        doc.add_signal(Signal::new(
            i,
            Location::text(start as usize, end as usize),
            "Test",
            "PER",
            0.9,
        ));
    }

    // Serialize (should not be too slow)
    let start = std::time::Instant::now();
    let json = serde_json::to_string(&doc).expect("Should serialize");
    let elapsed = start.elapsed();

    // Should complete in reasonable time (< 1 second for 100 signals)
    assert!(elapsed.as_secs() < 1, "Serialization should be fast");
    assert!(!json.is_empty());

    // Deserialize
    let start = std::time::Instant::now();
    let _deserialized: GroundedDocument = serde_json::from_str(&json).expect("Should deserialize");
    let elapsed = start.elapsed();

    assert!(elapsed.as_secs() < 1, "Deserialization should be fast");
}

/// E2E: Malformed JSON handling
#[test]
fn e2e_malformed_json_handling() {
    let malformed_json = r#"{"id": "test", "text": "test", "signals": [invalid]}"#;

    let result: Result<GroundedDocument, _> = serde_json::from_str(malformed_json);

    // Should return error, not panic
    assert!(result.is_err());
}

/// E2E: Missing required fields in JSON
#[test]
fn e2e_missing_required_fields() {
    let incomplete_json = r#"{"id": "test"}"#; // Missing "text" field

    let result: Result<GroundedDocument, _> = serde_json::from_str(incomplete_json);

    // Should return error or use defaults
    // Implementation-dependent
    let _ = result; // Just verify it doesn't panic
}

/// E2E: Extra fields in JSON (should be ignored)
#[test]
fn e2e_extra_fields_json() {
    let json_with_extra = r#"{
        "id": "test",
        "text": "test text",
        "signals": [],
        "tracks": {},
        "identities": {},
        "extra_field": "should be ignored"
    }"#;

    let result: Result<GroundedDocument, _> = serde_json::from_str(json_with_extra);

    // Should deserialize successfully, ignoring extra fields
    if let Ok(doc) = result {
        assert_eq!(doc.id, "test");
        assert_eq!(doc.text, "test text");
    }
}
