//! Integration tests for output formatting
//!
//! Tests the hierarchical verbose output system and various output formats.

use anno::cli::output::{color, confidence_bar, print_signals, type_color};
use anno_core::{GroundedDocument, Location, Signal, Track};

/// Test: Level 0 output (default) - entities grouped by type
#[test]
fn test_output_level_0() {
    let text = "Marie Curie won the Nobel Prize. She was a physicist.";
    let mut doc = GroundedDocument::new("test", text);

    doc.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Marie Curie",
        "PER",
        0.95,
    ));
    doc.add_signal(Signal::new(1, Location::text(38, 41), "She", "PER", 0.85));
    doc.add_signal(Signal::new(
        2,
        Location::text(17, 29),
        "Nobel Prize",
        "AWARD",
        0.92,
    ));

    // Level 0 should show entities grouped by type
    // Note: This test verifies the function can be called without panicking
    // Actual output format is tested via CLI e2e tests
    print_signals(&doc, text, 0);

    // Verify document has signals
    assert_eq!(doc.signals().len(), 3);
}

/// Test: Level 1 output (-v) - adds confidence and context
#[test]
fn test_output_level_1() {
    let text = "Barack Obama was president. He served from 2009 to 2017.";
    let mut doc = GroundedDocument::new("test", text);

    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Barack Obama",
        "PER",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(1, Location::text(38, 40), "He", "PER", 0.75)); // Low confidence

    // Level 1 should show confidence for low/high confidence entities
    print_signals(&doc, text, 1);

    // Verify signals exist
    assert_eq!(doc.signals().len(), 2);
}

/// Test: Level 2 output (-vv) - adds tracks (coreference)
#[test]
fn test_output_level_2() {
    let text = "Marie Curie won the Nobel Prize. She was a physicist.";
    let mut doc = GroundedDocument::new("test", text);

    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Marie Curie",
        "PER",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(1, Location::text(38, 41), "She", "PER", 0.85));

    // Create track for coreference
    let mut track = Track::new(0, "marie curie");
    track.add_signal(sig1, 0);
    track.add_signal(sig2, 1);
    track.entity_type = Some("PER".to_string());
    doc.add_track(track);

    // Level 2 should show tracks
    print_signals(&doc, text, 2);

    // Verify track exists
    assert_eq!(doc.tracks().count(), 1);
}

/// Test: Level 3 output (-vvv) - adds identities and annotated text
#[test]
fn test_output_level_3() {
    let text = "Marie Curie won the Nobel Prize.";
    let mut doc = GroundedDocument::new("test", text);

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

    // Add identity
    use anno_core::Identity;
    let identity = Identity::from_kb(0, "Marie Curie", "wikidata", "Q7186");
    let identity_id = doc.add_identity(identity);
    doc.link_track_to_identity(track_id, identity_id);

    // Level 3 should show identities and annotated text
    print_signals(&doc, text, 3);

    // Verify identity exists
    assert_eq!(doc.identities().count(), 1);
}

/// Test: Confidence bar formatting
#[test]
fn test_confidence_bar() {
    let bar1 = confidence_bar(0.95);
    let bar2 = confidence_bar(0.50);
    let bar3 = confidence_bar(0.20);

    // Should produce colored bars
    assert!(!bar1.is_empty());
    assert!(!bar2.is_empty());
    assert!(!bar3.is_empty());

    // High confidence should have more filled characters
    let filled1 = bar1.matches('#').count();
    let filled2 = bar2.matches('#').count();
    let filled3 = bar3.matches('#').count();

    assert!(filled1 > filled2);
    assert!(filled2 > filled3);
}

/// Test: Type color assignment
#[test]
fn test_type_color() {
    let col1 = type_color("PER");
    let col2 = type_color("ORG");
    let col3 = type_color("LOC");

    // Should return color codes
    assert!(!col1.is_empty());
    assert!(!col2.is_empty());
    assert!(!col3.is_empty());
}

/// Test: Empty document output
#[test]
fn test_output_empty_document() {
    let text = "No entities here.";
    let doc = GroundedDocument::new("test", text);

    // Should handle empty document gracefully
    print_signals(&doc, text, 0);

    assert_eq!(doc.signals().len(), 0);
}

/// Test: Document with only low-confidence entities
#[test]
fn test_output_low_confidence() {
    let text = "Maybe an entity, maybe not.";
    let mut doc = GroundedDocument::new("test", text);

    // Add low-confidence signal
    doc.add_signal(Signal::new(0, Location::text(0, 5), "Maybe", "PER", 0.3));

    // Level 0 should filter low-confidence (<0.5)
    print_signals(&doc, text, 0);

    // Level 1 should show it with confidence
    print_signals(&doc, text, 1);

    assert_eq!(doc.signals().len(), 1);
}

/// Test: Document with negated entities
#[test]
fn test_output_negated() {
    let text = "John is not a doctor.";
    let mut doc = GroundedDocument::new("test", text);

    let mut sig = Signal::new(0, Location::text(0, 4), "John", "PER", 0.95);
    sig.negated = true;
    doc.add_signal(sig);

    // Level 1+ should show [NEG] marker
    print_signals(&doc, text, 1);

    let signals: Vec<_> = doc.signals().iter().collect();
    assert!(signals[0].negated);
}

/// Test: Document with quantifiers
#[test]
fn test_output_quantified() {
    use anno_core::Quantifier;

    let text = "All students passed.";
    let mut doc = GroundedDocument::new("test", text);

    let mut sig = Signal::new(0, Location::text(0, 3), "All", "QUANT", 0.95);
    sig.quantifier = Some(Quantifier::Universal);
    doc.add_signal(sig);

    // Level 1+ should show quantifier
    print_signals(&doc, text, 1);

    let signals: Vec<_> = doc.signals().iter().collect();
    assert_eq!(signals[0].quantifier, Some(Quantifier::Universal));
}

/// Test: Multiple entity types
#[test]
fn test_output_multiple_types() {
    let text = "Barack Obama works at Apple Inc. in California.";
    let mut doc = GroundedDocument::new("test", text);

    doc.add_signal(Signal::new(
        0,
        Location::text(0, 12),
        "Barack Obama",
        "PER",
        0.95,
    ));
    doc.add_signal(Signal::new(
        1,
        Location::text(23, 33),
        "Apple Inc.",
        "ORG",
        0.90,
    ));
    doc.add_signal(Signal::new(
        2,
        Location::text(37, 47),
        "California",
        "LOC",
        0.95,
    ));

    // Should group by type
    print_signals(&doc, text, 0);

    assert_eq!(doc.signals().len(), 3);
}
