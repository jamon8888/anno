//! Property tests for track consistency
//!
//! Ensures tracks maintain invariants: all signals from same document,
//! canonical_surface matches at least one signal, etc.

use anno_core::{GroundedDocument, Location, Signal, Track, TrackId};

/// Property: All signals in a track must be from the same document
#[test]
fn prop_track_signals_same_doc() {
    let mut doc = GroundedDocument::new("doc1", "Barack Obama was president. He served from 2009.");

    let sig1 = Signal::new(0, Location::text(0, 12), "Barack Obama", "PER", 0.95);
    let sig1_id = doc.add_signal(sig1);

    let sig2 = Signal::new(1, Location::text(45, 47), "He", "PRON", 0.85);
    let sig2_id = doc.add_signal(sig2);

    // Create track with both signals
    let track = Track {
        id: TrackId::new(0), // Will be reassigned by add_track
        signals: vec![
            anno_core::SignalRef {
                signal_id: sig1_id,
                position: 0,
            },
            anno_core::SignalRef {
                signal_id: sig2_id,
                position: 1,
            },
        ],
        canonical_surface: "barack obama".to_string(),
        entity_type: Some("PER".to_string()),
        identity_id: None,
        cluster_confidence: 0.90,
        embedding: None,
    };

    let track_id = doc.add_track(track);

    // Verify all signals exist in the document
    let track_ref = doc.get_track(track_id).unwrap();
    for signal_ref in &track_ref.signals {
        assert!(
            doc.get_signal(signal_ref.signal_id).is_some(),
            "Signal {} should exist in document",
            signal_ref.signal_id
        );
    }
}

/// Property: Track canonical_surface should match at least one signal
#[test]
fn prop_track_canonical_matches_signal() {
    let mut doc = GroundedDocument::new("doc1", "Barack Obama was president.");

    let sig1 = Signal::new(0, Location::text(0, 12), "Barack Obama", "PER", 0.95);
    let sig1_id = doc.add_signal(sig1);

    let track = Track {
        id: TrackId::new(0), // Will be reassigned by add_track
        signals: vec![anno_core::SignalRef {
            signal_id: sig1_id,
            position: 0,
        }],
        canonical_surface: "barack obama".to_string(), // Lowercase version
        entity_type: Some("PER".to_string()),
        identity_id: None,
        cluster_confidence: 0.90,
        embedding: None,
    };

    let track_id = doc.add_track(track);

    // Canonical should be similar to signal text (case-insensitive)
    let track_ref = doc.get_track(track_id).unwrap();
    let signal = doc.get_signal(track_ref.signals[0].signal_id).unwrap();
    let canonical_lower = track_ref.canonical_surface.to_lowercase();
    let signal_lower = signal.surface().to_lowercase();

    // Canonical should contain words from signal (or vice versa)
    let canonical_words: std::collections::HashSet<&str> =
        canonical_lower.split_whitespace().collect();
    let signal_words: std::collections::HashSet<&str> = signal_lower.split_whitespace().collect();

    // At least one word should overlap
    assert!(
        !canonical_words.is_disjoint(&signal_words),
        "Canonical '{}' should share words with signal '{}'",
        track_ref.canonical_surface,
        signal.surface()
    );
}

/// Property: Track entity_type should match signal labels
#[test]
fn prop_track_type_matches_signals() {
    let mut doc = GroundedDocument::new("doc1", "Barack Obama was president.");

    let sig1 = Signal::new(0, Location::text(0, 12), "Barack Obama", "PER", 0.95);
    let sig1_id = doc.add_signal(sig1);

    let track = Track {
        id: TrackId::new(0), // Will be reassigned by add_track
        signals: vec![anno_core::SignalRef {
            signal_id: sig1_id,
            position: 0,
        }],
        canonical_surface: "barack obama".to_string(),
        entity_type: Some("PER".to_string()), // Should match signal label
        identity_id: None,
        cluster_confidence: 0.90,
        embedding: None,
    };

    let track_id = doc.add_track(track);

    // Verify entity type matches
    let track_ref = doc.get_track(track_id).unwrap();
    let signal = doc.get_signal(track_ref.signals[0].signal_id).unwrap();

    assert_eq!(track_ref.entity_type.as_deref(), Some("PER"));
    assert_eq!(signal.label(), "PER");
}
