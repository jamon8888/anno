//! Integration tests for corpus management
//!
//! Tests adding, removing, merging documents, and identity management.

use anno_coalesce::Resolver;
use anno_core::{Corpus, GroundedDocument, Identity, IdentityId, Location, Signal, Track, TrackId};

/// Test: Add multiple documents to corpus
#[test]
fn test_corpus_add_documents() {
    let mut corpus = Corpus::new();

    for i in 1..=5 {
        let doc = GroundedDocument::new(&format!("doc{}", i), &format!("Document {} text.", i));
        corpus.add_document(doc);
    }

    assert_eq!(corpus.documents().count(), 5);
    assert!(corpus.get_document("doc1").is_some());
    assert!(corpus.get_document("doc5").is_some());
    assert!(corpus.get_document("doc6").is_none());
}

/// Test: Document replacement (adding same ID replaces)
#[test]
fn test_corpus_document_replacement() {
    let mut corpus = Corpus::new();

    let doc1 = GroundedDocument::new("doc1", "Text 1");
    let doc2 = GroundedDocument::new("doc2", "Text 2");

    corpus.add_document(doc1);
    corpus.add_document(doc2);

    assert_eq!(corpus.documents().count(), 2);

    // Replace doc1 with new content
    let doc1_replaced = GroundedDocument::new("doc1", "Replaced text");
    corpus.add_document(doc1_replaced);

    // Should still have 2 documents (doc1 replaced, doc2 unchanged)
    assert_eq!(corpus.documents().count(), 2);
    assert_eq!(corpus.get_document("doc1").unwrap().text, "Replaced text");
    assert_eq!(corpus.get_document("doc2").unwrap().text, "Text 2");
}

/// Test: Identity creation and linking
#[test]
fn test_corpus_identity_linking() {
    let mut corpus = Corpus::new();

    // Create document with track
    let mut doc = GroundedDocument::new("doc1", "Marie Curie won the Nobel Prize.");
    let sig = doc.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Marie Curie",
        "PER",
        0.95,
    ));
    let mut track = Track::new(0, "marie curie");
    track.add_signal(sig, 0);
    let track_id = doc.add_track(track);
    corpus.add_document(doc);

    // Link track to KB (this creates the identity)
    let track_ref = anno_core::TrackRef {
        doc_id: "doc1".to_string(),
        track_id,
    };
    let identity_id = corpus
        .link_track_to_kb(&track_ref, "wikidata", "Q7186", "Marie Curie")
        .unwrap();

    // Verify linking
    let doc_ref = corpus.get_document("doc1").unwrap();
    let track_ref = doc_ref.tracks().find(|t| t.id == track_id).unwrap();
    assert_eq!(track_ref.identity_id, Some(identity_id));

    // Verify identity exists
    assert!(corpus.identities().get(&identity_id).is_some());
}

/// Test: Identity merging when tracks are merged
#[test]
fn test_corpus_identity_merging() {
    let mut corpus = Corpus::new();

    // Doc1: "Barack Obama"
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
    track1.entity_type = Some("PER".to_string());
    let track1_id = doc1.add_track(track1);
    corpus.add_document(doc1);

    // Doc2: "Obama"
    let mut doc2 = GroundedDocument::new("doc2", "Obama served from 2009 to 2017.");
    let sig2 = doc2.add_signal(Signal::new(0, Location::text(0, 5), "Obama", "PER", 0.90));
    let mut track2 = Track::new(0, "obama");
    track2.add_signal(sig2, 0);
    track2.entity_type = Some("PER".to_string());
    let track2_id = doc2.add_track(track2);
    corpus.add_document(doc2);

    // Run crossdoc coreference
    let resolver = Resolver::new().with_threshold(0.5);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should create 1 identity
    assert_eq!(identity_ids.len(), 1);

    // Both tracks should link to same identity
    let doc1_ref = corpus.get_document("doc1").unwrap();
    let doc2_ref = corpus.get_document("doc2").unwrap();

    let track1_ref = doc1_ref.tracks().find(|t| t.id == track1_id).unwrap();
    let track2_ref = doc2_ref.tracks().find(|t| t.id == track2_id).unwrap();

    assert_eq!(track1_ref.identity_id, track2_ref.identity_id);
    assert!(track1_ref.identity_id.is_some());

    // Identity should reference both tracks
    let identity_id = track1_ref.identity_id.unwrap();
    let identity = corpus.identities().get(&identity_id).unwrap();

    // Check source (should be CrossDocCoref with both track refs)
    if let Some(anno_core::IdentitySource::CrossDocCoref { track_refs }) = &identity.source {
        assert_eq!(track_refs.len(), 2);
    }
}

/// Test: Corpus with empty documents
#[test]
fn test_corpus_empty_documents() {
    let mut corpus = Corpus::new();

    let doc1 = GroundedDocument::new("doc1", "");
    let doc2 = GroundedDocument::new("doc2", "Some text");
    let doc3 = GroundedDocument::new("doc3", "");

    corpus.add_document(doc1);
    corpus.add_document(doc2);
    corpus.add_document(doc3);

    assert_eq!(corpus.documents().count(), 3);

    // Run crossdoc (should handle empty documents gracefully)
    let resolver = Resolver::new();
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should not panic, may create 0 identities if no tracks
    // (No assertion needed - just verifying it doesn't panic)
    let _ = identity_ids;
}

/// Test: Corpus with documents containing no tracks
#[test]
fn test_corpus_no_tracks() {
    let mut corpus = Corpus::new();

    // Document with signals but no tracks
    let mut doc = GroundedDocument::new("doc1", "Some text with entities.");
    doc.add_signal(Signal::new(0, Location::text(0, 4), "Some", "OTHER", 0.5));
    corpus.add_document(doc);

    // Run crossdoc (should handle documents without tracks)
    let resolver = Resolver::new();
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should not create identities if no tracks
    assert_eq!(identity_ids.len(), 0);
}

/// Test: Corpus identity ID auto-increment
#[test]
fn test_corpus_identity_id_increment() {
    let mut corpus = Corpus::new();

    let id1 = corpus.add_identity(Identity::new(0, "Entity 1"));
    let id2 = corpus.add_identity(Identity::new(0, "Entity 2"));
    let id3 = corpus.add_identity(Identity::new(0, "Entity 3"));

    // IDs should be sequential
    assert_eq!(id1, IdentityId::new(0));
    assert_eq!(id2, IdentityId::new(1));
    assert_eq!(id3, IdentityId::new(2));

    // Verify all identities exist
    assert!(corpus.identities().get(&id1).is_some());
    assert!(corpus.identities().get(&id2).is_some());
    assert!(corpus.identities().get(&id3).is_some());
}

/// Test: Corpus with duplicate document IDs (should replace)
#[test]
fn test_corpus_duplicate_doc_ids() {
    let mut corpus = Corpus::new();

    let doc1 = GroundedDocument::new("doc1", "Original text");
    corpus.add_document(doc1);

    let doc2 = GroundedDocument::new("doc1", "Replaced text");
    corpus.add_document(doc2);

    // Should have only one document (replaced)
    assert_eq!(corpus.documents().count(), 1);
    assert_eq!(corpus.get_document("doc1").unwrap().text, "Replaced text");
}

/// Test: Corpus track reference validation
#[test]
fn test_corpus_track_ref_validation() {
    let mut corpus = Corpus::new();

    let doc = GroundedDocument::new("doc1", "Text");
    corpus.add_document(doc);

    // Invalid track ref (document doesn't have that track)
    let invalid_ref = anno_core::TrackRef {
        doc_id: "doc1".to_string(),
        track_id: TrackId::new(999), // Non-existent track ID
    };

    // Should return error
    let result = corpus.link_track_to_kb(&invalid_ref, "wikidata", "Q123", "Test");
    assert!(result.is_err());
}

/// Test: Corpus with very large number of documents
#[test]
fn test_corpus_large_scale() {
    let mut corpus = Corpus::new();

    // Add 100 documents
    for i in 0..100 {
        let doc = GroundedDocument::new(&format!("doc{}", i), &format!("Document {} content.", i));
        corpus.add_document(doc);
    }

    assert_eq!(corpus.documents().count(), 100);

    // Verify random access
    assert!(corpus.get_document("doc0").is_some());
    assert!(corpus.get_document("doc50").is_some());
    assert!(corpus.get_document("doc99").is_some());
    assert!(corpus.get_document("doc100").is_none());
}

/// Test: Corpus identity aliases
#[test]
fn test_corpus_identity_aliases() {
    let mut corpus = Corpus::new();

    let mut identity = Identity::new(0, "Barack Obama");
    identity.aliases = vec![
        "B. Obama".to_string(),
        "President Obama".to_string(),
        "Barack H. Obama".to_string(),
    ];

    let identity_id = corpus.add_identity(identity);

    let stored_identity = corpus.identities().get(&identity_id).unwrap();
    assert_eq!(stored_identity.aliases.len(), 3);
    assert!(stored_identity.aliases.contains(&"B. Obama".to_string()));
    assert!(stored_identity
        .aliases
        .contains(&"President Obama".to_string()));
}

/// Test: Corpus identity with KB information
#[test]
fn test_corpus_identity_kb_info() {
    let mut corpus = Corpus::new();

    let identity = Identity::from_kb(0, "Marie Curie", "wikidata", "Q7186")
        .with_description("Polish-French physicist and chemist");

    let identity_id = corpus.add_identity(identity);

    let stored_identity = corpus.identities().get(&identity_id).unwrap();
    assert_eq!(stored_identity.kb_name, Some("wikidata".to_string()));
    assert_eq!(stored_identity.kb_id, Some("Q7186".to_string()));
    assert_eq!(stored_identity.canonical_name, "Marie Curie");
    assert!(stored_identity.description.is_some());
}
