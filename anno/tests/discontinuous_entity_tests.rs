//! Tests for discontinuous entity span support.
//!
//! Discontinuous entities are entities that span multiple non-contiguous
//! regions of text. For example:
//!
//! - "New York and Los Angeles airports" - "airports" applies to both cities
//! - "chronic kidney and liver disease" - "chronic" and "disease" apply to both organs

use anno_core::{DiscontinuousSpan, Entity, EntityType};

#[test]
fn test_discontinuous_span_creation() {
    // Create a span with two separate segments
    let span = DiscontinuousSpan::new(vec![0..5, 10..15]);

    assert_eq!(span.segments().len(), 2);
    assert_eq!(span.segments()[0], 0..5);
    assert_eq!(span.segments()[1], 10..15);
}

#[test]
fn test_discontinuous_span_total_length() {
    let span = DiscontinuousSpan::new(vec![0..5, 10..15]);

    // Total length is sum of segment lengths
    assert_eq!(span.total_len(), 10); // 5 + 5
}

#[test]
fn test_discontinuous_span_contiguous() {
    // A contiguous span is a special case of discontinuous
    let span = DiscontinuousSpan::contiguous(0, 10);

    assert_eq!(span.segments().len(), 1);
    assert_eq!(span.total_len(), 10);
}

#[test]
fn test_discontinuous_span_is_contiguous() {
    let contiguous = DiscontinuousSpan::contiguous(0, 10);
    let discontinuous = DiscontinuousSpan::new(vec![0..5, 10..15]);

    assert!(contiguous.is_contiguous());
    assert!(!discontinuous.is_contiguous());
}

#[test]
fn test_discontinuous_span_sorting() {
    // Segments should be sorted by start position
    let span = DiscontinuousSpan::new(vec![10..15, 0..5, 20..25]);

    assert_eq!(span.segments()[0], 0..5);
    assert_eq!(span.segments()[1], 10..15);
    assert_eq!(span.segments()[2], 20..25);
}

#[test]
fn test_discontinuous_span_bounding_range() {
    let span = DiscontinuousSpan::new(vec![10..15, 0..5, 20..25]);

    // Bounding range covers the entire span
    let bounding = span.bounding_range().expect("should have bounding range");
    assert_eq!(bounding.start, 0);
    assert_eq!(bounding.end, 25);
}

#[test]
fn test_entity_with_discontinuous_span() {
    let mut entity = Entity::new("airports", EntityType::Location, 0, 8, 0.9);

    // Set discontinuous span
    entity.set_discontinuous_span(DiscontinuousSpan::new(vec![0..8, 25..33]));

    assert!(entity.is_discontinuous());
    // discontinuous_segments() returns Some for truly discontinuous entities
    let segments = entity
        .discontinuous_segments()
        .expect("should have segments");
    assert_eq!(segments.len(), 2);
}

#[test]
fn test_entity_is_discontinuous_default_false() {
    let entity = Entity::new("New York", EntityType::Location, 0, 8, 0.9);

    assert!(!entity.is_discontinuous());
    // discontinuous_segments() returns None for contiguous entities
    assert!(entity.discontinuous_segments().is_none());
}

#[test]
fn test_entity_discontinuous_total_length() {
    let mut entity = Entity::new("airports", EntityType::Location, 0, 8, 0.9);
    entity.set_discontinuous_span(DiscontinuousSpan::new(vec![0..5, 20..28]));

    // Total discontinuous length
    if let Some(span) = &entity.discontinuous_span {
        assert_eq!(span.total_len(), 13); // 5 + 8
    }
}

#[test]
fn test_discontinuous_span_extract_text() {
    let text = "New York and Los Angeles airports";
    //          01234567890123456789012345678901234
    //                    1111111111222222222233333
    // "New York" = 0..8
    // "Los Angeles" = 13..24
    let span = DiscontinuousSpan::new(vec![
        0..8,   // "New York"
        13..24, // "Los Angeles"
    ]);

    // Extract text from discontinuous spans
    let texts: Vec<&str> = span
        .segments()
        .iter()
        .filter_map(|seg| text.get(seg.start..seg.end))
        .collect();

    assert_eq!(texts, vec!["New York", "Los Angeles"]);
}

#[test]
fn test_discontinuous_span_empty() {
    let span = DiscontinuousSpan::new(vec![]);

    assert!(span.segments().is_empty());
    assert_eq!(span.total_len(), 0);
    assert!(span.is_contiguous()); // Empty span is technically contiguous
}

#[test]
fn test_discontinuous_span_single_segment() {
    #[allow(clippy::single_range_in_vec_init)]
    let span = DiscontinuousSpan::new(vec![0..10]);

    assert!(span.is_contiguous());
    assert_eq!(span.total_len(), 10);
}

// Test that discontinuous span is properly serialized
#[test]
fn test_discontinuous_span_serialization() {
    let span = DiscontinuousSpan::new(vec![0..5, 10..15]);

    let json = serde_json::to_string(&span).unwrap();
    let deserialized: DiscontinuousSpan = serde_json::from_str(&json).unwrap();

    assert_eq!(span.segments(), deserialized.segments());
}

// Test entity with discontinuous span serialization roundtrip
#[test]
fn test_entity_discontinuous_serialization() {
    let mut entity = Entity::new("airports", EntityType::Location, 0, 8, 0.9);
    entity.set_discontinuous_span(DiscontinuousSpan::new(vec![0..5, 10..15]));

    let json = serde_json::to_string(&entity).unwrap();
    let deserialized: Entity = serde_json::from_str(&json).unwrap();

    assert!(deserialized.is_discontinuous());
    assert_eq!(
        entity.discontinuous_span.as_ref().map(|s| s.segments()),
        deserialized
            .discontinuous_span
            .as_ref()
            .map(|s| s.segments())
    );
}
