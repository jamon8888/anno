//! Regression test: Crossdoc directory mode should automatically create tracks
//!
//! This test ensures that when processing a directory with `anno crossdoc --directory`,
//! tracks (Level 2) are automatically created from signals (Level 1), improving
//! clustering quality.

use anno_core::{GroundedDocument, Signal, Location};
use anno::HeuristicNER;
use std::collections::HashMap;

#[test]
fn test_crossdoc_creates_tracks() {
    // Simulate what crossdoc does: extract entities, create signals, then resolve coreference
    let text = "Apple was founded by Steve Jobs. Jobs later left Apple. He started NeXT.";
    let ner = HeuristicNER::new();
    let entities = ner.extract_entities(text, None).unwrap();
    
    // Build GroundedDocument (as crossdoc does)
    let mut doc = GroundedDocument::new("test_doc", text);
    let mut signal_ids = Vec::new();
    
    for e in &entities {
        let signal = Signal::new(
            0,
            Location::text(e.start, e.end),
            &e.text,
            e.entity_type.as_label(),
            e.confidence as f32,
        );
        let id = doc.add_signal(signal);
        signal_ids.push(id);
    }
    
    // Auto-create tracks (as crossdoc now does)
    use anno::cli::utils::resolve_coreference;
    resolve_coreference(&mut doc, text, &signal_ids);
    
    // Verify tracks were created
    let tracks: Vec<_> = doc.tracks().collect();
    assert!(
        !tracks.is_empty(),
        "Crossdoc should create tracks from signals. Found {} tracks",
        tracks.len()
    );
    
    // Verify "Steve Jobs" and "Jobs" are in the same track (coreference)
    let mut jobs_track_found = false;
    for track in &tracks {
        let mentions: Vec<String> = track.signals.iter()
            .filter_map(|s| doc.get_signal(s.signal_id))
            .map(|sig| sig.surface().to_string())
            .collect();
        
        // Check if this track contains both "Steve Jobs" and "Jobs"
        if mentions.iter().any(|m| m.contains("Steve Jobs")) && 
           mentions.iter().any(|m| m == "Jobs") {
            jobs_track_found = true;
            break;
        }
    }
    
    assert!(
        jobs_track_found,
        "Crossdoc should group 'Steve Jobs' and 'Jobs' into the same track via coreference"
    );
}

#[test]
fn test_crossdoc_tracks_have_canonical_forms() {
    let text = "The European Union (EU) has 27 member states. The EU was founded in 1993.";
    let ner = HeuristicNER::new();
    let entities = ner.extract_entities(text, None).unwrap();
    
    let mut doc = GroundedDocument::new("test_doc", text);
    let mut signal_ids = Vec::new();
    
    for e in &entities {
        let signal = Signal::new(
            0,
            Location::text(e.start, e.end),
            &e.text,
            e.entity_type.as_label(),
            e.confidence as f32,
        );
        let id = doc.add_signal(signal);
        signal_ids.push(id);
    }
    
    use anno::cli::utils::resolve_coreference;
    resolve_coreference(&mut doc, text, &signal_ids);
    
    // Verify tracks have canonical surface forms
    let tracks: Vec<_> = doc.tracks().collect();
    for track in &tracks {
        assert!(
            !track.canonical_surface.is_empty(),
            "Each track should have a canonical surface form"
        );
        assert!(
            !track.signals.is_empty(),
            "Each track should contain at least one signal"
        );
    }
}

