//! Property tests for cross-document coreference transitivity
//!
//! Ensures that if A≈B and B≈C, then A≈C (via identity linking).

use anno_coalesce::Resolver;
use anno_core::{Corpus, GroundedDocument, Location, Signal, Track, TrackId};

/// Property: Crossdoc coref should be transitive
///
/// If doc1 has track A, doc2 has track B (A≈B), and doc3 has track C (B≈C),
/// then all three should link to the same identity.
#[test]
fn prop_crossdoc_transitivity() {
    let mut corpus = Corpus::new();

    // Doc1: "Barack Obama"
    let mut doc1 = GroundedDocument::new("doc1", "Barack Obama was president.");
    let sig1 = Signal::new(0, Location::text(0, 12), "Barack Obama", "PER", 0.95);
    let sig1_id = doc1.add_signal(sig1);
    let track1 = Track {
        id: TrackId::new(0), // Will be reassigned by add_track
        signals: vec![anno_core::SignalRef {
            signal_id: sig1_id,
            position: 0,
        }],
        canonical_surface: "barack obama".to_string(),
        entity_type: Some("PER".to_string()),
        identity_id: None,
        cluster_confidence: 0.90,
        embedding: None,
    };
    let track1_id = doc1.add_track(track1);
    corpus.add_document(doc1);

    // Doc2: "Obama" (similar to doc1)
    let mut doc2 = GroundedDocument::new("doc2", "Obama served from 2009 to 2017.");
    let sig2 = Signal::new(0, Location::text(0, 5), "Obama", "PER", 0.85);
    let sig2_id = doc2.add_signal(sig2);
    let track2 = Track {
        id: TrackId::new(0),
        signals: vec![anno_core::SignalRef {
            signal_id: sig2_id,
            position: 0,
        }],
        canonical_surface: "obama".to_string(),
        entity_type: Some("PER".to_string()),
        identity_id: None,
        cluster_confidence: 0.85,
        embedding: None,
    };
    let track2_id = doc2.add_track(track2);
    corpus.add_document(doc2);

    // Doc3: "B. Obama" (similar to doc2)
    let mut doc3 = GroundedDocument::new("doc3", "B. Obama won the Nobel Prize.");
    let sig3 = Signal::new(0, Location::text(0, 8), "B. Obama", "PER", 0.80);
    let sig3_id = doc3.add_signal(sig3);
    let track3 = Track {
        id: TrackId::new(0),
        signals: vec![anno_core::SignalRef {
            signal_id: sig3_id,
            position: 0,
        }],
        canonical_surface: "b. obama".to_string(),
        entity_type: Some("PER".to_string()),
        identity_id: None,
        cluster_confidence: 0.80,
        embedding: None,
    };
    let track3_id = doc3.add_track(track3);
    corpus.add_document(doc3);

    // Run crossdoc coref with low threshold to ensure all merge
    let resolver = Resolver::new().with_threshold(0.3);
    let _identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // All three tracks should link to the same identity
    let doc1_track = corpus
        .get_document("doc1")
        .unwrap()
        .get_track(track1_id)
        .unwrap();
    let doc2_track = corpus
        .get_document("doc2")
        .unwrap()
        .get_track(track2_id)
        .unwrap();
    let doc3_track = corpus
        .get_document("doc3")
        .unwrap()
        .get_track(track3_id)
        .unwrap();

    assert_eq!(doc1_track.identity_id, doc2_track.identity_id);
    assert_eq!(doc2_track.identity_id, doc3_track.identity_id);
    assert!(
        doc1_track.identity_id.is_some(),
        "Tracks should be linked to identity"
    );
}

/// Property: Tracks from same document should not be merged
#[test]
fn prop_same_doc_tracks_not_merged() {
    let mut corpus = Corpus::new();

    // Doc with two different entities
    let mut doc = GroundedDocument::new(
        "doc1",
        "Apple Inc. and Microsoft Corporation are competitors.",
    );
    let sig1 = Signal::new(0, Location::text(0, 9), "Apple Inc.", "ORG", 0.95);
    let sig1_id = doc.add_signal(sig1);
    let track1 = Track {
        id: TrackId::new(0), // Will be reassigned by add_track
        signals: vec![anno_core::SignalRef {
            signal_id: sig1_id,
            position: 0,
        }],
        canonical_surface: "apple inc".to_string(),
        entity_type: Some("ORG".to_string()),
        identity_id: None,
        cluster_confidence: 0.90,
        embedding: None,
    };
    let track1_id = doc.add_track(track1);

    let sig2 = Signal::new(
        1,
        Location::text(14, 35),
        "Microsoft Corporation",
        "ORG",
        0.95,
    );
    let sig2_id = doc.add_signal(sig2);
    let track2 = Track {
        id: TrackId::new(0),
        signals: vec![anno_core::SignalRef {
            signal_id: sig2_id,
            position: 1,
        }],
        canonical_surface: "microsoft corporation".to_string(),
        entity_type: Some("ORG".to_string()),
        identity_id: None,
        cluster_confidence: 0.90,
        embedding: None,
    };
    let track2_id = doc.add_track(track2);

    corpus.add_document(doc);

    // Run crossdoc coref
    let resolver = Resolver::new().with_threshold(0.3);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should create 2 identities (one per track, since they're different entities)
    assert_eq!(identity_ids.len(), 2);

    let doc_ref = corpus.get_document("doc1").unwrap();
    let track1_ref = doc_ref.get_track(track1_id).unwrap();
    let track2_ref = doc_ref.get_track(track2_id).unwrap();

    // Tracks should have different identity IDs
    assert_ne!(track1_ref.identity_id, track2_ref.identity_id);
}
