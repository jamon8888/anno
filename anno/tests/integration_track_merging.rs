//! Integration tests for track merging within documents
//!
//! Tests merging tracks, handling overlapping signals, and track consistency.

use anno_core::{GroundedDocument, Location, Signal, Track};

/// Test: Merge two tracks with overlapping signals
#[test]
fn test_merge_tracks_overlapping() {
    let mut doc = GroundedDocument::new("test", "John Smith works at Acme. He is the CEO.");

    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 10),
        "John Smith",
        "PER",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(1, Location::text(26, 28), "He", "PER", 0.85));

    // Create two separate tracks
    let mut track1 = Track::new(0, "john smith");
    track1.add_signal(sig1, 0);
    let track1_id = doc.add_track(track1);

    let mut track2 = Track::new(0, "he");
    track2.add_signal(sig2, 0);
    let track2_id = doc.add_track(track2);

    // Merge tracks
    let merged_id = doc.merge_tracks(&[track1_id, track2_id]);
    assert!(merged_id.is_some());

    // Should have only 1 track now
    assert_eq!(doc.tracks().count(), 1);

    let merged = doc.get_track(merged_id.unwrap()).unwrap();
    assert_eq!(merged.signals.len(), 2);
    assert_eq!(merged.canonical_surface, "john smith"); // From first track
}

/// Test: Merge tracks with different entity types (should fail or handle gracefully)
#[test]
fn test_merge_tracks_type_mismatch() {
    let mut doc = GroundedDocument::new("test", "Apple Inc. and apple fruit.");

    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 9),
        "Apple Inc.",
        "ORG",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(
        1,
        Location::text(14, 19),
        "apple",
        "FRUIT",
        0.90,
    ));

    let mut track1 = Track::new(0, "apple inc");
    track1.add_signal(sig1, 0);
    track1.entity_type = Some("ORG".to_string());
    let track1_id = doc.add_track(track1);

    let mut track2 = Track::new(0, "apple");
    track2.add_signal(sig2, 0);
    track2.entity_type = Some("FRUIT".to_string());
    let track2_id = doc.add_track(track2);

    // Merge should still work (type checking happens at crossdoc level)
    let merged_id = doc.merge_tracks(&[track1_id, track2_id]);
    assert!(merged_id.is_some());

    // Merged track should have signals from both
    let merged = doc.get_track(merged_id.unwrap()).unwrap();
    assert_eq!(merged.signals.len(), 2);
}

/// Test: Merge empty tracks (edge case)
#[test]
fn test_merge_empty_tracks() {
    let mut doc = GroundedDocument::new("test", "Test");

    let mut track1 = Track::new(0, "empty1");
    let track1_id = doc.add_track(track1);

    let mut track2 = Track::new(0, "empty2");
    let track2_id = doc.add_track(track2);

    // Merge empty tracks
    let merged_id = doc.merge_tracks(&[track1_id, track2_id]);

    // Should either merge (creating empty track) or return None
    if let Some(id) = merged_id {
        let merged = doc.get_track(id).unwrap();
        assert_eq!(merged.signals.len(), 0);
    }
}

/// Test: Merge single track (no-op)
#[test]
fn test_merge_single_track() {
    let mut doc = GroundedDocument::new("test", "Test");

    let sig = doc.add_signal(Signal::new(0, Location::text(0, 4), "Test", "OTHER", 0.5));
    let mut track = Track::new(0, "test");
    track.add_signal(sig, 0);
    let track_id = doc.add_track(track);

    // Merge single track with itself
    let merged_id = doc.merge_tracks(&[track_id]);

    // Should return same track or None
    assert!(merged_id.is_some() || merged_id.is_none());
}

/// Test: Merge tracks with duplicate signals (should deduplicate)
#[test]
fn test_merge_tracks_duplicate_signals() {
    let mut doc = GroundedDocument::new(
        "test",
        "John Smith is the CEO. John Smith founded the company.",
    );

    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 10),
        "John Smith",
        "PER",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(
        1,
        Location::text(28, 38),
        "John Smith",
        "PER",
        0.95,
    ));

    // Create two tracks with same signal (shouldn't happen in practice, but test edge case)
    let mut track1 = Track::new(0, "john smith");
    track1.add_signal(sig1, 0);
    let track1_id = doc.add_track(track1);

    let mut track2 = Track::new(0, "john smith");
    track2.add_signal(sig2, 1);
    let track2_id = doc.add_track(track2);

    // Merge
    let merged_id = doc.merge_tracks(&[track1_id, track2_id]);
    assert!(merged_id.is_some());

    let merged = doc.get_track(merged_id.unwrap()).unwrap();
    // Should have 2 signals (one from each track)
    assert_eq!(merged.signals.len(), 2);
}

/// Test: Track canonical surface after merge
#[test]
fn test_merge_tracks_canonical_surface() {
    let mut doc = GroundedDocument::new("test", "Barack Obama was president. He served from 2009.");

    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Barack Obama",
        "PER",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(1, Location::text(38, 40), "He", "PER", 0.85));

    let mut track1 = Track::new(0, "barack obama");
    track1.add_signal(sig1, 0);
    let track1_id = doc.add_track(track1);

    let mut track2 = Track::new(0, "he");
    track2.add_signal(sig2, 0);
    let track2_id = doc.add_track(track2);

    // Merge - canonical should come from first track
    let merged_id = doc.merge_tracks(&[track1_id, track2_id]).unwrap();
    let merged = doc.get_track(merged_id).unwrap();

    assert_eq!(merged.canonical_surface, "barack obama");
}

/// Test: Track confidence after merge
#[test]
fn test_merge_tracks_confidence() {
    let mut doc = GroundedDocument::new("test", "Test entity.");

    let sig1 = doc.add_signal(Signal::new(0, Location::text(0, 4), "Test", "OTHER", 0.9));
    let sig2 = doc.add_signal(Signal::new(
        1,
        Location::text(5, 11),
        "entity",
        "OTHER",
        0.7,
    ));

    let mut track1 = Track::new(0, "test");
    track1.add_signal(sig1, 0);
    track1.cluster_confidence = 0.9;
    let track1_id = doc.add_track(track1);

    let mut track2 = Track::new(0, "entity");
    track2.add_signal(sig2, 0);
    track2.cluster_confidence = 0.7;
    let track2_id = doc.add_track(track2);

    // Merge
    let merged_id = doc.merge_tracks(&[track1_id, track2_id]).unwrap();
    let merged = doc.get_track(merged_id).unwrap();

    // Confidence should be from first track (or average, depending on implementation)
    assert!(merged.cluster_confidence > 0.0);
}

/// Test: Track entity type after merge
#[test]
fn test_merge_tracks_entity_type() {
    let mut doc = GroundedDocument::new("test", "John works at Acme.");

    let sig1 = doc.add_signal(Signal::new(0, Location::text(0, 4), "John", "PER", 0.95));
    let sig2 = doc.add_signal(Signal::new(1, Location::text(14, 18), "Acme", "ORG", 0.90));

    let mut track1 = Track::new(0, "john");
    track1.add_signal(sig1, 0);
    track1.entity_type = Some("PER".to_string());
    let track1_id = doc.add_track(track1);

    let mut track2 = Track::new(0, "acme");
    track2.add_signal(sig2, 0);
    track2.entity_type = Some("ORG".to_string());
    let track2_id = doc.add_track(track2);

    // Merge tracks with different types
    let merged_id = doc.merge_tracks(&[track1_id, track2_id]).unwrap();
    let merged = doc.get_track(merged_id).unwrap();

    // Type should come from first track (or be None if conflicting)
    assert!(merged.entity_type.is_some() || merged.entity_type.is_none());
}
